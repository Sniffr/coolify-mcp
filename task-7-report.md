

# Fix Round 2 Addendum

- Refresh now checks grant-family revocation before acceptance. Reuse of a rotated/invalid refresh token revokes the entire family and all family tokens fail afterward.
- All automatic mutation paths snapshot in-memory state and restore it if atomic persistence fails, preventing memory/disk replay divergence.
- Added deterministic `/dev/null` persistence-failure rollback coverage.
- Added explicit invalid client-secret exchange and attacker resource-host authorization tests.
- Preserved authenticated state, hash-only persistence, constant-time comparisons, atomic replacement, and mode-600 permissions.

Verification:

- `cargo fmt --all` — passed
- `cargo test -p oauth` — passed (10 integration tests)
- `cargo clippy -p oauth --all-targets -- -D warnings` — passed
- `cargo test --workspace` — passed
