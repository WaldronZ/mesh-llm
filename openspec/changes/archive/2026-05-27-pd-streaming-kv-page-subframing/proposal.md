# Proposal: PD Streaming KV Page Subframing

## Goal

Replace the current one-frame-per-logical-KV-segment page stream shape with
bounded page subframes for `pd-kv-stream/1`, so large ISWA `swa` segments no
longer require a 1 GiB frame cap and no longer appear to the router as a
misleading page stream timeout.

## Problem

Production `pd-kv-stream/1` has passed short and 4k correctness smokes, but
the page stream still carries each logical KV segment as a single frame. That
works for small prompts and for the 4k correctness harness when capacity is
large enough, but it is not a good production pipeline shape:

- 4k total page bytes are `3,690,987,520`.
- The observed byte rate is `901,120` bytes per prompt token.
- A 495-token manual request produced `446,054,400` page bytes.
- That request's ISWA `base` segment was `40,550,400` bytes.
- That request's ISWA `swa` segment was `405,504,000` bytes.

With the old `64 MiB` frame cap, source rejected the `iswa/swa` segment with
`frame_too_large`. The router was blocked waiting for that page frame and
surfaced the failure as `page_read_timeout`. The manual environment currently
uses `--pd-stream-max-frame-bytes 1073741824` as a workaround. That restores
correctness for the manual path, but it increases head-of-line blocking risk,
memory pressure, and makes future overlap telemetry less meaningful.

## Scope

This change defines and implements bounded page subframes for production
`pd-kv-stream/1`. A logical KV page segment can be split into multiple
subframes on the source side, reassembled by the router, validated against the
logical segment manifest, and then imported through the existing
`import_kv_page` runtime API.

The first implementation should preserve the current logical segment import
path. It must not assume native streaming import support.

## Non-Goals

- No 8k requirement.
- No KV compression or lower precision.
- No production async coordinator/importer worker.
- No scheduler, multi-worker placement, or production concurrency work.
- No UI changes.
- No production performance-readiness claim.
- No full-state fallback as a streaming KV pass.
- No prompt text, generated content, full token arrays, KV/native payload
  contents, private paths, real hostnames, endpoint URLs, or credentials in
  diagnostics or reports.
