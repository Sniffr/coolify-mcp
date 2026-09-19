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
