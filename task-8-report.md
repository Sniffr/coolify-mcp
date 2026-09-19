# Task 8 report — stdio and Streamable HTTP transports

## Status
Implemented and committed as requested.

## Changes
- Added `crates/transport/src/stdio.rs` with NDJSON and Content-Length framing, first-frame mode selection, parse-error JSON-RPC responses, notification suppression, EOF handling, explicit flushes, and stderr diagnostics helper.
- Added `crates/transport/src/http.rs` with HTTPS public URL normalization, HTTP configuration, body limits, and server startup.
- Added `crates/transport/src/http_app.rs` with health and OAuth discovery/resource endpoints, dynamic registration, authorization-code/PKCE authorization, token/refresh routes, bearer-protected `/mcp`, JSON-RPC initialize/tools/list/tools/call handling, notification handling, and MCP session response headers.
- Added focused stdio and HTTP integration tests written before implementation.
- Added application startup dispatch in `src/main.rs`: stdio default, HTTP selected by `MCP_TRANSPORT=http`, read-only HTTP profile, public URL validation, bind configuration, and OAuth provider setup.
- Made OAuth token responses serializable and `ToolContext` clonable for the transport application adapter.

## Verification
- `cargo test -p transport` — passed (7 transport tests).
- `cargo test --workspace` — passed.
- `cargo clippy --workspace --all-targets -- -D warnings` — passed.
- `cargo fmt --all` — completed.

## Concerns / follow-up
- The current HTTP adapter uses Axum body limits and request-level application handling, but does not yet install a dedicated 15-second header timeout or explicit registration/authorization/token rate limiter. These should be added before production exposure.
- OAuth endpoints are exposed under `/oauth/*` alongside discovery metadata; aliases to any deployment-specific standard endpoint paths may be desirable for client interoperability.
- MCP session IDs are currently generated per successful response rather than persisted in a session store; full resumable Streamable HTTP session lifecycle should be strengthened in a subsequent task.
- Startup currently requires Coolify configuration for both modes, consistent with the existing API configuration contract.

## Fix round 1 — 2026-09-19

Addressed review findings:

- Reworked stdio processing to consume frames incrementally from `BufReader`, preserving the first-frame framing mode, responding and flushing before the input stream closes, and returning `-32700 Parse error` for malformed JSON. Added a duplex-stream end-to-end test.
- Added actual Axum request and request-body timeout layers using the configured 30-second timeout, while retaining the 5 MiB body limit.
- Added bounded per-IP rate limiting for registration, authorization, and token endpoints with safe `429`/`Retry-After` responses.
- Added MCP content negotiation validation, bearer-protected GET behavior, session ID validation, and an in-memory session store so IDs remain stable across requests rather than being minted on every response.
- Wired `MCP_OAUTH_STATE_FILE` (default `/data/oauth-state.json`) through `OAuthProvider::with_store`; persistence failures produce degraded `/healthz` while discovery remains public. OAuth mutations persist through the provider store.
- Added degraded-health and rate-limit tests; all prior transport tests remain passing.

Fix-round verification:

- `cargo test -p transport` — passed (10 transport tests).
- `cargo test --workspace` — passed.
- `cargo clippy --workspace --all-targets -- -D warnings` — passed.
- `cargo fmt --all` — completed.

## Fix round 2 — 2026-09-19

Addressed remaining lifecycle and security findings:

- Replaced the prior Axum-only listener with a Hyper-util connection loop. Each HTTP/1 connection now applies Hyper's real `header_read_timeout` (15 seconds), distinct from the 30-second request/body timeout layers.
- Added timeout configuration assertions covering both deadlines.
- Changed MCP sessions to bounded `HashMap<session_id, last_seen>`, with UUID validation, TTL cleanup, configurable capacity, last-seen refresh, safe capacity rejection, and authenticated `DELETE /mcp` termination.
- Rate limiting now uses the actual peer address inserted by the connection service. Forwarded IP headers are ignored unless `MCP_TRUSTED_PROXY=true`; spoofed forwarded addresses cannot evade limits.
- Added shared persistence health state. OAuth persistence failures transition `/healthz` to degraded, and provider flushing is performed during graceful shutdown after SIGINT/CTRL-C.
- OAuth state remains provider-managed and hash-only; startup continues to validate/load the configured state path.

Fix-round 2 verification:

- `cargo test -p transport` — passed (11 transport tests).
- `cargo test --workspace` — passed.
- `cargo clippy --workspace --all-targets -- -D warnings` — passed.
- `cargo fmt --all` — completed.

## Fix round 3 — 2026-09-19

Addressed the remaining transport findings:

- HTTP handlers now extract `ConnectInfo<SocketAddr>` directly. The production connection service supplies the accepted peer address, while the test-only `router` helper supplies a deterministic local peer. Rate-limit keys use the complete accepted `SocketAddr`; forwarded headers remain ignored unless trusted-proxy mode is explicitly enabled. Added a server-style test with two independent peer buckets and spoofed forwarded headers.
- HTTP/2 is explicitly disabled (`HTTP2_SUPPORTED == false`) in the Hyper connection builder. The 15-second Hyper HTTP/1 header-read deadline therefore applies to every supported protocol; the configuration test asserts the enforcement choice.
- Added SIGTERM handling on Unix alongside Ctrl-C, stopped accepting connections on shutdown, tracked connection tasks in `JoinSet`, drained them for up to the bounded 30-second `DRAIN_TIMEOUT`, and aborted any remaining tasks before flushing OAuth state.

Fix-round 3 verification:

- `cargo test -p transport` — passed (12 transport tests).
- `cargo test --workspace` — passed.
- `cargo clippy --workspace --all-targets -- -D warnings` — passed.
- `cargo fmt --all` — completed.

## Fix round 4 — 2026-09-19

Corrected rate-limit keying to use the actual peer IP from `ConnectInfo<SocketAddr>.ip()`, avoiding separate buckets for ephemeral source ports while retaining accepted-peer extraction and trusted-proxy opt-in handling. Added a regression test proving two source ports from `127.0.0.1` share a bucket, a distinct IP receives an independent bucket, and spoofed forwarding headers do not affect untrusted keying.

Fix-round 4 verification:

- Focused peer-IP regression test — passed.
- `cargo test -p transport` — passed.
- `cargo test --workspace` — passed.
- `cargo clippy --workspace --all-targets -- -D warnings` — passed.
- `cargo fmt --all` — completed.
