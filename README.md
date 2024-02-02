# Cordtap

A discord bot that bypasses voice to RTMP streaming server such as YouTube Live.

## Deployment

1. Copy **TEMPLATE_config.json** as **config.json** and fill settings

```json
{
    "discord_token": "<YOUR DISCORD TOKEN HERE>",
    "rtmp_url": "<RTMP_URL>"
}
```

2. Build docker image

```shell
docker build -t TAG_NAME .
```

3. Run

```shell
docker run TAG_NAME
```

## Usage

1. Invite deployed Cordtap to your server

2. Post command to let Cordtap join the voice channel:

```
~join <VOICE_CHANNEL_ID>
```

> [!NOTE]
> `<VOICE_CHANNEL_ID>` can be copied from context menu of the voice channel.

3. Now your voice will be streamed onto RTMP server you specified in config.json

4. Post command to let Cordtap leave the voice channel and stop streaming:

```
~leave
```