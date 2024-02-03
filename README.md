# Cordtap

A discord bot that bypasses voice to RTMP streaming server such as YouTube Live.

## Deployment

1. Copy **TEMPLATE_config.json** as **config.json** and fill settings

```json
{
    "discord_token": "<YOUR DISCORD TOKEN HERE>"
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

1. Invite deployed Cordtap to your server.

2. Post the command below to let Cordtap show you menus to enter your RTMP url: 

```
/join
```

![image](https://github.com/ruccho/cordtap/assets/16096562/c951d559-77c4-411b-af3c-d890d04626ed)

3. Select **YouTube...** or **Custom RTMP URL...** and enter information.

![image](https://github.com/ruccho/cordtap/assets/16096562/5359b4a5-f5ea-4565-8430-423700462fe0)

4. Your voice is now streamed!

5. Post command to let Cordtap leave the voice channel and stop streaming:

```
/leave
```