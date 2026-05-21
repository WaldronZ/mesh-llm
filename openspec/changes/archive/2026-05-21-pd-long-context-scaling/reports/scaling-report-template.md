# PD Long Context Scaling Report Template

## Result

- result: pass / fail / inconclusive / blocked
- recommendation: proceed_to_4k_smoke / proceed_to_8k_smoke / chunked_prefill_required / redesign
- production-ready: no

## Sanitization

This report excludes prompt text, complete token arrays, generated content, KV
payload contents, credentials, private paths, private machine labels, endpoint
URLs, and raw host identifiers.

## Calibration

| Field | Value |
|---|---|
| model/topology label | `<bounded-label>` |
| `pd.estimated_kv_bytes_per_token` | `<integer>` |
| `pd.kv_bytes_per_token_source` | `measured / configured / conservative_default` |
| previous estimate used | `<integer-or-none>` |

## Admission Policy

| Field | Value |
|---|---:|
| `pd.max_prompt_tokens` | `<tokens>` |
| `pd.max_prefill_batch` | `<tokens>` |
| `pd.max_ctx_size` | `<tokens>` |
| requested max tokens | `<tokens>` |
| `pd.max_handoff_bytes` | `<bytes>` |
| `pd.token_context_prefill_limit` | `<tokens>` |
| `pd.effective_prompt_limit` | `<tokens>` |

## Smoke Cases

| Prompt ID | Prompt tokens | Expected | Observed | Admission result | Admission reason | PGX prefill received request |
|---|---:|---|---|---|---|---|
| 4k-near-threshold | `<count>` | admitted or blocked | `<result>` | `<result>` | `<reason>` | yes/no/n/a |
| 8k-near-threshold | `<count>` | admitted or blocked | `<result>` | `<result>` | `<reason>` | yes/no/n/a |
| over-threshold | `<count>` | reject/fallback before PGX | `<result>` | `<result>` | `<reason>` | no |

## Timing

| Prompt ID | Estimated KV bytes | Actual KV bytes | Export ms | Network transfer ms | Import ms | TTFT ms | Decode tok/s |
|---|---:|---:|---:|---:|---:|---:|---:|
| 4k-near-threshold | `<bytes>` | `<bytes-or-n/a>` | `<ms>` | `<ms>` | `<ms>` | `<ms>` | `<tok/s>` |
| 8k-near-threshold | `<bytes>` | `<bytes-or-n/a>` | `<ms>` | `<ms>` | `<ms>` | `<ms>` | `<tok/s>` |

## No-Go Checks

| Check | Result | Notes |
|---|---|---|
| one-shot KV payload within budget | pass/fail/blocked | `<notes>` |
| transfer time within budget | pass/fail/blocked | `<notes>` |
| Mac memory budget sufficient | pass/fail/blocked | `<notes>` |
| PGX prefill safe without chunking | pass/fail/blocked | `<notes>` |
| over-threshold rejected before PGX | pass/fail | `<notes>` |

## Recommendation

- proceed / hold / redesign
- next suggested change: `pd-chunked-prefill` if 4k/8k are blocked by prefill batch
