# Task 3 Report — Response Safety and Audit Policy

## Summary

Implemented recursive API-boundary masking, nonce-safe untrusted output framing, capability profiles, and chained SHA-256 audit logging. Integrated default sanitization into `coolify-api::CoolifyClient::request_json` while preserving bounded response handling and existing error behavior.

## TDD red/green evidence

### Red

After writing the requested masking/framing/policy/audit integration tests and before production implementations:

```text
$ cargo test -p safety
error[E0432]: unresolved imports `safety::AuditEvent`, `safety::AuditLogger`
error[E0432]: unresolved imports `safety::frame_untrusted`, `safety::sanitize_json`
error[E0432]: unresolved imports `safety::allows`, `safety::default_profile_for_transport`, `safety::Action`, `safety::CapabilityProfile`
error: could not compile `safety` due to previous errors
```

This was the expected feature-missing red state. The test files also initially required the production crate's `serde_json` dependency, which was added as part of the implementation.

### Green

Focused safety tests after implementation:

```text
$ cargo test -p safety
5 passed; 0 failed
```

Coolify API regression tests:

```text
$ cargo test -p coolify-api
14 passed; 0 failed
```

## Implementation details

- `sanitize_json` recursively traverses objects and arrays, masks the closed set of always-secret fields, masks the broader sensitive set by default, preserves nulls, and leaves non-secret metadata unchanged.
- `frame_untrusted` uses a caller nonce (or generates a random nonce when empty), normalizes/replaces case-insensitive forged boundary phrases, and emits one explicit begin/end boundary.
- `CapabilityProfile` supports `ReadOnly`, `Operations`, and `Admin`; read actions are universally allowed, HTTP defaults to read-only, and stdio defaults to operations compatibility.
- `AuditLogger` serializes only safe event fields, hashes `previous_hash || canonical_event`, writes one JSON line per event, and chains the previous digest in memory. Request bodies and response values are not represented by the event type.
- JSON API responses are parsed as `serde_json::Value`, sanitized with `reveal=false`, then decoded into the requested model. Bounded reads, content-type checks, and existing error redaction remain intact.

## Verification

```text
$ cargo fmt --all -- --check
PASS (after formatting)

$ cargo clippy --workspace --all-targets -- -D warnings
Finished successfully; no warnings

$ cargo test --workspace
All workspace unit, integration, and doc tests passed: 22 passed; 0 failed
```

## Changed files

- `crates/safety/Cargo.toml`
- `crates/safety/src/lib.rs`
- `crates/safety/src/masking.rs`
- `crates/safety/src/untrusted.rs`
- `crates/safety/src/policy.rs`
- `crates/safety/src/audit.rs`
- `crates/safety/tests/masking_tests.rs`
- `crates/safety/tests/policy_tests.rs`
- `crates/safety/tests/audit_tests.rs`
- `crates/coolify-api/Cargo.toml`
- `crates/coolify-api/src/client.rs`
- `Cargo.lock`

## Commit

- `06eec67d14e5f57b88b81f0e2caed302e2609a10` — `feat: add response safety and tamper-evident audit policy`

## Concerns

- The `reveal` argument is intentionally an explicit low-level policy input; no higher-level server confirmation/policy plumbing exists yet and remains for later tasks.
- Typed response models that declare a secret field as a non-string scalar could reject the masked `"***"` value; current client models/tests are unaffected, and later model design should represent masked secrets as strings or optional values.
- Audit chain continuity currently starts fresh on process restart; persistent rotation/restart verification is outside Task 3.
