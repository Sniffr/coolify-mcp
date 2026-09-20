# Hosting guide

Choose the transport first. Local stdio is simplest; hosted HTTP is for remote MCP clients and adds GitHub identity plus per-user encrypted Coolify connections.

## Hosted multi-tenant HTTP (recommended for remote clients)

The production endpoint is `https://mcp.social.dpdns.org/mcp`. Follow [`docs/hosted-multitenant-setup.md`](docs/hosted-multitenant-setup.md) for the complete procedure. In summary:

1. Create a GitHub OAuth App with callback `https://mcp.social.dpdns.org/auth/github/callback` and identity-only scopes.
2. Put `GITHUB_CLIENT_ID`, `GITHUB_CLIENT_SECRET`, `GITHUB_CALLBACK_URL`, `MCP_PUBLIC_URL`, and a stable `MCP_CONNECTION_ENCRYPTION_KEY` in `/etc/coolify-mcp/multitenant.env` with mode `0600`.
3. Do **not** configure `COOLIFY_BASE_URL`, `COOLIFY_ACCESS_TOKEN`, `COOLIFY_URL`, or `COOLIFY_TOKEN` in hosted Compose. Users supply their own URL/token in the authenticated Settings page.
4. Run `deploy/multitenant-compose.yaml`. It uses UID/GID `10001`, a private persistent `/data` volume, the existing `brightbean-studio_default` Caddy network, and no published host port.
5. Configure Caddy as `mcp.social.dpdns.org { reverse_proxy mcp:8080 }` and validate `/healthz` before client login.
6. Configure Claude or OpenCode with the remote URL only: `https://mcp.social.dpdns.org/mcp`. Complete GitHub login in a browser, then save your Coolify connection at `/settings`.

```bash
# Claude Code CLI
claude mcp add --transport http coolify https://mcp.social.dpdns.org/mcp
claude mcp login coolify
claude mcp list   # want: coolify - ✔ Connected
claude mcp get coolify
```

End-user walkthrough with sample prompts, URL rules, and every Settings/Claude
error explained: [`docs/using-the-hosted-mcp.md`](../docs/using-the-hosted-mcp.md).

HTTP defaults to `read-only`; use `operations` or `admin` only after review. Existing confirmation checks still protect writes, deploys, and deletes. Rotate a connection by saving a replacement token in Settings. Delete it there to remove ciphertext and revoke the user's grants. Preserve `/data` and the encryption key during upgrades and rollback.

## Local stdio (no public endpoint)

The Python server is dependency-free and remains the fallback:

```bash
export COOLIFY_URL='https://coolify.example.com'
export COOLIFY_TOKEN='complete-token'
python3 coolify_mcp_server.py
```

Rust stdio is also available with the same local environment aliases:

```bash
cargo run --release
```

Use an OS secret manager or a mode-0600 environment file; never commit or paste tokens. Local stdio does not need GitHub OAuth, `/data`, a tenant database, or Caddy.

## Local Docker/Compose fixture

`compose.rust.yaml` is intentionally a local fixture and retains its localhost port, host-docker-internal Coolify URL, and fixture token defaults. Do not use it for production. The production file is `deploy/multitenant-compose.yaml`.

## Troubleshooting and rollback

- `503 /healthz`: check `/data` ownership/mode, database initialization, and the protected env file without printing its values.
- OAuth failures: callback URL, public HTTPS URL, and GitHub client pair must match exactly.
- Settings failures: the user's Coolify host must be reachable from the container and the user's token must allow the selected read-only validation.
- Caddy failures: both Caddy and `mcp` must be attached to `brightbean-studio_default`; use upstream `mcp:8080`, not a host port.

```bash
# operator checks (no secrets printed)
curl -fsS https://mcp.social.dpdns.org/healthz
curl -fsS https://mcp.social.dpdns.org/.well-known/oauth-authorization-server | head -c 400; echo
MCP_ENV_FILE=/home/sidney/coolify-mcp/secrets/multitenant.env BASE_URL=https://mcp.social.dpdns.org bash deploy/remote-check.sh
```

User-side error tables (Claude `mcp list` messages, Settings red banners) and
sample prompts live in [`docs/using-the-hosted-mcp.md`](../docs/using-the-hosted-mcp.md) — point users there instead of debugging blind.

Rollback by stopping the new service and restoring the previous image and Caddyfile backup. Keep `/data` and protected secrets intact; do not revoke credentials as part of rollback. See `deploy/remote-check.sh` for safe public health/discovery checks.

## Security and token lifecycle

Use a separate least-privilege Coolify token per user/integration. Prefer read-only. Users should revoke old Coolify tokens after rotation. Never log Authorization headers, raw tool arguments/responses, GitHub secrets, cookies, or environment values.
