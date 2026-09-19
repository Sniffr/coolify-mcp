# Task 6 report

Implemented fleet routing foundations, prompts, resources, and documentation search.

## Changes

- Added `InstanceRegistry`/`Instance` parsing for `COOLIFY_INSTANCES` JSON (array and name-keyed object forms), URL/token validation, redacted debug/error behavior, and request-scoped instance selection.
- Fleet registration now adds only `list_instances` and adds `instance` to other tool schemas; single-instance registration remains unchanged. `list_instances` has no required selector.
- Removed blanket pagination envelopes; pagination metadata is now emitted only for the endpoint families that accept page/per_page (`list_applications` and `list_servers`).
- Added prompt registration for `troubleshoot_application`, `explain_failed_deploy`, and `estate_health`, filtering prompts against the actual registered tool roster.
- Added read-only resource metadata for `coolify://overview` and `coolify://application/{uuid}`.
- Added bounded, deterministic-ranked embedded docs search with `safety::frame_untrusted` wrapping for returned documentation text.
- Framed all log outputs in tool dispatch, including failed-deployment log tails, with `safety::frame_untrusted`.
- Added attributed/regeneration-noted `src/data/coolify-docs.json` fixture.
- Added fleet and prompt/resource/search call-level contract tests; updated roster expectation for fleet-only `list_instances`.

## Verification

- `cargo test -p mcp-tools` — passed.
- `cargo test --workspace` — passed.
- `cargo fmt --all -- --check` — passed.
- `cargo clippy --workspace --all-targets -- -D warnings` — passed.

## Concerns / follow-up

- OAuth and transport were intentionally not implemented.
- The current `ToolContext` already carries the selected request-scoped client/name; higher-level server wiring to construct contexts from a registry remains for the transport/server task. `list_instances` currently returns an empty projection at the low-level dispatcher because registry ownership belongs to that wiring layer.
