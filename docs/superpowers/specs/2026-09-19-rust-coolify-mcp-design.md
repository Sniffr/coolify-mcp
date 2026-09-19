# Rust Coolify MCP Rewrite — Design Specification

**Date:** 2026-09-19  
**Repository:** `Sniffr/coolify-mcp`  
**Reference:** `StuMason/coolify-mcp`  
**Deployment target:** `sidney@77.90.40.213`, public endpoint `https://mcp.social.dpdns.org`

## 1. Purpose and success criteria

Replace the current minimal Python stdio bridge with a production Rust implementation that provides the reference project's complete default tool surface and operational behavior while remaining recognizably owned and maintained by Sniffr.

Success means:

- The Rust server exposes all 45 default reference tools, plus the fleet-only `list_instances` tool when fleet configuration is enabled.
- Existing local users can continue using stdio MCP.
- Remote clients can connect through Streamable HTTP at `https://mcp.social.dpdns.org/mcp` using OAuth 2.1.
- Coolify v4.0–v4.3 compatibility behavior represented by the reference is preserved and covered by tests.
- Secrets are masked at the API boundary by default, destructive actions are policy-controlled, and audit events never contain credentials or raw responses.
- The service is deployable as a non-root container with persistent OAuth state at `/data`.
- The server passes local protocol tests, fake-Coolify integration tests, container smoke tests, and post-deployment read-only checks.

The reference implementation is the behavioral specification for tool names, schemas, prompts, resources, compatibility behavior, and safety semantics. It is MIT licensed; any directly ported code must retain appropriate license and attribution notices.

## 2. Scope

### Included

The default tool roster is:

`application`, `application_logs`, `bulk_env_update`, `cloud_tokens`, `control`, `database`, `database_backups`, `deploy`, `deployment`, `diagnose_app`, `diagnose_server`, `env_vars`, `environments`, `find_issues`, `get_application`, `get_database`, `get_infrastructure_overview`, `get_mcp_version`, `get_server`, `get_service`, `get_version`, `github_apps`, `hetzner`, `list_applications`, `list_databases`, `list_deployments`, `list_destinations`, `list_servers`, `list_services`, `logs`, `private_keys`, `projects`, `redeploy_project`, `restart_project_apps`, `scheduled_tasks`, `search_docs`, `server_domains`, `server_resources`, `service`, `stop_all_apps`, `storages`, `system`, `tags`, `teams`, and `validate_server`.

Fleet configuration additionally registers `list_instances` and adds an optional instance selector to applicable tools.

Supporting features include:

- stdio MCP transport;
- Streamable HTTP MCP transport;
- OAuth 2.1 authorization-code flow with PKCE S256;
- dynamic client registration and OAuth discovery metadata;
- opaque short-lived access tokens and rotating refresh tokens;
- persistent OAuth state containing hashes and metadata only;
- Coolify API client with typed request/response models;
- Coolify v4 method and response compatibility fallbacks;
- compact list summaries and full detail tools;
- deployment polling and bounded log tails;
- diagnostics/doctor command;
- prompts and resources;
- documentation search;
- read-only mode;
- destructive-operation confirmation policy;
- fleet support;
- audit logging;
- secret masking and log-injection defenses.

### Not included in the first implementation

- Reimplementing Coolify's entire 193-path API as one MCP tool per endpoint. The grouped 45-tool surface remains the public contract.
- A hosted multi-tenant account system unrelated to Coolify authorization.
- Automatic execution of destructive Coolify operations during deployment or smoke testing.
- Storing raw Coolify tokens in OAuth state, logs, Docker layers, or repository files.

## 3. Architecture

Use a Cargo workspace with focused crates/modules:

```text
coolify-mcp/
├── crates/
│   ├── coolify-api/       # typed REST client, auth, errors, compatibility
│   ├── mcp-tools/         # tool registry, schemas, handlers, prompts/resources
│   ├── safety/            # masking, policy profiles, confirmations, audit chain
│   ├── oauth/             # PKCE, registration, token lifecycle, persistence
│   ├── transport/         # stdio and Streamable HTTP adapters
│   └── doctor/             # startup and connectivity diagnostics
├── tests/
│   ├── protocol/
│   ├── coolify-api/
│   ├── security/
│   └── client-interop/
└── Dockerfile
```

Use Tokio for async execution and cancellation, Serde for JSON models, a typed HTTP client for Coolify requests, and an HTTP framework/adapter that can expose the official Streamable HTTP MCP contract. The exact MCP crate version will be pinned after verifying stdio, HTTP, elicitation, and resource support against the test client; any missing SDK feature will be isolated behind the `transport` boundary rather than spread through tool code.

The `coolify-api` crate owns URL construction, token sourcing, request timeouts, response decoding, error normalization, status-aware compatibility fallbacks, and centralized sanitation. Tool handlers never construct authorization headers or independently parse sensitive responses.

The `mcp-tools` crate owns the same grouped action model as the reference. Every tool has a typed schema, title, annotations, description, and explicit safety classification. Registration is generated or checked from one roster so a new handler cannot silently omit annotations, tests, or documentation.

## 4. Configuration and compatibility

The server accepts both the current repository's names and the reference names:

- `COOLIFY_BASE_URL` takes precedence over `COOLIFY_URL`.
- `COOLIFY_ACCESS_TOKEN` takes precedence over `COOLIFY_TOKEN`.
- `COOLIFY_ACCESS_TOKEN_FILE` is supported for token rotation without a process restart.
- `MCP_TRANSPORT=stdio` is the default; `MCP_TRANSPORT=http` selects hosted mode.
- `MCP_PUBLIC_URL` is required in HTTP mode and must be HTTPS outside explicit local development.
- `MCP_PORT` or `PORT` defaults to `8080`.
- `MCP_HOST` controls the bind interface.
- `MCP_READONLY=true` registers only read-only tools.
- `MCP_CAPABILITY_PROFILE` supports `read-only`, `operations`, and `admin`; HTTP defaults to `read-only`, while stdio defaults to the current operations-capable behavior for compatibility.
- `MCP_OAUTH_STATE_FILE` defaults to `/data/oauth-state.json` in the container. The OAuth HMAC signing key is persisted atomically beside it as `oauth-state.key` with mode 600; both files must survive restarts. Hosted audit events are appended to `MCP_AUDIT_LOG` (default `/data/audit.jsonl`) with mode 600.
- `MCP_ACCESS_TOKEN_TTL` and `MCP_REFRESH_TOKEN_TTL` control OAuth lifetimes.
- `COOLIFY_MCP_AUDIT` defaults on for HTTP and off for local stdio.
- `MCP_ALLOW_INSECURE_HTTP=true` is permitted only for local development.
- Fleet configuration follows the reference's `COOLIFY_INSTANCES` format.

Startup diagnostics collect all configuration errors before exiting, describe fixes without echoing values, and distinguish malformed URL, missing token, inaccessible token file, unusable persistence path, and insecure public URL.

## 5. Security and unique Rust contribution

The Rust implementation adds a capability policy engine with profiles such as `read-only`, `operations`, and `admin`. Policies determine which tools/actions are registered and which operation classes can reach Coolify. HTTP mode is read-only by default; mutation enablement is explicit and auditable.

Responses are recursively sanitized at the API boundary. Default masking covers environment values, passwords, private keys, webhook secrets, connection URLs, Compose bodies, log-drain credentials, API/client secrets, and other reference-identified sensitive fields. Raw model-facing logs are bounded and framed as untrusted data.

Destructive operations must use the MCP client's human-confirmation mechanism where available. HTTP mode fails closed when a dangerous operation cannot be confirmed by a human-capable client; a model-supplied boolean is never treated as equivalent to human approval.

Every hosted tool call emits one structured audit event containing timestamp, client identifier where available, tool/action, safe resource identifiers, outcome, status, and duration. Arguments and responses are excluded by construction. Each event includes the digest of the previous event and its own digest, creating a tamper-evident ordered chain without storing secrets. Rotation/restart behavior and chain verification are documented.

OAuth state stores only registered-client metadata, authorization-code hashes, access/refresh-token hashes, expiry, grant identifiers, and resource bindings. PKCE S256, exact redirect matching, single-use codes, resource binding, token expiry, refresh rotation, replay detection, and rate limits are required.

## 6. Deployment design

Build a multi-stage container that compiles the Rust binary, copies only the runtime artifact and CA certificates, runs as a non-root user, exposes port `8080`, and mounts `/data` for OAuth state. The deployment will be performed over SSH to `sidney@77.90.40.213` without placing credentials in shell history or image layers.

The intended public configuration is:

```text
MCP_TRANSPORT=http
MCP_PUBLIC_URL=https://mcp.social.dpdns.org
MCP_PORT=8080
```

The actual Coolify URL and token will be injected on the host through runtime environment/secrets. `mcp.social.dpdns.org` is the concrete host under the wildcard DNS zone `*.social.dpdns.org`; it must resolve to `77.90.40.213`, and the existing proxy must terminate HTTPS and forward to port `8080`. The deployment must configure a health check for `/healthz` and a persistent `/data` volume.

No destructive Coolify operation will be issued as part of deployment validation. The first remote call will be health/discovery, followed by MCP initialization, tool listing, and a safe read-only inventory call.

## 7. Testing and acceptance

### Unit tests

Cover URL normalization, query encoding, API error parsing, method fallback, token-file rotation, sensitive-field masking, untrusted-log framing, capability decisions, audit-chain hashing, PKCE, redirect validation, OAuth token lifecycle, and stdio framing.

### Integration tests

Run a deterministic fake Coolify server that exercises:

- successful list/detail/read calls;
- structured 401/403/404/405/422/429/500 responses;
- v4 compatibility method changes;
- pagination and compact summaries;
- deployment trigger, polling, timeout, and failed-log handling;
- secrets at multiple nested response depths;
- poisoned logs and API text;
- token rotation and refresh-token replay;
- persistence reload and corrupt-state recovery.

### MCP contract tests

Assert the complete default roster, schemas, annotations, prompts, resources, read-only filtering, fleet-only registration, initialization response, and tool-call result shapes. Use the official MCP client where possible for both stdio and Streamable HTTP.

### Container and deployment acceptance

- Build the image reproducibly.
- Verify non-root execution and `/data` writeability.
- Run `/healthz` and OAuth discovery locally.
- Complete a local OAuth PKCE flow against the container.
- Initialize an MCP session and verify the complete tool roster.
- Deploy to the target host.
- Verify `https://mcp.social.dpdns.org/healthz` and OAuth discovery.
- Connect a real MCP client and perform a safe read-only call.
- Confirm no token or secret appears in container logs.

Completion claims require captured command output from the relevant verification steps; a successful image build alone is not sufficient evidence of a working hosted MCP service.

## 8. Migration and rollback

The existing Python server remains in the repository during migration and is not deleted until Rust passes the protocol and remote acceptance suite. The Rust binary becomes the documented default after acceptance. If deployment fails, the previous Python/stdio path remains available for local users, and the hosted container can be rolled back to the last known image without revoking Coolify credentials.

Changes to the reference behavior will be tracked as compatibility notes and tests rather than silently diverging. The Rust implementation will retain license/attribution files required for any directly ported MIT-licensed material.
