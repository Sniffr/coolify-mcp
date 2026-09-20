# Hosted multi-tenant Rust MCP deployment

## Production topology

- Public URL: `https://mcp.social.dpdns.org/mcp`
- Health URL: `https://mcp.social.dpdns.org/healthz`
- Caddy route: `mcp.social.dpdns.org` → `mcp:8080`
- Docker network: `brightbean-studio_default`
- Compose file: `deploy/multitenant-compose.yaml`
- Protected env file: `/etc/coolify-mcp/multitenant.env` (mode `0600`)

The service has no published host port and has no global Coolify URL or token. GitHub identifies users; users save their own encrypted Coolify connection in Settings. `/data` is persistent and private, and the container runs as `10001:10001`.

## Provision and start

```sh
sudo install -d -m 0700 /etc/coolify-mcp
sudo install -m 0600 deploy/multitenant.env.example /etc/coolify-mcp/multitenant.env
sudoedit /etc/coolify-mcp/multitenant.env

docker build -f Dockerfile.rust -t sniffr-coolify-mcp:multitenant .
docker compose -f deploy/multitenant-compose.yaml config --quiet
docker compose -f deploy/multitenant-compose.yaml up -d
./deploy/remote-check.sh
```

Set `MCP_CONNECTION_ENCRYPTION_KEY`, `GITHUB_CLIENT_ID`, `GITHUB_CLIENT_SECRET`, and the exact public/callback URLs in the protected file. Never put secrets in command arguments, Compose YAML, image layers, logs, or Git. The OAuth callback is `https://mcp.social.dpdns.org/auth/github/callback`.

Configure Caddy with:

```caddyfile
mcp.social.dpdns.org {
    reverse_proxy mcp:8080
}
```

Validate Caddy and reload it only after the container is healthy. Do not expose `8080` on the host.

## Client setup and operation

Configure Claude or OpenCode with the URL only, `https://mcp.social.dpdns.org/mcp`. Complete GitHub browser login, open `/settings`, and enter the user's own Coolify URL/token. The default capability profile is `read-only`; writes/deploys/deletes retain confirmation safeguards. Rotate by saving a new token, and delete a connection in Settings to remove its ciphertext and revoke grants.

For full OAuth app, client, lifecycle, troubleshooting, and rollback instructions, see [`../docs/hosted-multitenant-setup.md`](../docs/hosted-multitenant-setup.md).

## Checks and rollback

`remote-check.sh` checks the routed health endpoint and OAuth discovery without sending credentials. It rejects credential-like response content. For a failed rollout, stop the new Compose service and restore the previous image and Caddyfile backup; preserve `/data` and do not revoke credentials.

## Continuous delivery (GitHub Actions)

`.github/workflows/hosted-deploy.yml` runs on every push to `main` (and manually via
`workflow_dispatch`): `cargo fmt` + `cargo test` + `cargo clippy`, then a cached
`linux/amd64` image build pushed to
`ghcr.io/<owner>/coolify-mcp:multitenant` (plus a short-sha tag), then an SSH deploy
that pulls, retags to `sniffr-coolify-mcp:multitenant`, recreates Compose, and runs
`remote-check.sh`. No secrets ever enter image layers or logs.

Required repository secrets (Settings → Secrets and variables → Actions):

| Secret | Value |
| --- | --- |
| `SSH_HOST` | `77.90.40.213` |
| `SSH_USER` | `sidney` |
| `SSH_PRIVATE_KEY` | Private key whose public half is in the remote `authorized_keys` |
| `SSH_PORT` | Optional, defaults to `22` |

Optional repository variables: `REMOTE_APP_DIR` (default `/home/sidney/coolify-mcp`),
`REMOTE_ENV_FILE` (default `/home/sidney/coolify-mcp/secrets/multitenant.env`),
`BASE_URL` (default `https://mcp.social.dpdns.org`). Until the SSH secrets exist, the
`deploy` job skips gracefully and CI still verifies and publishes the image; keep using
the manual `docker save` flow below in that window.

## Local fallback

Keep `compose.rust.yaml` for local fixture behavior only. Local clients can use Rust stdio or `python3 coolify_mcp_server.py` with local `COOLIFY_URL`/`COOLIFY_TOKEN` environment variables. These local credentials are never part of hosted Compose.
