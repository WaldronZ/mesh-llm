# Tasks

## 1. Protocol And Scope

- [x] Document the `pd-kv-stream/1` page subframe schema.
- [x] Define how subframe metadata references the logical segment manifest.
- [x] Decide the initial policy for subframe ordering. Proposed first policy:
      fail closed on out-of-order subframes.
- [x] Document current capacity boundary: a `64 MiB` `max_frame_bytes` cap is
      validated by short, 494-token regression, and 4k-class foreground smokes;
      `max_in_flight_bytes=1 GiB` is still required for 1024-token ISWA/SWA
      logical segment reassembly before the existing `import_kv_page` call.
      Future logical-segment/in-flight policy, native streaming import, and
      payload reduction remain deferred.
- [x] Keep full-state handoff explicitly out of the streaming pass path.

## 2. Source Subframe Writer

- [x] Split each exported logical KV page segment into bounded subframes.
- [x] Preserve existing logical segment manifest and checksum behavior.
- [x] Add per-subframe checksum metadata.
- [x] Emit sanitized source diagnostics for subframe write start/end and
      source frame-size failures.
- [x] Fail closed if a logical segment cannot be split or written completely.

## 3. Router Reassembly And Import

- [x] Reassemble subframes by request, chunk, and segment.
- [x] Validate subframe count or final marker.
- [x] Validate byte offsets, payload length, per-subframe checksum, logical
      segment checksum, identity, cache kind, segment kind, dtype/layout, and
      token range.
- [x] Call `import_kv_page` only after the full logical segment is reassembled.
- [x] Preserve final contiguous gate and trim-replay bootstrap semantics.

## 4. Control Error Visibility

- [x] Ensure valid large logical segments no longer surface as source
      `frame_too_large`; invalid subframe/config frame-size failures remain
      bounded as `source_frame_too_large`.
- [x] Ensure source `frame_too_large` or other control errors are observed as
      bounded control errors instead of surfacing as misleading
      `page_read_timeout`.
- [x] Add diagnostics for source subframe writes, router subframe receipt, and
      segment reassembly failures.
- [x] Add a concurrent/bounded control-error observer while the router is
      blocked on page-stream reads.

## 5. Tests

- [x] Valid multi-subframe logical segment reassembles and imports.
- [x] Missing subframe fails closed.
- [x] Duplicate subframe fails closed.
- [x] Out-of-order subframe fails closed under the initial policy.
- [x] Byte offset gap fails closed.
- [x] Byte offset overlap fails closed.
- [x] Per-subframe checksum mismatch fails closed.
- [x] Logical segment checksum mismatch fails closed.
- [x] Payload byte count mismatch fails closed.
- [x] Segment identity/cache kind/segment kind mismatch fails closed.
- [x] Source frame-size failure reaches router as a bounded control error.
- [x] Full-state frame remains rejected in streaming mode.
- [x] Diagnostics and reports are sanitized.

## 6. Foreground Validation

- [x] Run short production serving smoke with smaller frame cap.
- [x] Run 495-token regression smoke and confirm large `iswa/swa` payload no
      longer needs 1 GiB single-frame cap.
- [x] Confirm no `frame_too_large` / misleading `page_read_timeout` for the
      regression path.
- [x] Compare against the 1 GiB cap workaround in the report.
- [x] Run 4k subframing smoke after short and 495-token regression smokes pass.
- [x] Keep 8k deferred.

## 7. Deferred

- [x] Keep production async coordinator/importer worker deferred.
- [x] Keep production-grade overlap and TTFT telemetry deferred.
- [x] Keep KV compression or lower precision deferred.
- [x] Keep scheduler, multi-worker placement, and production concurrency
      deferred.
- [x] Keep UI changes out of scope.
