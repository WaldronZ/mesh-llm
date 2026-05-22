# PD KV Page Baseline Harness Report

## Result

`result`: `pass`

`recommendation`: `return_to_pd_kv_page_handoff_spike_closure`

## Scope

This report covers the local implementation stage and the follow-up
128-token two-chunk foreground smoke for `pd-kv-page-baseline-harness`.

No 4k or 8k prompts were run.

## Root Cause

The prior one-shot baseline used full-state restore as the primary comparison
path. It created a source session, exported full state, and then attempted to
create a restore session on the same `StageModel` while the source session was
still alive.

The coordinator runtime is configured with a single execution lane for this
harness. With that lane still occupied by the source session, creating the
restore session failed with:

`no skippy execution lane is available`

This was a baseline harness lifecycle issue, not a page import, ISWA trim, or
decode bootstrap failure.

## Implementation Summary

- Added a local one-shot prefill/decode baseline path.
- The baseline uses the same prompt tokens, seed, deterministic sampling, and
  max token limit as the page path.
- The baseline no longer depends on full-state restore as its primary
  correctness comparison.
- Full-state handoff remains disallowed as a page-path pass condition.
- Baseline unavailable now produces an `inconclusive` report instead of a
  misleading pass.
- Token divergence reporting is bounded to first divergence metadata and token
  counts only.

## Foreground Result

- local baseline harness implementation: complete;
- local manifest and comparison tests: complete;
- foreground 128-token two-chunk smoke: complete;
- local one-shot baseline completed;
- page-path decode exact-matched the local one-shot baseline;
- full-state handoff was not used as a page-path pass;
- `pd-streaming-kv-handoff`: may be reassessed using this two-chunk evidence.

## Scope Closure

This change does not implement `pd-streaming-kv-handoff`. It only supplies the
baseline proof required to close the 128-token two-chunk page handoff spike.

4k/8k, overlap/pipeline transfer, production scheduling, and throughput claims
are deferred to future changes.

## Privacy

The report excludes prompt text, generated content, complete token arrays,
KV/native payload contents, credentials, private paths, endpoint URLs, real
machine labels, raw pointers, and device addresses.
