
## Fix round 3 continuation

Added explicit closed action allowlists and required actions for all action-bearing configuration/collection tools, including backups, storages, tags, credentials, scheduled tasks, environments, env vars, and bulk updates. Read-classified routes use fixed GET methods. Dispatch now deserializes every request through the closed `CommonInput` serde struct; instance and arbitrary method/body/input escape hatches are rejected. Typed body construction only copies validated named fields. Result success envelopes now consistently include bounded `_actions` and bounded pagination metadata for arrays; errors are structured JSON with stable code/message/details and `is_error`. Added closed-schema action enum tests.

Verification:
- `cargo test -p mcp-tools` — passed (6 tests)
- `cargo test --workspace` — passed
- `cargo fmt --all -- --check` — passed
- `cargo clippy --workspace --all-targets -- -D warnings` — passed
