# PD Streaming KV Production 4k Foreground Serving Smoke

## Result

`pass / foreground_4k_production_serving_smoke`

The 4k foreground serving smoke proved that the real production
`--pd-serving-mvp --pd-streaming-kv-handoff` path can serve a bounded
`/v1/chat/completions` streaming request through `pd-kv-stream/1`.

The request completed with HTTP 200, SSE completion, non-empty assistant
content, and finish reason `length` with `max_tokens=32`. This smoke did not
use the `skippy-correctness` harness and did not count a full-state handoff as
a streaming KV pass.

## Scope

- 4k production serving smoke only.
- No 8k request was run.
- No model, tokenizer, or private config was modified.
- No full-state handoff was used as the pass path.
- No transparent fallback was observed.

## Observed Layers

| Layer | Result | Evidence |
| --- | --- | --- |
| Production source listener | pass | `source_listener_active` |
| Source request start | pass | `source_request_start` |
| Source chunk request | pass | `source_chunk_request_received` for 4 chunks |
| Source prefill | pass | `source_prefill_chunk_start`, `source_prefill_chunk_end` |
| Source KV page export | pass | `source_export_kv_page_segments_start`, `source_export_kv_page_segments_end` |
| Source page stream write | pass | `source_page_frame_write_start`, `source_page_frame_write_end` |
| Source chunk done | pass | `source_chunk_done` |
| Router streaming connection | pass | `router_connect_start`, `router_connect_end` |
| Router chunk dispatch | pass | `router_chunk_request_send`, `router_chunk_request_sent` |
| Router control events | pass | `router_control_event_received` for prefill/export/chunk done events |
| Router page receive | pass | `router_page_frame_received` |
| Router KV page import | pass | `router_import_kv_page_start`, `router_import_kv_page_end` |
| Final contiguous gate | pass | `router_final_contiguous_gate_pass` |
| Trim/replay bootstrap | pass | `router_trim_replay_bootstrap_start`, `router_trim_replay_bootstrap_end` with `logits_ready=true` |
| Decode | pass | `router_decode_start` |
| Cleanup | pass | `router_cleanup`, `request_cleanup`, `listener_continue` |
| SSE | pass | HTTP 200, SSE `[DONE]`, non-empty assistant content, finish reason `length` |

## Bounded Request Metadata

| Field | Value |
| --- | --- |
| prompt_id | `pd-prod-4k-synthetic-2026-05-26` |
| target_token_class | `4k` |
| prompt_token_count | 4096 |
| chunk_count | 4 |
| chunk_token_ranges | `0..1024`, `1024..2048`, `2048..3072`, `3072..4096` |
| segment_count | 8 |
| segment_kinds | `iswa/base` x4, `iswa/swa` x4 |
| page_bytes_per_chunk | 922746880 |
| total_page_bytes | 3690987520 |
| bytes_per_token | 901120 |
| decode_start_position | 4096 |
| requested_max_tokens | 32 |
| stream | true |
| temperature | 0 |
| seed | 42 |
| response_status | 200 |
| sse_done | true |
| assistant_content_nonempty | true |
| content_delta_count | 32 |
| reasoning_delta_count | 0 |
| finish_reason | `length` |
| elapsed_s | 92.201 |

## Timing Notes

Production direct lifecycle diagnostics currently prove source/export/import,
final gate, bootstrap, and decode event coverage, but they do not yet emit
complete per-phase wall-clock timing buckets for prefill, export, transfer,
import, or overlap.

| Metric | Value | Notes |
| --- | --- | --- |
| request_elapsed_s | 92.201 | End-to-end smoke request wall time, including decode and runtime/process conditions. |
| bootstrap_ms | 117.263708 | Recorded by production lifecycle diagnostics. |
| prefill_total | `not_recorded` | Requires production timing telemetry. |
| export_total | `not_recorded` | Requires production timing telemetry. |
| transfer_total | `not_recorded` | Requires production timing telemetry. |
| import_total | `not_recorded` | Requires production timing telemetry. |
| lifecycle_overlap | `not_recorded` | Requires production timing telemetry. |

The previous `skippy-correctness` 4k split-channel harness remains a reference
only: decode-ready was approximately 43.12s, page bytes were 3,690,987,520, and
measured overlap was 23.31s. This production smoke used the same page byte
footprint and proved the real serving path, but the timing measurements are not
the same benchmark.

## Cleanup

The Mac router, SSH tunnels, and PGX source process started for this smoke were
stopped with SIGINT. The local and remote smoke role ports were verified as
released after cleanup.

The requested default router role port was already occupied by an unrelated
local process, so this smoke used an alternate local router role port. The
report omits endpoint URLs by design.

## Privacy

This report does not include prompt text, generated content, full token arrays,
KV/native payload, private paths, endpoint URLs, real machine labels, or
credentials.
