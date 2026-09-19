# Rust Coolify MCP Rewrite Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port the reference Coolify MCP server's complete 45-tool default surface and supporting behavior to Rust, preserve local stdio compatibility, add a safe hosted Streamable HTTP/OAuth transport, and deploy it at `https://mcp.social.dpdns.org`.

**Architecture:** Keep the existing Python server as a fallback while adding a Cargo workspace. The Rust binary is split into API, safety, OAuth, transport, tools, and doctor boundaries; the root binary selects stdio or HTTP mode from environment. The reference repository is used as a behavioral and contract specification, with its MIT attribution preserved if code is directly ported.

**Tech Stack:** Rust stable, Cargo workspace, Tokio, Serde/serde_json, reqwest with rustls, axum, tracing, SHA-256/base64/random token primitives, Rust MCP protocol SDK where its verified features cover the required contract, and Docker multi-stage builds.

**Spec:** `docs/superpowers/specs/2026-09-19-rust-coolify-mcp-design.md`

## Global Constraints

- Expose all 45 default reference tools; register fleet-only `list_instances` only when fleet configuration is enabled.
- Preserve local stdio MCP behavior and accept both `COOLIFY_BASE_URL`/`COOLIFY_ACCESS_TOKEN` and legacy `COOLIFY_URL`/`COOLIFY_TOKEN` names.
- HTTP mode requires `MCP_PUBLIC_URL` and HTTPS unless `MCP_ALLOW_INSECURE_HTTP=true` is explicitly used for local development.
- HTTP mode defaults to read-only and fails closed when a destructive action cannot receive human confirmation.
- Mask secrets at the API boundary before any MCP response; never log request bodies, raw responses, tokens, or secret values.
- OAuth persistence stores hashes/metadata only under `/data`; OAuth uses PKCE S256, exact redirect matching, short-lived tokens, refresh rotation, replay detection, and resource binding.
- API compatibility must cover the reference behavior for Coolify v4.0–v4.3, including safe method fallbacks and status-aware errors.
- The container runs as a non-root user, listens on port `8080`, exposes `/healthz`, and persists `/data`.
- No destructive Coolify call is used in tests, deployment smoke checks, or acceptance verification.
- Every task ends with focused tests before its commit; completion claims require captured verification output.

## Review Focus

1. **Legacy and reference environment names with conflicting values** — precedence must be deterministic and startup diagnostics must never print either value. Test in Task 2.
2. **Coolify method compatibility and unsafe retries** — retry only a proven routing miss/405, never retry a potentially executed POST/PATCH/DELETE. Test in Task 4.
3. **Nested secrets and prompt-injection text in API/log responses** — recursive masking and untrusted framing must survive arbitrary nesting and forged boundary text. Test in Task 3.
4. **OAuth state and redirect attacks** — reject non-HTTPS redirects except loopback, enforce PKCE, bind tokens to `/mcp`, rotate refresh tokens, and revoke on replay. Test in Task 7.
5. **HTTP protocol lifecycle and resource exhaustion** — bound request bodies/timeouts, handle notifications and session headers, and keep long-lived responses from leaking state. Test in Task 8.

---

### Task 1: Bootstrap the Rust workspace and preserve the fallback

**Files:**
- Create: `Cargo.toml`
- Create: `Cargo.lock`
- Create: `rust-toolchain.toml`
- Create: `src/main.rs`
- Create: `crates/coolify-api/Cargo.toml`, `crates/coolify-api/src/lib.rs`
- Create: `crates/safety/Cargo.toml`, `crates/safety/src/lib.rs`
- Create: `crates/oauth/Cargo.toml`, `crates/oauth/src/lib.rs`
- Create: `crates/transport/Cargo.toml`, `crates/transport/src/lib.rs`
- Create: `crates/mcp-tools/Cargo.toml`, `crates/mcp-tools/src/lib.rs`
- Create: `crates/doctor/Cargo.toml`, `crates/doctor/src/lib.rs`
- Create: `tests/protocol/.gitkeep`
- Modify: `README.md` only after the binary can run; do not remove Python instructions yet.

**Interfaces:**
- The scaffold root binary exposes `main() -> Result<(), AppError>`; transport dispatch is wired in Task 8 after the shared application exists.
- Each crate exports a compiling `lib.rs`; no task may use cross-crate private modules.
- Keep `coolify_mcp_server.py`, `Dockerfile`, and existing stdio docs unchanged until the Rust fallback is verified.

- [ ] **Step 1: Install and pin the Rust toolchain**

Run:

```bash
rustup toolchain install stable
rustup component add rustfmt clippy --toolchain stable
rustup default stable
```

Create `rust-toolchain.toml`:

```toml
[toolchain]
channel = "stable"
components = ["rustfmt", "clippy"]
profile = "minimal"
```

- [ ] **Step 2: Write the workspace smoke test**

Create `tests/workspace_smoke.rs`:

```rust
#[test]
fn workspace_binary_has_a_main_entrypoint() {
    assert!(std::path::Path::new("src/main.rs").exists());
}
```

- [ ] **Step 3: Add the workspace and minimal crates**

The root `Cargo.toml` must define the package `sniffr-coolify-mcp`, list all six workspace members, and use Rust 2024 edition. Add only dependencies needed by the current scaffold: `tokio`, `serde`, `serde_json`, `thiserror`, and path dependencies on the six crates.

Implement `src/main.rs` with a temporary startup error:

```rust
fn main() {
    eprintln!("sniffr-coolify-mcp Rust runtime scaffold");
}
```

- [ ] **Step 4: Verify formatting, compilation, and tests**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Expected: all commands pass and the smoke test passes.

- [ ] **Step 5: Commit the scaffold**

```bash
git add Cargo.toml Cargo.lock rust-toolchain.toml src crates tests/workspace_smoke.rs
git commit -m "build: bootstrap Rust Coolify MCP workspace"
```

---

### Task 2: Implement configuration, token sources, and the typed HTTP client foundation

**Files:**
- Create: `crates/coolify-api/src/config.rs`
- Create: `crates/coolify-api/src/token_source.rs`
- Create: `crates/coolify-api/src/error.rs`
- Create: `crates/coolify-api/src/client.rs`
- Create: `crates/coolify-api/tests/config_tests.rs`
- Create: `crates/coolify-api/tests/token_source_tests.rs`
- Create: `crates/coolify-api/tests/client_error_tests.rs`
- Modify: `crates/coolify-api/src/lib.rs`

**Interfaces:**
- `pub struct CoolifyConfig { pub base_url: Url, pub token_source: TokenSource, pub custom_headers: HeaderMap, pub timeout: Duration }`.
- `pub fn config_from_env(env: &HashMap<String, String>, http_mode: bool) -> Result<CoolifyConfig, ConfigError>`.
- `pub struct TokenSource` with `from_env`, `from_file`, `current`, and `refresh`; file reads trim one trailing newline/whitespace and never expose values in `Debug`.
- `pub struct CoolifyClient` with `request_json<T>()`, `request_text()`, `get_version()`, and typed error results.
- `pub enum CoolifyApiError { Config, Transport, Http { status, body, hint }, Decode }`.

- [ ] **Step 1: Write failing environment precedence tests**

Test that `COOLIFY_BASE_URL` wins over `COOLIFY_URL`, `COOLIFY_ACCESS_TOKEN` wins over `COOLIFY_TOKEN`, a missing URL/token produces named errors, an invalid URL is rejected, and error formatting contains variable names but not values.

- [ ] **Step 2: Run the focused tests and verify failure**

```bash
cargo test -p coolify-api --test config_tests
```

Expected: compile failures for missing `config_from_env` and `ConfigError`.

- [ ] **Step 3: Implement configuration and token loading**

Parse URLs with `url::Url`, reject non-http(s) schemes, normalize one trailing slash, and implement the precedence exactly:

```rust
let base_url = env.get("COOLIFY_BASE_URL")
    .filter(|v| !v.trim().is_empty())
    .or_else(|| env.get("COOLIFY_URL"));
let token = env.get("COOLIFY_ACCESS_TOKEN")
    .filter(|v| !v.is_empty())
    .or_else(|| env.get("COOLIFY_TOKEN"));
```

If `COOLIFY_ACCESS_TOKEN_FILE` exists, it wins over both inline token variables. Store only the token in private memory and redact it from error/debug output.

- [ ] **Step 4: Write failing client response/error tests**

Use a local test server or injected `HttpExecutor` to assert JSON, text, empty, 401, 403, 404, 405, 422, 429, and 500 responses become structured results. Assert `Retry-After` is retained and body size is bounded.

- [ ] **Step 5: Implement `CoolifyClient` request plumbing**

Use `reqwest::Client` with rustls, `Content-Type: application/json`, `Accept: application/json`, a 45-second default request timeout, the current token source, and custom headers excluding caller overrides for `Authorization` and `Content-Type`. Decode JSON only when the content type is JSON; preserve bounded text otherwise.

- [ ] **Step 6: Verify and commit**

```bash
cargo fmt --all
cargo test -p coolify-api
cargo clippy -p coolify-api --all-targets -- -D warnings
git add crates/coolify-api
git commit -m "feat: add typed Coolify client foundation"
```

---

### Task 3: Add masking, untrusted output framing, capability policy, and chained audit logging

**Files:**
- Create: `crates/safety/src/masking.rs`
- Create: `crates/safety/src/untrusted.rs`
- Create: `crates/safety/src/policy.rs`
- Create: `crates/safety/src/audit.rs`
- Create: `crates/safety/tests/masking_tests.rs`
- Create: `crates/safety/tests/policy_tests.rs`
- Create: `crates/safety/tests/audit_tests.rs`
- Modify: `crates/safety/src/lib.rs`
- Modify: `crates/coolify-api/src/client.rs` to call the sanitizer before returning model-facing data.

**Interfaces:**
- `pub fn sanitize_json(value: &Value, reveal: bool) -> Value`.
- `pub fn frame_untrusted(text: &str, nonce: &str) -> String`.
- `pub enum CapabilityProfile { ReadOnly, Operations, Admin }` and `pub fn allows(profile, action) -> bool`.
- `pub struct AuditEvent` and `pub struct AuditLogger { pub fn record(&mut self, event: AuditEvent) -> io::Result<String> }`.

- [ ] **Step 1: Write failing nested-secret tests**

Build JSON containing `value`, `real_value`, `password`, `private_key`, `webhook_secret`, `internal_db_url`, `docker_compose`, `custom_labels`, `client_secret`, and nested `environment_variables`. Assert all default output values become `***`, null values remain null, non-secret metadata remains unchanged, and `reveal=false` is the default.

- [ ] **Step 2: Write failing log-injection tests**

Pass log text containing forged `[END UNTRUSTED LOG OUTPUT]`, mixed case, extra whitespace, newlines, and a fake system instruction. Assert the exact generated terminator cannot be forged and the complete payload remains inside one boundary.

- [ ] **Step 3: Implement recursive masking and framing**

Use a closed set for always-masked fields and a separate sensitive set for fields revealable only by an explicit server policy. Generate a random per-call nonce with `rand`; replace boundary phrases inside payloads before adding the real boundary.

- [ ] **Step 4: Write policy and audit tests**

Assert read actions are allowed in all profiles, deploy/write/delete actions are denied in `ReadOnly`, HTTP defaults to `ReadOnly`, stdio defaults to compatibility `Operations`, and audit output contains only allowed identifiers. Assert event `hash` changes when the previous hash, tool, outcome, or timestamp changes.

- [ ] **Step 5: Implement policy and chained audit output**

Serialize a canonical audit payload, compute SHA-256 over `previous_hash || canonical_payload`, write one JSON line to stderr/file, and keep the previous digest in memory. Never serialize request bodies or response values.

- [ ] **Step 6: Verify and commit**

```bash
cargo test -p safety
cargo test -p coolify-api
cargo clippy --workspace --all-targets -- -D warnings
git add crates/safety crates/coolify-api/src/client.rs
git commit -m "feat: add response safety and tamper-evident audit policy"
```

---

### Task 4: Implement typed Coolify resource operations and compatibility behavior

**Files:**
- Create: `crates/coolify-api/src/models.rs`
- Create: `crates/coolify-api/src/api_shape.rs`
- Create: `crates/coolify-api/src/compatibility.rs`
- Create: `crates/coolify-api/src/resources/servers.rs`
- Create: `crates/coolify-api/src/resources/projects.rs`
- Create: `crates/coolify-api/src/resources/applications.rs`
- Create: `crates/coolify-api/src/resources/databases.rs`
- Create: `crates/coolify-api/src/resources/services.rs`
- Create: `crates/coolify-api/src/resources/deployments.rs`
- Create: `crates/coolify-api/src/resources/configuration.rs`
- Create: `crates/coolify-api/src/resources/diagnostics.rs`
- Create: `crates/coolify-api/tests/compatibility_tests.rs`
- Create: `crates/coolify-api/tests/resource_tests.rs`
- Modify: `crates/coolify-api/src/client.rs`, `src/lib.rs`

**Interfaces:**
- Typed summaries: `ServerSummary`, `ProjectSummary`, `ApplicationSummary`, `DatabaseSummary`, `ServiceSummary`, `DeploymentSummary`.
- Typed client methods matching the tool layer: list/get/create/update/delete resources, deployment trigger/get/cancel, logs, env vars, backups, tags, storages, diagnostics, and system operations.
- `pub async fn post_with_legacy_get_fallback<T>(&self, key: LegacyEndpoint, path: &str, body: Option<Value>) -> Result<T, CoolifyApiError>`.
- `pub fn error_hint(status: u16, path: &str) -> Option<&'static str>`.
- `pub fn is_running_status(status: Option<&str>) -> bool` with `unhealthy` not misclassified as healthy.

- [ ] **Step 1: Extract and record the reference API contract**

Use `coolify-api-llm-reference.md`, the reference OpenAPI chunks, and the reference client/tests to create Rust serde models and a route matrix. The matrix must explicitly record method, path, required arguments, return projection, safety class, and v4 compatibility behavior for every handler.

- [ ] **Step 2: Write failing compatibility tests**

Fake `POST` returning 405 and routing-catch-all 404 must trigger one `GET` fallback and cache the method per endpoint. A controller-owned 404, 500, or 422 must not trigger a fallback. A remembered GET that later returns 405 must reprobe POST. Assert no unsafe method is retried after a non-routing error.

- [ ] **Step 3: Implement models, API shape detection, and compatibility client**

Keep routing-catch-all detection body-based, not status-only. Attach known endpoint hints for scheduled-task 500s, method changes, missing permissions, old-version routes, and wrong resource UUIDs.

- [ ] **Step 4: Write failing resource tests**

Test list summary projection, pagination query encoding, application/domain mapping, database type normalization, log response unwrapping from `{logs: string}` and bare strings, deployment polling projections, and `running:unhealthy`/`exited:unhealthy` status handling.

- [ ] **Step 5: Implement resource modules**

Port resource methods by domain, keeping each module below one responsibility. Apply centralized sanitation to all responses and bounded projections to list/deployment/log methods. Do not expose raw upstream objects where the reference returns summaries.

- [ ] **Step 6: Verify and commit**

```bash
cargo test -p coolify-api
cargo fmt --all -- --check
cargo clippy -p coolify-api --all-targets -- -D warnings
git add crates/coolify-api
git commit -m "feat: implement typed Coolify resource operations"
```

---

### Task 5: Build the complete MCP tool registry and handlers

**Files:**
- Create: `crates/mcp-tools/src/registry.rs`
- Create: `crates/mcp-tools/src/annotations.rs`
- Create: `crates/mcp-tools/src/schemas.rs`
- Create: `crates/mcp-tools/src/handlers/infrastructure.rs`
- Create: `crates/mcp-tools/src/handlers/applications.rs`
- Create: `crates/mcp-tools/src/handlers/databases.rs`
- Create: `crates/mcp-tools/src/handlers/services.rs`
- Create: `crates/mcp-tools/src/handlers/deployments.rs`
- Create: `crates/mcp-tools/src/handlers/configuration.rs`
- Create: `crates/mcp-tools/src/handlers/operations.rs`
- Create: `crates/mcp-tools/src/handlers/diagnostics.rs`
- Create: `crates/mcp-tools/src/handlers/docs.rs`
- Create: `crates/mcp-tools/tests/roster_tests.rs`
- Create: `crates/mcp-tools/tests/tool_contract_tests.rs`
- Modify: `crates/mcp-tools/src/lib.rs`

**Interfaces:**
- `pub const DEFAULT_TOOL_ROSTER: &[ToolSpec]` containing exactly the 45 default names from the spec.
- `pub struct ToolSpec { name, title, description, input_schema, annotations, safety }`.
- `pub struct ToolContext { client, policy, audit, instance, request_metadata }`.
- `pub async fn call_tool(ctx: ToolContext, name: &str, args: Value) -> ToolResult`.
- `pub fn registered_tools(profile: CapabilityProfile, fleet: Option<&InstanceRegistry>) -> Vec<ToolSpec>`.

- [ ] **Step 1: Write failing roster and annotation tests**

Assert the default roster equals the 45-name list in the specification, every name has a title/schema/annotation/safety classification, no duplicate names exist, fleet adds only `list_instances`, and `MCP_READONLY` removes every mutating tool.

- [ ] **Step 2: Implement registry, schemas, and annotations**

Use typed serde input structs per action group. Keep tool descriptions aligned with the reference safety language. Register grouped tools with worst-case annotations where a group contains destructive actions, while action-level policy still permits safe reads under a group.

- [ ] **Step 3: Write failing handler contract tests**

Use a fake client to test at least one action in every group: overview, list/get, application/database/service CRUD, env vars, deployment, logs, server/system, diagnostics, docs, tags, storages, backups, cloud/private keys, fleet, and emergency/bulk operations. Assert result envelopes, `_actions`, `_pagination`, bounded logs, and structured errors.

- [ ] **Step 4: Implement handlers by domain**

Port all action variants from the reference into the domain handler files. Route all Coolify calls through the typed API client; route all output through sanitation/untrusted framing; route mutating actions through policy/confirmation/audit. Do not duplicate URL or auth logic in handlers.

- [ ] **Step 5: Add deployment wait behavior and blast-radius summaries**

Implement terminal statuses `finished`, `failed`, and `cancelled`, bounded poll intervals, timeout responses with a next action, failure log tails, tag multi-deployment reporting, and confirmation text that names the affected resources.

- [ ] **Step 6: Verify and commit**

```bash
cargo test -p mcp-tools
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
git add crates/mcp-tools
git commit -m "feat: add complete Coolify MCP tool surface"
```

---

### Task 6: Port prompts, resources, documentation search, and fleet routing

**Files:**
- Create: `crates/mcp-tools/src/prompts.rs`
- Create: `crates/mcp-tools/src/resources.rs`
- Create: `crates/mcp-tools/src/docs_search.rs`
- Create: `crates/mcp-tools/src/instances.rs`
- Create: `crates/mcp-tools/tests/prompts_resources_tests.rs`
- Create: `crates/mcp-tools/tests/fleet_tests.rs`
- Copy/modify: `crates/mcp-tools/src/data/coolify-docs.json` from the reference with attribution and a regeneration note.

**Interfaces:**
- `pub struct InstanceRegistry` with `default`, `all`, `is_fleet`, and `get(name)` methods.
- `pub fn register_prompts(...)` and `pub fn register_resources(...)`.
- `pub struct DocsSearchEngine` with `search(query, limit) -> Vec<DocHit>`.

- [ ] **Step 1: Write failing fleet tests**

Assert one instance costs no `instance` argument, fleet tools include the selector, unknown instance names are refused without an API call, and concurrent calls do not cross instance clients.

- [ ] **Step 2: Implement instance registry and request-scoped routing**

Parse `COOLIFY_INSTANCES` JSON, validate names/URLs/tokens without echoing credentials, create one API client per instance, and carry the selected instance through request context rather than a mutable global field.

- [ ] **Step 3: Write failing prompt/resource/search tests**

Assert prompts disappear when required tools are unavailable in read-only mode, resources use the same projections as their tools, search is bounded and deterministic, and returned documentation text is marked as untrusted data.

- [ ] **Step 4: Implement prompts, resources, and search**

Port the reference workflows `troubleshoot_application`, `explain_failed_deploy`, and `estate_health`; resources `coolify://overview` and `coolify://application/{uuid}`; and the indexed docs search. Filter suggested actions against the actual registered roster.

- [ ] **Step 5: Verify and commit**

```bash
cargo test -p mcp-tools
cargo fmt --all -- --check
git add crates/mcp-tools
git commit -m "feat: add fleet routing prompts resources and docs search"
```

---

### Task 7: Implement OAuth 2.1 provider and persistent state

**Files:**
- Create: `crates/oauth/src/model.rs`
- Create: `crates/oauth/src/provider.rs`
- Create: `crates/oauth/src/persistence.rs`
- Create: `crates/oauth/src/pkce.rs`
- Create: `crates/oauth/tests/pkce_tests.rs`
- Create: `crates/oauth/tests/provider_tests.rs`
- Create: `crates/oauth/tests/persistence_tests.rs`
- Modify: `crates/oauth/src/lib.rs`

**Interfaces:**
- `pub struct OAuthProvider` with metadata, registration, authorization validation/completion, token exchange, refresh exchange, and bearer verification methods.
- `pub struct OAuthStateStore` persisting only serializable hashes/metadata.
- `pub fn canonical_resource(value: &str) -> Result<String, OAuthError>`.
- `pub fn redirect_uri_matches(registered, requested) -> bool`.
- `pub fn verify_pkce(verifier, challenge) -> bool`.

- [ ] **Step 1: Write failing OAuth attack tests**

Test rejection of missing/weak PKCE, non-HTTPS redirects except loopback, redirect mismatch, fragment redirects, wrong resource, unknown client, expired/single-use codes, invalid client secrets, and token replay. Test exact loopback port relaxation only when scheme/host/path match.

- [ ] **Step 2: Implement opaque token and hash primitives**

Generate random `client`, `code`, access, and refresh values; persist only SHA-256 hashes. Store grant family IDs, expiry, resource, client ID, scope, rotation/revocation state. Use constant-time hash comparisons where applicable.

- [ ] **Step 3: Implement registration, PKCE, and authorization code flow**

Support RFC 7591 registration, exact redirect rules, `response_type=code`, `code_challenge_method=S256`, resource binding to `/mcp`, single-use ten-minute codes, and signed/validated state needed by the MCP client.

- [ ] **Step 4: Implement refresh rotation and persistence**

Rotate refresh tokens on every exchange. Reuse of a rotated token revokes the entire grant family. Persist atomically through a mode-600 temporary file and rename; load corrupt state as a clean reauthorization state and report degraded persistence to health checks.

- [ ] **Step 5: Verify and commit**

```bash
cargo test -p oauth
cargo clippy -p oauth --all-targets -- -D warnings
git add crates/oauth
git commit -m "feat: add PKCE OAuth provider with persistent hashed state"
```

---

### Task 8: Implement stdio and Streamable HTTP transports

**Files:**
- Create: `crates/transport/src/stdio.rs`
- Create: `crates/transport/src/http.rs`
- Create: `crates/transport/src/http_app.rs`
- Create: `crates/transport/tests/stdio_tests.rs`
- Create: `crates/transport/tests/http_tests.rs`
- Modify: `crates/transport/src/lib.rs`
- Modify: `src/main.rs`

**Interfaces:**
- `pub enum TransportKind { Stdio, Http }`.
- `pub async fn run_stdio(app: McpApplication) -> Result<(), TransportError>`.
- `pub async fn run_http(app: McpApplication, config: HttpConfig) -> Result<(), TransportError>`.
- `pub fn normalize_public_url(raw: &str) -> Result<Url, UrlError>`.
- HTTP routes: `/healthz`, OAuth discovery/registration/authorize/token endpoints, and protected `/mcp`.

- [ ] **Step 1: Write failing stdio framing tests**

Test newline-delimited JSON, Content-Length framing, first-message framing selection, notifications with no response, invalid JSON error responses, EOF shutdown, and flush behavior. Assert stdout contains protocol bytes only; diagnostics go to stderr.

- [ ] **Step 2: Implement stdio adapter**

Use buffered stdin/stdout, preserve the detected framing mode for the session, dispatch JSON-RPC through the shared MCP application, and write one response per request with explicit flushes.

- [ ] **Step 3: Write failing HTTP lifecycle/security tests**

Test health/discovery without bearer access, invalid/expired bearer tokens, registration and token flows, `/mcp` method/content negotiation, session headers, notifications, 5 MiB body rejection, 15-second header timeout, 30-second request timeout, and absence of secret values in errors.

- [ ] **Step 4: Implement HTTP adapter and OAuth routes**

Use the verified MCP SDK/transport contract if available; otherwise adapt the shared JSON-RPC application behind axum while matching Streamable HTTP response/status/session behavior. Enforce bearer verification before `/mcp`, rate-limit registration/authorization/token endpoints, and call the OAuth provider only through its public interface.

- [ ] **Step 5: Add application startup dispatch**

`src/main.rs` must select HTTP only when `MCP_TRANSPORT=http`; otherwise stdio remains the default. HTTP startup validates URL, token, persistence path, and HTTPS before binding. Shutdown flushes OAuth/audit state and closes listeners.

- [ ] **Step 6: Verify and commit**

```bash
cargo test -p transport
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
git add src/main.rs crates/transport
git commit -m "feat: support stdio and authenticated Streamable HTTP"
```

---

### Task 9: Add doctor diagnostics, CLI behavior, and operator documentation

**Files:**
- Create: `crates/doctor/src/checks.rs`
- Create: `crates/doctor/src/report.rs`
- Create: `crates/doctor/tests/doctor_tests.rs`
- Modify: `crates/doctor/src/lib.rs`
- Modify: `src/main.rs`
- Modify: `README.md`
- Modify: `HOSTING.md`
- Modify: `.env.example`
- Create: `docs/rust-migration.md`
- Copy/modify: `LICENSE`/attribution notice if direct MIT code is ported.

**Interfaces:**
- `pub struct DoctorReport { pub ok: bool, pub checks: Vec<DoctorCheck> }`.
- `pub async fn run_doctor(env, fetcher) -> DoctorReport`.
- CLI command: `coolify-mcp doctor [--json] [--header "Key: Value"]`.

- [ ] **Step 1: Write failing doctor tests**

Test missing URL/token, literal `${VAR}`, doubled `/api/v1`, unreachable Coolify, Cloudflare redirect, invalid token, missing deploy ability, out-of-range version, routing-catch-all shape, and clean JSON output with no secret values.

- [ ] **Step 2: Implement doctor checks**

Use only side-effect-free GET probes. Bound all network calls to ten seconds. Return named checks with status, detail, and one-line fix; use exit code 0 only when there are no failures/inconclusive checks.

- [ ] **Step 3: Update documentation and compatibility setup**

Document Rust stdio/HTTP usage, both variable naming styles, `MCP_CAPABILITY_PROFILE`, `/data`, OAuth setup, `mcp.social.dpdns.org`, deployment health checks, rollback to Python, and the rule never to paste a real token into Git or chat. Keep the current Python installer clearly marked as fallback until the Rust installer is verified.

- [ ] **Step 4: Verify and commit**

```bash
cargo test -p doctor
cargo run -- doctor --json
cargo fmt --all -- --check
git diff --check
git add crates/doctor src/main.rs README.md HOSTING.md .env.example docs/rust-migration.md LICENSE
git commit -m "docs: document Rust runtime and hosted deployment"
```

---

### Task 10: Build the production container and local acceptance harness

**Files:**
- Create: `Dockerfile.rust`
- Create: `compose.rust.yaml`
- Create: `.dockerignore`
- Create: `scripts/smoke-rust.sh`
- Create: `tests/fake-coolify/server.rs`
- Create: `tests/client-interop/stdio_client.rs`
- Create: `tests/client-interop/http_oauth.rs`
- Modify: `Cargo.toml` for test-only workspace members if needed.

**Interfaces:**
- Container command: `sniffr-coolify-mcp`.
- Health endpoint: `GET /healthz` on port `8080`.
- Smoke script accepts only non-secret URL/config arguments and never echoes token values.

- [ ] **Step 1: Write the fake-server acceptance tests**

Start a deterministic fake Coolify server, launch Rust in HTTP mode with a temporary `/data`, complete OAuth PKCE, initialize MCP, call `tools/list`, call a safe inventory tool, and assert the expected 45-tool roster. Add a stdio client run over both framing modes.

- [ ] **Step 2: Implement the fake server and acceptance clients**

Return only fixture data needed by tests, including nested secrets and poisoned logs. Ensure the fake server can count requests so tests prove no unauthorized API request occurs and no unsafe compatibility retry occurs.

- [ ] **Step 3: Write and test the multi-stage non-root image**

Builder stage compiles `--release`; runtime stage contains the binary, CA certificates, `/data`, and a non-root UID. Set `PYTHONUNBUFFERED` only for the fallback image, not Rust. Do not copy `.env`, source tokens, or Git metadata.

- [ ] **Step 4: Verify local container acceptance**

```bash
docker build -f Dockerfile.rust -t sniffr-coolify-mcp:acceptance .
docker run --rm --user 10001:10001 -p 18080:8080 \
  -e MCP_TRANSPORT=http \
  -e MCP_PUBLIC_URL=http://127.0.0.1:18080 \
  -e MCP_ALLOW_INSECURE_HTTP=true \
  -e COOLIFY_BASE_URL=http://host.docker.internal:19000 \
  -e COOLIFY_ACCESS_TOKEN=fixture-token \
  -v "$(pwd)/.tmp/oauth:/data" \
  sniffr-coolify-mcp:acceptance
```

Use only the fixture token shown above; never substitute a real token in the command or repository.

- [ ] **Step 5: Verify and commit**

```bash
cargo test --workspace
./scripts/smoke-rust.sh http://127.0.0.1:18080

git add Dockerfile.rust compose.rust.yaml .dockerignore scripts tests
 git commit -m "test: add Rust container and protocol acceptance harness"
```

---

### Task 11: Deploy to `mcp.social.dpdns.org` and complete remote acceptance

**Files:**
- Create: `deploy/README.md`
- Create: `deploy/remote-check.sh`
- Modify: `HOSTING.md` with the final host-specific procedure only after it succeeds.

**Interfaces:**
- Remote host: `sidney@77.90.40.213`.
- Public MCP URL: `https://mcp.social.dpdns.org/mcp`.
- Public health URL: `https://mcp.social.dpdns.org/healthz`.

- [ ] **Step 1: Perform read-only host discovery over SSH**

Run with `BatchMode=yes` and no secrets:

```bash
ssh -o BatchMode=yes sidney@77.90.40.213 'uname -a; docker --version; docker compose version; getent hosts mcp.social.dpdns.org'
```

Record whether Docker, Compose, DNS, and the existing HTTPS proxy are available. If the account cannot run Docker or the proxy is not configured, stop and report the exact missing prerequisite rather than attempting privileged changes.

- [ ] **Step 2: Transfer/build the release without secrets**

Build the image locally or on the host from the reviewed commit. Transfer only source/build artifacts and compose configuration. Inject `COOLIFY_BASE_URL`, `COOLIFY_ACCESS_TOKEN`, and any OAuth key through the host's secret mechanism; never place them in Git, a command argument, or a committed `.env` file.

- [ ] **Step 3: Start the service with persistent state**

Run the Rust container as non-root with port `8080`, `/data` persistence, `MCP_TRANSPORT=http`, `MCP_PUBLIC_URL=https://mcp.social.dpdns.org`, and the configured capability profile. Configure the existing proxy to route HTTPS to the container and health-check `/healthz`.

- [ ] **Step 4: Verify public health and OAuth discovery**

Run:

```bash
curl --fail --silent https://mcp.social.dpdns.org/healthz
curl --fail --silent https://mcp.social.dpdns.org/.well-known/oauth-protected-resource
curl --fail --silent https://mcp.social.dpdns.org/.well-known/oauth-authorization-server
```

Expected: health is `ok`, protected-resource metadata names `/mcp`, authorization metadata names `https://mcp.social.dpdns.org`, and no response contains a Coolify token.

- [ ] **Step 5: Complete one real client read-only smoke test**

Use a real MCP client to complete OAuth and initialize against `https://mcp.social.dpdns.org/mcp`. Verify the complete tool roster and execute only a safe inventory/version call. Inspect logs for absence of token, password, env value, and raw response bodies.

- [ ] **Step 6: Document rollback and commit deployment notes**

Record image tag, health output, tool count, OAuth persistence path, and rollback command without recording secrets. Keep the Python local fallback documented. Commit only non-secret deployment notes:

```bash
git add deploy HOSTING.md
git commit -m "ops: document hosted Rust MCP deployment"
```

---

## Final verification checklist

Run after all tasks and before claiming completion:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
git diff --check
git status --short
```

Then run the local container acceptance harness and the remote checks from Task 11. Report the exact pass/fail output, the deployed commit/image tag, the public health result, the verified default tool count, and any remaining limitation. Never claim the hosted server works based only on compilation or a successful Docker build.
