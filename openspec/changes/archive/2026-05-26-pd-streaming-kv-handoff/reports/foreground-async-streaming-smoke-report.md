# PD Streaming KV Handoff Foreground Smoke Report

result: `pass`

recommendation: `proceed_to_4k_streaming_smoke`

role: `async_coordinator`

protocol: `pd-kv-stream/1`

runtime export/import: `observed` / `observed`

bootstrap: `trim_replay_last_token` `pass`

baseline: `local_one_shot_prefill_decode` `exact_token_match`

final decode start position: `Some(128)`

## Scope

- Workload: `128` prompt tokens split as `64 + 64`.
- Pipeline mode: `async`.
- Result scope: foreground correctness and measurable async lifecycle evidence only.
- Not run: `4k`, `8k`, `32k+`, production scheduler, multi-worker placement.

## Lifecycle Evidence

- True PGX source `export_kv_page`: observed per chunk.
- True Mac coordinator `import_kv_page`: observed per chunk.
- Segment kinds: `iswa/base`, `iswa/swa`.
- Token ranges: contiguous `0..64`, `64..128`.
- Full-state handoff as pass path: no.
- Fallback: no.
- Final contiguous gate: pass.
- Bootstrap: `trim_replay_last_token`, `logits_ready=true`.
- Decode start position: `128`.

## Timing Summary

- chunk count: `2`.
- chunk tokens: `64`, `64`.
- page bytes per chunk: `57671680`, `57671680`.
- bytes per token: `901120`.
- actual overlap ms: `46.870168`.
- source idle ms: `0`.
- importer idle ms: `0`.
- backpressure wait ms: `0`.
- page queue depth: `2`.

## Baseline

- baseline strategy: `local_one_shot_prefill_decode`.
- baseline comparison: `exact_token_match`.
- baseline token count: `16`.
- streaming token count: `16`.

## Privacy

This report excludes prompt text, generated content, complete token arrays,
KV/native payload contents, credentials, private paths, endpoint URLs, and real
machine labels.
