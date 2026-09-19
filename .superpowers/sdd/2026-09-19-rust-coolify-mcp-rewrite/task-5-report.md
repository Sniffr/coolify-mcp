
## Fix round 1 continuation

Addressed review findings:
- Removed the generic public dispatch fallback; unregistered tools fail before API access.
- Added closed common-argument validation, per-group action allowlists, method validation, bounded `per_page`, and audit emission for rejected calls.
- Added bounded success envelopes (200KB), structured bounded deployment wait projections with terminal status handling, timeout `timed_out` plus `next_action`, and bounded failed deployment log tails.
- Changed schemas to reject unknown top-level properties and added independent exact-45 fixture coverage.
- Replaced empty handler files with explicit domain ownership/validation functions.
- Preserved centralized API sanitation through `request_value` and encoded child route methods.

Verification:
- `cargo test -p mcp-tools` — passed (5 tests)
- `cargo test --workspace` — passed
- `cargo fmt --all` — passed
- `cargo clippy --workspace --all-targets -- -D warnings` — passed

Remaining concern: schemas currently model the shared closed argument envelope rather than generating distinct serde-derived schemas for every action variant; arbitrary endpoint request payloads remain confined to the `body` field and are sanitized at the API boundary.
