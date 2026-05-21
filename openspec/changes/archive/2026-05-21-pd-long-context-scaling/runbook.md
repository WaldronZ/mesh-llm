# PD Long Context Scaling Runbook

This runbook is for the `pd-long-context-scaling` change. It does not authorize
starting Mac/PGX foreground processes by itself. Foreground validation requires
separate operator approval.

## Scope

The runbook validates the scaling ladder for the existing default-off scoped PD
MVP path. It does not implement 256k context, chunked prefill, chunked KV
handoff, multi-worker placement, scheduler behavior, or production concurrency.

## Calibration

Use calibrated KV bytes per token before increasing prompt caps:

| Item | Value |
|---|---:|
| Active topology | Gemma4 native full-state PD handoff |
| Calibrated estimate | `902000` bytes/token |
| Calibration source | `measured` |
| Previous low estimate | `524288` bytes/token |

For non-calibrated model/topology combinations, pass an explicit
`--pd-estimated-kv-bytes-per-token` value or fail safe. Do not treat a missing
estimate as unlimited capacity.

## Effective Limit Formula

For each request:

```text
token_context_prefill_limit =
  min(max_prompt_tokens, max_prefill_batch, max_ctx_size - requested_max_tokens)

effective_prompt_limit =
  min(token_context_prefill_limit, max_handoff_bytes / calibrated_kv_bytes_per_token)
```

Memory and network/SLA budgets are additional gates when they are configured or
available from runtime capacity data.

## Phase A Preflight

Before running 4k or 8k foreground smoke:

1. Confirm PD serving remains explicit/default-off.
2. Confirm the active model/topology has calibrated bytes/token data or an
   explicit override.
3. Confirm `ctx_size >= prompt_target + requested_max_tokens`.
4. Confirm `max_handoff_bytes >= prompt_target * calibrated_kv_bytes_per_token`.
5. Confirm `max_prefill_batch >= prompt_target`, or document that the target is
   blocked until chunked prefill or a larger proven batch is available.
6. Confirm memory and network/SLA budgets are acceptable before admission is
   raised.

Expected one-shot handoff cost using `902000` bytes/token:

| Target | Estimated KV bytes | Estimated transfer at 115 MB/s |
|---:|---:|---:|
| 4k | ~3.7 GB | ~31 s |
| 8k | ~7.4 GB | ~63 s |

Those estimates are warnings, not performance guarantees.

## Smoke Cases

Record prompt IDs and token counts only. Do not record prompt text.

| Case | Expected result |
|---|---|
| `4k-near-threshold` | admitted only if all gates declare it safe |
| `8k-near-threshold` | admitted only if all gates declare it safe |
| `over-threshold` | pre-content reject/fallback before PGX prefill |

If `max_prefill_batch` remains below the prompt target and chunked prefill is
not implemented, the corresponding near-threshold case should be marked
blocked rather than forced into PGX.

## Required Evidence

Report:

- `pd.admission.result`
- `pd.admission.reason`
- `pd.prompt_token_count`
- `pd.token_context_prefill_limit`
- `pd.effective_prompt_limit`
- `pd.estimated_kv_bytes`
- `pd.estimated_kv_bytes_per_token`
- `pd.kv_bytes_per_token_source`
- `pd.max_prompt_tokens`
- `pd.max_prefill_batch`
- `pd.max_ctx_size`
- `pd.max_handoff_bytes`
- actual `pd.kv_payload_bytes` when admitted
- export latency
- isolated network transfer latency
- import latency
- TTFT
- decode tokens/sec
- PGX prefill survival for over-threshold cases

## Sanitization

Reports must exclude prompt text, complete token arrays, generated content, KV
payload contents, credentials, private paths, private machine labels, endpoint
URLs, and raw host identifiers.
