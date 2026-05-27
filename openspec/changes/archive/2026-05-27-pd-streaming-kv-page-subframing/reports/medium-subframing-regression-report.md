# 494-Token Subframing Regression Smoke

Date: 2026-05-27

## Result

Result: pass

Scope: production `/v1/responses` regression smoke for the medium prompt class
that previously failed when a single `iswa/swa` logical segment exceeded the
64 MiB frame cap. The smoke used the production Mac router and PGX
`serve-binary` streaming KV source, not the `skippy-correctness` harness.

## Request

- Prompt id: `pd-subframing-medium-synthetic-2026-05-27`
- Target token class: 450-520
- Observed prompt token count: 494
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
- Content delta count: 32
- Protocol: `pd-kv-stream/1`
- Chunk count: 1
- Segment count: 2
- Segment kinds: `iswa/base`, `iswa/swa`
- Max frame bytes: 67,108,864
- Max in-flight bytes: 536,870,912
- Total page bytes: 445,153,280
- `iswa/base` logical bytes: 40,468,480
- `iswa/base` subframe count: 1
- `iswa/swa` logical bytes: 404,684,800
- `iswa/swa` subframe count: 7
- `iswa/swa` subframe bytes: six frames at 67,108,864 bytes and one final
  frame at 2,031,616 bytes
- Final contiguous gate: pass
- Trim/replay bootstrap: pass
- `logits_ready`: true
- `decode_start_position`: 494
- Decode start observed: yes
- Full-state fallback used as pass: no
- Transparent fallback: no
- Source listener alive after request: yes

## Regression Outcome

The medium prompt no longer requires a 1 GiB single-frame cap. The large
`iswa/swa` logical segment was transmitted as seven bounded subframes under the
64 MiB cap, then reassembled before `import_kv_page`.

## Negative Checks

- `frame_too_large`: not observed
- `page_read_timeout`: not observed
- Full-state fallback: not observed
- Transparent fallback: not observed

## Privacy

This report contains no prompt text, generated content, complete token arrays,
KV/native payload contents, private paths, real hostnames, endpoint URLs, or
credentials.
