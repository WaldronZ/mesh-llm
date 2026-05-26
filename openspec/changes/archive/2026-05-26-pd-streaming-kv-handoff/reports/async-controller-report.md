# PD Streaming KV Handoff Async Controller Report

result: `inconclusive`

recommendation: `ready_for_async_foreground_smoke`

role: `local_async_controller`

protocol: `pd-kv-stream/1`

scope: local async/pipelined controller simulation only; no Mac/PGX foreground
processes were started.

pipeline model:

- lifecycle: `async_prefill_export_bounded_queue_importer_final_contiguous_gate`
- out-of-order policy: `fail_closed`
- full-state handoff allowed as pass: `false`
- final gate: all chunks contiguous, imported, and bootstrap-ready before decode

simulated timing:

- chunk count: `2`
- chunk tokens: `64,64`
- prefill start/end ms: `0..12`, `16..29`
- export start/end ms: `12..16`, `29..34`
- transfer start/end ms: `16..17`, `34..35`
- import start/end ms: `17..37`, `37..57`
- actual overlap ms: `18`
- source idle ms: `0`
- importer idle ms: `17`
- backpressure wait ms: `0`
- page queue depth: `2`
- bytes per token: `64`
- final decode start position: `128`

status:

- local async controller/tests: complete
- foreground async smoke: not run
- 4k/8k smoke: not run

privacy:

- prompt text: `excluded`
- generated content: `excluded`
- complete token arrays: `excluded`
- KV/native payload contents: `excluded`
- credentials/private paths/real machine names: `excluded`
