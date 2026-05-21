# Change: PD Long Context Admission

## Why

The default-off scoped PD serving MVP has passed for the approved topology:
one Mac coordinator/router/decode worker and one PGX prefill/export worker.
The hardening/regression change has also completed local coverage for normal
path compatibility, Skippy split compatibility, lifecycle cleanup, streaming
semantics, telemetry privacy, status/capability fields, and `inflight_limit=1`
admission behavior.

Manual long prompt testing exposed the next production-readiness blocker:
the PGX stage currently uses a bounded prefill batch, observed as
`n_batch=2048`, and very long inputs can exceed that prefill capacity. When a
request is admitted into the PD path anyway, the PGX prefill process may exit.
The Mac router then observes a downstream handoff read failure such as:

```text
Chat stream failed: failed to fill whole buffer
```

That failure is too late in the request lifecycle. The router should decide
whether a request is safe for the scoped PD path before PGX prefill starts.

Carry-forward evidence:

- `openspec/specs/pd-disaggregated-serving-mvp/spec.md`
- `openspec/specs/pd-disaggregated-serving-hardening/spec.md`
- `openspec/changes/archive/2026-05-20-pd-disaggregated-serving-mvp/reports/mvp-validation-report.md`
- `docs/PD-detach/phase-2/PREFILL_DECODE_REQUIREMENTS.zh.md`
- `docs/PD-detach/phase-3/VALIDATION_PLAN.zh.md`
- `docs/PD-detach/phase-3/ROLE_AND_SCHEDULING.zh.md`
- `docs/PD-detach/phase-3/KV_HANDOFF_DESIGN.zh.md`

## What Changes

Add an admission gate before a request enters the PD serving lane. The gate
uses prompt token count, context limits, prefill batch limits, and estimated KV
handoff bytes to decide whether the request may proceed to PGX prefill.

The request flow becomes:

```text
/v1/chat/completions
  -> PD enabled and eligible?
  -> coordinator tokenization
  -> long-context admission gate
  -> admit to PGX prefill/export
     OR pre-content fallback/rejection
```

The change shall include:

- token counting before PGX prefill;
- `max_prompt_tokens`, `max_prefill_batch`, `max_ctx_size`,
  `max_handoff_bytes`, or equivalent admission policy inputs;
- conservative safe behavior when admission policy data is missing;
- pre-content fallback or documented pre-content rejection for requests that
  exceed the policy;
- sanitized telemetry/status for admission decisions:
  - `pd.admission.result`
  - `pd.admission.reason`
  - `pd.prompt_token_count`
  - `pd.estimated_kv_bytes`
  - `pd.max_prompt_tokens`
  - `pd.max_prefill_batch`
- local tests for below-threshold, exactly-at-threshold, above-threshold,
  missing-config, normal path, and Skippy split behavior;
- a manual smoke/runbook that verifies near-threshold and over-threshold prompts
  do not crash the PGX prefill process.

## Scope

In scope:

- PD path admission gate.
- Coordinator-side prompt token count calculation.
- Admission configuration or equivalent runtime policy fields.
- Estimated KV handoff byte calculation using explicit configuration or
  carry-forward measured bytes-per-token data.
- Pre-content fallback or documented pre-content rejection when limits are
  exceeded.
- Sanitized telemetry/status for admission result and limits.
- Local regression tests.
- Manual foreground smoke/runbook for long-context admission behavior.

Out of scope:

- Chunked prefill.
- Incremental KV transfer.
- KV compression.
- Multiple workers.
- Automatic placement.
- Production scheduler.
- Long-context performance optimization guarantees.
- Changing the default-off PD MVP policy.
- Silently sending over-limit long inputs into the PD path.

## Impact

This proposal is docs/spec only. It does not modify business code, does not
apply the change, does not run tests, and does not start local or remote
validation processes.

When applied later, the change is expected to touch PD serving configuration,
OpenAI ingress/coordinator admission logic, telemetry/status reporting, local
tests, and an operator smoke/runbook. It must preserve existing normal routing
and Skippy split serving when PD is disabled or when a request is not admitted
to PD.
