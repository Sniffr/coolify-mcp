# Task 9 report

## Scope

Implemented side-effect-free bounded doctor diagnostics, JSON/human reports, CLI dispatch for `coolify-mcp doctor [--json] [--header "Key: Value"]`, and operator documentation for the Rust runtime and hosted deployment.

## Doctor behavior

- Collects configuration errors without echoing configured values.
- Checks URL/token presence, unresolved `${...}` placeholders, duplicate `/api/v1`, hosted HTTPS, and capability profile.
- Performs only GET probes for version, MCP routing, and application access.
- Bounds each probe to ten seconds.
- Distinguishes unreachable service, unauthorized token, proxy/Cloudflare redirect, HTML catch-all, unsupported version, and inconclusive probes.
- Reports named checks with status, detail, and one-line remediation guidance.
- Exit status is nonzero for failure or inconclusive checks; JSON output is secret-free.
- Header option is parsed into the Coolify client's custom headers while protected auth/content headers remain controlled by the API client.

## Documentation

Updated README, HOSTING, and `.env.example`; added `docs/rust-migration.md`. Documentation covers Rust stdio/HTTP, compatibility variable aliases, capability profiles, OAuth and `/data`, `mcp.social.dpdns.org`, health checks, rollback to Python, deployment safety, and the prohibition on placing real tokens in Git/chat/logs. No direct MIT code was ported, so no attribution change was necessary.

## Verification

- `cargo test -p doctor` — passed (4 tests).
- `cargo test --workspace` — passed.
- `cargo run -- doctor --json` with missing credentials — produced clean JSON and expected nonzero status.
- `cargo fmt --all -- --check` — passed.
- `cargo clippy --workspace --all-targets -- -D warnings` — passed.
- `git diff --check` — passed.

No deployment was performed.

## Hostname correction

The hosted endpoint was corrected to the concrete `mcp.social.dpdns.org` host under the wildcard DNS zone `*.social.dpdns.org`. README, HOSTING, `.env.example`, migration documentation, and the approved plan/spec now use the corrected hostname; no product behavior changed.

## Fix round 1

Addressed review findings: routing diagnostics now probe a deliberately invalid Coolify API route and classify structured JSON errors versus HTML/Cloudflare redirects/catch-alls using status, body, and content type; `/mcp` is never probed. Deployment ability is inferred only from explicit capability profile/permission configuration and never triggers a mutation. Added deterministic fixtures for unreachable services, invalid tokens, redirects, unsupported versions, missing deployment capability, routing shapes, transport defaults, and token-file configuration. All probes remain bounded to ten seconds, and token-file paths/contents are not emitted.

## Fix round 2

Addressed remaining issues: added `CoolifyClient::probe_get`, a side-effect-free GET probe with automatic redirects disabled (10s timeout, bounded token-redacted body) that preserves real status, Content-Type, Location, redirect, and body metadata into `doctor::ProbeResponse`; the CLI adapter no longer synthesizes 200/application-json. `ProbeResponse` carries `location`, and classification rejects 3xx/Location responses plus HTML without an `<html>` marker (doctype/head/body/title/Cloudflare/challenge markers and text/html content type), including an HTML version body failing reachability. Removed `MCP_DEPLOY_PERMISSION`; deploy ability derives from the effective runtime capability (`MCP_READONLY` > `MCP_CAPABILITY_PROFILE` > transport default, stdio operations / otherwise read-only, matching the fixed `main.rs` profile resolution which now honors `MCP_CAPABILITY_PROFILE` and rejects invalid values), and passes only when Coolify is reachable. Added local-server integration tests proving redirect and markerless-HTML shapes are rejected end to end through real HTTP, plus mismatch tests (HTTP default fails, explicit operations passes, `MCP_READONLY` beats admin, removed escape hatch ignored, offline operations is inconclusive). Token-file, version, unreachable, invalid-token, profile, and `mcp.social.dpdns.org`/wildcard docs preserved; no deployment performed.

## Fix round 3

Made doctor `MCP_CAPABILITY_PROFILE` parsing case-insensitive to match the runtime exactly (`main.rs` compares case-insensitively): `OPERATIONS`/`Operations`, `ADMIN`/`Admin`, and `READ-ONLY`/`Read-Only` are now recognized, with deploy ability following the effective profile (operations/admin pass when reachable, read-only fails), while truly unknown values such as `superuser` still fail both checks. Added regression coverage for uppercase/mixed-case values. Token-file, version, unreachable, invalid-token, and `mcp.social.dpdns.org`/wildcard docs preserved; no deployment performed.

## Task 9 final verification (2026-09-20)

### Local gate

All commands exited zero:

- `cargo fmt --all -- --check` — passed.
- `cargo test --workspace` — passed; all workspace unit, integration, and doc-test groups reported zero failures.
- `cargo clippy --workspace --all-targets -- -D warnings` — passed with no diagnostics.
- `./scripts/acceptance-rust.sh` — passed; output: `two-user hosted acceptance passed`. The acceptance fixture exercised separate user A/B Coolify inventories and request routing, and its assertions covered secret-free output/audit/stats.
- `git diff --check` — passed.

### Image and Compose validation

- `docker build -f Dockerfile.rust -t sniffr-coolify-mcp:task9-verify .` — passed. Local image digest: `sha256:618505f681893836eeacf3d6ec3baca913ff3288259a4960cc58e8b9b35cfd78`.
- `docker image inspect ...` — confirmed runtime user `10001:10001`, entrypoint `sniffr-coolify-mcp`, and `/healthz` healthcheck.
- Container inspection — confirmed `/data` owner `10001:10001`, mode `700`; binary owner `root:root`, mode `755`.
- Binary scan — no fixture token, placeholder secret, or private-key marker found in the executable strings.
- `MCP_ENV_FILE=<temporary mode-0600 fixture file> docker compose -f deploy/multitenant-compose.yaml config --quiet` — passed. The same validation without the required external env file failed as expected (`env file /etc/coolify-mcp/multitenant.env not found`); no repository secret file was created.
- Direct local smoke container using fixture-only GitHub values and `MCP_TRANSPORT=http` — `/healthz` returned HTTP 200; container reached `running`; no fixture secret-like value appeared in logs. No real GitHub/Coolify credentials or deployment were used.

### Tracked-file secret scan

Scanned 142 tracked files using location-only regex reporting for private keys, GitHub tokens, common provider tokens, AWS access keys, Coolify token assignments, and GitHub client-secret assignments. Results: private keys 0; GitHub PATs/tokens 0; OpenAI-style tokens 0; Slack tokens 0; AWS access keys 0. The 11 assignment matches were documented placeholders in `.env.example`, `deploy/multitenant.env.example`, README/HOSTING/reference docs, the plan, or fixture-only values in the acceptance script; no real secret was found and no secret value is reproduced here. The image scan likewise found no secret-like value.

### Residual risks and deployment blockers

- Public deployment was not performed; HTTPS, OAuth discovery/callback, Caddy routing, persistence across restart, and remote `/healthz` remain unverified.
- Deployment requires the operator-provisioned mode-0600 env file, stable encryption key, GitHub OAuth credentials, external `brightbean-studio_default` network, persistent `/data`, and Caddy route `mcp.social.dpdns.org -> mcp:8080`. None were available or inspected remotely in this task.
- The local direct smoke test proves the image health endpoint with fixture configuration, not end-to-end GitHub/Coolify connectivity. Deployment remains blocked until Task 10 supplies and validates those remote prerequisites without exposing secrets.
