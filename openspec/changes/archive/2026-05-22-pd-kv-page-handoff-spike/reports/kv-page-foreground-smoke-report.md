# PD KV Page Handoff Foreground Smoke Report

result: `pass`

recommendation: `proceed_to_streaming_handoff`

role: `coordinator`

local manifest validation: `pass`

runtime export/import: `observed` / `observed`

page segments: `4` (`iswa/base`, `iswa/swa`)

bootstrap: `trim_replay_last_token` `pass`

baseline strategy: `local_one_shot_prefill_decode`

baseline comparison: `exact_token_match`

decode start position: `128`

decode TTFT after import: `1506.04 ms`

scope: `128-token two-chunk proof only; no 4k/8k run`

privacy: prompt text, generated content, complete token arrays,
KV/native payload contents, credentials, private paths, endpoint URLs, and
real machine labels are excluded.
