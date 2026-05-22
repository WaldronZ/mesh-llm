# Tasks: PD KV Page Baseline Harness

## 1. Proposal And Scope

- [x] Create proposal, design, tasks, and spec.
- [x] Define the boundary as baseline harness correctness, not streaming KV.
- [x] Document why this follows ISWA trim and decode bootstrap work.

## 2. Baseline Harness Audit

- [x] Locate the current `kv-page-handoff` one-shot full-state baseline helper.
- [x] Identify why baseline restore session reports no execution lane.
- [x] Determine whether the issue is runtime config, session lifecycle, model
      ownership, or unsupported local lane behavior.
- [x] Check whether a local one-shot prefill/decode baseline can avoid the
      restore-session failure.
- [x] Check whether existing state-handoff correctness helpers can be reused.

## 3. Baseline Strategy Implementation

- [x] Select the smallest baseline strategy that preserves correctness.
- [x] Keep baseline and page path session/state ownership separate.
- [x] Ensure full-state handoff, if used, is labeled baseline-only.
- [x] Ensure baseline unavailable produces `inconclusive`, not `pass`.
- [x] Ensure page-path decode is compared with baseline under the same
      deterministic settings.

## 4. Comparison And Reporting

- [x] Record exact token match result.
- [x] Record bounded first divergence metadata on mismatch.
- [x] Do not record generated text or complete token arrays.
- [x] Record baseline strategy used.
- [x] Record baseline failure reason when unavailable.
- [x] Preserve existing page export/import/trim/replay telemetry.

## 5. Tests

- [x] Baseline unavailable returns `inconclusive`.
- [x] Baseline success runs comparison.
      Proven by the 128-token two-chunk foreground smoke with the local
      one-shot prefill/decode baseline.
- [x] Exact match report shape covered by local comparison unit tests.
- [x] Mismatch reports bounded divergence metadata.
- [x] Full-state baseline cannot count as page-path pass.
- [x] Reports remain sanitized.

## 6. Foreground Smoke

- [x] Rerun the same 128-token two-chunk foreground smoke.
- [x] Verify PGX exports ISWA page segments.
- [x] Verify Mac imports ISWA page segments.
- [x] Verify trim/replay bootstrap still succeeds.
- [x] Verify page-path decode completes.
- [x] Verify one-shot baseline completes.
- [x] Verify page-path tokens exact-match baseline tokens.
- [x] Do not run 4k/8k until this two-chunk proof passes.
      4k/8k were not run in this change and are deferred to future validation.

## 7. Final Decision

- [x] If exact-match pass, return to `pd-kv-page-handoff-spike` closure and
      reassess `pd-streaming-kv-handoff`.
- [x] If baseline remains unavailable, keep result inconclusive and redesign
      the proof.
      The previous unavailable baseline condition has been cleared by the
      local one-shot baseline.
- [x] If page tokens diverge from baseline, keep streaming KV paused and
      record bounded divergence metadata.
      Not applicable in the final two-chunk proof: page-path tokens exact
      matched the local one-shot baseline.
