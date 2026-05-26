# PD Streaming KV Handoff Split-Channel Telemetry Report

result: `inconclusive`

recommendation: `ready_for_split_channel_4k_smoke`

scope: local Phase 3 controller/readiness only; no Mac/PGX foreground process was started.

protocol: `pd-kv-stream/1`

transport shape: split control channel plus page stream.

runtime export/import: `not_run` / `not_run`

bootstrap: `trim_replay_last_token` `not_run`

baseline: `local_one_shot_prefill_decode` `not_run`

local simulated chunks: `2`

local simulated source/transfer overlap: `18.0 ms`

clock alignment status: `simulated_same_clock`

new telemetry fields covered:

- source prefill start/end per chunk
- source export start/end per chunk
- page write start/end and flush duration
- writer queue send wait and source backpressure wait
- control event emit/receive/lag
- source-relative, coordinator-observed, and true compute/transfer overlap labels

privacy: no prompt text, generated content, complete token arrays, KV/native
payload contents, credentials, private paths, endpoint URLs, or real machine
labels are recorded.

next required authorization: run a 4k split-channel PGX/Mac foreground smoke
and compare timing against the previous single-stream async 4k result. 8k
remains deferred.
