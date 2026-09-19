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

## Fix round 1 (review findings)

- Added `scripts/acceptance-rust.sh`: single local orchestrator that builds all binaries, starts the deterministic fake Coolify fixture and the Rust HTTP server on free loopback ports, waits for readiness (fails loudly if either never becomes ready), runs OAuth PKCE discovery → registration → signed-state → authorize → invalid-grant rejection → token exchange, then MCP `initialize`, `tools/list` (asserts exactly 45 tools), `get_version`, `list_applications`, and `application_logs`, then runs the stdio interop over both framing modes, then asserts fixture stats show `unauthorized == 0` with a tight `9..=12` request bound (3 HTTP + 6 stdio calls) proving the safe calls occurred with no unauthorized traffic and no compatibility-fallback retries. Cleans up both processes via `EXIT/INT/TERM` trap even on failure. Uses `fixture-token` only.
- Fixture `__fixture__/stats` no longer counts its own probes, so the request tally reflects only real Coolify API traffic; added percent-decoding so encoded UUID paths match; added `/api/v1/applications/app-1/logs` fixture route with nested-secret inventory and poisoned-log payloads.
- HTTP interop now parses JSON-RPC (ID/result/error assertions), asserts the exact 45-tool roster, safe inventory result with secret masking, untrusted-log framing (`UNTRUSTED` boundary + original injection text), OAuth discovery issuer, signed-state round-trip, invalid-grant rejection, session binding, and no `fixture-token` leakage in any response.
- Stdio interop now parses JSON-RPC responses via `transport::stdio::decode_frames`, verifies exact 45 roster, request IDs/results, child exit status, both NDJSON and Content-Length framing modes, safe inventory masking, and untrusted-log framing. Keeps stdout protocol-only (fixture notice goes to stderr).
- `compose.rust.yaml` healthcheck is now a real `GET /healthz` on port 8080 (`curl --fail ... http://127.0.0.1:8080/healthz`) while retaining the `/data` writeability gate; `Dockerfile.rust` runtime installs `curl` so the check works; smoke script checks `/healthz` plus unauthenticated MCP rejection.
- Fixed OAuth loopback acceptance: `canonical_resource` allows plain-HTTP loopback resources, added `/oauth/state` endpoint for signed-state issuance, and fixed double-slash `…//mcp` resource construction via `public_base`/`mcp_resource_url` helpers used consistently by provider init, discovery, bearer verification, and the interop clients.

## Fix round 1 verification

- `cargo fmt --all` — passed.
- `cargo test --workspace` — passed (all suites green).
- `cargo clippy --workspace --all-targets -- -D warnings` — passed (fixed `manual_async_fn` in interop client).
- `./scripts/acceptance-rust.sh` — passed (`Rust local acceptance passed`, fixture stats `{"requests":9,"unauthorized":0}`).
- `docker build -f Dockerfile.rust -t sniffr-coolify-mcp:acceptance .` — passed.
- Container smoke run — passed (`Rust container smoke test passed`); image verified `uid=10001`, `/data drwx------`, `curl` present, no secrets in image env.
- `docker compose -f compose.rust.yaml config` — validates with the real `/healthz` healthcheck.

## Concerns

- The HTTP interop client is an executable acceptance harness and expects an already-running server; it does not spawn the fake Coolify fixture itself.
- The fake fixture is intentionally local-only and uses the documented fixture token literal; it does not contain or emit real credentials.
