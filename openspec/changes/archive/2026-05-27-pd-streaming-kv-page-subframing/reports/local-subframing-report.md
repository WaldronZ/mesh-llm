# PD Streaming KV Page Subframing Local Report

Date: 2026-05-27

## Result

Result: inconclusive / ready_for_short_subframing_smoke

This local phase implements the `pd-kv-stream/1` page segment subframe schema,
source-side splitting, router-side reassembly, and fail-closed unit coverage.
No Mac/PGX foreground smoke, 4k smoke, or 8k smoke was run in this phase.

## Implemented Scope

- Logical KV page segment payloads can be split into bounded page subframes.
- Each subframe carries request, chunk, segment, subframe index/count, byte
  offset, subframe byte count, subframe checksum, and the original logical
  segment manifest/checksum.
- The router reassembles subframes in order and calls `import_kv_page` only
  after the full logical segment validates.
- Full-state frames remain rejected in streaming mode.
- Diagnostics use bounded labels and counts only:
  `source_subframe_write_start`, `source_subframe_write_end`,
  `router_subframe_received`, `router_segment_reassembly_start`,
  `router_segment_reassembly_end`, and `router_segment_reassembly_error`.

## Local Test Coverage

- Valid multi-subframe logical segment reassembles into the original payload.
- Missing subframe fails closed.
- Duplicate subframe fails closed.
- Out-of-order subframe fails closed.
- Byte offset gap and overlap fail closed.
- Per-subframe checksum mismatch fails closed.
- Logical checksum mismatch fails closed.
- Logical payload byte mismatch fails closed.
- Segment identity mismatch fails closed.
- Full-state frame remains rejected.
- Source-side subframe splitting keeps each payload at or below the configured
  frame cap.

## Not Yet Proven

- No foreground production serving smoke has been run with subframing.
- The 495-token regression path that previously needed a 1 GiB frame cap has
  not been rerun.
- Source control errors while the router is blocked on a page read are covered
  by the later control-error watch phase; see `control-error-watch-report`.
- 4k and 8k validation remain deferred.

## Privacy

The report contains no prompt text, generated content, complete token arrays,
KV/native payload contents, private paths, real hostnames, endpoint URLs, or
credentials.
