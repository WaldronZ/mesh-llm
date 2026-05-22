# PD KV Page Memory Type Support Report

result: `pass`

recommendation: `return_to_pd_kv_page_handoff_spike_closure`

## Scope

This phase addressed the native/Rust blocker observed by
`pd-kv-page-handoff-spike`: Gemma4-style ISWA KV memory was rejected before any
page record could be exported. The final follow-up two-chunk foreground smoke
proved the 128-token CUDA-to-Metal page handoff path. It did not run 4k/8k
prompts.

## Root Cause

The foreground error was caused by the Skippy native KV page wrapper accepting
only regular `llama_kv_cache` or `llama_memory_hybrid` attention KV caches.
Gemma4 uses ISWA attention memory, which is represented as separate base and
SWA KV caches. The old wrapper rejected that memory object before reaching the
backend tensor copy path.

## Local Changes

- Added sanitized memory object labels:
  - `llama_kv_cache`
  - `llama_kv_cache_iswa`
  - `llama_memory_hybrid`
  - `llama_memory_hybrid_iswa`
  - `unknown`
- Added ISWA page segment support:
  - `cache_kind=iswa`
  - `segment_kind=base`
  - `segment_kind=swa`
- Preserved the existing regular page path:
  - `cache_kind=regular`
  - `segment_kind=regular`
- Updated Rust descriptor helpers and the correctness/server manifest
  validation to fail closed on missing, duplicate, mismatched, or out-of-order
  page segments.

## Current Result

The two-chunk foreground proof was rerun after this local implementation. The
old memory-kind blocker did not reproduce: the source exported four ISWA page
segments, and the coordinator reached Mac-side page import.

The follow-up 128-token two-chunk foreground proof later completed with decode
bootstrap, direct ISWA trim support, and the local one-shot prefill/decode
baseline:

- the old memory-kind blocker did not reproduce;
- the source exported ISWA `base` and `swa` segments;
- the coordinator imported those segments;
- `decode_start_position=128`;
- page-path decode exact-matched the local one-shot baseline.

The memory-type support blocker is cleared for the 128-token two-chunk scope.

## Scope Closure

This change does not implement `pd-streaming-kv-handoff`. It only clears the
native memory-kind blocker needed by the page-level correctness spike.

4k/8k, overlap/pipeline transfer, production scheduling, and throughput
claims are deferred to future changes.

## Privacy

The report and local test outputs exclude prompt text, generated content,
complete token arrays, KV/native payload contents, credentials, private paths,
real machine names, raw pointers, and device addresses.
