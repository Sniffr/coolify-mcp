# Rust migration and rollback

The Rust runtime is the intended implementation for stdio and authenticated Streamable HTTP. The hosted name is the concrete `mcp.social.dpdns.org`, served under the wildcard DNS zone `*.social.dpdns.org`. It supports the compatibility aliases `COOLIFY_BASE_URL`/`COOLIFY_ACCESS_TOKEN` and `COOLIFY_URL`/`COOLIFY_TOKEN`, capability profiles, OAuth 2.1 PKCE state in `/data`, and the hosted endpoint `https://mcp.social.dpdns.org/mcp`.

Run the side-effect-free checks before a switch:

```bash
cargo run -- doctor --json
cargo test --workspace
```

Production health checks must cover `/healthz`, OAuth discovery, MCP initialization, tool listing, and one safe read-only inventory call. Configure proxy forwarding for `/mcp` without a catch-all rewrite. Never perform a deployment, restart, delete, or other destructive call during deployment validation.

If Rust fails, roll back to the last known-good Rust image or the existing Python stdio installer. Keep Python available until Rust protocol and hosted acceptance tests pass. Rollback does not require revoking Coolify credentials. Never place a real token in Git, chat, Docker layers, or logs.
