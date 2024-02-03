use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use dashmap::DashMap;

use byte_slice_cast::*;

use gst::glib::Cast;
use gst::prelude::{ElementExt, ElementExtManual, GstBinExtManual};
use gst::{Pipeline, State};

use gst_app::AppSrc;
use gst_audio::AudioInfo;

use serde::{Deserialize, Serialize};

use serenity::all::Ready;
use serenity::async_trait;
use serenity::prelude::*;

use songbird::driver::DecodeMode;
use songbird::events::context_data::DisconnectData;
use songbird::model::id::UserId;
use songbird::model::payload::{ClientDisconnect, Speaking};
use songbird::packet::Packet;
use songbird::EventHandler as VoiceEventHandler;
use songbird::{CoreEvent, Event, EventContext, SerenityInit};

extern crate gstreamer as gst;
extern crate gstreamer_app as gst_app;
extern crate gstreamer_audio as gst_audio;

#[derive(Serialize, Deserialize, Debug)]
struct Config {
    discord_token: String,
    rtmp_url: String,
}

static GLOBAL_CONFIG: std::sync::Mutex<Option<Config>> = std::sync::Mutex::new(Option::None);

struct Handler;

#[derive(Clone)]
struct Receiver {
    inner: Arc<InnerReceiver>,
}

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, _: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }
}

struct InnerReceiver {
    last_tick_was_empty: AtomicBool,
    known_ssrcs: DashMap<u32, UserId>,
    pipeline: Pipeline,
    appsrc: AppSrc,
}
struct PoiseData {}

type PoiseError = Box<dyn std::error::Error + Send + Sync>;
type PoiseContext<'a> = poise::Context<'a, PoiseData, PoiseError>;

impl Receiver {
    pub fn new(url: &str) -> Self {
        let pipeline = gst::Pipeline::new();

        let video_source = gst::ElementFactory::make("videotestsrc")
            .property("is-live", true)
            .build()
            .unwrap();
        let video_convert = gst::ElementFactory::make("videoconvert").build().unwrap();
        let clock_overlay = gst::ElementFactory::make("clockoverlay")
            .property_from_str("halignment", "center")
            .property_from_str("valignment", "top")
            .property("shaded-background", true)
            .build()
            .unwrap();
        let video_rate = gst::ElementFactory::make("videorate")
            .property("drop-only", true)
            .build()
            .unwrap();
        let video_encode = gst::ElementFactory::make("x264enc")
            .property("bitrate", 4000 as u32)
            .property_from_str("tune", "zerolatency")
            .property("key-int-max", 60 as u32)
            .property_from_str("speed-preset", "ultrafast")
            .property("option-string", "keyint=60:min-keyint=60")
            .build()
            .unwrap();
        let queue = gst::ElementFactory::make("queue").build().unwrap();

        let flvmux = gst::ElementFactory::make("flvmux")
            .property("streamable", true)
            .build()
            .unwrap();
        let rtmpsink = gst::ElementFactory::make("rtmpsink")
            .property("location", url)
            .build()
            .unwrap();

        let audio_rate = gst::ElementFactory::make("audiorate").build().unwrap();
        let audio_convert = gst::ElementFactory::make("audioconvert").build().unwrap();
        let audio_resample = gst::ElementFactory::make("audioresample").build().unwrap();

        let aac = gst::ElementFactory::make("voaacenc")
            .property("bitrate", 96000)
            .build()
            .unwrap();

        let info = AudioInfo::builder(gst_audio::AudioFormat::S16le, 48000, 2)
            .build()
            .unwrap();
        let voice_cap = info.to_caps().unwrap();

        let appsrc = gst_app::AppSrc::builder()
            .caps(&voice_cap)
            .format(gst::Format::Time)
            .is_live(true)
            .do_timestamp(true)
            .build();

        pipeline
            .add_many([
                &video_source,
                &video_convert,
                &clock_overlay,
                &video_rate,
                &video_encode,
                &queue,
                &audio_rate,
                &audio_convert,
                &audio_resample,
                &aac,
                &flvmux,
                &rtmpsink,
                appsrc.upcast_ref(),
            ])
            .unwrap();

        let video_caps = gst::Caps::builder("video/x-raw")
            .field("width", 1280 as i32)
            .field("height", 720 as i32)
            .build();

        video_source
            .link_filtered(&video_convert, &video_caps)
            .unwrap();
        video_convert.link(&clock_overlay).unwrap();
        clock_overlay.link(&video_rate).unwrap();
        video_rate.link(&video_encode).unwrap();
        video_encode.link(&queue).unwrap();
        queue.link(&flvmux).unwrap();

        appsrc.link(&audio_rate).unwrap();

        gst::Element::link_many([&audio_rate, &audio_convert, &audio_resample, &aac, &flvmux])
            .unwrap();

        flvmux.link(&rtmpsink).unwrap();

        pipeline.set_state(State::Playing).unwrap();

        Self {
            inner: Arc::new(InnerReceiver {
                last_tick_was_empty: AtomicBool::default(),
                known_ssrcs: DashMap::new(),
                pipeline: pipeline,
                appsrc: appsrc,
            }),
        }
    }
}

#[async_trait]
impl VoiceEventHandler for Receiver {
    #[allow(unused_variables)]
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        use EventContext as Ctx;
        match ctx {
            Ctx::SpeakingStateUpdate(Speaking {
                speaking,
                ssrc,
                user_id,
                ..
            }) => {
                if let Some(user) = user_id {
                    self.inner.known_ssrcs.insert(*ssrc, *user);
                }
            }
            Ctx::VoiceTick(tick) => {
                let speaking = tick.speaking.len();
                let total_participants = speaking + tick.silent.len();
                let last_tick_was_empty = self.inner.last_tick_was_empty.load(Ordering::SeqCst);

                // https://github.com/serenity-rs/songbird/issues/100
                // > Each event contains up to 20ms of mono 16-bit PCM audio from a single user at 48kHZ -- each i16 is a sample.
                // but it seems 3840 bytes so it's stereo?
                // 48000Hz * (20ms / 1000ms) * (16bit / 8bit) = 1920 bytes
                let mut buffer = gst::Buffer::with_size(1920 * 2).unwrap();

                {
                    let buffer = buffer.get_mut().unwrap();
                    let mut samples = buffer.map_writable().unwrap();
                    let samples = samples.as_mut_slice_of::<i16>().unwrap();
                    for i in &mut samples[..] {
                        *i = 0
                    }

                    if speaking == 0 && !last_tick_was_empty {
                        println!("No speakers");

                        self.inner.last_tick_was_empty.store(true, Ordering::SeqCst);
                    }
                    if speaking != 0 {
                        self.inner
                            .last_tick_was_empty
                            .store(false, Ordering::SeqCst);

                        println!("Voice tick ({speaking}/{total_participants} live):");

                        for (ssrc, data) in &tick.speaking {
                            let user_id_str = if let Some(id) = self.inner.known_ssrcs.get(ssrc) {
                                format!("{:?}", *id)
                            } else {
                                "?".into()
                            };

                            if let Some(decoded_voice) = data.decoded_voice.as_ref() {
                                let voice_len = decoded_voice.len();
                                let audio_str = format!(
                                    "first samples from {}: {:?}",
                                    voice_len,
                                    &decoded_voice[..voice_len.min(5)]
                                );

                                if let Some(packet) = &data.packet {
                                    let rtp = packet.rtp();
                                    println!(
                                        "\t{ssrc}/{user_id_str}: packet seq {} ts {} {:?} -- {audio_str}",
                                        rtp.get_sequence().0,
                                        rtp.get_timestamp().0,
                                        rtp.get_payload_type()
                                    );

                                    for i in 0..samples.len() {
                                        samples[i] += decoded_voice[i];
                                    }
                                } else {
                                    println!(
                                        "\t{ssrc}/{user_id_str}: Missed packet -- {audio_str}"
                                    );
                                }
                            } else {
                                println!("\t{ssrc}/{user_id_str}: Decode disabled.");
                            }
                        }
                    }
                }

                self.inner.appsrc.push_buffer(buffer).unwrap();
            }
            Ctx::RtpPacket(packet) => {
                let rtp = packet.rtp();
                println!(
                    "Received voice packet from SSRC {}, sequence {}, timestamp {} -- {}B long",
                    rtp.get_ssrc(),
                    rtp.get_sequence().0,
                    rtp.get_timestamp().0,
                    rtp.payload().len()
                );
            }
            Ctx::RtcpPacket(data) => {}
            Ctx::ClientDisconnect(ClientDisconnect { user_id, .. }) => {
                println!("Client disconnected: user {:?}", user_id);
            }
            Ctx::DriverDisconnect(DisconnectData { .. }) => {
                self.inner.pipeline.set_state(State::Null).unwrap();
            }
            _ => {
                println!("unimplemented");
            }
        }
        None
    }
}

#[tokio::main]
async fn main() {
    gst::init().unwrap();

    let token = {
        let config_raw = fs::read_to_string("config.json").expect("JSON Read Failed.");
        let config: Config = serde_json::from_str(&config_raw).unwrap();
        let token = config.discord_token.to_string();
        let mut config_box = GLOBAL_CONFIG.lock().unwrap();
        *config_box = Some(config);
        token
    };

    let intents: GatewayIntents =
        GatewayIntents::non_privileged() | GatewayIntents::MESSAGE_CONTENT;

    let songbird_config = songbird::Config::default().decode_mode(DecodeMode::Decode);

    let options = poise::FrameworkOptions {
        commands: vec![join(), leave()],
        ..Default::default()
    };

    let framework = poise::Framework::builder()
        .options(options)
        .setup(move |ctx, _ready, framework| {
            Box::pin(async move {
                println!("Logged in as {}", _ready.user.name);

                let commands =
                    poise::builtins::create_application_commands(&framework.options().commands);

                serenity::model::application::Command::set_global_commands(ctx, commands).await?;

                Ok(PoiseData {})
            })
        })
        .build();

    let mut client = Client::builder(token, intents)
        //.event_handler(Handler)
        .framework(framework)
        .register_songbird_from_config(songbird_config)
        .await
        .expect("Error creating client");

    // start listening for events by starting a single shard
    if let Err(why) = client.start().await {
        println!("An error occurred while running the client: {:?}", why);
    }
}

#[poise::command(slash_command)]
async fn join(ctx: PoiseContext<'_>) -> Result<(), PoiseError> {
    let (guild_id, connect_to) = {
        let user_id = ctx.author().id;

        let (guild_id, voice_state) = {
            let Some(guild) = ctx.guild() else {
                ctx.reply("❌ Failed to guild.").await?;
                return Ok(());
            };

            let voice_channel = guild.voice_states.get(&user_id).cloned();

            let guild_id = (&(guild.id)).clone();
            (guild_id, voice_channel)
        };

        let Some(voice_state) = voice_state else {
            ctx.reply("❌ Failed to get your voice state.").await?;
            return Ok(());
        };

        let Some(connect_to) = voice_state.channel_id else {
            ctx.reply("❌ Failed to get your voice channel.").await?;
            return Ok(());
        };
        (guild_id, connect_to)
    };

    let rtmp_url = {
        let config = {
            if let Ok(config_box) = GLOBAL_CONFIG.lock() {
                if let Some(config) = config_box.as_ref() {
                    Some(config.rtmp_url.clone())
                } else {
                    None
                }
            } else {
                None
            }
        };
        let Some(rtmp_url) = config else {
            ctx.reply("❌ Initialization failed. (config load)")
                .await?;
            return Ok(());
        };
        rtmp_url
    };

    let Some(manager) = songbird::get(ctx.serenity_context()).await else {
        ctx.reply("❌ Initialization failed. (retrieving songbird client)").await?;
        return Ok(());
    };

    let manager = manager.clone();

    if let Ok(handler_lock) = manager.join(guild_id, connect_to).await {
        let mut handler = handler_lock.lock().await;

        let evt_receiver = Receiver::new(rtmp_url.as_str());

        handler.add_global_event(CoreEvent::SpeakingStateUpdate.into(), evt_receiver.clone());
        handler.add_global_event(CoreEvent::RtpPacket.into(), evt_receiver.clone());
        handler.add_global_event(CoreEvent::RtcpPacket.into(), evt_receiver.clone());
        handler.add_global_event(CoreEvent::ClientDisconnect.into(), evt_receiver.clone());
        handler.add_global_event(CoreEvent::DriverDisconnect.into(), evt_receiver.clone());
        handler.add_global_event(CoreEvent::VoiceTick.into(), evt_receiver);

        ctx.reply("🔴 Your voice is now ON AIR!").await?;
    } else {
        ctx.reply("❌ Failed to join the voice channel.").await?;
        return Ok(());
    }

    Ok(())
}

#[poise::command(slash_command)]
async fn leave(ctx: PoiseContext<'_>) -> Result<(), PoiseError> {
    let guild_id = {
        let Some(guild) = ctx.guild() else {
            ctx.reply("❌ Failed to guild.").await?;
            return Ok(());
        };

        guild.id
    };
    let Some(manager) = songbird::get(ctx.serenity_context()).await else {
        ctx.reply("❌ Cordtap is not in a voice channel.").await?;
        return Ok(());
    };

    if let Err(e) = manager.remove(guild_id).await {
        ctx.reply(format!("❌ Error: {:?}", e)).await?;
    };

    ctx.reply("✅ Left.").await?;

    Ok(())
}