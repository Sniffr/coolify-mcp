
## Fix round 4 continuation

Made the closed serde input authoritative through dispatch: `call_tool` deserializes once into `CommonInput`, validates it, serializes only that typed value for dispatch, and no longer dispatches the raw caller object. Removed arbitrary body/input/method escapes; request bodies are built only from named typed fields. Expanded schemas with exact action enums and required action fields for all action-bearing groups and fixed-action tools. Success truncation now preserves `_actions` and `_pagination`, while structured errors remain stable JSON envelopes.

Verification:
- `cargo test -p mcp-tools` — passed (6 tests)
- `cargo test --workspace` — passed
- `cargo fmt --all` — passed
- `cargo clippy --workspace --all-targets -- -D warnings` — passed
