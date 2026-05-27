# Change: PD Streaming KV Production Integration

## Why

`pd-streaming-kv-handoff` proved the split-channel `pd-kv-stream/1`
streaming KV handoff in the `skippy-correctness` foreground harness:

- 4096 prompt tokens split into four 1024-token chunks;
- PGX exported KV page segments per chunk;
- Mac imported KV page segments per chunk;
- Gemma4 ISWA `base` and `swa` segments were present;
- final contiguous gate and `trim-replay-last-token` bootstrap passed;
- decode start position was `4096`;
- local one-shot prefill/decode baseline was an exact token match;
- full-state handoff was not used as the streaming proof;
- split-channel decode-ready improved from about `58.93s` to about `43.12s`;
- measured overlap improved from about `0.252ms` to about `23.31s`.

That result is a correctness and overlap proof for the harness, not a
production serving path. The current `skippy-server --pd-serving-mvp`
OpenAI-compatible router still performs chunked prefill followed by final
full-state handoff (`pd-handoff/1` / native full-state), then Mac import and
decode. Real `/v1/chat/completions` traffic does not yet use the proven
`pd-kv-stream/1` split-channel lifecycle.

## What Changes

This change implements a default-off production integration that wires the
proven streaming KV lifecycle into the existing PD serving MVP path when an
explicit capability/config flag is enabled.

The target request flow is:

1. Mac router/coordinator receives `/v1/chat/completions`;
2. coordinator-owned tokenization and existing admission choose the PD path;
3. the prompt is split with the existing chunked prefill planner;
4. PGX prefills chunk `N` and exports KV page segments for chunk `N`;
5. Mac validates and imports chunk `N` while later chunks are prefilling or
   transferring where capacity permits;
6. final contiguous gate verifies all chunks and segments;
7. Mac runs `trim-replay-last-token` bootstrap to make logits ready;
8. Mac decodes and streams SSE only after the verified decode start position is
   continuous.

The integration remains explicitly enabled. It does not silently replace the
existing full-state MVP path.

## Closure Status

This change closes as a default-off `pd-kv-stream/1` production serving
correctness proof:

- short production serving smoke passed;
- 4k production serving smoke passed with 4096 prompt tokens, four 1024-token
  chunks, eight ISWA `base`/`swa` page segments, final gate pass,
  `trim-replay-last-token` bootstrap pass, `decode_start_position=4096`, and
  SSE decode;
- full-state handoff was not used as the streaming pass path;
- the `skippy-correctness` harness was not used as the production pass path.

Production performance readiness is deferred. The 4k production smoke records
bounded lifecycle evidence, but it does not yet emit full per-phase production
timing or overlap telemetry. The end-to-end production request time is not the
same benchmark as the previous harness decode-ready measurement.

## Scope

Must:

- add default-off configuration for production streaming KV handoff, such as
  `--pd-streaming-kv-handoff`;
- reject invalid flag/capability combinations with clear errors;
- reuse existing OpenAI ingress, coordinator tokenization, chunked prefill
  planner, admission, PD role/status, and telemetry surfaces;
- implement production split control/page channel lifecycle for the PD request;
- export/import per-chunk KV page segments, including Gemma4 ISWA `base` and
  `swa` segments;
- validate `pd-kv-stream/1` manifests, identity, layout, checksums, ordering,
  and final decode start position;
- fail closed on ambiguity, corruption, import/bootstrap failure, or lifecycle
  cancellation;
- preserve pre-content fallback/rejection semantics and forbid transparent
  post-content fallback;
- emit bounded lifecycle diagnostics and sanitized proof metadata;
- add local tests and a separately authorized foreground validation plan for a
  short request and 4k request.

Should:

- reuse `skippy-correctness` controller and manifest concepts without copying
  test-only shortcuts into the serving path;
- keep large-state full-state handoff available as an explicit disabled-path
  fallback/reference, not as a streaming proof;
- preserve existing `--pd-serving-mvp` behavior unless the new streaming flag
  is enabled;
- start with bounded single-request/single-lane serving behavior.

Won't:

- require or claim 8k support;
- claim production performance readiness;
- add production-grade per-phase timing, overlap, TTFT, control-lag, writer
  wait, or backpressure telemetry;
- complete timeout/cancel hardening;
- reduce KV payload footprint;
- add KV compression or low-precision KV;
- add multi-worker placement or a scheduler;
- add production concurrency;
- change UI behavior;
- make PD serving default-on;
- add public mesh or cross-owner PD serving.

## Implemented Areas

- `skippy-server` CLI/config and OpenAI frontend PD router path;
- PD admission and lifecycle orchestration;
- binary/control/page-stream transport glue;
- manifest validation and sanitized telemetry;
- local tests for default-off config, failure semantics, and regression paths;
- foreground smoke for short and 4k requests after explicit authorization.
