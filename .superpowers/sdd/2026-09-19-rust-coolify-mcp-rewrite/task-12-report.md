# Final fix wave report

## Changes

- Hosted Streamable HTTP now opens an append-only mode-600 audit log, resumes its hash chain from the last record, flushes every event, and records tool name, client id, safe resource id, outcome/status, and duration. Unauthorized, malformed, content-negotiation, and tool-result failures are recorded without request arguments, responses, tokens, or secrets.
- Added an end-to-end transport test proving a rejected HTTP tool call writes an audit record while excluding arguments and a secret.
- OAuth state signing keys are durably generated beside `MCP_OAUTH_STATE_FILE` as an atomic mode-600 `.key` file, so restarts preserve state validation. Deployment docs require mounting both files and document `MCP_AUDIT_LOG`.
- Acceptance remains compatible when the state path is overridden: the default audit path follows the state directory.

## Verification

- `cargo fmt --all` — passed
- `cargo test --workspace` — passed
- `cargo clippy --workspace --all-targets -- -D warnings` — passed
- `./scripts/acceptance-rust.sh` — passed
- `docker build -f Dockerfile.rust -t coolify-mcp-rust-fix .` — passed
- `git diff --check` — passed
- `./scripts/smoke-rust.sh` — not run successfully because no service was listening on `127.0.0.1:8080`; the script assumes a running deployment.
