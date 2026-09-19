
## Fix round 5 continuation

Made serialized typed inputs presence-aware with `skip_serializing_if`, so request bodies contain only supplied fields. Pagination metadata is rewritten from validated typed page/per_page values and survives bounded truncation. Environments now dispatch explicit project-environment routes with fixed action-derived methods and typed environment names. Existing closed schemas and action contracts remain enforced.

Verification:
- `cargo test -p mcp-tools` — passed
- `cargo test --workspace` — passed
- `cargo fmt --all` — passed
- `cargo clippy --workspace --all-targets -- -D warnings` — passed

Concern: the repository's existing fake-server harness is concentrated in coolify-api; mcp-tools currently has contract/schema tests but not a new independent server fixture for every listed domain in this round.
