# Change: PD KV Page Baseline Harness

## Why

The latest `pd-kv-page-handoff-spike` 128-token two-chunk foreground smoke
advanced past the prior page-handoff blockers:

- PGX source reached real `export_kv_page`;
- Mac coordinator reached real `import_kv_page`;
- Gemma4 ISWA `base` and `swa` page segments were present;
- manifest and checksum validation passed;
- `trim_session(N - 1)` succeeded;
- replaying the final prompt token succeeded;
- `logits_ready=true`;
- `decode_start_position=128`;
- page-path decode was observed.

The remaining blocker is the deterministic correctness comparison. The
one-shot full-state baseline path failed while creating the baseline restore
session:

`no skippy execution lane is available`

This means the page path may already be able to decode, but it cannot be
claimed correct until it exact-matches a deterministic baseline.

## What Changes

This change proposes a focused fix for the `skippy-correctness
kv-page-handoff` baseline harness. It does not change page export/import,
ISWA trim, decode bootstrap, or streaming KV behavior.

The goal is to make the foreground two-chunk harness produce a reliable
baseline token sequence and compare the page-path token sequence against it.

## Scope

Must:

- define the baseline harness blocker precisely;
- audit how the current coordinator constructs its one-shot full-state
  baseline;
- determine whether the failure is caused by runtime config, lifecycle/order
  of model/session creation, or unsupported local execution lane behavior;
- choose a minimal baseline strategy:
  - local one-shot prefill/decode baseline in the Mac runtime;
  - source full-state handoff baseline using existing state export/import;
  - reuse an existing `skippy-correctness` state-handoff baseline path;
  - or, only if no baseline is possible, define an explicit alternative proof;
- compare the page-path decoded token sequence with the baseline token
  sequence under identical deterministic settings;
- record bounded divergence metadata without generated text;
- keep full-state baseline clearly separated from the page path and never
  count full-state restore as page-path pass;
- rerun the same 128-token two-chunk foreground smoke before claiming pass.

Should:

- prefer a small change inside the `skippy-correctness` harness;
- reuse existing one-shot/full-state helpers where possible;
- avoid loading more runtime/model instances than necessary;
- keep reports sanitized and bounded.

Won't:

- implement `pd-streaming-kv-handoff`;
- run 4k or 8k prompts;
- add KV compression;
- add multi-worker placement, scheduler behavior, or production concurrency;
- change Chat UI behavior;
- use full-state handoff as a fake page-path pass.

## Impact

This change is a gate before `pd-streaming-kv-handoff`. Streaming KV should
remain paused until the small page-path proof has a deterministic exact-match
baseline comparison.

The expected next proof remains the same small foreground smoke:

```text
PGX export ISWA page segments
Mac import ISWA page segments
Mac trim/replay bootstrap
Mac page-path decode
Mac baseline decode
page tokens exact-match baseline tokens
```
