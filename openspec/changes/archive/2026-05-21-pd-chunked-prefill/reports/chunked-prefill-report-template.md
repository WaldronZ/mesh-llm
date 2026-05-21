# PD Chunked Prefill Smoke Report

## Result

- result: pass | fail | inconclusive
- recommendation: proceed_to_next_scale | redesign | run_more_validation
- scope: 4k/8k chunked prefill foreground smoke

## Environment

- Mac binary sha256: `<sha256>`
- PGX binary sha256: `<sha256>`
- model artifact sha256: `<sha256>`
- tokenizer metadata hash: `<sha256>`
- chat template hash: `<sha256>`
- real machine names recorded: no
- private paths recorded: no

## Prompt Suite

| prompt id | target class | token count | expected admission | observed admission |
|---|---:|---:|---|---|
| baseline-safe | baseline | TBD | admitted | TBD |
| chunked-4k | 4k | TBD | admitted | TBD |
| chunked-8k | 8k | TBD | admitted | TBD |
| over-policy | over-policy | TBD | reject/fallback before PGX | TBD |

Prompt text and complete token arrays must not be recorded.

## Chunked Prefill Metrics

| prompt id | chunked enabled | chunk size | chunk count | chunk tokens | total prefill ms | final decode start position |
|---|---|---:|---:|---|---:|---:|
| chunked-4k | TBD | TBD | TBD | bounded list only | TBD | TBD |
| chunked-8k | TBD | TBD | TBD | bounded list only | TBD | TBD |

## Handoff And Decode Metrics

| prompt id | KV payload bytes | export ms | network ms | import ms | TTFT ms | decode tok/s |
|---|---:|---:|---:|---:|---:|---:|
| baseline-safe | TBD | TBD | TBD | TBD | TBD | TBD |
| chunked-4k | TBD | TBD | TBD | TBD | TBD | TBD |
| chunked-8k | TBD | TBD | TBD | TBD | TBD | TBD |

## Failure And Cleanup

- over-policy happened before PGX: TBD
- chunk error fail-closed: TBD
- timeout cleanup: TBD
- cancel cleanup: TBD
- PGX process survived: TBD
- port release confirmed: TBD

## Privacy Review

Confirm absent:

- prompt text
- complete token arrays
- generated content
- KV payload contents
- credentials
- private paths
- endpoint URLs
- real machine labels
