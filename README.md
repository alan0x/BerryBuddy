# BerryBuddy

BerryBuddy is a small realtime AI voice companion for Raspberry Pi Zero 2 W.

It connects a USB microphone and speaker to Ark realtime dialogue, streams microphone audio to Ark, and plays Ark TTS audio back through the speaker.

## Run On Raspberry Pi

Copy the whole project directory to the Pi:

```bash
scp -r BerryBuddy pi@berrybuddy.local:~/BerryBuddy
ssh pi@berrybuddy.local
cd ~/BerryBuddy
```

Create config:

```bash
cp .env.example .env
nano .env
```

Set at least:

```bash
ARK_APP_ID=your-app-id
ARK_ACCESS_KEY=your-access-key
```

Then run:

```bash
cargo run --release
```

## Audio Devices

To list available devices:

```bash
BERRYBUDDY_LIST_DEVICES=true cargo run
```

If the Pi does not pick the USB microphone or speaker by default, set a case-insensitive substring:

```bash
BERRYBUDDY_INPUT_DEVICE=USB
BERRYBUDDY_OUTPUT_DEVICE=USB
```

## Main Settings

- `BERRYBUDDY_MODEL`: Ark realtime model version, default `1.2.1.1`
- `BERRYBUDDY_SPEAKER`: Ark voice, default `zh_female_vv_jupiter_bigtts`
- `BERRYBUDDY_ASR_FORMAT`: Ark ASR audio format field, default `pcm`
- `BERRYBUDDY_SYSTEM_ROLE`: assistant persona
- `BERRYBUDDY_SPEAKING_STYLE`: response style
- `BERRYBUDDY_INPUT_MODE`: optional Ark input mode. Leave unset for normal always-on microphone streaming.

BerryBuddy sends microphone audio to Ark as mono little-endian i16 PCM at 16 kHz and requests TTS audio as mono `pcm_s16le` at 24 kHz. Local device sample rates are resampled automatically.
