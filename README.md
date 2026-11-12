# Discord Voice CLI

Currently alpha. Works mostly, needs polish.

A headless client for Discord voice chat, using Serenity for the Discord gateway,
Songbird for voice, and optional CPAL microphone capture and speaker playback.
By default it joins silently with virtual audio and self-mute/self-deafen enabled,
so a VPS needs no microphone, speakers, audio server, or OS virtual audio drivers.
This repository uses a locally patched Serenity to authenticate with a user token.

Requires Rust 1.89 or newer, a C/C++ compiler, and CMake to build the bundled Opus
codec. On Windows, install Visual Studio Build Tools with the C++ workload and
CMake, or use a Rust GNU toolchain with MinGW and CMake. On Linux, install the ALSA
development headers, OpenSSL development headers, and pkg-config (for example,
`libasound2-dev`, `libssl-dev`, and `pkg-config`
on Debian/Ubuntu); an installed `libopus-dev` can be used instead of building Opus.
TLS uses the platform's native backend.

Build with `cargo build --locked` and run with:

```sh
cargo run --locked -- --token <USER_TOKEN> --guild_id <GUILD_ID> --channel_id <CHANNEL_ID>
```

The same settings can be supplied through `DISCORD_TOKEN`, `DISCORD_GUILD`, and
`DISCORD_CHANNEL`. To configure them in a file, copy `.env.example` to `.env` in
the repository root, fill in the token and IDs, then run `cargo run --locked`.
The app automatically loads the nearest `.env` in the working directory or a
parent directory. A missing `.env` is fine; an unreadable or malformed file
produces an error without printing its contents.

Configuration precedence is command-line arguments, then existing shell
environment variables, then `.env` values. Local `.env` files are ignored by Git.
`--guild-id` and `--channel-id` are also accepted. Supply the raw user token,
without a `Bot ` or `Bearer ` prefix. The account must already belong to the guild
and have permission to connect to the selected voice channel. Sending microphone
audio in device mode also requires permission to speak.

For a VPS, keep these settings in `.env` (they are also the defaults when omitted):

```dotenv
DISCORD_AUDIO_MODE=virtual
DISCORD_SELF_MUTE=true
DISCORD_SELF_DEAFEN=true
```

| Environment variable | CLI option | Default | Behavior |
| --- | --- | --- | --- |
| `DISCORD_AUDIO_MODE` | `--audio-mode virtual\|device` | `virtual` | Choose virtual silent/discard audio or the system's default audio devices. |
| `DISCORD_SELF_MUTE` | `--self-mute true\|false` | `true` | Set the account's self-mute state in the voice channel. |
| `DISCORD_SELF_DEAFEN` | `--self-deafen true\|false` | `true` | Set the account's self-deafen state in the voice channel. |

These options use the same CLI > shell environment > `.env` precedence as the
token and IDs. Boolean values must be `true` or `false`. Mute and deafen are
independent settings, applied before the initial voice-channel join through
Discord's [voice-state update](https://docs.discord.com/developers/events/gateway-events#update-voice-state).
They control this account's self state, not moderator-enforced server mute/deafen.
Restart the process to apply configuration changes.

Virtual mode provides a silent microphone and discarded output within the app:
it opens no local audio devices, creates no transmitting audio track, and skips
decoding received audio. The voice connection still runs normally. Setting
mute/deafen to `false` changes those Discord flags but does not enable sound in
virtual mode. No sound or artificial speech is sent to keep the connection open.
The native build dependencies listed above are still required to compile CPAL.

For local microphone/speaker use, set `DISCORD_AUDIO_MODE=device`,
`DISCORD_SELF_MUTE=false`, and `DISCORD_SELF_DEAFEN=false`. Device mode uses the
default input and output devices, which must support mono or stereo PCM at
48 kHz. The microphone is opened only when self-mute is off, and playback is
opened only when self-deafen is off. An active microphone transmits continuously
without push-to-talk or speech detection. Requested but unavailable audio devices
produce an error; virtual mode does not probe them.

Press Enter or Ctrl+C to disconnect. EOF on stdin is ignored, so the process can
run with stdin redirected from `/dev/null` or under a service manager. Use a
service manager's restart policy to recover from process failures. Continuous
channel uptime still depends on Discord, permissions, and network availability.

The Cargo patch in `Cargo.toml` selects `vendor/serenity` for both this program and
Songbird. Its `user_token` feature preserves raw HTTP authorization, sends a
user-account gateway handshake, and accepts READY data without a bot application.
See [the patch notes](vendor/serenity/PATCHES.md) for the exact changes and test
commands. These paths are tested locally; live user-account login and voice
connectivity still need verification against Discord.

Run `cargo test --locked` and `cargo clippy --all-targets --locked -- -D warnings`
to validate changes. `cargo update` refreshes the lockfile within the dependency
ranges in `Cargo.toml`.

The dependency audit on 2026-11-11 reports 10 RustSec vulnerability findings in
transitive dependencies, including optional dependencies in the lockfile.
Songbird's upstream version constraints currently prevent compatible fixes for
the affected `crypto-bigint`, `libcrux`, and `ringbuf` packages. The native TLS
backend avoids the older Rustls/WebPKI dependency chain and its advisories.
