# PD Streaming KV Production Lifecycle Diagnostics Report

## Result

`inconclusive / ready_for_short_foreground_smoke_rerun_with_direct_diagnostics`

This segment adds direct, grep-friendly production diagnostics for the
`pd-kv-stream/1` source and router lifecycle. It does not start Mac/PGX
processes and does not run short, 4k, or 8k foreground smoke.

## Implemented Diagnostics

All direct diagnostics use the stable line prefix:

`pd.kv_stream.lifecycle`

Source-side events:

- `source_listener_active`
- `source_request_start`
- `source_chunk_request_received`
- `source_prefill_chunk_start`
- `source_prefill_chunk_end`
- `source_export_kv_page_segments_start`
- `source_export_kv_page_segments_end`
- `source_page_frame_write_start`
- `source_page_frame_write_end`
- `source_chunk_done`
- `request_eof`
- `request_error`
- `request_cleanup`
- `listener_continue`
- `listener_shutdown`

Router-side events:

- `router_connect_start`
- `router_connect_end`
- `router_chunk_request_send`
- `router_chunk_request_sent`
- `router_control_event_received`
- `router_page_frame_receive_start`
- `router_page_frame_received`
- `router_import_kv_page_start`
- `router_import_kv_page_end`
- `router_final_contiguous_gate_pass`
- `router_final_contiguous_gate_fail`
- `router_trim_replay_bootstrap_start`
- `router_trim_replay_bootstrap_end`
- `router_decode_start`
- `router_cleanup`

## Bounded Fields

Diagnostics only include bounded metadata:

- protocol version
- source/router side label
- event label
- request id
- chunk index and total chunks
- token start/end/count
- segment index/count
- cache kind and segment kind
- payload byte count
- checksum presence/pass result
- identity validation pass result
- decode start position
- logits-ready boolean
- bounded failure phase/reason

## Privacy

Diagnostics and tests do not include prompt text, generated content, full token
arrays, KV/native payload bytes, private paths, endpoint URLs, real machine
labels, or credentials.

## Local Test Coverage

- Source diagnostic line formatting is bounded and sanitized.
- Router diagnostic line formatting is bounded and sanitized.
- Existing source wire, manifest, listener lifetime, and full-state rejection
  tests still pass.
- Existing router lifecycle happy path, fail-closed, cleanup, and default-off
  tests still pass.

## Not Run

- No short foreground serving smoke was rerun in this segment.
- No 4k or 8k production serving smoke was run.

## Next Step

Rerun the short production serving smoke and require direct
`pd.kv_stream.lifecycle` evidence before using the same diagnostics for a 4k
production serving smoke.
