# Change: PD Long Context Scaling

## Why

The default-off scoped PD serving MVP has passed for the approved single Mac
coordinator/router/decode worker plus single PGX prefill/export worker topology.
The follow-up long-context admission change prevents over-limit prompts from
entering PGX prefill and crashing the prefill process.

The next question is not whether to remove admission. It is how to scale the PD
lane safely beyond the current admission envelope.

Current evidence:

- the target GGUF artifact advertises `gemma4.context_length=262144`;
- the current true-machine PD runtime was validated with `ctx_size=8192`;
- the observed PGX prefill batch boundary is `n_batch=2048`;
- the current safe PD admission envelope is about 1800 prompt tokens;
- the current PD MVP path uses one-shot native KV/decode-state handoff;
- measured native KV payload size is about 900 KB per prompt token;
- measured isolated network transfer is about 115 MB/s.

At those measured rates, a direct 256k one-shot KV handoff may require more
than 200 GB of payload and tens of minutes of transfer time. Therefore 256k
context must remain a long-term feasibility target, not the next implementation
step.

Carry-forward evidence:

- `openspec/specs/pd-disaggregated-serving-mvp/spec.md`
- `openspec/specs/pd-disaggregated-serving-hardening/spec.md`
- `openspec/changes/pd-long-context-admission/reports/admission-smoke-report.md`
- `openspec/changes/archive/2026-05-20-pd-disaggregated-serving-mvp/reports/mvp-validation-report.md`
- `docs/PD-detach/phase-2/PREFILL_DECODE_REQUIREMENTS.zh.md`
- `docs/PD-detach/phase-3/KV_HANDOFF_DESIGN.zh.md`
- `docs/PD-detach/phase-3/ROLE_AND_SCHEDULING.zh.md`
- `docs/PD-detach/phase-3/VALIDATION_PLAN.zh.md`

## What Changes

Define and validate a staged long-context scaling plan for the PD lane:

1. recalibrate `estimated_kv_bytes_per_token` using measured PD handoff data;
2. clarify the relationship between `ctx_size`, `n_batch`,
   `max_prompt_tokens`, and `max_handoff_bytes`;
3. define Phase A for safe 4k/8k scaling;
4. define Phase B prerequisites for 32k chunked prefill;
5. define an admission ladder that evaluates token, context, prefill batch,
   KV bytes, memory, and network/SLA gates;
6. add a runbook and report shape for 4k/8k near-threshold and over-threshold
   smoke validation;
7. keep existing admission guard behavior intact until later changes prove a
   larger envelope is safe.

The change may include implementation work later for calibrated admission
policy, local tests, and smoke/reporting support, but it SHALL NOT directly
implement 256k context support.

## Scope

In scope:

- measurement model and bytes-per-token calibration;
- documented relationship among runtime context, prefill batch, prompt token
  cap, handoff byte cap, and requested generation budget;
- staged admission ladder design;
- Phase A 4k/8k safe scaling implementation or smoke plan;
- Phase B 32k chunked prefill prerequisites and design notes;
- local tests for calibrated admission and gate ordering;
- operator runbook/reporting for 4k/8k validation;
- optional foreground smoke that proves over-threshold prompts still do not
  reach PGX prefill.

Out of scope:

- directly implementing 256k context;
- KV compression;
- streaming or chunked KV handoff;
- multi-worker placement;
- production scheduler;
- production multi-request concurrency;
- performance benefit guarantees;
- removing or weakening the existing admission guard.

## Impact

This proposal is docs/spec only. It does not modify business code, does not
apply the change, does not start local or remote validation processes, and does
not change runtime configuration.

When applied later, expected implementation areas include PD admission policy,
telemetry/reporting, runbook/report generation, and local tests. Any foreground
Mac/PGX smoke must be authorized separately and must remain bounded to 4k/8k
scaling validation unless a later change explicitly expands the scope.
