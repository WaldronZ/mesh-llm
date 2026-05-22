# Tasks: PD KV Page Memory Type Support

## 1. Proposal And Scope

- [x] Create proposal, design, tasks, and spec.
- [x] Carry forward the foreground smoke blocker from
      `pd-kv-page-handoff-spike`.
- [x] Keep `pd-streaming-kv-handoff`, 4k/8k, compression, scheduler, and UI
      work out of scope.

## 2. Read-Only Native Audit

- [x] Inspect `stage_export_kv_page` memory type checks.
- [x] Inspect `stage_import_kv_page` memory type checks.
- [x] Inspect Rust `RuntimeKvPageDesc`, `export_kv_page`, and
      `import_kv_page` wrappers.
- [x] Produce a supported/unsupported memory type matrix.
- [x] Identify whether the foreground blocker was CPU-only, CUDA-only,
      unified, host-mapped, split, or mixed memory.
- [x] Confirm whether K and V can have different memory placement.

## 3. Sanitized Diagnostics

- [x] Add bounded memory type labels at export/import boundaries.
- [x] Record bounded memory object label through
      `skippy_query_kv_page_memory_kind`; per-buffer backend labels remain
      deferred to future diagnostics if foreground smoke needs them. This is
      not required for the 128-token two-chunk pass scope.
- [x] Ensure diagnostics do not include raw pointers, device addresses,
      private paths, endpoint URLs, credentials, prompt text, generated
      content, token arrays, or KV/native payload contents.
- [x] Convert unsupported memory type failure into an explicit sanitized
      reason label.

## 4. Support Strategy

- [x] Choose direct device copy, CPU staging, per-layer copy, forced supported
      memory for spike-only validation, or explicit full-state fallback/no-go.
- [x] Preserve existing full-state export/import behavior.
- [x] Keep implementation local to native/runtime KV page APIs and the spike
      harness unless the audit proves a broader API change is required.
- [x] Ensure full-state framing cannot be reported as page-handoff pass.

## 5. Tests

- [x] Unsupported memory type returns explicit error.
- [x] Supported regular path remains available and ISWA path is expressed as
      base/swa segment exports.
- [x] Memory type labels are sanitized bounded enums.
- [x] Reports exclude raw pointers, device addresses, private paths,
      credentials, prompt text, generated content, token arrays, and KV/native
      payload contents.
- [x] Full-state blob still fails page-handoff proof.
- [x] Existing full-state export/import tests remain unchanged.

## 6. Foreground Smoke

- [x] Rebuild the PGX and Mac smoke binaries after the memory type support
      patch.
- [x] Rerun only the small two-chunk `pd-kv-page-handoff-spike` foreground
      proof.
      Result: initial retry was `inconclusive`; the final retry passed after
      decode bootstrap, direct ISWA trim, and local one-shot baseline support.
      The old memory-kind blocker did not reproduce.
- [x] Confirm source exports page/range `0`.
      Observed ISWA `base` and `swa` segments for token range `0..64`.
- [x] Confirm source exports page/range `1`.
      Observed ISWA `base` and `swa` segments for token range `64..128`.
- [x] Confirm Mac imports pages in token-position order.
      Coordinator reached Mac-side import without an import error before the
      later decode failure.
- [x] Confirm final decode start position equals imported token count.
      Proven in the follow-up foreground smoke after decode bootstrap and
      direct ISWA trim support: `decode_start_position=128`.
- [x] Compare page-path decode against one-shot baseline.
      Proven in the follow-up foreground smoke using the local one-shot
      prefill/decode baseline.
- [x] Do not run 4k/8k until the small two-chunk proof passes.

## 7. Decision

- [x] If two-chunk page handoff passes, recommend resuming
      `pd-streaming-kv-handoff` with this evidence.
- [x] If memory type support remains blocked, recommend redesign or retaining
      large-state full-state framing as the honest fallback.
      Not applicable in the final two-chunk proof: memory type support is no
      longer the observed blocker.
- [x] Document any remaining no-go condition before streaming KV apply.
      The memory-type blocker is cleared. Streaming KV still needs a separate
      implementation change for overlap/pipeline lifecycle and 4k/8k
      validation.
