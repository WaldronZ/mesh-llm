# PD Streaming KV Page Subframing Control Error Watch Report

Date: 2026-05-27

## Result

Result: inconclusive / ready_for_4k_subframing_smoke

This local phase adds a router-side concurrent control-error watch while the
production `pd-kv-stream/1` router is blocked on page-stream reads or subframe
reassembly. No Mac/PGX foreground smoke, 4k smoke, or 8k smoke was run in this
phase.

## Implemented Scope

- The router starts a bounded control watcher after `ExportCompleted` and before
  reading page subframes for a chunk.
- If the source emits a control `Error` while the router is waiting for the next
  page frame or subframe, the router interrupts the page read, emits
  `router_control_error_received`, cleans up the request, and returns the source
  error reason.
- Normal page-stream success remains unchanged. If the watcher observes
  `ChunkDone` early, the router caches it and validates it after all logical
  segments are reassembled.
- Control EOF or malformed control frames during page receive fail closed with
  bounded control/page error labels and cleanup.
- Full-state fallback remains unavailable in streaming KV mode.

## Local Test Coverage

- Source control error while waiting for the first page frame is observed as the
  source error instead of `page_read_timeout`.
- Source control error while waiting for a later subframe is observed as the
  source error instead of `page_read_timeout`.
- Page-stream success is unaffected when `ChunkDone` is observed before the
  page payload has finished reading.
- Control EOF mid-page fails closed and interrupts the page wait.
- Source `source_frame_too_large` control errors clean up and do not leave the
  next mocked request blocked behind a stale generation slot.
- Existing page timeout behavior still returns `page_read_timeout` when no
  control error is available.
- Full-state frames remain rejected in streaming mode.

## Remaining Validation

- 4k subframing foreground smoke has not been run yet.
- 8k remains deferred.
- Production-grade overlap, TTFT, backpressure, and payload-reduction work
  remain out of scope for this change.

## Privacy

This report contains no prompt text, generated content, complete token arrays,
KV/native payload contents, private paths, real hostnames, endpoint URLs, or
credentials.
