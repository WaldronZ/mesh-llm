# Design: PD KV Page Baseline Harness

## Current State

The page-handoff proof has cleared the runtime stages required to reach page
decode:

1. PGX prefilled two 64-token chunks.
2. PGX exported four ISWA page segment records.
3. Mac imported all page segments in order.
4. Mac trimmed the imported sequence from `N` to `N - 1`.
5. Mac replayed the final prompt token at `N - 1`.
6. Mac produced current logits and restored decode position to `N`.
7. Mac entered page-path decode.

The proof stopped when the harness attempted to build a one-shot full-state
baseline and the baseline restore session failed with:

`no skippy execution lane is available`

This is not a page import failure and not an ISWA trim failure. It is a
baseline harness issue that prevents the exact-match correctness comparison.

## Baseline Problem

The current coordinator tries to compute a deterministic one-shot full-state
baseline after it has already opened models/sessions for:

- tokenization and source coordination;
- page import;
- page-path decode.

It then creates additional sessions for the full-state baseline path. The
failure may come from one of three classes:

1. runtime configuration does not expose an execution lane for the baseline
   restore session;
2. lifecycle/order of session creation exhausts or invalidates the available
   local lane;
3. the full-state baseline helper assumes a serving topology that is not valid
   inside this foreground harness.

The apply phase should start by auditing the current baseline code path before
choosing a fix.

## Candidate Baseline Strategies

### A. Local One-Shot Prefill/Decode Baseline

The Mac coordinator can run a local one-shot baseline on the same model/runtime
configuration and decode from the fully-prefilled local session.

Benefits:

- smallest conceptual baseline;
- avoids using full-state restore during the baseline;
- directly compares page import+bootstrap decode against local full prefill.

Risks:

- must ensure the baseline uses the exact same prompt tokens and deterministic
  sampling parameters;
- must avoid reusing page-path session state;
- may still need a fresh session or explicit cleanup.

This is the preferred first option if the runtime can create a clean local
baseline session.

### B. Source Full-State Handoff Baseline

The source could export full state for the same prompt and the Mac coordinator
could import it using the existing full-state path, but only as the baseline.

Benefits:

- aligns with earlier native full-state baseline behavior;
- proves page path against the known full-state import path.

Risks:

- still depends on restore-session lane availability;
- can be confused with page-path pass unless reports are explicit;
- may be heavier than needed.

This remains acceptable only when reports clearly separate baseline from page
path.

### C. Reuse Existing State-Handoff Correctness Baseline

If `skippy-correctness` already has a stable state-handoff baseline helper, the
page harness can call or adapt it.

Benefits:

- reduces one-off baseline logic;
- keeps correctness semantics aligned across harnesses.

Risks:

- helper may assume different model/config ownership;
- integration may be larger than the minimal fix.

### D. Alternative Correctness Proof

If none of the baseline paths can run, the change may define an explicit
alternative proof. This should be a last resort.

Requirements:

- it must compare deterministic token sequences or equivalent logits/top-k
  evidence;
- it must not use generated text as the correctness criterion;
- it must remain non-pass unless the evidence is strong enough to replace
  exact token match.

## Recommended Path

Start with a read-only baseline harness audit:

1. locate the current one-shot full-state baseline helper;
2. identify when source/page/baseline model sessions are created and dropped;
3. identify which `StageModel`/runtime config is used for the baseline;
4. reproduce the baseline creation failure locally if possible without remote
   processes;
5. try the smallest lifecycle/config fix that lets the baseline run.

If local full prefill/decode baseline works, prefer it over full-state baseline
because it avoids another restore session and keeps the page path and baseline
cleanly separated.

## Correctness Criteria

The change can pass only when:

- prompt tokens are identical between page path and baseline;
- deterministic settings are identical;
- page-path decode tokens exact-match baseline decode tokens;
- mismatch records bounded first divergence metadata;
- baseline failure reports `inconclusive`, never `pass`;
- full-state handoff is used only as baseline if selected, never as page-path
  proof.

## Safety And Privacy

Reports must not include prompt text, generated content, complete token arrays,
KV/native payload contents, credentials, private paths, endpoint URLs, real
machine labels, raw pointers, or device addresses.

The harness must remain fail-closed:

- no baseline means no pass;
- no exact comparison means no pass;
- mismatch means fail or inconclusive with bounded divergence metadata.

## Relationship To Streaming KV

`pd-streaming-kv-handoff` remains blocked until this change enables the small
two-chunk page proof to complete the baseline comparison. Optimizing transfer
or overlapping prefill with page transfer is premature until page import +
bootstrap decode has proven correctness against baseline.
