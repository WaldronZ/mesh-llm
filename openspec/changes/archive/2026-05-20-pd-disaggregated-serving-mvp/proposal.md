# Change: PD Disaggregated Serving MVP

## Why

Phase 2 and Phase 3 define the target as heterogeneous Prefill/Decode
separation: PGX performs prompt/prefill work, Mac Studio imports the resulting
KV/decode state and performs token-by-token decode, while external callers keep
using the OpenAI-compatible API.

The validation sequence has now closed the minimum feasibility blockers needed
to propose a scoped MVP:

- `pd-kv-handoff-spike` records `critical_native_handoff_result: pass` and
  `prompt_suite_handoff_result: pass`; PGX prefill/export -> Mac import/decode
  native handoff is feasible for the tested prompt suite.
- `pd-router-validation` records a real validation-only live path through
  `/v1/chat/completions -> PD router validation -> PGX prefill/export -> Mac
  import/decode`; positive prompts, manifest mismatch fail-closed, and
  pre-token fallback passed, while isolated timing and stricter post-content
  failure semantics remained open.
- `pd-router-validation-followup` records `result: pass` and
  `recommendation: proceed_to_pd_mvp`; isolated network-only KV transfer timing
  and post-content-token failure semantics both passed.

Evidence inputs:

- `docs/PD-detach/phase-2/PREFILL_DECODE_REQUIREMENTS.zh.md`
- `docs/PD-detach/phase-2/PHASE_2_EXIT_REVIEW.zh.md`
- `docs/PD-detach/phase-3/TARGET_ARCHITECTURE.zh.md`
- `docs/PD-detach/phase-3/PD_DATA_FLOW.zh.md`
- `docs/PD-detach/phase-3/KV_HANDOFF_DESIGN.zh.md`
- `docs/PD-detach/phase-3/API_AND_PROTOCOL.zh.md`
- `docs/PD-detach/phase-3/ROLE_AND_SCHEDULING.zh.md`
- `docs/PD-detach/phase-3/VALIDATION_PLAN.zh.md`
- `openspec/changes/pd-kv-handoff-spike/reports/spike-report.md`
- `openspec/changes/pd-router-validation/reports/router-validation-report.md`
- `openspec/changes/pd-router-validation/reports/router-validation-report.json`
- `openspec/changes/pd-router-validation-followup/reports/router-validation-followup-report.md`
- `openspec/changes/pd-router-validation-followup/reports/router-validation-followup-report.json`

## What Changes

Define a bounded PD serving MVP that graduates the validation-only path into an
operator-enabled, default-off serving lane:

```text
OpenAI-compatible /v1/chat/completions
  -> Mac coordinator/router
  -> one configured PGX prefill/export worker
  -> native KV/decode-state handoff with manifest validation
  -> Mac import/decode
  -> OpenAI-compatible response
```

The MVP shall include:

- Single Mac coordinator/router/decode worker.
- Single configured PGX prefill worker.
- Single request in-flight or strictly bounded MVP concurrency.
- Explicit configuration gate; disabled by default.
- OpenAI-compatible `/v1/chat/completions` path.
- Coordinator-owned tokenization; PGX receives token IDs plus metadata.
- Manifest validation, fail-closed behavior, and defined fallback semantics.
- Network timing telemetry for export, isolated transfer, import, TTFT, and
  decode throughput.
- Report and runbook for foreground machine validation.
- Regression guardrails so the normal path and existing Skippy split path are
  not broken.

## Scope

In scope:

- Harden the existing validation-only path into a scoped MVP path.
- Keep the MVP disabled unless explicitly configured.
- Require explicit prefill/decode role configuration.
- Preserve existing OpenAI-compatible external API behavior.
- Enforce model artifact, tokenizer metadata, chat template, dtype/layout,
  runtime ABI, position, byte count, and checksum validation.
- Allow fallback before the first client-visible assistant content token.
- Block transparent fallback after a client-visible assistant content token.
- Emit sanitized lifecycle status and telemetry.
- Produce a runbook and validation report template.
- Run local tests plus foreground Mac/PGX validation before declaring the MVP
  complete.

Out of scope:

- Multiple decode workers.
- Multiple PGX automatic placement.
- Production multi-request concurrency.
- Public mesh cross-owner PD.
- KV compression or incremental transfer.
- Low precision KV compatibility.
- Automatic model selection.
- Production-grade scheduler.
- Enabling PD by default for external callers.
- Re-proving native KV handoff feasibility already covered by
  `pd-kv-handoff-spike`.
- Re-running broad router validation already covered by `pd-router-validation`
  and `pd-router-validation-followup`, except as MVP regression evidence.

## Impact

This proposal itself is docs/spec only. It does not modify business code, does
not apply the change, does not alter runtime defaults, and does not start remote
deployment.

When applied later, the change is expected to touch serving/router, PD
configuration, manifest validation, telemetry/reporting, tests, and runbook
documentation. It must preserve existing normal routing and Skippy split serving
when PD is disabled or ineligible.
