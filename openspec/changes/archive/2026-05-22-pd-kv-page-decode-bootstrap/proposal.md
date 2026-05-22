# Change: PD KV Page Decode Bootstrap

## Why

`pd-kv-page-handoff-spike` reran the small PGX/Mac two-chunk foreground smoke
after `pd-kv-page-memory-type-support` added ISWA native KV page segments. The
old blocker did not reproduce:

- PGX CUDA source reached real `export_kv_page`;
- four ISWA page/segment records were exported for two prompt chunks:
  `0..64 base`, `0..64 swa`, `64..128 base`, and `64..128 swa`;
- total page bytes were `115343360`;
- checksums and manifest validation passed;
- Mac coordinator reached `import_kv_page`;
- no full-state fallback was used as a page pass.

The new blocker appeared after page import, at the first decode/sample step:

`imported_page_state_has_no_current_logits_output_buffer`

This means KV page import restored KV/history data far enough for import to
complete, but the target runtime did not yet have the current logits/output
buffer needed to sample the next token. Full-state import can decode because
it carries broader decode/runtime state; page handoff intentionally carries a
smaller surface and needs an explicit decode bootstrap strategy.

## What Changes

This proposal defines a focused spike for page-import decode bootstrap. It
decides how a Mac target runtime should become decode-ready after importing
native KV pages from a PGX source.

The change is not a streaming KV pipeline and does not target 4k/8k scaling.
It only tries to unblock the small two-chunk page correctness proof.

## Scope

Must:

- define the decode bootstrap problem clearly: imported KV pages are not
  automatically equivalent to a decode-ready full native state;
- evaluate candidate bootstrap strategies:
  - Mac-side last prompt token bootstrap;
  - PGX-provided decode seed token or logits metadata;
  - minimal non-KV decode-state export/import;
  - final logits/output buffer export when the native runtime supports it;
- select a correctness-first spike path;
- fail closed when logits are missing and no bootstrap path is available;
- prove the two-chunk page path against the one-shot full-state baseline under
  deterministic settings before claiming pass;
- keep full-state baseline separate from page handoff proof;
- report bootstrap telemetry without prompt text, generated content, complete
  token arrays, KV/native payload contents, credentials, private paths,
  endpoint URLs, or real machine labels.

Should:

- reuse the existing `pd-kv-page-handoff-spike` two-chunk harness;
- start with the existing 128-token two-chunk proof before any larger prompt;
- prefer a Mac-side last-token bootstrap only if it preserves token-position
  semantics and exact-match correctness;
- keep `pd-streaming-kv-handoff` paused until this proof passes.

Won't:

- implement the full streaming KV handoff pipeline;
- run or promise 4k/8k, 32k/128k/256k support;
- add KV compression or low-precision KV changes;
- add multi-worker placement, scheduler behavior, or production concurrency;
- change Chat UI behavior;
- use full-state export/import as a fake page handoff pass.

## Impact

The first apply phase should start with a read-only runtime audit of decode
state and logits readiness APIs. If the audit shows a safe and testable path,
implementation can proceed in the spike harness.

The expected follow-up foreground proof is the same small two-chunk run:

```text
PGX prefill chunk 0 -> export KV page segments
PGX prefill chunk 1 -> export KV page segments
Mac import page segments in order
Mac bootstrap decode state
Mac decode output matches one-shot full-state baseline
```

Only after that proof passes should `pd-kv-page-handoff-spike` return to pass
criteria and `pd-streaming-kv-handoff` resume.
