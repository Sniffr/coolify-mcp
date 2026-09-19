# Hosted Rust MCP deployment

## Target

- Public host: `mcp.social.dpdns.org`
- MCP endpoint: `https://mcp.social.dpdns.org/mcp`
- Health endpoint: `https://mcp.social.dpdns.org/healthz`
- DNS zone: wildcard `*.social.dpdns.org`
- Remote account: `sidney@77.90.40.213`

The deployment was **not activated by Task 11**. Read-only discovery found Docker 29.7.2, Docker Compose v5.5.0, and DNS resolution to `77.90.40.213`. Caddy is the existing proxy and owns ports 80/443, but its verified configuration only contains the `${APP_DOMAIN:localhost}` Brightbean route; the target hostname is not configured. Public HTTPS currently fails during TLS negotiation. Docker Swarm secrets/configs are unavailable because the host is not a Swarm manager, and no approved host secret injection mechanism was verified.

Per the deployment brief, do not make privileged or irreversible changes until both prerequisites are supplied:

1. An existing host secret mechanism that can inject `COOLIFY_BASE_URL`, `COOLIFY_ACCESS_TOKEN`, and OAuth signing/persistence secrets without putting values in Git, command arguments, image layers, or a committed `.env` file.
2. An approved Caddy/proxy configuration and certificate path for `mcp.social.dpdns.org`, forwarding `/mcp` and `/healthz` to the Rust container on port 8080.

## Intended runtime configuration

Use the reviewed Rust image built from the deployment commit with:

- `MCP_TRANSPORT=http`
- `MCP_PUBLIC_URL=https://mcp.social.dpdns.org`
- `MCP_PORT=8080`
- `MCP_CAPABILITY_PROFILE=read-only`
- container user `10001:10001` (non-root)
- persistent writable `/data` volume for OAuth state
- health check `GET /healthz`

Inject Coolify URL/token and OAuth key material only through the approved host secret mechanism. Never place them in `compose.yaml`, Dockerfile, image labels, shell arguments, logs, or Git.

## Activation checklist (after blockers are resolved)

1. Build and tag `Dockerfile.rust` from the reviewed commit without secrets.
2. Transfer only source/build artifacts and deployment configuration.
3. Start the container with the runtime settings above and persistent `/data`.
4. Configure the existing proxy for the concrete hostname and verify TLS.
5. Run `./deploy/remote-check.sh`.
6. Complete one real OAuth PKCE MCP client session against `/mcp`; list the approved roster and make only a safe inventory/version call.
7. Inspect service logs for credentials, environment values, and raw response bodies.
8. Record the deployed image tag, health output, tool count, OAuth state path, and rollback command in the task report before switching traffic.

Rollback is to stop the new container and restore the previous Rust image/proxy route, or retain the documented Python stdio fallback. Do not revoke credentials as part of rollback.

## Local fallback

For local clients, keep using the Python stdio server described in `HOSTING.md`; it does not provide a public HTTP endpoint.
