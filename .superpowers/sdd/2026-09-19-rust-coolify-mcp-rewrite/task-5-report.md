
## Continuation (2026-09-19)

Implemented continuation commit: complete 45-entry runtime ToolSpec roster with title/schema/annotation/safety metadata, read-only filtering, action-level policy and fail-closed confirmation, audit hook invocation, structured error envelopes, bounded log projections, pagination envelope support, typed dispatch for the available Coolify resource methods, deployment/log routing, project/environment/application/database/service/server/system routes, shared `McpApplication` interface, and database/service child path encoding methods. Added exact roster/schema contract tests and excluded fleet-only `list_instances` pending Task 6.

Verification:
- `cargo test -p mcp-tools` — passed (4 tests)
- `cargo test --workspace` — passed
- `cargo fmt --all` — passed
- `cargo clippy --workspace --all-targets -- -D warnings` — passed after fixing redundant closures

Remaining concerns: the existing coolify-api surface does not yet expose every OpenAPI family (configuration/cloud/private/GitHub/diagnostic/docs/bulk variants), so those tools return structured unsupported-handler errors rather than placeholder successes. Full fake-server route contract coverage and deployment wait projection tests remain follow-up work; no prompts, fleet, or transport was implemented.
