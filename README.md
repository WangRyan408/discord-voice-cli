# Discord Voice CLI

Currently alpha. Works mostly, needs polish.

A headless client for Discord voice chat, using Serenity for the Discord gateway,
Songbird for voice, and CPAL for microphone capture and speaker playback. This
repository uses a locally patched Serenity to authenticate with a user token.

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
and have permission to connect and speak in the selected voice channel.

The Cargo patch in `Cargo.toml` selects `vendor/serenity` for both this program and
Songbird. Its `user_token` feature preserves raw HTTP authorization, sends a
user-account gateway handshake, and accepts READY data without a bot application.
See [the patch notes](vendor/serenity/PATCHES.md) for the exact changes and test
commands. These paths are tested locally; live user-account login and voice
connectivity still need verification against Discord.

Uses the default input and output devices, which must support mono or stereo PCM
at 48 kHz. Voice is always transmitted while connected. Press Enter or Ctrl+C to
disconnect.

Run `cargo test --locked` and `cargo clippy --all-targets --locked -- -D warnings`
to validate changes. `cargo update` refreshes the lockfile within the dependency
ranges in `Cargo.toml`.

The dependency audit on 2026-11-11 reports 10 RustSec vulnerability findings in
transitive dependencies, including optional dependencies in the lockfile.
Songbird's upstream version constraints currently prevent compatible fixes for
the affected `crypto-bigint`, `libcrux`, and `ringbuf` packages. The native TLS
backend avoids the older Rustls/WebPKI dependency chain and its advisories.
