# Task 10 report

Implemented the Rust production-container and local acceptance harness.

## Changes

- Added `Dockerfile.rust`: multi-stage release build, CA certificates, `/data`, UID/GID `10001`, non-root runtime, no source/env/Git metadata copied.
- Added `compose.rust.yaml` with HTTP defaults, persistent OAuth volume, non-root user, and local fixture-token defaults.
- Added `.dockerignore` excluding credentials, Git metadata, build output, temporary files, and Python cache files.
- Added `scripts/smoke-rust.sh`; accepts a URL only, never prints or sends credentials, checks `/healthz` and unauthenticated MCP rejection.
- Added deterministic `fake-coolify` fixture binary with nested secret values, poisoned logs, bearer enforcement, and request/unauthorized counters.
- Added `stdio-interop` client covering NDJSON and Content-Length framing.
- Added `http-oauth-interop` client covering registration, PKCE authorization-code exchange, MCP initialization, and `tools/list`.
- Added the interop binaries to `Cargo.toml` and required test dependencies.
- Added explicit `MCP_ALLOW_INSECURE_HTTP=true` URL handling for local acceptance only.
- Fixed HTTP server timer configuration so configured header timeouts do not panic under Docker.

## Verification

- `cargo fmt --all` — passed.
- `cargo test --workspace` — passed.
- `cargo clippy --workspace --all-targets -- -D warnings` — passed.
- `docker build -f Dockerfile.rust -t sniffr-coolify-mcp:acceptance .` — passed on Docker Desktop/macOS.
- Container smoke run with only `fixture-token` and a temporary `/data` mount — passed (`Rust container smoke test passed`).

## Concerns

- The HTTP interop client is an executable acceptance harness and expects an already-running server; it does not spawn the fake Coolify fixture itself.
- The fake fixture is intentionally local-only and uses the documented fixture token literal; it does not contain or emit real credentials.
