# Tasks: PD Streaming KV Handoff

## 1. Proposal And Spec

- [x] 1.1 Create OpenSpec proposal, design, tasks, and spec.
- [x] 1.2 Validate the proposed change with `openspec validate
      pd-streaming-kv-handoff --strict`.

## 2. Runtime Capability Audit

- [x] 2.1 Audit PGX/native runtime state export APIs for delta or page-range
      export support.
- [x] 2.2 Audit Mac/native runtime state import APIs for incremental append or
      page-range import support.
- [x] 2.3 Identify whether existing session/page identity is stable across
      prefill chunks.
- [x] 2.4 Decide whether implementation can proceed or needs a smaller native
      runtime API spike.
      Completed through the archived KV page support changes. The 128-token
      two-chunk page handoff proof passed, so this change can proceed with a
      controller/pipeline layer. Later foreground runs proved streaming
      overlap in the async and split-channel harnesses.

## 3. Protocol And Manifest Design

- [x] 3.1 Define `pd-kv-stream/1` capability and versioning.
- [x] 3.2 Define per-chunk delta/page manifest fields.
- [x] 3.3 Bind per-chunk manifests to artifact, tokenizer, chat template,
      dtype/layout, position range, checksum, and total expected chunks/tokens.
- [x] 3.4 Define final request manifest and final decode-start validation.
      Implemented as a local controller/report model in `skippy-correctness`;
      foreground transport/runtime manifests were validated through the
      `pd-kv-stream/1` source/coordinator harness. Production router manifest
      wiring remains future integration work.

## 4. Streaming Lifecycle

- [x] 4.1 Reuse the existing chunked prefill planner for chunk boundaries.
- [x] 4.2 Export chunk `N` delta/page range after prefill chunk `N`.
- [x] 4.3 Transfer chunk `N` while PGX computes chunk `N+1`, where runtime
      capability permits.
      The 128-token foreground smoke proved chunk `0` page segments were
      transferred/imported before the final gate and before chunk `1` final
      import. The async 128-token foreground smoke additionally recorded
      positive overlap (`actual_overlap_ms > 0`) through the live control/page
      stream harness. A 4k async foreground smoke later passed with
      `actual_overlap_ms > 0`. The 4k split-channel foreground smoke passed
      with `true_compute_transfer_overlap_ms=23314.888567`; 8k timing remains
      deferred.
- [x] 4.4 Import and validate chunk `N` on Mac in token position order.
- [x] 4.5 Start decode only after all chunks are contiguous and verified.
- [x] 4.6 Add a test-only live harness skeleton with explicit `source` and
      `coordinator` roles.
      `skippy-correctness kv-streaming-handoff source` now owns the PGX-side
      foreground runtime/source loop shape, and `coordinator` owns the Mac-side
      tokenization, per-chunk import, final gate, bootstrap, and baseline
      comparison shape. The 128-token PGX/Mac foreground smoke passed.
- [x] 4.7 Add an async/pipelined coordinator phase.
      The current foreground harness is serial: coordinator sends chunk `N`,
      reads all page frames, imports them synchronously, then sends chunk
      `N+1`. Phase 2 must allow chunk `N+1` dispatch while chunk `N` page
      transfer/import is still in progress, subject to capacity and ordering
      gates.
      Implemented as a local async controller simulation with a source worker,
      bounded page queue, importer worker, fail-closed final gate, and overlap
      telemetry. Live harness wiring now has an async mode with a full-duplex
      control/page stream, bounded source writer queue, coordinator reader, and
      importer worker. Foreground async and split-channel smokes both passed.

## 5. Ordering, Failure, And Cleanup

- [x] 5.1 Fail closed on out-of-order, missing, duplicated, or oversized
      chunk/frame.
- [x] 5.2 Fail closed on checksum, length, dtype/layout, identity, or position
      mismatch.
- [x] 5.3 Fail closed on Mac import failure.
- [x] 5.4 Define timeout, cancel, and cleanup behavior for PGX, transport, and
      Mac importer.
      Local harness cancellation and cleanup paths are covered by fail-closed
      tests and foreground cleanup evidence. Production-grade timeout/cancel
      hardening is deferred to a future production integration change.
- [x] 5.5 Preserve pre-content fallback and post-content no-transparent-
      fallback semantics.
      This change stays in the default-off `skippy-correctness` foreground
      harness and does not alter production router fallback behavior.
      Production router integration and post-content semantics remain future
      work.

## 6. Capacity Gates

- [x] 6.1 Add or design max in-flight KV chunks.
- [x] 6.2 Add or design max queued bytes for transport and import.
- [x] 6.3 Add or design max frame bytes and per-chunk timeout limits.
      Controller enforces frame bytes and in-flight bytes/chunks. Timeout
      limits remain part of foreground/runtime wiring.
- [x] 6.4 Keep single-request lane; do not add production scheduler behavior.

## 7. Telemetry And Reporting

- [x] 7.1 Record per-chunk prefill latency.
- [x] 7.2 Record per-chunk KV delta export, network, and import latency.
- [x] 7.3 Record overlap, pipeline idle time, TTFT, and bytes per token.
      Local controller report records chunk timings, overlap/idle, in-flight
      bytes, page bytes, bytes per token, and final decode-start position.
      The 128-token foreground smoke completed decode and exact-match baseline
      comparison. The 4k async smoke records per-chunk page bytes, transfer,
      native import, overlap, and approximate decode-ready timing against the
      previous one-shot large-state reference.
- [x] 7.7 Add async overlap telemetry.
      Required fields: prefill start/end per chunk, export start/end per chunk,
      transfer start/end per page segment, import start/end per page segment,
      actual overlap ms, source idle ms, importer idle ms,
      backpressure wait ms, in-flight bytes, and page queue depth.
      Local report:
      `reports/async-controller-report.{md,json}`. The simulation records
      `actual_overlap_ms=18` for the 128-token / 64+64 model. Foreground async
      telemetry was validated by
      `reports/foreground-async-streaming-smoke-report.{md,json}` with
      positive measured overlap and exact-match baseline comparison.
- [x] 7.8 Add split-channel source-side telemetry.
      Phase 3 local implementation adds separate control/page channel timing,
      source prefill/export start/end fields, page write/flush fields,
      writer queue wait/backpressure fields, control event emit/receive/lag
      fields, and explicit `clock_alignment_status`. The 4k split-channel
      foreground smoke passed with `true_compute_transfer_overlap_ms`
      `23314.888567`, decode-ready about `43.12s`, and exact token match.
- [x] 7.4 Keep telemetry sanitized and bounded.
- [x] 7.5 Produce report template with `pass`, `inconclusive`, and `redesign`
      outcomes.
- [x] 7.6 Produce a foreground streaming smoke report template.
      `reports/foreground-streaming-smoke-report.{md,json}` now records the
      128-token foreground pass.

## 8. Correctness Tests

- [x] 8.1 Add local manifest validation tests for positive and negative
      chunked streaming handoff.
- [x] 8.2 Add local order/duplicate/gap/overlap/checksum/import failure tests.
- [x] 8.3 Add CLI/report tests for live source/coordinator harness readiness.
- [x] 8.4 Reject full-state frames as streaming KV proof in local tests.
- [x] 8.5 Compare streaming handoff output with a deterministic baseline.
      The accepted correctness baseline for this foreground harness is
      `local_one_shot_prefill_decode`, which exact-matched the 128-token,
      4k async, and 4k split-channel streaming outputs. The earlier
      large-state framing result remains a timing/reference path, not the
      exact-match baseline for this change.
- [x] 8.6 Preserve large-state framing as fallback/reference.
      This change does not modify the production large-state framing path.
      It is kept as the known previous path and performance reference; it was
      not used as a streaming KV pass condition.

## 9. Foreground Smoke Plan

- [x] 9.1 Run the minimal 128-token / 64+64 foreground streaming smoke.
      This must use the new `kv-streaming-handoff source` +
      `kv-streaming-handoff coordinator` live harness, not the older
      `kv-page-handoff` correctness harness.
      Result: pass. PGX exported per-chunk ISWA page segments, Mac imported
      per-chunk page segments, the final contiguous gate passed,
      trim-replay-last-token bootstrap produced logits, decode started at 128,
      and local one-shot baseline comparison was exact token match. No full-
      state path was used as a streaming proof.
- [x] 9.2 Write runbook for 4k streaming KV handoff smoke.
      Captured as the foreground 4k smoke command/report path in
      `reports/foreground-4k-async-streaming-smoke-report.{md,json}`.
- [x] 9.3 Run 4k foreground smoke only after local async coordinator tests and
      explicit authorization.
      Result: pass for 4096-token synthetic prompt, four 1024-token chunks,
      real per-chunk PGX `export_kv_page`, real per-chunk Mac `import_kv_page`,
      final gate pass, trim-replay bootstrap pass, and exact-match local
      one-shot baseline comparison. No full-state path was used as proof.
- [x] 9.4 Treat 8k as optional and run only after 4k pass.
      8k remains deferred/future after the 4k split-channel pass.
- [x] 9.5 Do not attempt 32k/128k/256k production validation in this change.
      No 32k/128k/256k validation was run or claimed.

## 10. Phase 2 Async/Pipelined Coordinator

- [x] 10.1 Decide transport shape for the foreground harness:
      control channel + page stream channel, or single connection with
      request pipelining and a background reader.
- [x] 10.2 Add a bounded page segment queue and importer worker.
- [x] 10.3 Dispatch chunk `N+1` after chunk `N` export is accepted and capacity
      permits, without waiting for chunk `N` import completion.
- [x] 10.4 Preserve `out_of_order_policy=fail_closed` for the first async
      proof; out-of-order buffering is out of scope unless explicitly added and
      tested.
- [x] 10.5 Enforce `max_in_flight_chunks`, `max_in_flight_bytes`,
      `max_frame_bytes`, page queue depth, and backpressure.
- [x] 10.6 Add cancellation, timeout, and cleanup paths for source session,
      transport reader, and importer worker.
- [x] 10.7 Keep final decode gated on contiguous imports, bootstrap readiness,
      and exact-match local baseline comparison.
- [x] 10.8 Add local tests:
      chunk `1` can be dispatched while chunk `0` import is pending,
      backpressure blocks dispatch, importer failure fails closed, source
      failure cancels importer, final gate refuses incomplete imports, and
      full-state frames still cannot pass.
- [x] 10.9 Run a 128-token async foreground smoke and require
      `actual_overlap_ms > 0` or a clearly bounded explanation of why the
      request is too small to measure overlap.
      Result: pass. The foreground async run recorded positive measured
      overlap, per-chunk page transfer/import telemetry, final gate pass,
      trim-replay bootstrap pass, and exact-match local one-shot baseline
      comparison. No full-state path was used as proof.
- [x] 10.10 Only after 10.9 passes, request authorization for 4k streaming
      smoke.
      Completed after explicit authorization. The 4k async foreground smoke
      passed, but 8k remains deferred pending review of the 4k timing and
      overlap behavior.

## 11. Phase 2 Live Async Harness

- [x] 11.1 Add `--pipeline-mode async` to live source and coordinator roles.
- [x] 11.2 Implement the source-side bounded page writer:
      source prefill/export enqueues page frames and can return to the control
      loop for the next chunk without waiting for Mac import completion.
- [x] 11.3 Implement the coordinator-side background reader and importer
      worker:
      page frames are read from the full-duplex stream, validated, imported,
      and committed through the final contiguous gate.
- [x] 11.4 Keep serial foreground mode available as the baseline lifecycle.
- [x] 11.5 Add async live harness readiness report:
      `reports/async-live-harness-report.{md,json}`.
      Result remains `inconclusive` and recommendation is
      `ready_for_async_foreground_smoke`; no PGX/Mac process was started in
      this phase.
- [x] 11.6 Add local tests for async CLI flags and source page/control frame
      emission.
- [x] 11.7 Run 128-token async foreground smoke with real PGX/Mac source and
      coordinator.
      Result: pass. PGX source exported per-chunk KV page segments, Mac
      coordinator imported per-chunk page segments through the async
      control/page stream harness, and cleanup released the smoke port.

## 12. Phase 3 Split Control/Page Channel

- [x] 12.1 Add `--pipeline-mode split-channel` to local, source, and
      coordinator roles.
- [x] 12.2 Add source control and page bind addresses:
      `--control-bind-addr`/`--bind-addr` for lifecycle events and
      `--page-bind-addr` for page frames.
- [x] 12.3 Add coordinator control and page addresses:
      `--control-addr`/`--source-addr` for lifecycle events and `--page-addr`
      for page frames.
- [x] 12.4 Implement split-channel source loop.
      Control ACKs are written on the control channel; page frames are sent
      through a bounded page writer queue on the page stream. The source can
      return to the control loop after enqueueing page frames instead of
      waiting for Mac import completion.
- [x] 12.5 Implement split-channel coordinator readers.
      Separate control and page readers feed the existing fail-closed importer
      and final contiguous gate. `chunk_done` does not commit a chunk until the
      expected page segments have been imported.
- [x] 12.6 Preserve existing serial and single-stream async modes.
- [x] 12.7 Add local tests for split-channel CLI flags, control/page
      separation, source-side overlap fields, and report sanitization.
- [x] 12.8 Add split-channel readiness report:
      `reports/split-channel-telemetry-report.{md,json}`.
      Result remains `inconclusive` with recommendation
      `ready_for_split_channel_4k_smoke`; no PGX/Mac process was started in
      this phase.
- [x] 12.9 Run 4k split-channel foreground smoke and compare timing against
      the single-stream async 4k smoke.
      Result: pass. The 4096-token, four 1024-token chunk smoke used
      split control/page channels, observed real PGX `export_kv_page` and Mac
      `import_kv_page` per chunk, passed final gate and trim/replay bootstrap,
      exact-matched `local_one_shot_prefill_decode`, and did not use
      full-state handoff as proof. Decode-ready improved from about `58.93s`
      to about `43.12s`; overlap improved from `0.252ms` to about `23.31s`.
- [x] 12.10 Keep 8k deferred until split-channel 4k telemetry proves useful
      overlap or records a bounded reason why overlap is still limited.
      8k is explicitly deferred/future. Production router integration,
      payload reduction, and production timeout/cancel hardening are also
      deferred.
