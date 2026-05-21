# Tasks: PD Large State Framing

## 1. Protocol Investigation

- [x] 1.1 Confirm all current places where `StateImport` payload length is
      encoded or decoded through `i32` / `token_count`.
- [x] 1.2 Document current small-payload compatibility expectations.
- [x] 1.3 Define the explicit capability name/version for large-state framing.

## 2. Framing Design

- [x] 2.1 Decide the concrete MVP wire shape for large-state start/data/end
      frames or equivalent explicit envelope.
- [x] 2.2 Define per-frame max bytes, total payload max bytes, and memory
      allocation bounds.
- [x] 2.3 Define how old `StateImport` and new large-state framing are selected.
- [x] 2.4 Define explicit reject behavior when capability or limits are missing.

## 3. Integrity And Manifest Binding

- [x] 3.1 Add payload byte count, frame count, checksum algorithm, and checksum
      to the large-state metadata model.
- [x] 3.2 Bind large-state metadata to `pd-handoff/1` manifest validation.
- [x] 3.3 Ensure Mac import/decode starts only after payload integrity and
      manifest validation pass.

## 4. Failure Semantics

- [x] 4.1 Fail closed on partial frame, truncated stream, frame count mismatch,
      offset mismatch, duplicate/out-of-order frame, and oversized frame.
- [x] 4.2 Fail closed on checksum mismatch and import failure.
- [x] 4.3 Preserve pre-content fallback/rejection and post-content no
      transparent fallback semantics.
- [x] 4.4 Ensure cleanup releases request capacity and does not leave ambiguous
      import state.

## 5. Telemetry And Privacy

- [x] 5.1 Emit sanitized payload bytes, frame count, frame bytes, write/read
      latency, checksum latency, and bounded result labels.
- [x] 5.2 Ensure logs/reports do not include prompt text, complete token arrays,
      generated content, KV/native state payload contents, credentials, private
      paths, endpoint URLs, or real machine labels.

## 6. Tests

- [x] 6.1 Test small payload backward-compatible `StateImport`.
- [x] 6.2 Test payload just below the legacy `i32` limit without requiring a
      multi-GB fixture.
- [x] 6.3 Test payload over the legacy limit uses large-state framing or
      explicit reject according to capability.
- [x] 6.4 Test truncated stream / partial frame fail-closed behavior.
- [x] 6.5 Test checksum mismatch fail-closed behavior.
- [x] 6.6 Test import failure after valid transfer fails closed.
- [x] 6.7 Test telemetry privacy and required metric presence.
- [x] 6.8 Run relevant cargo fmt/test/check commands serially.
- [x] 6.9 Run `openspec validate pd-large-state-framing --strict`.

## 7. Follow-Up Validation

- [x] 7.1 After local implementation passes, rerun only the
      `pd-chunked-prefill` 4k foreground smoke with separate explicit
      authorization.
- [x] 7.2 8k validation is deferred/future validation. The separately
      authorized 4k rerun proved Mac import/decode and SSE normal completion
      with large-state framing; 8k is not part of this archive scope.

Local implementation, local validation, and the separately authorized 4k
foreground smoke are complete. The 4k rerun proved large-state framing on final
PGX export, Mac import/decode success, and normal SSE completion. 8k remains
intentionally unrun and deferred to future validation.
