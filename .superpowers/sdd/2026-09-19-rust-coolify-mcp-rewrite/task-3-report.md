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

## Fix Round 1 (review findings)

### Findings addressed

1. Added `sanitize_text` at the API boundary and applied it to `request_text`, preserving ordinary/version text while masking sensitive key/value patterns. Added client integration coverage for both sanitized JSON and text responses.
2. Changed `environment_variables` handling to preserve its object/array shape and metadata while masking nested `value`/`real_value` members.
3. Kept the existing `frame_untrusted(text, nonce)` signature for compatibility, but now accepts only safe nonce characters and generates a random internal nonce for unsafe/empty input. Payload boundary phrases are still replaced.
4. Documented audit canonicalization: serde struct-field JSON bytes are concatenated after UTF-8 bytes of the previous lowercase hex digest, then SHA-256 is computed. Added independent digest-change tests for previous hash, tool, outcome, and timestamp; audit events still have no request/response fields.
5. Added API-level client tests proving sanitation before model-facing JSON/text results.
6. Cached the framing regex with `OnceLock` rather than compiling it per call. The sensitive text regex is cached similarly.

### Fix-round TDD red evidence

```text
$ cargo test -p safety -p coolify-api
error[E0432]: unresolved import `safety::sanitize_text`
error: could not compile `safety` (test "masking_tests") due to previous error
```

This was the expected feature-missing failure for the new text-boundary test before implementing the fix.

### Fix-round green evidence

```text
$ cargo test -p safety -p coolify-api
coolify-api: 8 passed; 0 failed
safety: 11 passed; 0 failed
```

Final verification:

```text
$ cargo fmt --all -- --check
PASS

$ cargo clippy --workspace --all-targets -- -D warnings
Finished successfully; no warnings

$ cargo test --workspace
All workspace tests passed: 28 passed; 0 failed
```

Fix-round files include `crates/safety/src/masking.rs`, `crates/safety/src/untrusted.rs`, `crates/safety/src/audit.rs`, `crates/safety/src/lib.rs`, `crates/safety/tests/masking_tests.rs`, `crates/safety/tests/audit_tests.rs`, `crates/coolify-api/src/client.rs`, and `crates/coolify-api/tests/client_error_tests.rs`.

## Fix Round 2 (nonce isolation)

### TDD evidence

Added tests requiring two calls with the same supplied nonce to have independent delimiters and requiring malicious nonce text not to influence the generated boundary. Before implementation:

```text
$ cargo test -p safety
untrusted_frame_generates_independent_delimiters_and_neutralizes_payload_boundaries ... FAILED
assertion `left != right` failed
left: "[BEGIN UNTRUSTED LOG OUTPUT:caller-nonce]"
right: "[BEGIN UNTRUSTED LOG OUTPUT:caller-nonce]"
```

### Fix and verification

`frame_untrusted(text, _supplied_nonce)` now ignores the compatibility argument entirely and generates a fresh 128-bit random nonce on every call. The cached boundary regex continues to replace boundary-shaped payload text before the real delimiters are added. A compatibility comment documents why the unused parameter remains public.

```text
$ cargo test -p safety
11 passed; 0 failed

$ cargo fmt --all -- --check
PASS

$ cargo clippy --workspace --all-targets -- -D warnings
Finished successfully; no warnings

$ cargo test --workspace
All workspace tests passed: 28 passed; 0 failed
```
