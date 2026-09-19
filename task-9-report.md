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
