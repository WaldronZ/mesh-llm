# Change: PD KV Page ISWA Trim Support

## Why

`pd-kv-page-handoff-spike` has advanced through two earlier blockers:

1. `runtime memory type is not supported for native KV pages`
   - addressed by `pd-kv-page-memory-type-support`;
   - PGX/CUDA source now exports Gemma4 ISWA `base` and `swa` page segments.
2. `imported_page_state_has_no_current_logits_output_buffer`
   - addressed at the harness design level by `pd-kv-page-decode-bootstrap`;
   - the selected bootstrap strategy is `trim_replay_last_token`.

The latest small two-chunk foreground smoke reached the next blocker:

```text
trim_session(N - 1) after imported ISWA page state
-> runtime memory type is not supported for trim
```

The smoke proved that PGX `export_kv_page` and Mac `import_kv_page` can reach
the ISWA page path, but it did not prove decode correctness. Replay did not
run, `logits_ready` remained false, and the one-shot full-state baseline
comparison was not reached.

## What Changes

This change focuses on native/runtime trim support for imported Gemma4 ISWA KV
page state. It defines how `trim_session` should handle ISWA `base` and `swa`
sub-caches consistently so the existing `trim_replay_last_token` bootstrap can
run.

This is not a streaming KV handoff implementation and does not expand to
4k/8k validation.

## Scope

Must:

- define the exact trim blocker carried from the latest two-chunk smoke;
- audit the current native trim path and accepted memory/cache types;
- add or design trim support for `llama_kv_cache_iswa` / imported ISWA
  `base` and `swa` page state;
- preserve regular non-ISWA trim behavior;
- fail closed if `base` and `swa` trimming cannot both complete consistently;
- expose sanitized trim memory kind/result telemetry;
- rerun the same 128-token two-chunk foreground smoke after implementation;
- require exact-match page-path decode against the one-shot full-state
  baseline before claiming pass.

Should:

- keep implementation local to native/runtime trim support;
- reuse ISWA diagnostics from `pd-kv-page-memory-type-support`;
- avoid re-prefilling the whole prompt;
- preserve full-state behavior.

Won't:

- implement `pd-streaming-kv-handoff`;
- run 4k/8k or larger context validation;
- add KV compression, multi-worker placement, scheduler behavior, or
  production concurrency;
- change Chat UI behavior;
- use full-state fallback as page-path proof.

## Impact

The first apply phase should be a read-only native trim audit. It should
identify the function returning `runtime memory type is not supported for trim`
and confirm whether ISWA trim needs native patching, Rust wrapper changes, or
both.

Only after trim support is implemented should the existing small foreground
proof be rerun:

```text
PGX exports ISWA page segments
Mac imports pages
Mac trims imported state to N - 1
Mac replays last prompt token at N - 1
logits_ready=true
decode_start_position=N
page decode exact-matches one-shot full-state baseline
```

`pd-streaming-kv-handoff` remains blocked until this proof passes.
