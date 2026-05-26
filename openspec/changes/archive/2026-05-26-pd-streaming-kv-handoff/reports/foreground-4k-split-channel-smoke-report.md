# PD Streaming KV Handoff Foreground Smoke Report

result: `pass`

recommendation: `compare_4k_split_channel_against_single_stream_async`

role: `split_channel_coordinator`

protocol: `pd-kv-stream/1`

runtime export/import: `observed` / `observed`

bootstrap: `trim_replay_last_token` `pass`

baseline: `local_one_shot_prefill_decode` `exact_token_match`

final decode start position: `Some(4096)`

## Scope Closure

This report closes the current change within the 4k foreground harness scope:

- prompt size: `4096` tokens;
- chunk shape: `4 x 1024`;
- protocol: `pd-kv-stream/1`;
- transport: split control channel plus page stream;
- source export path: PGX `export_kv_page` observed per chunk;
- target import path: Mac `import_kv_page` observed per chunk;
- cache shape: Gemma4 ISWA `base` and `swa` segments present;
- final contiguous gate: pass;
- bootstrap: `trim_replay_last_token` pass;
- logits ready: `true`;
- decode start position: `4096`;
- baseline strategy: `local_one_shot_prefill_decode`;
- baseline comparison: `exact_token_match`;
- full-state handoff used as pass: `false`;
- fallback used: `false`.

The previous large-state handoff path is retained only as a reference/fallback
from earlier PD work. It was not used as this streaming KV pass condition.

## Timing Summary

- total page bytes: `3,690,987,520`;
- bytes per token: `901,120`;
- split-channel decode-ready: about `43.12s`;
- previous single-stream async decode-ready: about `58.93s`;
- decode-ready improvement: about `15.81s` / `26.8%`;
- split-channel overlap: about `23.31s`;
- previous single-stream async overlap: about `0.252ms`.

The split-channel report uses richer source-side/control-channel telemetry, so
summed transfer/export buckets are not directly comparable one-for-one with
the previous single-stream async report. Decode-ready and overlap are the
primary comparison signals.

## Deferred

- 8k split-channel smoke;
- production PD router/admission integration;
- production timeout/cancel hardening;
- KV compression or lower-precision payload reduction;
- 32k/128k/256k validation.

## Privacy

The report excludes prompt text, generated content, complete token arrays,
KV/native payload contents, credentials, private paths, endpoint URLs, and
real machine labels.
