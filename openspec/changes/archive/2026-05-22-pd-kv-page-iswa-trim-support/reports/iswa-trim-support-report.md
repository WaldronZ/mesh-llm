# PD KV Page ISWA Trim Support Report

## Result

`result`: `pass`

`recommendation`: `return_to_pd_kv_page_handoff_spike_closure`

## Scope

This report records the local implementation stage and the follow-up
128-token two-chunk foreground smoke for `pd-kv-page-iswa-trim-support`.

No 4k prompt or 8k prompt was run for this report. Full-state handoff was not
used as a page-path pass condition.

## Root Cause

`StageSession::trim_session` calls the native `skippy_trim_session` ABI. The
native trim wrapper accepts hybrid memory, hybrid ISWA memory, regular
`llama_kv_cache`, and recurrent memory, but it did not accept direct
`llama_kv_cache_iswa`.

Gemma4 imported KV page state can appear as direct `llama_kv_cache_iswa`, so the
old code fell through to:

`runtime memory type is not supported for trim`

This was a Skippy trim wrapper dispatch gap, not a CUDA/Metal inference backend
limitation.

## Local Change

- durable native patch:
  `third_party/llama.cpp/patches/0083-Trim-direct-ISWA-KV-cache-sessions.patch`;
- actual prepared source updated:
  `.deps/llama.cpp/src/skippy.cpp`;
- new trim branch:
  direct `llama_kv_cache_iswa` calls `seq_rm(session->seq_id, p0, -1)`;
- regular `llama_kv_cache`, hybrid, hybrid ISWA, and recurrent trim paths were
  left unchanged;
- no Rust public API change was required.

## Expected ISWA Semantics

The direct ISWA cache owns two sub-caches:

- `base`;
- `swa`.

The existing native `llama_kv_cache_iswa::seq_rm` delegates trim to both
sub-caches. If trim fails, Skippy returns a fail-closed runtime error and does
not continue to replay or sample.

## Validation Status

Local build/test validation passed before foreground smoke:

- patch queue clean apply check: passed;
- `cargo fmt --all -- --check`: passed;
- `cargo test -p skippy-runtime --lib`: passed;
- `cargo test -p skippy-correctness`: passed;
- `cargo check -p skippy-runtime`: passed;
- `cargo check -p skippy-correctness`: passed;
- `openspec validate pd-kv-page-iswa-trim-support --strict`: passed;
- `openspec validate pd-kv-page-decode-bootstrap --strict`: passed;
- `git diff --check`: passed.

The first foreground retry accidentally used a stale Mac Metal-linked native
library and reproduced the old trim error. After rebuilding the Mac
Metal-linked correctness binary with the direct ISWA trim patch, the same
128-token two-chunk smoke advanced past trim:

- PGX/CUDA exported ISWA `base` and `swa` page segments;
- Mac/Metal imported those segments;
- trim target `N - 1` succeeded;
- replay position `N - 1` succeeded;
- `logits_ready=true`;
- `decode_start_position=N`;
- page-path decode was observed before baseline comparison.

The later local one-shot baseline foreground smoke completed the exact-match
comparison:

- trim target `N - 1` succeeded;
- replay position `N - 1` succeeded;
- `logits_ready=true`;
- `decode_start_position=N`;
- page-path decode exact-matched the local one-shot baseline.

The ISWA trim blocker is cleared for the 128-token two-chunk scope.

## Scope Closure

This change does not implement `pd-streaming-kv-handoff`. It only clears the
direct ISWA trim support needed by page-import decode bootstrap.

4k/8k, overlap/pipeline transfer, production scheduling, and throughput claims
are deferred to future changes.

## Privacy

This report does not include prompt text, generated content, complete token
arrays, KV/native payload contents, credentials, private paths, endpoint URLs,
real machine labels, raw pointers, or device addresses.
