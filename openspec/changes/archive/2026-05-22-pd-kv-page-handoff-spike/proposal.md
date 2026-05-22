# Change: PD KV Page Handoff Spike

## Why

The `pd-streaming-kv-handoff` read-only runtime capability audit found an
important boundary: the native/runtime layer already exposes KV page/range
APIs, but the current PD live path and binary protocol do not yet carry KV
pages as ordered handoff units.

Evidence from the audit:

- `RuntimeKvPageDesc`, `export_kv_page`, and `import_kv_page` exist in the
  skippy runtime surface.
- The native llama.cpp stage ABI exposes `stage_export_kv_page` and
  `stage_import_kv_page`.
- The current 4k chunked prefill path still performs a final full-state
  one-shot handoff through large-state framing.
- Large-state framing is a full payload framing mechanism, not streaming KV
  handoff.

Before implementing a full streaming pipeline, the project needs a narrow
proof that page-level KV handoff can cross the target heterogeneous runtime
boundary: PGX CUDA prefill to Mac Metal decode.

## What Changes

This proposal defines a spike that proves or disproves page-level KV handoff
correctness:

1. PGX pre-fills chunk `0`;
2. PGX exports KV page/range `0`;
3. PGX pre-fills chunk `1`;
4. PGX exports KV page/range `1`;
5. Mac imports the pages in token-position order;
6. Mac decodes from the final imported position;
7. the result is compared with the existing one-shot full-state baseline.

This change is not a performance optimization and does not implement the
complete `pd-streaming-kv-handoff` pipeline. Its output is a spike report with
one of `pass`, `inconclusive`, or `redesign`.

## Scope

Must:

- prove KV page export/import correctness, not production streaming
  performance;
- define a minimal deterministic two-or-more-chunk test path;
- define page-level manifest metadata for identity, layout, position, and
  integrity;
- define negative checks for missing, duplicate, out-of-order, overlapping,
  mismatched, or corrupted pages;
- compare Mac decode after page import against the one-shot full-state
  baseline under deterministic settings;
- report page export, transfer/read/write, import, bytes, total pages, and
  decode TTFT after import;
- fail closed if the implementation falls back to a full-state blob or cannot
  validate page identity.

Should:

- reuse the chunked prefill planner position model;
- reuse large-state framing checksum concepts at page granularity;
- start with a small two-chunk proof before trying 4k;
- run 4k only after the small proof passes;
- keep PD serving default-off and capability-gated.

Won't:

- implement the full `pd-streaming-kv-handoff` pipeline;
- overlap prefill, export, network, and import yet;
- promise or validate 8k/32k/128k/256k;
- add KV compression;
- change KV precision or low-precision compatibility;
- add multi-worker placement, scheduler behavior, or production concurrency;
- change Chat UI behavior.

## Impact

This proposal is docs/spec only. It does not apply code changes, start remote
processes, archive existing changes, commit, or push.

When applied later, expected implementation areas are limited to a temporary
spike harness or local binary-control path, page manifest validation, local
tests, and separately authorized foreground machine validation.
