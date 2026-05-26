# PD Streaming KV Handoff Local Controller Report

result: `inconclusive`

recommendation: `ready_for_foreground_streaming_smoke`

protocol: `pd-kv-stream/1`

scope: local controller and protocol lifecycle only; no Mac/PGX foreground
processes were started, no 4k/8k smoke was run, and no production serving path
was changed.

controller lifecycle: prefill chunk N -> export page segments N ->
transfer/import N while later chunks may continue -> final contiguous import
and decode gate.

out-of-order policy: `fail_closed`

local validation:
- two chunk in-order controller path: `pass`
- chunk 0 import before chunk 1 prefill: `pass`
- duplicate chunk: `fail_closed`
- missing chunk: `fail_closed`
- out-of-order chunk: `fail_closed`
- position gap/overlap: `fail_closed`
- checksum mismatch: `fail_closed`
- in-flight bytes cap: `fail_closed`
- import failure: `fail_closed`
- incomplete final gate: `fail_closed`
- full-state blob as streaming proof: `rejected`

telemetry shape includes per-chunk prefill/export/transfer/import timing,
overlap, pipeline idle, in-flight bytes, page bytes per chunk, bytes per token,
and final decode-start position. TTFT and real network overlap remain foreground
smoke fields.

privacy: prompt text, generated content, complete token arrays, KV/native
payload contents, credentials, private paths, endpoint URLs, and real machine
labels are excluded.

remaining work:
- run 128-token foreground streaming smoke;
- compare streaming decode against one-shot handoff baseline;
- validate 4k before optional 8k.
