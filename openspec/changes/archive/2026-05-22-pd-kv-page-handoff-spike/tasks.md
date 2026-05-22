# Tasks: PD KV Page Handoff Spike

## 1. Proposal And Design

- [x] Create proposal, design, tasks, and spec for the spike.
- [x] Define the spike boundary as page-level correctness, not streaming
      performance.
- [x] Document why this precedes `pd-streaming-kv-handoff`.

## 2. Runtime And Protocol Spike Shape

- [x] Identify the minimal harness or binary-control path for exporting and
      importing KV pages.
- [x] Define `pd-kv-page/1` manifest fields and validation rules.
- [x] Ensure the spike cannot pass by using full-state export/import.
- [x] Define capability labels for source page export and target page import.
- [x] Add `skippy-correctness kv-page-handoff source` CLI skeleton.
- [x] Add `skippy-correctness kv-page-handoff coordinator` CLI skeleton.
- [x] Add local coordinator report shape for foreground smoke readiness.
- [x] Implement test-only source runtime loop that loads a runtime when
      `--model` is provided, receives token chunks, pre-fills one session, and
      calls `export_kv_page`.
- [x] Implement test-only coordinator runtime loop that tokenizes synthetic
      input, drives the source, validates page manifests, calls
      `import_kv_page`, decodes, and compares with a one-shot full-state
      baseline.
- [x] Add test-only JSON header plus raw payload transport for source and
      coordinator foreground smoke.
- [x] Add source/coordinator message serialization and payload-byte tests.

## 3. Positive Correctness Path

- [x] Build a deterministic local small two-or-more-chunk manifest proof.
- [x] Export page/range `0` after prefill chunk `0`.
      After `pd-kv-page-memory-type-support`, the source exported ISWA
      `base` and `swa` page segments for token range `0..64`.
- [x] Export page/range `1` after prefill chunk `1`.
      The source exported ISWA `base` and `swa` page segments for token range
      `64..128`.
- [x] Import pages on Mac in token-position order.
      The coordinator validated manifests and reached Mac-side page import
      without an import error before the later decode failure.
- [x] Verify final decode start position equals total imported tokens.
      Proven in the retry after rebuilding the Mac Metal-linked correctness
      binary with direct ISWA trim support: bootstrap reached
      `logits_ready=true` and `decode_start_position=128`.
- [x] Compare decode output with the one-shot baseline.
      Proven in the retry with the local one-shot prefill/decode baseline:
      page-path decode produced an exact token match for the 128-token
      two-chunk proof. Full-state handoff was not used as a page-path pass.

## 4. Negative Validation

- [x] Missing page fails closed.
- [x] Duplicate page fails closed.
- [x] Out-of-order page fails closed.
- [x] Position gap fails closed.
- [x] Position overlap fails closed.
- [x] Checksum mismatch fails closed.
- [x] Dtype/layout mismatch fails closed.
- [x] Import failure fails closed.
- [x] Unsupported/no-model runtime path remains `inconclusive` and cannot be
      counted as pass.
- [x] Full-state fallback cannot be counted as page-handoff pass.

## 5. Telemetry And Report

- [x] Produce a spike report with `pass`, `fail`, or `inconclusive`.
- [x] Record page export latency.
- [x] Record page transfer/read/write latency.
- [x] Record page import latency.
- [x] Record page bytes and total page count.
- [x] Record decode TTFT after final import.
- [x] Record correctness comparison against one-shot baseline.
- [x] Confirm the report excludes prompt text, generated content, complete
      token arrays, KV/native payload contents, credentials, private paths,
      endpoint URLs, and real machine labels.
- [x] Add foreground smoke report template with current `inconclusive` result.
- [x] Update foreground smoke report to show source/coordinator runtime loops
      are implemented locally but not yet run on PGX/Mac.

## 6. Optional Foreground Validation

- [x] Run small two-chunk foreground validation only after separately
      authorized.
      First result: `inconclusive`; PGX source exported four ISWA page
      segments (`base` and `swa` for two chunks), Mac import was reached, and
      decode then failed before the first sampled token because the imported
      page state lacked a current logits/output buffer.
- [x] Add `trim_replay_last_token` bootstrap wiring for the next foreground
      smoke.
- [x] Rerun small two-chunk foreground validation with
      `trim_replay_last_token`.
      Result: `inconclusive`; after rebuilding the Mac Metal-linked
      correctness binary with direct ISWA trim support, export/import,
      trim/replay, logits readiness, and page-path decode were observed. The
      run stopped at the required one-shot baseline restore session. That
      exact-comparison gap was later cleared by the local one-shot baseline
      retry below.
- [x] Rerun small two-chunk foreground validation with the local one-shot
      prefill/decode baseline.
      Result: `pass`; PGX source exported ISWA `base` and `swa` page segments,
      Mac coordinator imported them, trim/replay produced logits, and
      page-path decode exact-matched the local one-shot baseline.
- [x] Run 4k validation only after the small two-chunk proof passes.
      Deferred to a future change. This spike closes at the 128-token
      two-chunk correctness proof and did not run 4k.
- [x] Defer 8k and larger context validation to later changes.

## 7. Final Decision

- [x] Recommend `proceed_to_streaming_handoff`, `redesign`, or
      `run_more_spike`.
      Current recommendation: fix the foreground baseline harness before
      applying `pd-streaming-kv-handoff`.
- [x] If the result is pass, define the minimum scope for applying
      `pd-streaming-kv-handoff`.
      Minimum scope: use the proven 128-token two-chunk page export/import,
      ISWA base/swa segment validation, trim/replay decode bootstrap, and
      local one-shot exact-match baseline as the correctness seed. 4k/8k and
      overlap/pipeline performance remain separate follow-up validation.
- [x] If the result is inconclusive or fail, document the runtime/protocol
      blocker before any streaming pipeline work.
      Historical blockers were documented and cleared in follow-up changes:
      ISWA memory type support, decode bootstrap, direct ISWA trim, and local
      one-shot baseline comparison.
