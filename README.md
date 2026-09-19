# 🌈 Coolify MCP Bridge

> Give Claude Code, OpenHands, and OpenCode a safe, reusable MCP connection to Coolify.

[![MCP](https://img.shields.io/badge/MCP-compatible-7c3aed?style=for-the-badge)](https://modelcontextprotocol.io/) [![Python](https://img.shields.io/badge/Python-3.10%2B-3776ab?style=for-the-badge&logo=python&logoColor=white)](https://python.org/) [![Coolify](https://img.shields.io/badge/Coolify-API-00b894?style=for-the-badge)](https://coolify.io/docs/api/overview)

## ⚡ What it does

This dependency-free MCP server exposes Coolify through two tools:

- `coolify_health` — public reachability check.
- `coolify_request` — documented REST requests under `/api/v1`.

It keeps the API key in environment variables rather than prompts, tool arguments, source code, or Git history.

> ⚠️ `coolify_request` is intentionally powerful. Configure a least-privilege Coolify token and require confirmation before deployments, restarts, stops, deletes, writes, production changes, or sensitive reads.

## 🔐 Configure credentials

```bash
export COOLIFY_URL='https://coolify.example.com'
export COOLIFY_TOKEN='your-complete-coolify-token'
```

The token must be the complete value shown by Coolify, including its ID and `|` separator. Never commit `.env` files or paste tokens into prompts.

## 🧑‍💻 Run locally

```bash
python3 coolify_mcp_server.py
```

MCP clients launch this process and communicate over stdin/stdout. Python 3.10+ is recommended; no third-party packages are required.

## 🐳 Run with Docker

Build and run using environment variables supplied at runtime:

```bash
docker build -t coolify-mcp .
docker run --rm -i \
  -e COOLIFY_URL \
  -e COOLIFY_TOKEN \
  coolify-mcp
```

Or use Compose:

```bash
docker compose run --rm coolify-mcp
```

For a long-running hosted deployment, use a private MCP gateway or a platform that supports stdio MCP processes. Do not publish the token in an image, Dockerfile, public logs, or a public HTTP endpoint. See [`HOSTING.md`](HOSTING.md).

## Rust runtime (current default after verification)

Build and run the Rust implementation over stdio:

```bash
cargo run --release
cargo run -- doctor --json
```

Remote mode uses Streamable HTTP at `/mcp` (the hosted service is `https://mcp.dpdns.org/mcp`) and OAuth 2.1 with PKCE. Set `MCP_TRANSPORT=http`, `MCP_PUBLIC_URL`, `MCP_PORT` (default `8080`), and mount persistent OAuth state at `/data`. `MCP_CAPABILITY_PROFILE` is `read-only`, `operations`, or `admin`; HTTP defaults to read-only. Both `COOLIFY_BASE_URL`/`COOLIFY_ACCESS_TOKEN` and the legacy `COOLIFY_URL`/`COOLIFY_TOKEN` names are accepted (new names win). Never paste a real token into Git, chat, images, or logs.

The Python installer and `coolify_mcp_server.py` remain the fallback until Rust protocol and hosted acceptance checks are complete.

## 🟣 Claude Code

## ✨ One-command Claude Code install

Claude Code has a built-in MCP CLI: `claude mcp add`. This installer downloads the server, creates a private environment file, and registers a **user-scoped stdio MCP server**:

```bash
curl -fsSL https://raw.githubusercontent.com/Sniffr/coolify-mcp/main/install-claude-code.sh | bash
```

The installer creates:

```text
~/.local/share/coolify-mcp/coolify_mcp_server.py
~/.local/share/coolify-mcp/run.sh
~/.config/coolify-mcp/env       # mode 600
```

Edit the environment file:

```bash
${EDITOR:-nano} ~/.config/coolify-mcp/env
```

Set:

```dotenv
COOLIFY_URL=https://your-coolify.example.com
COOLIFY_TOKEN=your-complete-coolify-token
```

Then restart Claude Code and verify:

```bash
claude mcp list
claude mcp get coolify
```

The installer uses the official Claude Code command form `claude mcp add --transport stdio --scope user`. To install only for the current project, change `--scope user` to `--scope project` in the downloaded installer or run the equivalent command manually.

> ⚠️ Piped shell installers are convenient but require trust. For maximum reviewability, download first, inspect, then execute:
>
> ```bash
> curl -fsSLO https://raw.githubusercontent.com/Sniffr/coolify-mcp/main/install-claude-code.sh
> less install-claude-code.sh
> bash install-claude-code.sh
> ```

### Where should the environment variables live?

For this installer, use `~/.config/coolify-mcp/env` with permissions `600`. It is loaded only by the local MCP launcher and is not sent to the LLM. Alternatives are an OS secret manager, a systemd credential, a Docker/Kubernetes secret, or Claude Code’s supported MCP environment configuration. Never put the real token in `.mcp.json`, a prompt, source code, or GitHub.

## 🧰 MCP harness / tool

The MCP server is the harness: Claude Code starts it as a child process and calls its tools through MCP. The built-in Claude CLI is the setup tool; no extra MCP harness package is needed for local stdio use. The same server can be registered manually with OpenHands or OpenCode using their local MCP command configuration.


Add this to the MCP configuration used by Claude Code (project `.mcp.json` or user configuration):

```json
{
  "mcpServers": {
    "coolify": {
      "command": "python3",
      "args": ["/absolute/path/to/coolify_mcp_server.py"],
      "env": {
        "COOLIFY_URL": "${COOLIFY_URL}",
        "COOLIFY_TOKEN": "${COOLIFY_TOKEN}"
      }
    }
  }
}
```

Launch Claude Code from a shell where both variables are exported. If your version does not expand `${...}`, configure those values through its supported environment/secret mechanism. Confirm the `coolify_health` and `coolify_request` tools appear.

## 🟢 OpenHands

Register the same stdio MCP server in the OpenHands MCP/tool configuration:

- command: `python3`
- argument: `/absolute/path/to/coolify_mcp_server.py`
- environment: `COOLIFY_URL`, `COOLIFY_TOKEN`

For Dockerized OpenHands, mount or bake the server into the agent runtime and inject credentials through the container secret/environment facility. Keep the token out of the repository and conversation context.

## 🔵 OpenCode

Add the server to OpenCode’s MCP configuration using its local/command server form:

```json
{
  "mcp": {
    "coolify": {
      "type": "local",
      "command": ["python3", "/absolute/path/to/coolify_mcp_server.py"],
      "enabled": true,
      "environment": {
        "COOLIFY_URL": "${COOLIFY_URL}",
        "COOLIFY_TOKEN": "${COOLIFY_TOKEN}"
      }
    }
  }
}
```

OpenCode configuration keys can vary by release; if `environment` is unsupported, export the variables before starting OpenCode. Verify the server with a read-only health or inventory call first.

## ☁️ Hosting options

### Recommended: host the process near your client

Use local stdio when Claude Code/OpenHands/OpenCode run on the same machine. This is simplest and avoids exposing an MCP network endpoint.

### Docker host / VM

Run the container under a supervisor such as systemd, Docker Compose, or Kubernetes with secrets injected at runtime. A stdio MCP server is client-launched; for remote clients, use a trusted MCP gateway that supports your client’s transport and authentication. Never expose raw stdio or the Coolify token to the public Internet.

### Coolify itself

You can deploy this repository as a private application on Coolify, but a hosted MCP service needs a network transport/gateway. The included server is stdio-only by design. If you deploy it on Coolify, place `COOLIFY_URL` and `COOLIFY_TOKEN` in Coolify runtime environment variables/secrets and restrict inbound access. See [`HOSTING.md`](HOSTING.md) before exposing anything.

## 📚 Documentation

- [Hosting and security guide](HOSTING.md)
- [Complete API + LLM reference](coolify-api-llm-reference.md)
- [Official Coolify API docs](https://coolify.io/docs/api/overview)
- [Official OpenAPI snapshot](https://raw.githubusercontent.com/coollabsio/coolify/main/openapi.json)

## 🛡️ Security checklist

- [ ] Revoke the PAT that was pasted into chat and create a replacement.
- [ ] Use a dedicated Coolify token per integration/team.
- [ ] Start with `read`; add `deploy`, `write`, or `read:sensitive` only when required.
- [ ] Require human confirmation for mutations.
- [ ] Restrict self-hosted Coolify API access with an IP allowlist where practical.
- [ ] Keep secrets in runtime environment/secret managers.
- [ ] Review logs for accidental token disclosure.
