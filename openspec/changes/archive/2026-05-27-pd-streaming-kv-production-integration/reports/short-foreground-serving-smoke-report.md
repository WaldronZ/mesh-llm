# PD Streaming KV Production Short Foreground Serving Smoke

## Result

`pass / short_production_serving_smoke_with_direct_diagnostics`

The short foreground serving smoke proved the production `pd-kv-stream/1` path
for a bounded short request and verified that direct lifecycle diagnostics are
observable in the real production source/router path.

The request completed with HTTP 200, SSE completion, non-empty assistant
content, and finish reason `stop`. This smoke did not use the
`skippy-correctness` harness and did not count a full-state handoff as a
streaming KV pass.

## Scope

- Short production serving smoke only.
- No 4k or 8k request was run.
- No model, tokenizer, or private config was modified.
- No full-state handoff was used as the pass path.
- No transparent fallback was observed.

## Observed Layers

| Layer | Result | Evidence |
| --- | --- | --- |
| Production source listener | pass | `source_listener_active` |
| Source request start | pass | `source_request_start` |
| Source chunk request | pass | `source_chunk_request_received` |
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
| SSE | pass | HTTP 200, SSE `[DONE]`, non-empty assistant content, finish reason `stop` |

## Bounded Request Metadata

| Field | Value |
| --- | --- |
| prompt_id | `short-synthetic-smoke-2026-05-26-diagnostics-rerun` |
| prompt_token_count | 19 |
| chunk_count | 1 |
| segment_count | 2 |
| segment_kinds | `iswa/base`, `iswa/swa` |
| page_bytes | 17121280 |
| decode_start_position | 19 |
| requested_max_tokens | 32 |
| stream | true |
| temperature | 0 |
| seed | 42 |
| response_status | 200 |
| sse_done | true |
| assistant_content_nonempty | true |
| content_delta_count | 1 |
| reasoning_delta_count | 23 |
| finish_reason | `stop` |
| elapsed_s | 3.810 |

## Lifecycle Diagnostics

Direct diagnostics were observed with stable prefix:

`pd.kv_stream.lifecycle`

The diagnostics include bounded chunk ranges, token counts, segment kinds,
payload byte counts, checksum validation booleans, identity validation booleans,
final gate status, bootstrap logits-ready status, and decode start. They do not
include prompt text, generated content, full token arrays, or KV/native payload.

## Cleanup

The Mac router, SSH tunnels, and PGX source process started for this smoke were
stopped with SIGINT. The local and remote smoke ports were verified as released
after cleanup.

## Note

The requested default router port was already occupied by an unrelated local
process, so this smoke used an alternate local router port. The report omits
endpoint URLs by design.

## Privacy

This report does not include prompt text, generated content, full token arrays,
KV/native payload, private paths, endpoint URLs, real machine labels, or
credentials.
