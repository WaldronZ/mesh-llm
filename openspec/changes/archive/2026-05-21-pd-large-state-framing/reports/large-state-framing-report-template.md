# PD Large State Framing Report

## Result

- result: pass | fail | inconclusive
- recommendation: rerun_4k | proceed_to_8k | redesign | run_more_validation
- scope: local framing validation and optional authorized 4k foreground smoke

## Local Validation

| check | expected | observed |
|---|---|---|
| small payload legacy `StateImport` | backward-compatible | TBD |
| capability missing | fail closed before large payload | TBD |
| framed payload round-trip | pass | TBD |
| truncated frame | fail closed | TBD |
| checksum mismatch | fail closed | TBD |
| out-of-order frame | fail closed | TBD |
| manifest large-state provenance | validated | TBD |
| telemetry privacy | pass | TBD |

## Large-State Metrics

| prompt id | payload bytes | frame count | frame bytes | write ms | read ms | checksum ms | result |
|---|---:|---:|---:|---:|---:|---:|---|
| local-fixture | TBD | TBD | TBD | TBD | TBD | TBD | TBD |
| chunked-4k | TBD | TBD | TBD | TBD | TBD | TBD | not-run |

## Manifest And Import

- `pd-handoff/1` manifest validated payload bytes: TBD
- `pd-handoff/1` manifest validated checksum: TBD
- large-state framing provenance present when framed: TBD
- Mac import/decode result: not-run unless separately authorized
- SSE completion result: not-run unless separately authorized

## Privacy Review

Confirm absent:

- prompt text
- complete token arrays
- generated content
- KV/native state payload contents
- credentials
- private paths
- endpoint URLs
- real machine labels

## Failure Reason

TBD
