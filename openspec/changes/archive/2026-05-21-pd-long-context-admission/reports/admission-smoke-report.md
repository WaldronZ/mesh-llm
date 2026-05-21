# PD Long Context Admission Smoke Report

Date: 2026-05-20

## Result

- result: pass
- recommendation: proceed_to_review
- scope: minimal foreground smoke for long-context PD admission guard
- production-ready: no

## Sanitization

This report intentionally excludes prompt text, complete token arrays, generated
content, KV payload contents, credentials, private paths, and private machine
details. Machine references are role labels only.

## Environment

| Item | Value |
| --- | --- |
| Router/decode role | mac_coordinator_router_decode |
| Prefill/export role | pgx_prefill_export |
| Serving mode | `--pd-serving-mvp` |
| Router validation mode | not used |
| Admission over-limit action | reject |
| Local binary sha256 | `3d736da56c54b67e32ee7e691edaeef4107d4a8e3910c837681e54593d7d71d0` |
| Prefill binary sha256 | `b5dfb6cb16c5b99430e3461b3a56361edd927a38bf0036a2030e11e27086c283` |

## Admission Policy

| Field | Value |
| --- | --- |
| `pd.max_prompt_tokens` | 1800 |
| `pd.max_prefill_batch` | 1800 |
| `pd.max_ctx_size` | 8192 |
| `pd.max_handoff_bytes` | 1073741824 |
| `pd.estimated_kv_bytes_per_token` | 524288 |
| requested max tokens | 32 |

## Smoke Cases

| Prompt ID | Calibration Token Count | Router Admission Token Count | Expected | Observed |
| --- | ---: | ---: | --- | --- |
| near-threshold | 1613 | 1616 | admitted + PD path pass | admitted + PD path pass |
| over-threshold | 2013 | 2016 | reject before PGX prefill | rejected before PGX prefill |

The small calibration-vs-router token-count difference is recorded as observed
behavior from the live OpenAI request path. Both cases remain inside the
planned near/over target windows.

## Telemetry Summary

| Prompt ID | `pd.admission.result` | `pd.admission.reason` | `pd.prompt_token_count` | `pd.estimated_kv_bytes` | `pd.mvp.result` | Notes |
| --- | --- | --- | ---: | ---: | --- | --- |
| near-threshold | admitted | within_limits | 1616 | 847249408 | pass | `pd-handoff/1`; PD path completed |
| over-threshold | rejected | prompt_tokens_exceeded | 2016 | 1056964608 | fail | `pd.pre_content=true`; no assistant content delta |

## PD Path Evidence

Near-threshold completed through the PD MVP lane with:

- `pd.protocol_version=pd-handoff/1`
- `pd.kv_payload_bytes=1455349040`
- `pd.kv_transfer_isolated=true`
- `pd.kv_transfer_ms=12554.228458`
- `pd.router_overhead_ms=24977.331958`
- `pd.ttft_ms=21998.272`
- `pd.decode_tokens_per_sec=10.372235070623132`

## Over-threshold Guard Evidence

- Over-threshold returned a pre-content SSE error with
  `code=context_length_exceeded`.
- The response contained no assistant `content` delta.
- Router telemetry recorded `pd.mvp.failure_phase=admission`.
- Router telemetry recorded `pd.mvp.failure_reason=prompt_tokens_exceeded`.
- Prefill/export log line count remained unchanged across the over-threshold
  request.
- Prefill/export port remained listening after the over-threshold request.

## Cleanup

- Foreground router, decode/import, SSH tunnel, and prefill/export processes
  were stopped with SIGINT.
- Planned Mac ports were free after cleanup.
- Planned PGX prefill port was free after cleanup.
