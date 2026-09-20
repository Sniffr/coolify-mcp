# Hosted multi-tenant setup

This deployment provides one authenticated MCP endpoint for multiple people. GitHub identifies each user; each user then supplies their own Coolify URL and personal API token in **Settings**. The service stores the token encrypted in its private `/data` volume and never uses a shared/global Coolify credential.

## 1. Create the GitHub OAuth app

Create a GitHub OAuth App under the operator account or organization. Use:

- **Homepage URL:** `https://mcp.social.dpdns.org`
- **Authorization callback URL:** `https://mcp.social.dpdns.org/auth/github/callback`
- Request identity-only scopes (`read:user` and, when needed, `user:email`); do not request repository or administration scopes.

Keep the client secret in the protected host environment file. Do not put it in Compose, the image, Git, shell arguments, or logs.

## 2. Prepare protected configuration

Copy the example and make it readable only by the deployment operator:

```sh
sudo install -d -m 0700 /etc/coolify-mcp
sudo install -m 0600 deploy/multitenant.env.example /etc/coolify-mcp/multitenant.env
sudoedit /etc/coolify-mcp/multitenant.env
```

Required values are:

- `MCP_PUBLIC_URL=https://mcp.social.dpdns.org`
- `MCP_CONNECTION_ENCRYPTION_KEY`: a high-entropy, stable key. Back it up securely; changing it without a migration makes existing encrypted connections unavailable.
- `MCP_DATABASE_PATH=/data/tenant.sqlite3` (the Compose file sets this safe path)
- `GITHUB_CLIENT_ID`
- `GITHUB_CLIENT_SECRET`
- `GITHUB_CALLBACK_URL=https://mcp.social.dpdns.org/auth/github/callback`

There must be **no** `COOLIFY_BASE_URL`, `COOLIFY_ACCESS_TOKEN`, `COOLIFY_URL`, or `COOLIFY_TOKEN` in the production environment. Those values belong to individual users in Settings, not the service operator.

## 3. Deploy privately behind Caddy

Build the reviewed image, ensure the existing external Docker network exists, and start the service:

```sh
docker build -f Dockerfile.rust -t sniffr-coolify-mcp:multitenant .
docker network inspect brightbean-studio_default >/dev/null
docker compose -f deploy/multitenant-compose.yaml up -d
```

The service joins `brightbean-studio_default` as `mcp`, listens on container port 8080, and publishes no host port. Caddy should contain exactly this upstream relationship:

```caddyfile
mcp.social.dpdns.org {
    reverse_proxy mcp:8080
}
```

Keep `/data` persistent and private. Confirm health through Caddy at `https://mcp.social.dpdns.org/healthz`; do not expose port 8080 directly.

## 4. Configure a client and log in

Use the URL only—do not paste a Coolify token into Claude or OpenCode configuration.

- **Claude:** add the remote MCP server URL `https://mcp.social.dpdns.org/mcp` using Claude's remote MCP/server settings. Open the authorization link when prompted, sign in with GitHub, and approve the identity-only request.
- **OpenCode:** add a remote MCP server whose URL is `https://mcp.social.dpdns.org/mcp` using the remote/URL-only MCP configuration. Start the OAuth login flow in the browser and approve GitHub.

After login, open `https://mcp.social.dpdns.org/settings` in the same browser session. Enter your own Coolify base URL and API token, choose a capability profile, and save. The form validates the connection and returns only safe metadata (hostname, configured status, profile, and validation time); it never displays the token.

The default profile is **read-only**. Keep it for inventory and diagnostics. `operations` and `admin` expand capabilities and should be selected only when justified; write, deploy, and delete tools still require the existing confirmation safeguards.

## Rotation and deletion

To rotate a token, create/revoke the replacement in Coolify, then save the new token in Settings. The old ciphertext is replaced transactionally. To disconnect, use **Delete connection** in Settings; this removes the encrypted connection and revokes that user's MCP grants. Re-authenticate and save a new connection to use the service again.

If the encryption key must be rotated, stop the service, make a tested encrypted-store migration/backup with the old key, then replace the key and restart. Never delete `/data` as a shortcut: it destroys all tenant connections and OAuth state.

## Troubleshooting and rollback

- `503 /healthz`: inspect container status and permissions on `/data`; check that the database, key, and OAuth/audit state initialized. Do not print the env file.
- GitHub callback errors: verify the callback URL is an exact match, the public URL is HTTPS, and the OAuth client ID/secret pair belongs to that app.
- Login loops or rejected MCP requests: use the exact `/mcp` URL, retry the browser login, and check that client redirect/resource values were not changed by a proxy.
- Settings validation fails: verify the user's Coolify URL is reachable from the container and that their token has the required read permissions. Never test with a shared operator token.
- Caddy cannot reach the upstream: verify both containers are on `brightbean-studio_default` and that the upstream is `mcp:8080`.

For rollback, stop the new Compose service and restore the prior image and Caddy configuration/backup. Keep `/data` and the protected env file intact; do not revoke credentials during rollback. Validate `/healthz` and the prior route before switching traffic back.

## Local fallback

For a local, non-hosted setup use Rust stdio (`cargo run --release`) or the dependency-free Python server (`python3 coolify_mcp_server.py`). Configure `COOLIFY_BASE_URL`/`COOLIFY_ACCESS_TOKEN` (or the legacy `COOLIFY_URL`/`COOLIFY_TOKEN`) only in the local process environment or a mode-0600 secret file. Local stdio behavior is unchanged and does not require GitHub OAuth, a tenant database, or a public URL.
