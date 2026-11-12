# Local user-token compatibility patch

Base: Serenity 0.12.5 from crates.io, upstream commit
`1809beb0fc24f3942c500058ad4fa47e6a97d3f9`. The upstream ISC license is retained in
`LICENSE.md`. Registry cache markers, the upstream lockfile, and examples are not
vendored.

A pre-existing trailing space in a doc comment in `src/utils/message_builder.rs`
is removed so the complete staged snapshot passes `git diff --check`.
The token-validation documentation uses `example.account.token` in place of an
upstream credential-shaped example flagged by GitHub push protection. Its
three-part format-validation doctest is retained.

The root `[patch.crates-io]` entry makes both the CLI and Songbird use this copy.
The CLI explicitly enables the local `user_token` feature. Without that feature,
the original bot authentication and gateway payloads are preserved.

Changes enabled by `user_token`:

- HTTP constructors and token setters trim the token without adding `Bot `.
- Gateway IDENTIFY omits bot intents, shard information, and the initial presence
  update. Client properties continue to identify the library as Serenity.
- Gateway IDENTIFY and RESUME use the same raw token as HTTP authorization.
- READY accepts an absent/null bot application and an omitted resume URL. The
  ordinary gateway URL is the fallback; no application ID is fabricated.
- Autosharding uses a single connection without requesting `/gateway/bot`.
  Explicit multi-shard startup is rejected for user sessions.

The patch leaves voice state/server events and Songbird's voice implementation
intact. It targets guild voice connections; compatibility with other user-account
features is outside the patch's scope. Live Discord login and audio are not
covered by the local tests.

Validation from the repository root:

```sh
cargo test --all-targets --locked
cargo test --manifest-path vendor/serenity/Cargo.toml --lib --no-default-features --features client,gateway,model,native_tls_backend,user_token,voice --target-dir target
cargo test --manifest-path vendor/serenity/Cargo.toml --lib --no-default-features --features client,gateway,model,native_tls_backend,voice --target-dir target
cargo clippy --all-targets --locked -- -D warnings
```

The standalone invocations test the patched crate with `user_token` enabled and
disabled. Its separate lockfile is ignored by Git. Format application code with
`cargo fmt -- --check`; vendored source retains upstream formatting.

When updating Serenity, reapply these source changes, retain its license, and run
both dependency test configurations as well as the application's tests.
