# Tasks: PD KV Page ISWA Trim Support

## 1. Proposal And Scope

- [x] Create proposal, design, tasks, and spec.
- [x] Define the boundary as native/runtime ISWA trim support.
- [x] Keep `pd-streaming-kv-handoff`, 4k/8k validation, and UI work out of
      scope.

## 2. Read-Only Native Trim Audit

- [x] Locate the native function that returns
      `runtime memory type is not supported for trim`.
- [x] Identify current accepted trim memory/cache types.
- [x] Identify whether regular `llama_kv_cache` trim and hybrid memory trim
      already work.
- [x] Identify how Gemma4 ISWA state exposes `base` and `swa` sub-caches to
      trim code.
- [x] Determine whether Rust `StageSession::trim_session` needs descriptor or
      diagnostic changes.
- [x] Produce a sanitized support matrix for regular, ISWA, hybrid, and
      unknown memory kinds.

## 3. Native/Runtime Implementation

- [x] Add or update native trim support for `llama_kv_cache_iswa` or imported
      ISWA page state.
- [x] Trim `base` and `swa` sub-caches consistently to the requested target
      position.
- [x] Preserve the existing regular non-ISWA trim path.
- [x] Return sanitized error labels for unsupported memory kinds.
- [x] Fail closed on partial `base`/`swa` trim failure.
- [x] Avoid re-prefilling the whole prompt.

## 4. Harness Integration

- [x] Ensure `kv-page-handoff coordinator --bootstrap-strategy
      trim-replay-last-token` records the trim result.
- [x] Ensure bootstrap does not replay when trim fails.
- [x] Ensure `logits_ready` remains false on trim failure.
- [x] Ensure full-state fallback cannot count as page-path pass.

## 5. Tests

- [x] Regular trim remains backward-compatible.
- [x] ISWA trim logic delegates to existing `llama_kv_cache_iswa::seq_rm`,
      which trims both `base` and `swa` paths.
- [x] Unsupported memory kind returns sanitized error.
- [x] Partial `base`/`swa` trim failure fails closed.
- [x] Report output excludes prompt text, generated content, complete token
      arrays, KV/native payload contents, credentials, private paths, endpoint
      URLs, real machine labels, raw pointers, and device addresses.

Note: local tests can compile and exercise existing trim/harness behavior, but
cannot instantiate the real imported Gemma4 ISWA CUDA-to-Metal page state. The
foreground smoke remains the required correctness proof.

## 6. Foreground Smoke

- [x] Rerun the same 128-token two-chunk foreground smoke only after
      implementation.
- [x] Verify PGX/CUDA exports ISWA `base` and `swa` page segments.
- [x] Verify Mac/Metal imports page segments.
- [x] Verify trim target `N - 1` succeeds.
- [x] Verify replay position `N - 1` succeeds.
- [x] Verify `logits_ready=true`.
- [x] Verify `decode_start_position=N`.
- [x] Verify page-path decode exact-matches the one-shot baseline.
      Proven by the follow-up foreground smoke using the local one-shot
      prefill/decode baseline.
- [x] Do not run 4k/8k until this two-chunk proof passes.
      4k/8k were not run in this change and are deferred to future validation.

## 7. Final Decision

- [x] If the two-chunk proof passes, return to `pd-kv-page-handoff-spike`
      closure and reassess `pd-streaming-kv-handoff`.
- [x] If trim cannot safely support ISWA page state, keep streaming KV blocked
      and document full-state framing as the honest fallback.
      Not applicable in the final two-chunk proof: direct ISWA trim succeeded.
- [x] If decode diverges after trim/replay, keep the result non-pass and
      record bounded divergence metadata.
      No divergence occurred in the 128-token two-chunk foreground smoke.
- [x] If trim/replay succeeds but baseline comparison cannot run, keep the
      result inconclusive and fix the foreground baseline harness before
      resuming `pd-streaming-kv-handoff`.
      This historical blocker was cleared by `pd-kv-page-baseline-harness`.
