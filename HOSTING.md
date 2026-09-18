# 🚀 Hosting Guide

## Choose your transport first

`coolify_mcp_server.py` uses **stdio MCP**. This is ideal when the MCP client launches the process locally or inside the same agent runtime. It is not an HTTP server and should not be placed directly behind a public URL.

For a remote setup, use an authenticated MCP gateway/bridge that supports your client’s transport. Keep the bridge private behind VPN, an identity-aware proxy, or a strict allowlist. Do not invent a public unauthenticated HTTP wrapper around `coolify_request`.

## Option A — Local process (recommended)

```bash
python3 --version
export COOLIFY_URL='https://coolify.example.com'
export COOLIFY_TOKEN='complete-token'
python3 coolify_mcp_server.py
```

The MCP client launches the process. Do not run the command in a shared shell with history enabled if your shell records exported secrets; prefer your OS secret manager or client environment configuration.

## Option B — Docker / Compose

```bash
export COOLIFY_URL='https://coolify.example.com'
export COOLIFY_TOKEN='complete-token'
docker compose run --rm coolify-mcp
```

For a service manager, inject secrets through the host’s secret mechanism. Do not hard-code them in `compose.yaml`, the Dockerfile, image layers, or GitHub Actions logs.

## Option C — Deploy the repository on Coolify

1. Push this repository to a public or private GitHub repository.
2. In Coolify, create an Application from that repository.
3. Build using the included `Dockerfile`.
4. Add runtime environment variables named `COOLIFY_URL` and `COOLIFY_TOKEN` in Coolify’s secret/environment settings.
5. Do not expose a public domain unless you add a separate authenticated MCP transport gateway.
6. Give the Coolify token the minimum permissions needed by the agent.
7. Restrict inbound access to trusted clients and test with a read-only call.

Because stdio expects a client-managed process, a plain Coolify web deployment will not automatically create a usable remote MCP endpoint. For remote access, add a properly authenticated MCP transport adapter and document its authentication separately.

## Option D — VM/systemd

A stdio server is normally launched per client, not kept as a public daemon. If a gateway launches it, use a dedicated Unix user, locked-down filesystem permissions, firewall rules, TLS at the gateway, authentication, and log redaction. Never log `Authorization` headers or environment values.

## Coolify token setup

- Cloud: create an API token for the intended team.
- Self-hosted: enable API access under **Settings → Configuration → Advanced → API Settings**.
- Start with `read`.
- Add `deploy` only for deployment automation.
- Add `write` only for resource changes.
- Add `read:sensitive` only for required secrets/logs/configuration.
- Avoid `root` unless there is a documented, unavoidable need.
- Use a separate token per client/team and rotate it regularly.

## Smoke tests

Ask the MCP client to call `coolify_health`, then perform a safe read such as listing the current team or applications. Before any write, confirm the exact endpoint, target UUID, body, and expected effect.

## Incident response

If a token is ever pasted into chat, a terminal transcript, a public issue, or a Git commit: revoke it immediately in Coolify, remove it from logs/history where possible, create a replacement, and review API access logs. A token exposed in a conversation should be considered compromised.
