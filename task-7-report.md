

# Fix Round 1 Addendum

Addressed all Critical/Important findings:

- Client secrets, authorization codes, access tokens, and refresh tokens are represented in persisted state only by SHA-256 digests; maps use digest keys and persisted JSON tests assert issued secrets/codes are absent.
- Authorization, token exchange, bearer verification, and refresh flows require the configured canonical HTTPS `/mcp` resource.
- Secret/digest comparisons use constant-time comparison helpers.
- Added provider-keyed authenticated state tokens carrying client, redirect, resource, expiry, and nonce claims; tamper and replay are rejected.
- Redirect matching rejects malformed/hostless/userinfo/query/fragment URLs, permits port relaxation only for HTTP loopback, and requires exact HTTPS ports.
- `with_store` enables automatic atomic mode-600 persistence on all state mutations; corrupt state recovery remains degraded/clean.
- Added persistence, signed-state, replay, and redirect attack coverage.

Verification for this round:

- `cargo fmt --all` — passed
- `cargo test -p oauth` — passed (7 integration tests)
- `cargo clippy -p oauth --all-targets -- -D warnings` — passed
- `cargo test --workspace` — passed
