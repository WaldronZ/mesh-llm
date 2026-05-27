# Design: PD Streaming KV Production Integration

## Current State

Before this change, the production-facing PD serving MVP path was still the
full-state handoff path:

1. Mac router receives an OpenAI-compatible request.
2. Admission decides whether the request can enter the PD path.
3. PGX performs prefill, using chunked prefill when enabled.
4. PGX exports one final native full-state payload.
5. Mac imports the full state and decodes.

That was correct for the previous MVP/hardening scope, but it did not use the
`pd-kv-stream/1` split-channel page handoff proven in the correctness harness.

The archived `pd-streaming-kv-handoff` proof established that the lower layers
can move per-chunk KV page segments from PGX CUDA to Mac Metal and decode
correctly. This production integration translates that proof into the
long-running router lifecycle without relaxing safety boundaries.

## Closure Status

The implemented change closes at this scope:

- default-off `pd-kv-stream/1` production serving integration;
- short production serving smoke pass;
- 4k production serving smoke pass with 4096 prompt tokens, four 1024-token
  chunks, eight ISWA `base`/`swa` segments, final contiguous gate pass,
  `trim-replay-last-token` bootstrap pass, `logits_ready=true`,
  `decode_start_position=4096`, and SSE decode;
- no full-state handoff as the streaming pass path;
- no `skippy-correctness` harness as the production pass path.

Deferred work:

- 8k and longer validation;
- production-grade per-phase timing, overlap, TTFT, control-lag, writer-wait,
  backpressure, and queue-depth telemetry;
- production performance readiness claims;
- timeout/cancel hardening;
- payload reduction, KV compression, or lower-precision KV;
- scheduler, multi-worker placement, and production concurrency;
- UI work.

## Production Mode

The production integration is a new explicit mode layered on top of
`--pd-serving-mvp`, for example:

```text
--pd-serving-mvp --pd-streaming-kv-handoff
```

The behavior is default-off. Existing `--pd-serving-mvp` without the streaming
flag retains the current full-state path.

Invalid combinations should fail early and clearly. Examples:

- streaming KV enabled without PD serving enabled;
- streaming KV enabled without chunked prefill capability where chunking is
  required by admission;
- streaming KV enabled without compatible PGX source capability;
- streaming KV enabled with frame/in-flight byte limits too low for configured
  chunk size.

## Production Request Lifecycle

The lifecycle is one logical PD request:

```text
Mac router/coordinator
  -> tokenize and admit
  -> plan chunks
  -> open control channel and page stream
  -> dispatch chunk 0

PGX source
  -> prefill chunk 0
  -> export KV page segments for chunk 0
  -> write page frames
  -> continue later chunks while capacity permits

Mac coordinator/importer
  -> receive page frames
  -> validate manifest/checksum/layout/identity
  -> import chunk 0 segments
  -> keep final gate closed until every expected chunk is contiguous

After final gate:
  -> trim/replay last token bootstrap
  -> verify logits_ready and decode_start_position
  -> decode and stream SSE
```

The coordinator remains the owner of tokenization and chunk planning. The PGX
source must not independently tokenize prompt text. Reports and telemetry must
not contain prompt text or full token arrays.

## Reuse From Existing Pieces

The change should reuse these existing pieces where possible:

- OpenAI-compatible `/v1/chat/completions` ingress and SSE streaming behavior;
- existing PD serving MVP config and admission structure;
- chunked prefill planner and position continuity model;
- PD role/status telemetry;
- KV page manifest validation concepts from the correctness harness;
- split-channel timing fields from the `pd-streaming-kv-handoff` reports;
- large-state full-state handoff as a fallback/reference path before visible
  content, not as a streaming pass condition.

The implementation must not blindly copy test-only harness assumptions. In
particular, the serving path needs clear request cancellation, timeout,
connection cleanup, and post-content failure semantics.

## Channels And Protocol

The production streaming mode should use a versioned protocol label:

```text
pd-kv-stream/1
```

The transport shape should follow the proven split-channel design:

- control channel:
  - init/session;
  - chunk request;
  - prefill started/completed;
  - export started/completed;
  - chunk done;
  - stop/error;
- page stream:
  - page frame header;
  - raw page payload;
  - chunk/segment identifiers.

The production binary protocol may use existing binary transport building
blocks, but it must not reuse final full-state `StateExport`/`StateImport` as a
streaming KV proof.

## Manifest And Provenance

Each page frame or segment manifest should include:

- protocol version `pd-kv-stream/1`;
- request/session-scoped stream id;
- chunk index;
- total expected chunk count and prompt token count;
- token start/end positions;
- cache kind and segment kind, including Gemma4 ISWA `base` and `swa`;
- layer range;
- dtype and layout;
- payload byte count;
- checksum algorithm and checksum;
- model artifact identity;
- tokenizer metadata hash;
- chat template hash;
- source/target role labels that do not expose hostnames or private paths.

The final gate should bind all chunk/segment manifests and verify:

- chunks are contiguous;
- all required segments are present;
- checksums and payload lengths match;
- identity and layout match;
- imported token count equals final decode start position;
- no full-state handoff was used as the streaming pass path.

## Bootstrap And Decode

KV page import restores KV/history but does not by itself guarantee current
logits are available. Production streaming mode should use the proven
`trim-replay-last-token` bootstrap:

1. after importing `N` prompt tokens, trim session to `N - 1`;
2. replay the last prompt token at position `N - 1` with logits requested;
3. verify `logits_ready=true`;
4. verify decode start position returns to `N`;
5. only then begin decode.

If trim, replay, logits, or position checks fail, the streaming path must fail
closed. It must not sample from stale logits.

## Failure Semantics

The production path must distinguish before-content and after-content failure.

Before assistant content is visible:

- the router may reject the request with a bounded error;
- or, if explicitly configured and safe, use the existing full-state path as a
  fallback/reference.

After assistant content is visible:

- the router must not transparently fall back to a different path;
- it must surface a bounded stream error and clean up source/coordinator/import
  state.

Fail-closed cases include:

- missing capability;
- invalid config;
- checksum mismatch;
- payload length mismatch;
- dtype/layout/identity mismatch;
- out-of-order, duplicate, gap, or overlap chunk;
- unsupported cache/segment kind;
- oversized frame or queued bytes;
- PGX prefill/export failure;
- page stream failure;
- Mac import failure;
- trim/replay/bootstrap failure;
- cancellation or timeout.

## Telemetry

Telemetry and diagnostics are sanitized and bounded. The implemented
correctness proof emits direct lifecycle diagnostics for:

- `pd.kv_stream.enabled`;
- `pd.kv_stream.protocol_version`;
- `pd.kv_stream.chunk_count`;
- per-chunk token ranges and page bytes;
- source listener, request, chunk receive, prefill start/end, export start/end,
  page frame write start/end, and chunk done;
- router connect, chunk dispatch, control receive, page receive, import
  start/end, final gate, trim/replay bootstrap, decode start, and cleanup;
- cache kind, segment kind, payload byte counts, checksum validation booleans,
  identity validation booleans, and bounded failure reason labels;
- final decode start position;
- result and bounded failure reason labels.

Production-grade performance telemetry remains deferred. Future work should add
per-chunk prefill/export/transfer/import timings, true compute/transfer
overlap, coordinator-observed overlap, clock alignment status, control event
lag, page write/flush timing, writer queue wait, backpressure timing,
in-flight bytes, queue depth, final gate timing, decode-ready, and TTFT before
making production performance readiness claims.

Telemetry must not contain prompt text, generated content, full token arrays,
KV/native payload contents, credentials, private paths, endpoint URLs, or real
machine labels.

## Local Test Strategy

Local tests should cover:

- default-off config;
- invalid flag combinations;
- missing capability rejection;
- normal full-state PD path remains unchanged without the streaming flag;
- normal non-PD path remains unchanged;
- manifest validation;
- checksum, length, identity, dtype/layout, cache/segment mismatch;
- missing/duplicate/out-of-order/gap/overlap chunks;
- no full-state frame can satisfy streaming proof;
- pre-content fallback/rejection behavior;
- post-content no-transparent-fallback behavior;
- cancellation and cleanup plumbing where possible without remote processes;
- telemetry privacy.

## Foreground Validation Plan

Foreground validation requires separate authorization. The first production
serving validation should not jump directly to 8k.

Recommended order:

1. short request smoke with streaming flag enabled;
2. 4k request smoke with chunk size 1024 and generation concurrency bounded to
   one;
3. verify real `/v1/chat/completions` SSE completion;
4. verify PGX per-chunk `export_kv_page`;
5. verify Mac per-chunk `import_kv_page`;
6. verify final gate, bootstrap, `decode_start_position`, and telemetry;
7. compare against the current harness proof and known full-state reference
   timing, while avoiding same-benchmark claims unless measurement is aligned;
8. keep 8k deferred after 4k production serving smoke passes unless a new
   change explicitly takes it on.

## Out Of Scope

This change does not include:

- 8k as a requirement;
- 32k/128k/256k validation;
- production performance readiness;
- production-grade per-phase timing, overlap, TTFT, control-lag, writer-wait,
  backpressure, and queue-depth telemetry;
- timeout/cancel hardening;
- KV payload reduction;
- KV compression;
- low-precision KV payload changes;
- multi-worker placement;
- scheduler behavior;
- production concurrency;
- public mesh or cross-owner PD serving;
- UI work;
- making PD default-on.

## Risks

Key risks:

- the correctness harness lifecycle may not map directly onto long-running
  router state;
- split channel cleanup and cancellation are more complex in production;
- fallback semantics are stricter once SSE content is visible;
- page stream backpressure can still dominate end-to-end latency;
- payload size remains huge, so production integration will not solve network
  footprint by itself;
- telemetry must remain useful without exposing sensitive data.
