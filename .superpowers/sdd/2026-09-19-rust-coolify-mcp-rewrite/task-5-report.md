
## Fix round 2 continuation

Closed the generic-route bypass in the public dispatch: unknown tools now fail before dispatch, known grouped tools have explicit action allowlists, caller-supplied method/body/input fields are rejected, methods are selected internally by tool/action contracts, and generic collection routing is now a private fixed-path helper used only with literal route roots owned by each match arm. Replaced public arbitrary `CommonInput` escape hatches with a closed typed serde input struct and expanded advertised schemas with closed properties and action enums. Added bounded success envelopes and independent schema/roster tests; deployment wait projections retain terminal/timeout/failure-tail semantics.

Verification:
- `cargo test -p mcp-tools` — passed (6 tests)
- `cargo test --workspace` — passed
- `cargo fmt --all` — passed
- `cargo clippy --workspace --all-targets -- -D warnings` — passed

Concern: full network-backed mcp-tools fake-server integration coverage for every representative route remains limited by the current test harness; existing coolify-api fake-server route tests cover encoded child routes and the new closed-contract tests cover dispatch validation before requests.
