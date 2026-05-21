# PD Long Context Scaling Smoke Report

## Result

- result: pass
- recommendation: keep the current admission guard, do not admit 4k/8k prompts on the current one-shot handoff path, and move next to chunked prefill / long-context execution design before raising limits.
- date: 2026-05-21

## Scope

This smoke used the scoped PD serving path only:

- OpenAI-compatible ingress with `--pd-serving-mvp`
- one coordinator/router/decode role
- one PGX prefill/export role
- no `--pd-router-validation`
- no `n_batch` increase
- no chunked prefill
- no KV compression or incremental transfer
- no multi-worker placement or scheduler

The report is sanitized. It does not include prompt text, generated content,
complete token arrays, KV payload contents, credentials, private paths, or real
machine names.

## Configuration

| Field | Value |
|---|---:|
| `pd.max_prompt_tokens` | 1800 |
| `pd.max_prefill_batch` | 1800 |
| `pd.max_ctx_size` | 8192 |
| `pd.max_handoff_bytes` | 1073741824 |
| `pd.estimated_kv_bytes_per_token` | 902000 |
| `pd.kv_bytes_per_token_source` | configured |
| `pd.token_context_prefill_limit` | 1800 |
| `pd.effective_prompt_limit` | 1190 |
| `pd.admission.over_limit_action` | reject |

The effective prompt limit was dominated by the KV bytes gate:
`floor(1073741824 / 902000) = 1190`.

## Binaries

| Role | Binary SHA256 |
|---|---|
| coordinator/router/decode role | `aa7dc0f6067304ba1dcef28145eb1cae69994a175e07da7669651966dbdf9816` |
| prefill/export role | `b5dfb6cb16c5b99430e3461b3a56361edd927a38bf0036a2030e11e27086c283` |

## Prompt Cases

| Case ID | Prompt tokens | Expected | Observed |
|---|---:|---|---|
| `baseline-near-safe` | 1050 admission / 1049 handoff | admitted + PD path pass | admitted + PD path pass |
| `over-4k` | 4000 | reject before PGX | rejected before PGX |
| `over-8k` | 8000 | reject before PGX | rejected before PGX |

## Telemetry Summary

| Case ID | `pd.admission.result` | `pd.admission.reason` | `pd.prompt_token_count` | `pd.estimated_kv_bytes` | `pd.effective_prompt_limit` | `pd.mvp.result` |
|---|---|---|---:|---:|---:|---|
| `baseline-near-safe` | admitted | within_limits | 1050 | 947100000 | 1190 | pass |
| `over-4k` | rejected | prompt_tokens_exceeded | 4000 | 3608000000 | 1190 | fail |
| `over-8k` | rejected | prompt_tokens_exceeded | 8000 | 7216000000 | 1190 | fail |

The 4k/8k cases were rejected at admission. The recorded reason is
`prompt_tokens_exceeded` because the configured token gate runs before the KV
bytes gate, but the effective limit is still lower and is controlled by the KV
bytes gate.

## Baseline PD Path Metrics

| Metric | Value |
|---|---:|
| `pd.kv_payload_bytes` | 945301536 |
| `pd.kv_export_ms` | 4420.772458 |
| `pd.kv_transfer_ms` | 8154.415583 |
| `pd.kv_transfer_network_ms` | 8154.415583 |
| `pd.kv_import_ms` | 112.337166 |
| `pd.router_overhead_ms` | 19630.817083 |
| `pd.ttft_ms` | 16653.513125 |
| `pd.decode_tokens_per_sec` | 10.360376 |

## PGX Protection Evidence

- The baseline request completed the real PD path and emitted
  `pd.mvp.result=pass`.
- The 4k and 8k requests emitted admission rejection telemetry before any KV
  export/import telemetry.
- The prefill/export process stayed alive and was stopped explicitly at the end
  of the smoke.
- Target ports were released after the foreground processes were stopped.

## Conclusion

The current calibrated admission ladder prevents 4k/8k prompts from entering the
one-shot PGX prefill/export path under the current `ctx_size=8192`,
`n_batch=2048`, and 1 GiB handoff limit. This protects the PGX process from the
previous long-context crash mode. Raising the admitted limit to 4k/8k should not
be done by configuration alone; it needs chunked prefill or another bounded
long-context execution strategy.
