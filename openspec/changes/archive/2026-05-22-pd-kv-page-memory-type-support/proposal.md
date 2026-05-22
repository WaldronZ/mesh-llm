# Change: PD KV Page Memory Type Support

## Why

`pd-kv-page-handoff-spike` ran the first PGX/Mac small two-chunk foreground
smoke for native KV page handoff. The result was `inconclusive`, but the
failure is precise:

- PGX source loaded the runtime and listened for the coordinator.
- The coordinator connected through the test harness.
- The source entered the real `export_kv_page` path after prefill.
- Native export failed with
  `runtime memory type is not supported for native KV pages`.

The smoke produced no page records:

- `page_count = 0`;
- Mac `import_kv_page` was not reached;
- decode and one-shot baseline comparison were not run;
- the full-state path was not used as a fake page pass;
- 4k and 8k were not run.

This blocks `pd-streaming-kv-handoff`. Streaming KV depends on page export and
import correctness. The next step must be to understand and support the actual
KV memory types produced by the PGX source run before any streaming pipeline
work resumes.

## What Changes

This proposal defines a focused runtime/native change for KV page memory type
diagnostics and support.

It should identify the KV memory type involved in page export/import, audit
what the native page APIs currently support, choose a safe support strategy for
unsupported memory types, and rerun the small two-chunk page handoff proof.

This is not the streaming pipeline. It does not optimize TTFT, add overlap, or
expand to 4k/8k. It only unblocks the smallest page correctness proof.

## Scope

Must:

- expose sanitized KV memory type labels at page export/import boundaries;
- distinguish CUDA device, CPU, unified, mmap, split, or mixed KV memory
  layouts where available;
- record the affected layer range and token range without raw addresses or
  payload contents;
- audit `stage_export_kv_page` and `stage_import_kv_page` supported and
  unsupported memory types;
- define a support strategy such as direct device copy, CPU staging,
  per-layer copy, forcing a supported KV memory type for spike-only testing,
  or explicit full-state fallback when page export is impossible;
- fail closed on unsupported memory types;
- ensure full-state framing cannot be reported as a page handoff pass;
- rerun the two-chunk page handoff smoke before resuming streaming KV work.

Should:

- prefer minimal CPU staging-copy support if direct GPU page export is risky;
- keep implementation local to native/runtime KV page APIs and the spike
  harness;
- preserve existing full-state export/import behavior;
- preserve default-off PD serving behavior.

Won't:

- implement `pd-streaming-kv-handoff`;
- add overlap or pipeline scheduling;
- run 4k or 8k smoke;
- promise or implement 32k/128k/256k;
- add KV compression;
- add multi-worker placement or scheduler behavior;
- change Chat UI behavior.

## Impact

The first apply phase is limited to native stage/llama KV page APIs, Rust
runtime wrappers, sanitized manifest/reporting, unit tests, and OpenSpec
reporting. It does not start local or remote foreground smoke processes,
archive changes, commit, or push.

The foreground proof remains separate: after local validation, the small
two-chunk `pd-kv-page-handoff-spike` smoke must be rerun before
`pd-streaming-kv-handoff` can resume.
