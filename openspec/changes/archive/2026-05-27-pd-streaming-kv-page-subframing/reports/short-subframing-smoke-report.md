# Short Subframing Foreground Smoke

Date: 2026-05-27

## Result

Result: pass

Scope: short production `/v1/responses` smoke for `pd-kv-stream/1` page
subframing. This used the production Mac router and PGX `serve-binary`
streaming KV source, not the `skippy-correctness` harness.

## Request

- Prompt id: `pd-subframing-short-synthetic-2026-05-27`
- Prompt token count: 25
- Requested max output tokens: 32
- Stream: true
- Temperature: 0
- Reasoning effort: none
- Prompt text recorded: no
- Generated content recorded: no

## Evidence

- HTTP 200: yes
- SSE `[DONE]`: yes
- Assistant content observed: yes
- Content delta count: 9
- Protocol: `pd-kv-stream/1`
- Chunk count: 1
- Segment count: 2
- Segment kinds: `iswa/base`, `iswa/swa`
- Max frame bytes: 67,108,864
- Total page bytes: 22,528,000
- `iswa/base` logical bytes: 2,048,000
- `iswa/base` subframe count: 1
- `iswa/swa` logical bytes: 20,480,000
- `iswa/swa` subframe count: 1
- Final contiguous gate: pass
- Trim/replay bootstrap: pass
- `logits_ready`: true
- `decode_start_position`: 25
- Decode start observed: yes
- Full-state fallback used as pass: no
- Transparent fallback: no
- Source listener alive after request: yes

## Diagnostics Observed

- `source_chunk_request_received`
- `source_prefill_chunk_start` / `source_prefill_chunk_end`
- `source_export_kv_page_segments_start` / `source_export_kv_page_segments_end`
- `source_subframe_write_start` / `source_subframe_write_end`
- `source_chunk_done`
- `router_subframe_received`
- `router_segment_reassembly_start` / `router_segment_reassembly_end`
- `router_import_kv_page_start` / `router_import_kv_page_end`
- `router_final_contiguous_gate_pass`
- `router_trim_replay_bootstrap_start` / `router_trim_replay_bootstrap_end`
- `router_decode_start`
- `router_cleanup`

## Negative Checks

- `frame_too_large`: not observed
- `page_read_timeout`: not observed
- Full-state fallback: not observed
- Transparent fallback: not observed

## Privacy

This report contains no prompt text, generated content, complete token arrays,
KV/native payload contents, private paths, real hostnames, endpoint URLs, or
credentials.
