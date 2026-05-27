# Tasks: PD Streaming KV Production Integration

## 1. Proposal And Scope

- [x] 1.1 Create OpenSpec proposal, design, tasks, and spec.
- [x] 1.2 Validate with `openspec validate
      pd-streaming-kv-production-integration --strict`.

## 2. Read-Only Integration Audit

- [x] 2.1 Audit the current `--pd-serving-mvp` OpenAI router path.
- [x] 2.2 Identify where final full-state export/import is currently invoked.
      Current production path still dispatches chunked prefill and then calls
      final full-state export/import in the PD MVP router path.
- [x] 2.3 Map the `skippy-correctness` split-channel harness lifecycle to
      production serving components.
- [x] 2.4 Identify production-only lifecycle gaps: cancellation, timeout,
      cleanup, SSE failure semantics, and status reporting.
- [x] 2.5 Decide exact config/CLI names and invalid combination behavior.

## 3. Default-Off Configuration

- [x] 3.1 Add explicit streaming KV serving config, such as
      `--pd-streaming-kv-handoff`.
- [x] 3.2 Keep existing `--pd-serving-mvp` behavior unchanged unless streaming
      is explicitly enabled.
- [x] 3.3 Reject streaming KV without PD serving enabled.
- [x] 3.4 Reject missing streaming channel config and invalid capacity limits
      with clear bounded errors.
- [x] 3.5 Add config tests for default-off and invalid combinations.
- [x] 3.6 Defer live streaming peer capability handshake to future production
      capability policy work. This change validates explicit configured
      source/control/page channels and does not claim automatic peer discovery
      or capability negotiation.

## 4. Production Lifecycle Integration

- [x] 4.0 Add a production streaming KV mode skeleton that fails before
      content and does not fall back to full-state handoff.
- [x] 4.1 Reuse coordinator-owned tokenization and existing PD admission.
- [x] 4.2 Reuse the chunked prefill planner for token ranges.
- [x] 4.3 Establish source-side request-scoped split control/page
      listeners in `serve-binary` and connect the Mac router client to those
      production source listeners.
- [x] 4.4 Dispatch per-chunk token-id prefill requests through the
      production source loop when a valid `pd-kv-stream/1` control request is
      received.
- [x] 4.5 Export per-chunk KV page segments from the source runtime and write
      production page frames. Short and 4k foreground serving smokes later
      validated the live source path.
- [x] 4.6 Import per-chunk KV page segments on Mac in the production router
      backend using request-scoped runtime sessions.
- [x] 4.7 Preserve Gemma4 ISWA `base`/`swa` segment handling in production
      source manifests.
- [x] 4.8 Keep final decode gated on contiguous imported chunks.
- [x] 4.9 Add `trim-replay-last-token` bootstrap before decode.
- [x] 4.10 Verify final `decode_start_position` before SSE content is emitted.
- [x] 4.11 Add a mockable production lifecycle trait covering chunk dispatch,
      segment validation/import, final gate, bootstrap, decode, and cleanup.
      The production router now has both local lifecycle coverage and live
      source/client foreground validation.
- [x] 4.12 Add local lifecycle tests proving happy path reaches the decode gate
      and source/checksum/order/bootstrap failures fail before assistant
      content.
- [x] 4.13 Add production source wire/parser tests for protocol version,
      required token-id payloads, page frame identity/full-state rejection, and
      sanitized error events.

## 5. Manifest, Provenance, And Capacity

- [x] 5.1 Add `pd-kv-stream/1` production manifest/provenance records.
- [x] 5.2 Bind chunk index, token range, expected chunk/token totals, cache kind,
      segment kind, layer range, dtype/layout, payload bytes, checksum,
      artifact identity, tokenizer hash, and chat template hash.
- [x] 5.3 Enforce max frame bytes.
- [x] 5.4 Enforce max in-flight chunks.
- [x] 5.5 Enforce max in-flight or queued bytes.
- [x] 5.6 Ensure full-state frames cannot satisfy a streaming KV proof.

## 6. Failure Semantics And Cleanup

- [x] 6.1 Fail closed on checksum, length, dtype/layout, identity, cache/segment
      kind, order, duplicate, gap, overlap, and full-state-frame misuse in
      local manifest validation.
- [x] 6.1a Fail closed on runtime import and bootstrap mismatch in the live
      production lifecycle.
- [x] 6.2 Define pre-content rejection/fallback behavior.
- [x] 6.3 Define post-content no-transparent-fallback behavior.
- [x] 6.4 Send source stop/cleanup when coordinator/importer fails in the
      router backend.
- [x] 6.5 Close importer/page stream state when PGX source fails.
- [x] 6.6 Clean up request-scoped runtime sessions and source streams in local
      production lifecycle tests. Foreground short and 4k smoke also verified
      the smoke role ports were released after SIGINT cleanup.
- [x] 6.6a Ensure the local production lifecycle calls backend cleanup on
      success and on tested failure paths. Live request-scoped channel/session
      cleanup was exercised by short and 4k foreground smokes.
- [x] 6.6b Clean up source runtime sessions on source stop/error paths in the
      local production source implementation. Production-grade timeout/cancel
      hardening remains deferred.
- [x] 6.6c Keep production source control/page listeners alive across
      request-scoped EOF, bad frame, request error, and stop events. The source
      now returns to serial accept for the next control/page pair instead of
      silently ending the source listener thread.
- [x] 6.7 Add local tests for failure and cleanup paths where possible.
      Config, manifest, source wire, lifecycle, and cleanup tests cover the
      local production path; foreground smokes cover the happy-path live
      source/router cleanup.

## 7. Telemetry And Privacy

- [x] 7.1 Emit sanitized streaming enabled/protocol/chunk count telemetry.
- [x] 7.1a Emit direct, grep-friendly, bounded lifecycle diagnostics with
      stable prefix `pd.kv_stream.lifecycle` for production source listener,
      source request/chunk/prefill/export/page-write/chunk-done, router page
      receive/import, final gate, trim-replay bootstrap, decode start, and
      cleanup.
- [x] 7.2 Defer production-grade per-chunk prefill/export/transfer/import
      timing to future performance telemetry work. This change emits bounded
      lifecycle diagnostics and records 4k correctness evidence, but does not
      claim production performance readiness.
- [x] 7.3 Defer production-grade overlap metrics, clock alignment status,
      control lag, writer wait, backpressure, queue depth, in-flight bytes,
      decode-ready, and TTFT to future performance telemetry work. Current
      reports explicitly mark missing per-phase timing as `not_recorded` and
      do not treat production request elapsed time as the same benchmark as
      previous harness decode-ready/overlap numbers.
- [x] 7.4 Emit bounded failure reason labels.
- [x] 7.5 Add telemetry privacy tests or review ensuring no prompt text,
      generated content, full token arrays, KV/native payload, credentials,
      private paths, endpoint URLs, or real machine labels are logged.
      Direct diagnostics now have unit coverage for bounded/sanitized source
      and router lifecycle lines.

## 8. Regression Tests

- [x] 8.1 Existing normal/local OpenAI serving path remains unchanged.
- [x] 8.2 Existing full-state `--pd-serving-mvp` path remains unchanged when
      streaming flag is disabled.
- [x] 8.3 Chunked prefill path remains unchanged when streaming flag is
      disabled.
- [x] 8.4 Requested streaming mode fails before content while the production
      lifecycle is still unavailable.
- [x] 8.4a Defer missing streaming peer capability rejection/fallback to future
      capability policy work. Explicit configured control/page channels are
      validated in this change; automatic live capability detection is not
      claimed.
- [x] 8.5 Full-state handoff remains available as explicit fallback/reference,
      not as a streaming proof.
- [x] 8.6 Mocked production lifecycle reaches decode only after contiguous
      import and bootstrap.
- [x] 8.7 Mocked source, manifest, and bootstrap failures do not emit assistant
      content and do not fall back to full-state handoff.
- [x] 8.8 Router client backend imports segments, bootstraps, and decodes
      through the same production lifecycle abstraction. Short foreground
      serving smoke now validates the production source/client path for a
      bounded request.

## 9. Foreground Validation Plan

- [x] 9.1 Prepare a short production serving smoke plan.
- [x] 9.2 Prepare a 4k production serving smoke plan.
- [x] 9.3 Run short request foreground smoke only after explicit authorization.
      First run exposed a source listener lifetime issue. After the listener
      lifetime fix, the rerun passed: the production router completed a bounded
      short `/v1/chat/completions` streaming request with HTTP 200, SSE DONE,
      non-empty assistant content, `pd-kv-stream/1` enabled, no full-state pass
      path, and source control/page listeners still available after the request.
- [x] 9.3a Fix the production source listener/routing blocker locally.
- [x] 9.3b Rerun the short foreground smoke before any 4k production serving
      smoke. Result: pass within short production serving scope. Direct
      source/router lifecycle diagnostics were observed for source listener,
      source request, chunk receive, prefill, export, page frame write, router
      connect, chunk dispatch, control events, page receive, import, final gate,
      trim-replay bootstrap, decode start, and cleanup. The pass did not use
      the full-state path or the `skippy-correctness` harness.
- [x] 9.3c Add direct source/router lifecycle diagnostics before larger
      production smokes. The diagnostics rerun passed and the report records
      only bounded metadata: prompt id, token count, chunk/segment counts,
      segment kinds, byte counts, boolean validation results, and finish
      metadata.
- [x] 9.4 Run 4k foreground smoke only after explicit authorization.
      Result: pass within 4k production serving scope. The production
      `/v1/chat/completions` path completed a 4096-token synthetic prompt with
      HTTP 200, SSE DONE, non-empty assistant content, `pd-kv-stream/1`
      diagnostics, four 1024-token chunks, eight ISWA `base`/`swa` segments,
      `decode_start_position=4096`, `logits_ready=true`, no full-state pass
      path, and no transparent fallback.
- [x] 9.5 Compare production 4k telemetry against the existing harness proof
      and full-state reference. The production smoke uses the same 4k page
      footprint as the split-channel harness (3,690,987,520 bytes;
      901,120 bytes/token) and proves the production serving lifecycle.
      Production direct diagnostics do not yet emit complete per-phase timing
      buckets, so request elapsed time is not treated as the same benchmark as
      the earlier harness decode-ready/overlap numbers.
- [x] 9.6 Keep 8k deferred/future for this change.
- [x] 9.7 Do not run 32k/128k/256k validation in this change.

## 10. Documentation And Closure

- [x] 10.1 Update runbook/report template for production streaming KV serving.
- [x] 10.2 Document default-off flags and invalid combinations.
- [x] 10.3 Document fallback/rejection behavior before and after SSE content.
- [x] 10.4 Document out-of-scope items: 8k requirement, 32k/128k/256k
      validation, production performance telemetry/readiness,
      timeout/cancel hardening, payload reduction, KV compression, low
      precision, multi-worker placement, scheduler behavior, production
      concurrency, UI, public mesh/cross-owner serving, and default-on PD.
