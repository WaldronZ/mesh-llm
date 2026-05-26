# PD Streaming KV Handoff Foreground Smoke Report

result: `pass`

recommendation: `keep_8k_deferred_review_4k_pipeline_timing`

role: `async_coordinator`

protocol: `pd-kv-stream/1`

runtime export/import: `observed` / `observed`

bootstrap: `trim_replay_last_token` `pass`

baseline: `local_one_shot_prefill_decode` `exact_token_match`

final decode start position: `Some(4096)`

## Scope

- Workload: `4096` synthetic prompt tokens split as `1024 + 1024 + 1024 + 1024`.
- Pipeline mode: `async`.
- Output comparison: deterministic `max_tokens=16`.
- Not run: `8k`, `32k+`, production scheduler, multi-worker placement.

## Lifecycle Evidence

- True PGX source `export_kv_page`: observed per chunk.
- True Mac coordinator `import_kv_page`: observed per chunk.
- Segment kinds: `iswa/base`, `iswa/swa`.
- Token ranges: contiguous through `4096` imported tokens.
- Full-state handoff as pass path: no.
- Fallback: no.
- Final contiguous gate: pass.
- Bootstrap: `trim_replay_last_token`, `logits_ready=true`.
- Decode start position: `4096`.

## Timing Summary

- chunk count: `4`.
- chunk tokens: `1024`, `1024`, `1024`, `1024`.
- page bytes per chunk: `922746880` bytes each.
- total page bytes: `3690987520`.
- bytes per token: `901120`.
- prefill total ms: `35096.539952`.
- export total ms: `1015.336864`.
- transfer total ms: `31434.503252`.
- native import total ms: `472.708125`.
- final import end ms: `55681.264584`.
- bootstrap eval ms: `3247.684750`.
- approximate decode-ready ms: `58928.949334`.
- actual overlap ms: `0.252209`.
- source idle ms: `0`.
- importer idle ms: `0`.
- backpressure wait ms: `0`.
- page queue depth: `4`.

## Baseline

- baseline strategy: `local_one_shot_prefill_decode`.
- baseline comparison: `exact_token_match`.
- baseline token count: `16`.
- streaming token count: `16`.

## 4k One-Shot Reference Comparison

Previous one-shot large-state reference was approximately `3.5GB` payload,
`30.7s` network transfer, `14.1s` import, and `79.2s` TTFT. This 4k streaming
smoke transferred a similar total page byte volume but validated per-chunk
page export/import and exact-match decode. The measured transfer total remains
close to the previous network-only reference, while native import time is much
lower in this harness. End-to-end decode readiness was approximately `58.9s`.

The measured overlap is positive but small. This is enough to show the async
pipeline path is active, not enough to claim production-grade overlap.

## Privacy

This report excludes prompt text, generated content, complete token arrays,
KV/native payload contents, credentials, private paths, endpoint URLs, and real
machine labels.
