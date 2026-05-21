# Design: PD Chunked Prefill

## Context

The current PD path is:

```text
/v1/chat/completions
  -> Mac coordinator/router/decode
  -> PGX one-shot prefill/export
  -> pd-handoff/1 native KV/decode-state payload
  -> Mac import/decode
```

This works for prompts inside the current admission envelope. It fails as a
scaling strategy because PGX prefill is still constrained by `n_batch=2048`,
while one-shot native handoff grows at roughly 900 KB per prompt token for the
current Gemma topology. `pd-long-context-scaling` therefore keeps 4k/8k prompts
rejected before PGX until a safe prefill strategy exists.

Chunked prefill changes only the PGX prefill phase. It does not change the
external OpenAI-compatible surface, and it does not introduce chunked KV
handoff. PGX receives token chunks, advances a single native runtime session,
and exports one final native KV/decode-state payload after the last chunk.

## Target Lifecycle

```text
client request
  -> coordinator normalizes request and tokenizes once
  -> admission checks chunked prefill capability
  -> coordinator creates chunked prefill session
  -> for each token range:
       send chunk(token_ids, start_position, end_position, session_id)
       PGX prefill advances existing session state
       PGX returns ACK with consumed range and current position
  -> coordinator requests final export
  -> PGX exports native KV/decode state + pd-handoff/1 manifest
  -> Mac validates manifest and imports state
  -> Mac decodes from decode_start_position
  -> client receives OpenAI-compatible response
```

The coordinator continues to own tokenization. PGX receives token IDs plus
bounded metadata. The token array must not be written to reports or telemetry.

## Chunk Size And Admission

Chunked admission should compute:

```text
max_chunk_tokens =
  min(
    configured_chunk_tokens,
    max_prefill_batch,
    n_batch_safe_margin
  )
```

For the current PGX `n_batch=2048`, a conservative first chunk size should be
below the full batch limit, for example `<= 1800`, unless true-machine evidence
justifies a different value.

Admission changes from "prompt must fit one prefill batch" to "each chunk must
fit the prefill batch envelope and the whole request must fit context, KV byte,
memory, network/SLA, and lifecycle gates." The existing admission guard remains
active:

- If chunked prefill capability is absent, 4k/8k prompts remain rejected or
  fallback before PGX.
- If capability is present, 4k/8k may be admitted only when all gates pass.
- Raising `ctx_size` or `max_prompt_tokens` alone is still insufficient.

## Position And Runtime State

The most important correctness rule is monotonic position continuity:

- chunk `0` starts at prompt position `0`;
- each ACK reports the exact token range consumed;
- next chunk starts at the previous ACK's end position;
- final `decode_start_position` equals the total prompt token count accepted by
  PGX;
- exported state must match that final position.

Any mismatch between expected and acknowledged positions is fail-closed. The
coordinator must not request final export after a position mismatch.

The PGX runtime must preserve one session state across chunks. A later
implementation should avoid reinitializing KV state per chunk, because that
would not produce the same decode continuation state.

## ACK, Error, Retry-Or-Fail

Each chunk needs an explicit result:

- `ack`: chunk consumed, session position advanced;
- `reject`: chunk invalid before consuming tokens;
- `error_before_consume`: retry may be possible if no state advanced;
- `error_after_consume`: fail the PD path and cleanup; retry is unsafe unless
  the runtime can prove idempotent state;
- `cancelled`: request cancellation acknowledged;
- `timeout`: coordinator stops sending chunks and triggers cleanup.

MVP recommendation: fail closed on chunk errors instead of retrying. Retry can
be a later hardening change after idempotency is proven.

## Cancel, Timeout, Cleanup

The coordinator needs cleanup for every terminal path:

- success after final export;
- pre-content fallback/rejection;
- chunk reject/error;
- timeout;
- client cancel;
- Mac import failure;
- post-content decode failure.

Cleanup must release the PGX session, Mac decode/import state, coordinator
in-flight slot, temporary buffers, and telemetry span state. Cleanup failures
should be reported as sanitized secondary errors without leaking paths or
payloads.

## Final Export And Manifest

PGX exports native KV/decode state only after the final chunk ACK. The exported
manifest remains `pd-handoff/1`, with additive chunked provenance fields such
as:

- `prefill_mode=chunked`;
- `chunk_count`;
- `chunk_token_ranges`;
- `chunk_size_policy`;
- `final_decode_start_position`;
- `total_prompt_tokens`;
- `chunked_session_id` or bounded session label;
- `prefill_position_checksum` or equivalent bounded integrity marker, if
  available.

Existing manifest identity and fail-closed checks remain required: model
artifact hash, tokenizer hash, chat template hash, dtype/layout/ABI, byte
count, payload checksum, and decode start position.

## Telemetry And Reporting

Required telemetry/report fields:

- `pd.prefill.mode=chunked`;
- `pd.prefill.chunk_count`;
- `pd.prefill.chunk_tokens`;
- `pd.prefill.chunk_ms`;
- `pd.prefill.total_ms`;
- `pd.decode_start_position`;
- `pd.kv_payload_bytes`;
- `pd.kv_export_ms`;
- `pd.kv_transfer_ms`;
- `pd.kv_import_ms`;
- `pd.ttft_ms`;
- `pd.admission.result`;
- `pd.admission.reason`.

Telemetry must use bounded labels and numeric counters. It must not include
prompt text, full token arrays, generated content, KV payload contents,
credentials, private paths, endpoint URLs, or real machine labels.

## Local Correctness Strategy

Local tests should cover the state machine without requiring true-machine
processes:

- chunk planner splits 4k/8k token counts into safe ranges;
- positions advance monotonically;
- final decode position equals prompt token count;
- missing chunked capability rejects/fallbacks before PGX;
- chunk error/timeout/cancel cleans up;
- manifest provenance validates positive and negative cases;
- telemetry excludes sensitive content.

True correctness against native runtime state requires foreground smoke and may
require a later dedicated correctness harness if deterministic output diverges
across backends.

## 4k/8k Foreground Smoke Plan

When separately authorized, smoke should run:

1. baseline prompt below current one-shot limit to confirm the existing PD path
   still works;
2. 4k prompt admitted via chunked prefill and completed through final Mac
   decode;
3. 8k prompt admitted via chunked prefill and completed through final Mac
   decode;
4. 4k/8k over-policy request rejected before PGX;
5. chunk error injection, if available, confirming cleanup and no mixed-path
   output.

Reports should record chunk counts, per-chunk token counts, per-chunk latency,
total prefill latency, final KV bytes, export/import/network timing, TTFT,
decode tokens/sec, and PGX process survival.

## Major Risks

- Native runtime session state may not currently support incremental prefill
  chunks without reinitialization.
- Position accounting bugs can produce plausible but incorrect decode output.
- Chunk ACK/error semantics may require binary protocol changes.
- Final one-shot KV export for 8k can still be large and slow even if prefill
  itself is chunked.
- Cancellation and timeout cleanup must not leave GPU memory or in-flight slots
  stranded.
- Backend differences may complicate deterministic correctness checks.

## Non-goals

This change does not include 32k+ production support, 128k/256k implementation,
streaming/chunked KV handoff, KV compression, multi-worker placement,
scheduler behavior, production concurrency, or default-on PD serving.
