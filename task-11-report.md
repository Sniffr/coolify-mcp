# Task 11 report

## Status

**Blocked before deployment. No remote state was changed.** The required read-only discovery was completed against `sidney@77.90.40.213` using `ssh -o BatchMode=yes` and no secrets.

## Read-only discovery

Command:

```text
ssh -o BatchMode=yes sidney@77.90.40.213 'uname -a; docker --version; docker compose version; getent hosts mcp.social.dpdns.org'
```

Verified results:

- Host: `Linux talos 7.0.0-30-generic`, x86_64.
- Docker: `29.7.2`.
- Docker Compose: `v5.5.0`.
- DNS: `mcp.social.dpdns.org` resolves to `77.90.40.213`.
- The SSH account can run Docker and is in the `docker` group.
- Existing proxy: `brightbean-studio-caddy-1` (`caddy:2-alpine`) owns ports 80 and 443.
- Verified Caddyfile route: `${APP_DOMAIN:localhost}` to `app:8000`; no `mcp.social.dpdns.org` route.
- `docker secret ls` and `docker config ls` report that the node is not a Swarm manager; no alternative approved secret injection mechanism was verified.
- `curl --fail --silent --show-error https://mcp.social.dpdns.org/healthz` failed with TLS alert `internal error`.

## Step outcomes and blockers

- **Step 2 (transfer/build): not attempted.** No image or build artifact was transferred, and no credentials were requested, read, printed, or transmitted.
- **Step 3 (start/configure): not attempted.** No container, persistent volume, proxy route, or host configuration was changed.
- **Step 4 (public health/OAuth discovery): attempted and blocked.** The public health request reached TLS/route validation but failed with `tlsv1 alert internal error`; because the target Caddy route is absent, the health and OAuth discovery responses could not be validated.
- **Step 5 (real OAuth/MCP smoke): not attempted.** It was correctly withheld because public HTTPS and secret injection prerequisites were unavailable.

The target proxy/HTTPS route is not configured, and a host secret injection mechanism for the Coolify URL/token and OAuth key material is unavailable or unverified. No remote mutations were performed.

The concrete public host used throughout is `mcp.social.dpdns.org` under wildcard `*.social.dpdns.org`.

## Files

- Added `deploy/README.md` with the blocked status, intended non-root/persistent runtime settings, prerequisites, activation checklist, rollback, and Python stdio fallback.
- Added executable `deploy/remote-check.sh` for health and OAuth discovery checks. It has the concrete public host fixed in the script and rejects credential-like response content.
- `HOSTING.md` was not modified because the final host-specific procedure cannot be verified until deployment succeeds. Its existing Rust deployment section contains the verified target host and safety guidance.

## Verification

- `ssh -o BatchMode=yes ...` discovery: **passed**; no secrets output.
- `curl https://mcp.social.dpdns.org/healthz`: **blocked**, TLS `internal error` as described above.
- `./deploy/remote-check.sh`: **blocked**, exits on the same public TLS failure; no OAuth checks were run.
- `cargo fmt --all -- --check`: **passed**.
- `cargo clippy --workspace --all-targets -- -D warnings`: **passed**.
- `cargo test --workspace`: **passed** (all workspace suites green).
- `git diff --check`: **passed**.
- `./deploy/remote-check.sh`: **blocked** with exit 1 because the public TLS handshake returns `tlsv1 alert internal error`; no OAuth endpoint checks ran.
- `./scripts/acceptance-rust.sh`: **passed** (`Rust local acceptance passed`).
- Local Rust build/test acceptance was also previously recorded in `task-10-report.md`; Task 11 did not alter Rust source.
