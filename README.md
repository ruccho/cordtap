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

3. After entering RTMP url, Cordtap joins the voice channel you are in. Now your voice will be streamed! 

4. Post command to let Cordtap leave the voice channel and stop streaming:

```
/leave
```