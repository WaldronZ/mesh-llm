# Tasks: PD Chunked Prefill

## 1. Protocol / Design Prep

- [x] 1.1 Define chunked prefill request and ACK metadata: session id, token
      range, start position, end position, chunk index, chunk count, and
      bounded error labels.
- [x] 1.2 Define how chunked prefill extends existing binary stage transport
      without breaking non-PD binary serving.
- [x] 1.3 Define compatibility behavior when the PGX worker does not advertise
      chunked prefill capability.
- [x] 1.4 Define fail-closed behavior for position mismatch, chunk range
      mismatch, unsupported chunk metadata, and manifest mismatch.

## 2. Runtime / Binary Chunked Prefill Support

- [x] 2.1 Add or identify runtime API support for prefill chunks that advance
      one persistent session state.
- [x] 2.2 Ensure each chunk stays within `n_batch` / `max_prefill_batch` /
      configured safety margin.
- [x] 2.3 Ensure final export happens only after the last chunk ACK.
- [x] 2.4 Ensure `decode_start_position` equals the total consumed prompt
      tokens.
- [x] 2.5 Confirm non-chunked binary serving remains unchanged.

## 3. Router Lifecycle / Admission Integration

- [x] 3.1 Teach admission to distinguish one-shot prefill from chunked prefill
      capability.
- [x] 3.2 Admit 4k/8k only when chunked prefill capability and all token,
      context, KV byte, memory, network/SLA, and lifecycle gates pass.
- [x] 3.3 Keep over-policy requests pre-content fallback/reject before PGX.
- [x] 3.4 Keep `inflight_limit=1` and busy/admission behavior intact.
- [x] 3.5 Ensure missing chunked capability keeps current safe rejection for
      4k/8k.

## 4. Manifest / Telemetry / Reporting

- [x] 4.1 Add `pd-handoff/1` chunked provenance fields as additive manifest
      metadata.
- [x] 4.2 Validate chunked provenance before Mac import/decode.
- [x] 4.3 Emit chunk count, per-chunk tokens, per-chunk latency, total prefill
      latency, final KV payload bytes, export/import/network timing, TTFT, and
      decode tokens/sec.
- [x] 4.4 Ensure telemetry and reports exclude prompt text, complete token
      arrays, generated content, KV payload contents, credentials, private
      paths, endpoint URLs, and real machine labels.
- [x] 4.5 Add or update a sanitized chunked prefill report template.

## 5. Lifecycle / Cancel / Cleanup

- [x] 5.1 Clean up PGX chunked prefill sessions on success.
- [x] 5.2 Clean up PGX and coordinator state on chunk reject/error.
- [x] 5.3 Clean up on timeout and client cancel.
- [x] 5.4 Preserve pre-content fallback/rejection and post-content no
      transparent fallback semantics.
- [x] 5.5 Ensure cleanup failures are recorded as sanitized secondary errors.

## 6. Local Tests

- [x] 6.1 Test chunk planner splits 4k and 8k token counts into safe ranges.
- [x] 6.2 Test position continuity and final decode start position.
- [x] 6.3 Test missing capability rejects/fallbacks before PGX.
- [x] 6.4 Test chunk ACK, reject, error, timeout, and cancel state transitions.
- [x] 6.5 Test manifest provenance positive and negative validation.
- [x] 6.6 Test telemetry privacy and required metric presence.
- [x] 6.7 Test normal path, one-shot PD path, and Skippy split path remain
      unaffected.
- [x] 6.8 Run relevant cargo fmt/test/check commands serially.
- [x] 6.9 Run `openspec validate pd-chunked-prefill --strict`.

## 7. Runbook

- [x] 7.1 Document required CLI/config flags for chunked prefill.
- [x] 7.2 Document 4k and 8k synthetic prompt construction without recording
      prompt text.
- [x] 7.3 Document foreground process startup, stop, and port-release checks.
- [x] 7.4 Document failure injection or manual interruption steps for chunk
      error, timeout, cancel, and cleanup validation.
- [x] 7.5 Document report fields and privacy constraints.

## 8. Optional 4k / 8k Foreground Smoke

- [x] 8.1 Start Mac/PGX foreground observable validation processes only after
      separate explicit authorization.
- [x] 8.2 Baseline below the current one-shot limit is deferred. It was not
      rerun for this 4k-only foreground smoke and is not required to close the
      current 4k chunked prefill scope.
- [x] 8.3 Run 4k admitted through chunked prefill and final Mac decode.
- [x] 8.4 8k admitted foreground smoke is optional/future validation. It
      requires separate authorization and is not part of the current archive
      scope.
- [x] 8.5 Over-policy foreground smoke is deferred. Prior admission changes
      cover pre-PGX rejection/fallback semantics; it was not rerun here and is
      not required for the 4k chunked prefill pass.
- [x] 8.6 Confirm PGX process survival and cleanup.
- [x] 8.7 Record sanitized smoke report without prompt text, generated content,
      complete token arrays, KV payload contents, credentials, private paths,
      endpoint URLs, or real machine labels.

Notes:

- 4k foreground smoke was rerun with explicit authorization after
  `pd-large-state-framing`. It passed: admission admitted, chunked prefill used
  4 chunks, PGX final export used large-state framing, Mac import/decode
  completed, SSE reached normal completion, and all foreground validation
  processes survived until manual stop.
- 8k smoke remains intentionally unrun and is deferred to future validation.
- Chat UI reasoning/final-answer separation, `finish_reason=length` truncation
  UX, and configurable `max_output_tokens` are out of scope for this change and
  should be handled by a later UI/UX change.
