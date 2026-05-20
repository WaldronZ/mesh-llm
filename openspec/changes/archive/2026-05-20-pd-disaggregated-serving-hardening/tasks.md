# Tasks: PD Disaggregated Serving Hardening

Status note: local hardening implementation and local validation are complete.
This change intentionally did not start Mac/PGX foreground validation or rerun
the full MVP positive suite.

## 1. Normal Path Regression

- [x] 1.1 Add tests proving default normal route behavior is unchanged when PD is disabled.
- [x] 1.2 Add tests proving missing or incomplete PD MVP config does not silently alter the normal route.
- [x] 1.3 Add tests for PD-enabled but request-ineligible behavior.
- [x] 1.4 Confirm existing status/API clients remain compatible with additive PD fields.

## 2. Skippy Split Serving Regression

- [x] 2.1 Add or run regression coverage for existing Skippy split serving where shared code was touched.
- [x] 2.2 Confirm PD MVP test fault hooks cannot activate outside explicit MVP double opt-in.
- [x] 2.3 Confirm binary transport additions do not break existing split-serving message flow.
- [x] 2.4 Document any split-serving smoke check that cannot be automated locally.

## 3. Lifecycle Cleanup

- [x] 3.1 Add success cleanup tests for request/session/admission state.
- [x] 3.2 Add manifest failure cleanup tests.
- [x] 3.3 Add pre-content fallback cleanup tests.
- [x] 3.4 Add post-content failure cleanup tests.
- [x] 3.5 Add timeout cleanup tests.
- [x] 3.6 Add client cancellation cleanup tests.
- [x] 3.7 Add worker connection failure before content cleanup tests.

## 4. OpenAI-compatible Streaming Tests

- [x] 4.1 Add streaming success response-shape tests.
- [x] 4.2 Add pre-content fallback streaming tests.
- [x] 4.3 Add post-content failure streaming tests.
- [x] 4.4 Assert post-content failure does not produce duplicate, reordered, or mixed-path output.
- [x] 4.5 Assert streaming tests do not persist prompt text, complete token arrays, generated content, or KV payload bytes.

## 5. Telemetry Privacy And Metric Presence

- [x] 5.1 Add tests for required timing metric presence.
- [x] 5.2 Add tests for result, fallback reason, and failure phase presence.
- [x] 5.3 Add tests that telemetry/reporting excludes prompt text, full token arrays, KV payload contents, credentials, private paths, and private machine details.
- [x] 5.4 Add tests that isolated transfer timing is only marked isolated when export/import work is excluded.

## 6. Status And Capability Hardening

- [x] 6.1 Add sanitized worker health status fields.
- [x] 6.2 Add sanitized capacity and `inflight_limit=1` status fields.
- [x] 6.3 Add sanitized compatibility status for model/tokenizer/chat-template/KV ABI readiness.
- [x] 6.4 Add backward-compatible serialization tests for status/capability additions.

## 7. Busy / Admission Behavior

- [x] 7.1 Define the explicit busy policy for `inflight_limit=1`.
- [x] 7.2 Add tests for second eligible request while one PD request is admitted.
- [x] 7.3 Assert busy behavior uses normal-route fallback or documented pre-content rejection.
- [x] 7.4 Assert busy/admission outcome records only sanitized reason codes.
- [x] 7.5 Assert admission capacity is restored after completion, failure, fallback, timeout, and cancellation.

## 8. Docs / Runbook Polish

- [x] 8.1 Update runbook notes for hardening checks if implementation changes operator steps.
- [x] 8.2 Update report template or validation checklist if new sanitized status/telemetry fields are added.
- [x] 8.3 Record any manual-only smoke checks and why they are not automated.

## 9. Validation

- [x] 9.1 Run `cargo fmt --all -- --check`.
- [x] 9.2 Run relevant `cargo test` commands serially.
- [x] 9.3 Run relevant `cargo check` commands serially.
- [x] 9.4 Run `openspec validate pd-disaggregated-serving-hardening --strict`.
