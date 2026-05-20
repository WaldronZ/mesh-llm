# Tasks: PD Disaggregated Serving MVP

Status note: local implementation/validation is complete. Foreground Mac/PGX
machine validation passed for the approved single Mac coordinator/decode worker
plus single PGX prefill worker scope. Scoped MVP is pass; production-ready is
no. Hardening/regression items that were not completed directly in this change
were deferred to and resolved by `pd-disaggregated-serving-hardening`. See
`reports/mvp-validation-report.md`.

## 1. Harden Config / Default-off

- [x] 1.1 Define the MVP PD serving config surface with explicit enablement.
- [x] 1.2 Keep PD serving disabled by default for every existing entrypoint.
- [x] 1.3 Require explicit model artifact sha256, tokenizer metadata hash, and chat template hash.
- [x] 1.4 Require explicit prefill worker and decode/coordinator role configuration.
- [x] 1.5 Fail startup or mark PD unavailable when required MVP config is missing.
- [x] 1.6 Deferred/resolved by `pd-disaggregated-serving-hardening`; not completed directly in this change: add tests proving default normal route behavior is unchanged when PD is disabled.
- [x] 1.7 Deferred/resolved by `pd-disaggregated-serving-hardening`; not completed directly in this change: add tests proving existing Skippy split serving behavior is unchanged.

## 2. Role / Capability / Status

- [x] 2.1 Define sanitized MVP role states for coordinator, prefill worker, and decode worker.
- [x] 2.2 Add additive status fields for PD enabled/available/current state/last result.
- [x] 2.3 Deferred/resolved by `pd-disaggregated-serving-hardening`; not completed directly in this change: report worker health, capacity, and compatibility without exposing private paths or credentials.
- [x] 2.4 Expose current in-flight state and last fallback/failure reason.
- [x] 2.5 Add status serialization tests for backward-compatible additive behavior.

## 3. MVP Request Lifecycle

- [x] 3.1 Implement the scoped MVP route for OpenAI-compatible `/v1/chat/completions`.
- [x] 3.2 Use coordinator-owned request normalization and tokenization.
- [x] 3.3 Send token IDs plus metadata to the PGX prefill/export worker.
- [x] 3.4 Import validated native KV/decode state on the Mac decode worker.
- [x] 3.5 Stream decode output through the existing OpenAI-compatible response shape.
- [x] 3.6 Enforce single request in-flight or a strictly bounded MVP concurrency limit.
- [x] 3.7 Deferred/resolved by `pd-disaggregated-serving-hardening`; not completed directly in this change: clean up prefill/decode sessions on completion, failure, fallback, timeout, and cancellation.

## 4. Telemetry / Reporting

- [x] 4.1 Preserve isolated transfer timing boundaries proven by `pd-router-validation-followup`.
- [x] 4.2 Emit KV bytes, export, network read/write, isolated transfer, import, TTFT, and decode tok/s metrics.
- [x] 4.3 Emit sanitized fallback/failure result codes.
- [x] 4.4 Ensure telemetry/reporting excludes prompt text, full token arrays, KV payload contents, credentials, and private paths.
- [x] 4.5 Add a sanitized MVP validation report template.
- [x] 4.6 Deferred/resolved by `pd-disaggregated-serving-hardening`; not completed directly in this change: add tests for telemetry privacy and required metric presence.

## 5. Fallback / Error Semantics

- [x] 5.1 Validate manifest mismatches fail closed before Mac decode continuation.
- [x] 5.2 Allow normal-route fallback before a client-visible assistant content delta.
- [x] 5.3 Block transparent fallback after a client-visible assistant content delta.
- [x] 5.4 Emit explicit SSE error behavior or documented partial termination for post-content failures.
- [x] 5.5 Prevent duplicate, reordered, or mixed-path content after post-content failure.
- [x] 5.6 Record fallback/failure reason codes without sensitive data.
- [x] 5.7 Add a default-off, explicitly allowed MVP-safe test fault mechanism for true-machine failure validation.

## 6. Tests

- [x] 6.1 Add config default-off and missing-config tests.
- [x] 6.2 Add manifest positive and negative validation tests.
- [x] 6.3 Deferred/resolved by `pd-disaggregated-serving-hardening`; not completed directly in this change: add lifecycle admission/busy/fallback/cancel cleanup tests.
- [x] 6.4 Deferred/resolved by `pd-disaggregated-serving-hardening`; not completed directly in this change: add OpenAI-compatible streaming success tests.
- [x] 6.5 Deferred/resolved by `pd-disaggregated-serving-hardening`; not completed directly in this change: add pre-content fallback tests.
- [x] 6.6 Deferred/resolved by `pd-disaggregated-serving-hardening`; not completed directly in this change: add post-content failure tests.
- [x] 6.7 Deferred/resolved by `pd-disaggregated-serving-hardening`; not completed directly in this change: add normal path regression tests.
- [x] 6.8 Deferred/resolved by `pd-disaggregated-serving-hardening`; not completed directly in this change: add Skippy split path regression tests where the MVP integration touches shared code.
- [x] 6.9 Run relevant cargo checks/tests serially.
- [x] 6.10 Run `openspec validate pd-disaggregated-serving-mvp --strict`.
- [x] 6.11 Add local tests for MVP test fault default-off and explicit-allow guards.

## 7. Docs / Runbook

- [x] 7.1 Document MVP scope, non-goals, and default-off activation.
- [x] 7.2 Document required config fields and sanitized environment variable names.
- [x] 7.3 Document the request lifecycle and failure semantics.
- [x] 7.4 Document metrics and privacy rules.
- [x] 7.5 Produce a foreground validation runbook for one Mac coordinator/decode worker and one PGX prefill worker.
- [x] 7.6 Produce an MVP report template with pass/fail/inconclusive recommendation.

## 8. Foreground Machine Validation

- [x] 8.1 Build or stage current validation/MVP binaries for Mac and one PGX without modifying model/tokenizer/private config.
- [x] 8.2 Confirm model artifact sha256, tokenizer metadata hash, and chat template hash match.
- [x] 8.3 Check required ports read-only and pause if occupied.
- [x] 8.4 Start PGX prefill/export as a foreground observable process.
- [x] 8.5 Start Mac decode/import and OpenAI-compatible router as foreground observable processes.
- [x] 8.6 Run the positive prompt suite through `/v1/chat/completions`.
- [x] 8.7 Run manifest mismatch fail-closed validation.
- [x] 8.8 Run pre-content fallback validation.
- [x] 8.9 Run post-content failure validation.
- [x] 8.10 Record isolated network timing and decode metrics.
- [x] 8.11 Stop foreground processes and confirm ports release.
- [x] 8.12 Update the MVP validation report with sanitized evidence.
