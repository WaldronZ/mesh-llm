# PD Streaming KV Handoff Foreground Smoke Report

result: `pass`

recommendation: `proceed_to_4k_streaming_smoke`

role: `coordinator`

protocol: `pd-kv-stream/1`

runtime export/import: `observed` / `observed`

scope: `128-token / 64+64 foreground streaming smoke`

chunk lifecycle:

- chunk count: `2`
- chunk tokens: `64,64`
- chunk 0 prefill/export/import completed before final gate: `yes`
- chunk 1 prefill/export/import completed: `yes`
- page segment count observed: `4`
- page segment shape: `Gemma4 ISWA: two page segments per 64-token chunk`
- final contiguous gate: `pass`
- out-of-order policy: `fail_closed`
- measurable overlap in this small smoke: `0 ms`

telemetry:

- per-chunk prefill ms: `553.384016,27.915024`
- per-chunk export ms: `556.911552,716.57216`
- per-segment transfer ms: `1170.267416,444.825791,796.7155,446.561625`
- per-segment import ms: `2.158708,14.71875,1.708291,14.268333`
- page bytes per chunk: `57671680,57671680`
- bytes per token: `901120`

bootstrap: `trim_replay_last_token` `pass`

baseline: `local_one_shot_prefill_decode` `exact_token_match`

final decode start position: `128`

privacy:

- prompt text: `excluded`
- generated content: `excluded`
- complete token arrays: `excluded`
- KV/native payload contents: `excluded`
- credentials/private paths/real machine names: `excluded`
