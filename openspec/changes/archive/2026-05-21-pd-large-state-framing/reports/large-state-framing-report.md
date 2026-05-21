# PD Large State Framing Report

## Result

- result: pass
- recommendation: proceed_to_8k_optional_smoke
- scope: local framing validation plus separately authorized 4k foreground
  smoke through `pd-chunked-prefill`

## Local Validation

| check | expected | observed |
|---|---|---|
| small payload legacy `StateImport` | backward-compatible | pass |
| capability missing | fail closed before large payload | pass |
| framed payload round-trip | pass | pass |
| truncated frame | fail closed | pass |
| checksum mismatch | fail closed | pass |
| out-of-order frame | fail closed | pass |
| manifest large-state provenance | validated | pass |
| telemetry privacy | pass | pass |

## Large-State Metrics

| prompt id | payload bytes | frame count | frame bytes | write ms | read ms | checksum ms | result |
|---|---:|---:|---:|---:|---:|---:|---|
| chunked-4k | 3516265368 | 210 | 16777216 | 767.714 | 29917.972 | 13646.654 | framed |

## Manifest And Import

- `pd-handoff/1` manifest validated payload bytes: yes
- `pd-handoff/1` manifest validated large-state framing provenance: yes
- large-state framing protocol: `large-state-framing/1`
- Mac import/decode result: pass
- SSE completion result: pass

## Privacy Review

Confirmed absent:

- prompt text
- complete token arrays
- generated content
- KV/native state payload contents
- credentials
- private paths
- endpoint URLs
- real machine labels

## Failure Reason

None. The previous 4k blocker, `binary_state_payload_exceeds_i32_length`, was
not reproduced after large-state framing.

## Scope Closure

- current scope: large-state framing blocker fixed for 4k chunked prefill
- small legacy `StateImport` compatibility: preserved
- 4k foreground rerun: pass through large-state export, Mac import/decode, and
  SSE completion
- 8k: future validation, not part of this archive scope
- 32k/128k/256k: out of scope
- KV compression: out of scope
- scheduler / multi-worker placement / production concurrency: out of scope
