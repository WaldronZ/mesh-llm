# Tasks: PD KV Page Decode Bootstrap

## 1. Proposal And Scope

- [x] Create proposal, design, tasks, and spec for the decode bootstrap spike.
- [x] Define the boundary as page-import decode readiness, not streaming KV
      pipeline work.
- [x] Document why this follows `pd-kv-page-memory-type-support` and blocks
      `pd-streaming-kv-handoff`.

## 2. Read-Only Runtime Audit

- [x] Identify where `import_kv_page` writes KV data and what session state it
      updates.
- [x] Identify how the runtime represents current logits/output buffers.
- [x] Identify whether decode position is set by page import or must be set by
      a separate bootstrap step.
- [x] Determine whether the target runtime can evaluate a final prompt token
      for logits without duplicating KV entries.
- [x] Determine whether a minimal non-KV decode-state export/import API exists
      or is needed.

## 3. Bootstrap Strategy Selection

- [x] Evaluate Mac-side last-token bootstrap.
- [x] Evaluate PGX-provided decode seed token/position metadata.
- [x] Evaluate minimal non-KV decode-state export/import.
- [x] Evaluate final logits/output buffer export/import if native support
      exists.
- [x] Select a recommended spike path and document why rejected strategies are
      unsafe or too broad.

## 4. Harness Implementation

- [x] Extend the `pd-kv-page-handoff-spike` coordinator to perform the selected
      bootstrap after page import.
- [x] Ensure the harness records `logits_ready` before sampling.
- [x] Ensure the harness fails closed when bootstrap is unavailable or unsafe.
- [x] Ensure full-state baseline remains separate from page handoff proof.
- [x] Ensure full-state export/import cannot be used to pass the page path.

## 5. Correctness Validation

- [x] Rerun the small 128-token two-chunk foreground proof after bootstrap
      implementation.
- [x] Verify page import completes before bootstrap.
- [x] Verify decode start position equals imported token count or the
      documented bootstrap-adjusted position.
      Proven in the retry after direct ISWA trim support was linked into the
      Mac Metal correctness binary: `decode_start_position=128`.
- [x] Compare page-path decode with the one-shot baseline under deterministic
      settings.
      Proven by the follow-up foreground smoke using the local one-shot
      prefill/decode baseline.
- [x] If mismatch occurs, record bounded first divergence metadata and keep
      the result non-pass.
      No mismatch occurred in the follow-up foreground smoke.

## 6. Safety And Negative Tests

- [x] Missing logits and no bootstrap fails closed.
- [x] Stale logits cannot be sampled.
- [x] Position mismatch fails closed.
- [x] Bootstrap that would require full prompt re-prefill fails or remains
      no-go for streaming KV.
- [x] Full-state fallback cannot be counted as page bootstrap pass.
- [x] Reports exclude prompt text, generated content, complete token arrays,
      KV/native payload contents, credentials, private paths, endpoint URLs,
      real machine labels, raw pointers, and device addresses.

## 7. Telemetry And Reporting

- [x] Record page import result.
- [x] Record bootstrap strategy used.
- [x] Record bootstrap eval latency.
- [x] Record `logits_ready`.
- [x] Record decode start position and imported token count.
- [x] Record baseline comparison and first divergence metadata when needed.
- [x] Produce a final report with `pass`, `fail`, or `inconclusive`.
      Current result is `pass` for the 128-token two-chunk scope because the
      exact-match baseline comparison ran and matched.

## 8. Final Decision

- [x] If the two-chunk proof passes, return to `pd-kv-page-handoff-spike`
      closure and reassess `pd-streaming-kv-handoff`.
- [x] If bootstrap requires full prompt re-prefill, mark streaming KV blocked
      and redesign.
      Not applicable in the final two-chunk proof: trim/replay reconstructed
      logits without full prompt re-prefill.
- [x] If logits/decode state cannot be reconstructed without full-state blob,
      mark page handoff blocked and keep full-state framing as fallback.
      Not applicable in the final two-chunk proof: current logits were
      reconstructed by trim/replay and exact-matched the local one-shot
      baseline.
- [x] If bootstrap cannot run because native trim rejects the imported page
      state, keep streaming KV paused and address trim support first.
      This blocker is now cleared in foreground, and the later local one-shot
      baseline retry also cleared the comparison harness gap.
