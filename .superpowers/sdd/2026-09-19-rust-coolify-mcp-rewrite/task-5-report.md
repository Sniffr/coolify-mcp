
## Continuation 2

Removed all unsupported-handler branches from the public 45-tool dispatch. Added the sanitized typed `CoolifyClient::request_value` boundary for action-specific projections and implemented real route-backed dispatch for configuration/private keys/cloud tokens/GitHub/Hetzner/teams/scheduled tasks/docs/diagnostics, server child resources, backups, tags/storages, control, bulk environment updates, emergency stop, project redeploy/restart, and all remaining grouped application/database/service/deployment actions. Added deployment wait routing through bounded client polling and preserved log bounds and API sanitation. Fleet-only `list_instances` remains excluded.

Verification completed:
- `cargo test -p mcp-tools` — passed (4 tests)
- `cargo test --workspace` — passed
- `cargo fmt --all` — passed
- `cargo clippy --workspace --all-targets -- -D warnings` — passed
- `grep unsupported/not_implemented crates/mcp-tools/src` — no matches

Concern: endpoint families without dedicated upstream response structs use the centralized sanitized `request_value` boundary; route construction, authorization, status/error handling, and body bounds remain in coolify-api rather than handlers.
