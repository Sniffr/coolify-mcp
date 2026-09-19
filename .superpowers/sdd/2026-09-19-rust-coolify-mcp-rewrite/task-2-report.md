# Task 2 report

## Changed files

Committed in `5f2f3aec792cc85be7a9edfb8d956703625e05f3`:

- `crates/coolify-api/Cargo.toml` — reqwest/rustls, URL, serde, and error dependencies.
- `crates/coolify-api/src/lib.rs` — public crate exports.
- `crates/coolify-api/src/config.rs` — environment precedence, URL validation/normalization, defaults, and redacted configuration errors.
- `crates/coolify-api/src/token_source.rs` — inline/file token sources, refresh/current behavior, trailing whitespace trimming, and redacted Debug.
- `crates/coolify-api/src/error.rs` — typed configuration/transport/HTTP/decode errors, bounded response bodies, Retry-After retention, and redacted Debug.
- `crates/coolify-api/src/client.rs` — reqwest/rustls client foundation, bearer authentication, reserved-header protection, JSON/text requests, version request, and structured HTTP errors.
- `crates/coolify-api/tests/config_tests.rs` — environment precedence and safe error tests.
- `crates/coolify-api/tests/token_source_tests.rs` — token loading, trimming, refresh, and redaction tests.
- `crates/coolify-api/tests/client_error_tests.rs` — bounded HTTP error metadata and redaction tests.

## TDD evidence

### Red

Command:

```text
cargo test -p coolify-api --test config_tests
```

Observed expected failure before implementation:

```text
error[E0432]: unresolved imports `coolify_api::config_from_env`, `coolify_api::ConfigError`
no `ConfigError` in the root
no `config_from_env` in the root
error: could not compile `coolify-api` (test "config_tests") due to 1 previous error
```

### Green

After implementing the minimum configuration/token/error/client foundation:

```text
cargo test -p coolify-api
```

Result: 7 tests passed, 0 failed (including 2 client-error, 3 config, and 2 token-source tests; doc tests passed).

## Verification commands/output

```text
cargo fmt --all
```

Completed successfully.

```text
cargo test -p coolify-api
```

Result: 7 tests passed, 0 failed; doc tests passed.

```text
cargo test --workspace
```

Result: all workspace unit, integration, and doc tests passed; workspace smoke test passed.

```text
cargo clippy -p coolify-api --all-targets -- -D warnings
```

Completed successfully with no warnings.

## Commit

- `5f2f3aec792cc85be7a9edfb8d956703625e05f3` — `feat: add typed Coolify client foundation`

## Concerns

- `Cargo.lock` was generated/updated by dependency resolution but was intentionally not included because the task’s specified commit command stages only `crates/coolify-api`.
- The client foundation currently uses direct reqwest transport; injected executor/local-server coverage can be added by the later client integration work without changing the public configuration/token/error boundaries.

## Fix round (review findings)

Implemented in the focused follow-up commit for this fix round:

- Redacted HTTP response bodies containing the configured bearer token before constructing HTTP errors; `CoolifyApiError` now emits only safe generic messages in both `Display` and `Debug`, while retaining status and `Retry-After` metadata.
- Added byte-bounded response reading for JSON, text, and HTTP error bodies. UTF-8 truncation occurs only at a valid character boundary; JSON over the bound returns a decode error.
- Added local TCP-server integration coverage for JSON/text/empty responses, content-type gating, protected Authorization/Content-Type headers, custom-header retention, statuses 401/403/404/405/422/429/500, Retry-After, and multibyte/oversized bodies.
- Added environment-constructor tests proving token-file precedence and empty canonical URL/token fallback to legacy aliases.
- Removed the unused `_serialize` helper.
- Added the root `Cargo.lock` dependency resolution update, including the test-only Tokio dependency.

### Fix-round TDD evidence

Red command after adding the regression/integration tests and before production fixes:

```text
cargo test -p coolify-api --test client_error_tests --test config_tests
```

Observed failures included secret-bearing HTTP error display and all client integration cases failing against the existing unbounded/untested implementation; the first run also exposed a test-server read-to-EOF setup issue, which was corrected before the implementation green run.

### Fix-round verification

```text
cargo fmt --all
```

Completed successfully.

```text
cargo test --workspace
```

Result: all workspace unit, integration, and doc tests passed. `coolify-api` integration tests: 6 passed; configuration tests: 5 passed; token-source tests: 2 passed; workspace smoke test passed.

```text
cargo clippy --workspace --all-targets -- -D warnings
```

Result: completed successfully with no warnings.

```text
git status --short
```

Before the focused commit, only the intended source/tests/Cargo.toml changes and root `Cargo.lock` were modified; generated `target/` output was removed.
