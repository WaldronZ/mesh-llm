# Commit Scope Guidance

This file records the recommended pre-submit boundary for the current
Prefill/Decode separation work. It is intentionally a submission hygiene note,
not a product or runtime design document.

## Recommended for this PR

- `crates/skippy-protocol/`
- `crates/skippy-runtime/`
- `crates/skippy-server/`
- `crates/skippy-correctness/`
- `openspec/specs/pd-disaggregated-serving-mvp/spec.md`
- `openspec/specs/pd-disaggregated-serving-hardening/spec.md`
- `openspec/changes/archive/2026-05-20-pd-disaggregated-serving-mvp/`
- `openspec/changes/archive/2026-05-20-pd-disaggregated-serving-hardening/`
- `docs/PD-detach/` after path and machine-detail sanitization

## Keep Local or Defer

Do not include these active experiment changes in the main MVP PR unless they
are separately sanitized and intentionally reviewed as process evidence:

- `openspec/changes/pd-kv-handoff-spike/`
- `openspec/changes/pd-router-validation/`
- `openspec/changes/pd-router-validation-followup/`

Reasons:

- They are active validation/prototype history rather than final archived spec
  material.
- Their reports include operator machine labels, temporary build/log paths, and
  binary inventories from real validation machines.
- `openspec/changes/pd-kv-handoff-spike/scripts/pd_kv_spike_tool.py` contains
  synthetic prompt text used by the spike prompt-suite generator. Keep it local
  or sanitize it in a dedicated follow-up before tracking it.

## Privacy Boundary

Tracked commits should not include credentials, private config files, private
home paths, raw prompt text, full token arrays, KV payload contents, or raw
machine-specific worker IDs in telemetry/reporting artifacts.
