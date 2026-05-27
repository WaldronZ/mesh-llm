# 4k Subframing Foreground Smoke Report

## Result

Pass within the 4k-class production serving scope.

This smoke exercised the production `pd-kv-stream/1` path, not the
`skippy-correctness` harness:

- Mac router `/v1/responses`
- PGX streaming KV source
- per-chunk KV page export
- bounded page subframes on the page stream
- router subframe reassembly
- per-segment `import_kv_page`
- final contiguous gate
- trim-replay bootstrap
- decode/SSE

No full-state fallback or transparent fallback was used.

## Request

| Field | Value |
|---|---:|
| Prompt id | `pd-subframing-4k-synthetic-2026-05-27` |
| Target prompt token class | 4k |
| Observed prompt tokens | 4,063 |
| Chunk size | 1,024 |
| Chunk count | 4 |
| Max decode tokens | 16 |
| HTTP / SSE | 200 / done |
| Assistant content | non-empty |
| End-to-end request time | 94.524s |

The synthetic prompt text, generated content, full token array, KV payload, and
endpoint URL are intentionally not recorded.

## Capacity

| Setting | Value |
|---|---:|
| `max_frame_bytes` | 67,108,864 |
| `max_in_flight_bytes` | 1,073,741,824 |
| `max_queue_depth` | 4 |

`max_in_flight_bytes` was kept at 1 GiB because the router currently
reassembles each logical segment before calling the existing `import_kv_page`
path. A full 1,024-token `iswa/swa` logical segment is 838,860,800 bytes, so a
512 MiB in-flight cap would be too small even though each individual subframe is
bounded at 64 MiB.

## Page Stream

| Metric | Value |
|---|---:|
| Segment count | 8 |
| Segment kinds | `iswa/base` x4, `iswa/swa` x4 |
| Total page bytes | 3,661,250,560 |
| Bytes/token | 901,120 |
| Max observed subframe payload | 67,108,864 |
| `frame_too_large` | no |
| `page_read_timeout` | no |

The observed token count was 4,063 rather than exactly 4,096, so the total page
bytes are proportionally lower than the previous 4k production run. Bytes/token
matches the previous run.

## Chunks And Subframes

| Chunk | Tokens | Page bytes | `iswa/base` | `iswa/swa` |
|---:|---:|---:|---:|---:|
| 0 | 0..1024 | 922,746,880 | 83,886,080 bytes / 2 subframes | 838,860,800 bytes / 13 subframes |
| 1 | 1024..2048 | 922,746,880 | 83,886,080 bytes / 2 subframes | 838,860,800 bytes / 13 subframes |
| 2 | 2048..3072 | 922,746,880 | 83,886,080 bytes / 2 subframes | 838,860,800 bytes / 13 subframes |
| 3 | 3072..4063 | 893,009,920 | 81,182,720 bytes / 2 subframes | 811,827,200 bytes / 13 subframes |

Every `iswa/swa` logical segment had `subframe_count > 1`. Every subframe stayed
within the 64 MiB cap.

## Lifecycle Evidence

Observed direct lifecycle diagnostics:

- source chunk request received
- source prefill start/end
- source export start/end
- source subframe write start/end
- source chunk done
- router subframe received
- router segment reassembly start/end
- router `import_kv_page` start/end
- final contiguous gate pass
- trim-replay bootstrap pass
- `logits_ready=true`
- `decode_start_position=4063`
- decode start
- router cleanup
- source listener continued after request

## Comparison

Previous 4k production serving smoke without subframing used the 1 GiB
single-frame workaround and recorded 3,690,987,520 page bytes for 4,096 tokens.

This subframing smoke passed with a 64 MiB single-subframe cap. Total bytes were
lower only because the observed prompt was 4,063 tokens; bytes/token remained
901,120.

## Scope Closure

This is a 4k-class foreground proof that page subframing removes the need for a
1 GiB single-frame cap on large `iswa/swa` logical segments. It does not claim
8k readiness, production performance readiness, KV compression, lower precision,
or streaming import into the native runtime.
