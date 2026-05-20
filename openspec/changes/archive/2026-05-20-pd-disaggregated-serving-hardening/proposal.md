# Change: PD Disaggregated Serving Hardening

## Why

`pd-disaggregated-serving-mvp` has reached scoped MVP pass for the approved
single Mac coordinator/router/decode worker plus single PGX prefill/export
worker topology. Its validation report records:

- real `--pd-serving-mvp` positive serving through `/v1/chat/completions`;
- manifest mismatch fail-closed before Mac import/decode continuation;
- pre-content fallback;
- post-content failure after an assistant content delta without transparent
  fallback;
- isolated network timing, sanitized telemetry, and foreground cleanup.

Carry-forward evidence:

- `openspec/changes/pd-disaggregated-serving-mvp/reports/mvp-validation-report.md`
- `openspec/changes/pd-disaggregated-serving-mvp/reports/mvp-validation-report.json`
- `openspec/changes/pd-disaggregated-serving-mvp/tasks.md`
- `openspec/changes/pd-router-validation-followup/reports/router-validation-followup-report.md`
- `openspec/changes/pd-router-validation-followup/reports/router-validation-followup-report.json`
- `openspec/changes/pd-kv-handoff-spike/reports/spike-report.md`

The remaining work is not to re-prove PGX -> Mac native handoff or broaden the
MVP. It is to harden the scoped MVP with regression coverage, lifecycle cleanup
behavior, status/capability polish, and runbook/reporting refinements before
larger production-oriented changes are considered.

## What Changes

This change carries forward the hardening/regression tasks deferred from
`pd-disaggregated-serving-mvp`:

- normal path regression coverage when PD is disabled or ineligible;
- existing Skippy split serving regression coverage;
- lifecycle cleanup for success, failure, fallback, timeout, and cancellation;
- OpenAI-compatible streaming success and failure tests;
- telemetry privacy and required metric presence tests;
- sanitized status/capability hardening;
- `inflight_limit=1` busy/admission behavior;
- docs and runbook polish where needed.

## Scope

In scope:

- Add local tests, smoke checks, or narrowly scoped validation harnesses that
  prove the scoped MVP does not break existing normal routing or Skippy split
  serving.
- Add lifecycle cleanup assertions for request state, in-flight admission state,
  prefill/decode session state, and fallback/failure paths.
- Add streaming tests for success, pre-content fallback, and post-content
  failure semantics.
- Add telemetry privacy and required metric presence tests.
- Harden additive status/capability fields without exposing prompt text, full
  token arrays, KV payload contents, credentials, private paths, or private
  machine details.
- Document any operator-facing runbook/report improvements needed by the scoped
  MVP.

Out of scope:

- Multiple decode workers.
- Multiple PGX automatic placement.
- Production multi-request concurrency.
- Automatic scheduler.
- KV compression or incremental transfer.
- Public mesh cross-owner PD.
- Re-proving native KV handoff.
- Re-running the full MVP foreground positive suite unless a specific
  hardening item requires a minimal verification.
- Expanding the scoped MVP into production-grade serving.

## Impact

This proposal is docs/spec only. It does not apply the change, modify business
code, change runtime defaults, modify model/tokenizer/private configuration, or
start any local or remote validation process.

When applied later, expected changes are limited to regression tests, lifecycle
cleanup hardening, status/telemetry assertions, and runbook/report polish around
the already scoped MVP path.
