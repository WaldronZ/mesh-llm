# Design: PD KV Page Memory Type Support

## Foreground Blocker

The `pd-kv-page-handoff-spike` foreground smoke reached the first meaningful
native boundary:

```text
PGX source runtime loaded
PGX source listened on the test port
Mac coordinator connected
PGX source attempted export_kv_page
native export rejected the current KV memory type
```

The bounded error label is:

`runtime_memory_type_not_supported_for_native_kv_pages`

This means the current page proof is blocked before any page frame exists.
There is no evidence yet for Mac append/import correctness because Mac import
was not reached.

## Confirmed Root Cause

The source smoke reached the Skippy native KV page wrapper, but the failure
occurred before backend tensor copy. The wrapper accepted only regular
`llama_kv_cache` or `llama_memory_hybrid -> llama_kv_cache`. Gemma4-style ISWA
attention memory is represented as `llama_kv_cache_iswa` or
`llama_memory_hybrid_iswa`, which contains separate base and SWA KV caches.

Therefore the blocker was a high-level memory object shape mismatch, not proof
that CUDA device buffers cannot be copied. Backend buffer support still must be
verified by rerunning the small two-chunk foreground smoke.

## Memory Type Diagnostics

At the page export/import boundary, the runtime now exposes bounded memory
object labels that are safe to report:

- `llama_kv_cache`;
- `llama_kv_cache_iswa`;
- `llama_memory_hybrid`;
- `llama_memory_hybrid_iswa`;
- `unknown`.

Per-buffer backend labels such as CUDA/Metal/CPU are still deferred until the
foreground smoke needs that extra detail. The immediate blocker was the memory
object kind, so this phase keeps the diagnostic surface narrow.

Diagnostics must not include raw pointers, device addresses, private paths,
endpoint URLs, credentials, prompt text, generated content, token arrays, or
KV/native payload bytes.

## Native Support Audit

The local apply phase inspected and documented:

- `stage_export_kv_page`;
- `stage_import_kv_page`;
- `RuntimeKvPageDesc`;
- Rust `export_kv_page` and `import_kv_page` wrappers;
- native layer selection and token range handling;
- checksum and byte-count calculation;
- existing assumptions about contiguous buffers and supported backend buffer
  types.

The audit produced this support matrix:

| Memory shape | Export support | Import support | Notes |
| --- | --- | --- | --- |
| Regular `llama_kv_cache` | supported by existing page path | supported by existing page path | Existing descriptor remains unchanged |
| `llama_memory_hybrid` attention KV | supported through attention cache | supported through attention cache | Recurrent state remains out of page proof |
| `llama_kv_cache_iswa` | supported as `base` and `swa` segments | supported as `base` and `swa` segments | Needs foreground proof |
| `llama_memory_hybrid_iswa` attention KV | supported as `base` and `swa` segments | supported as `base` and `swa` segments | Needs foreground proof |
| Unknown memory object | fail closed | fail closed | Sanitized label only |

## Candidate Support Strategies

### Direct Device Read/Copy

Read the page bytes directly from supported backend buffers into the wire
payload.

Pros:

- fewer extra copies when supported;
- closest to the intended page API.

Risks:

- backend-specific APIs may differ for CUDA and Metal;
- synchronization and stream ordering must be correct;
- raw device pointers must never escape telemetry or reports.

### Selected Strategy: ISWA Segment Export/Import Through Existing Page Copy

Use the existing `llama_kv_cache::stage_export_kv_page` and
`stage_import_kv_page` implementation for each concrete KV sub-cache. Regular
models continue to produce one page record. ISWA models produce explicit
`base` and `swa` segment records for each token range.

Pros:

- keeps regular page behavior unchanged;
- avoids inventing a full-state fallback;
- lets backend tensor get/set continue to own CPU/device staging details;
- gives the harness enough metadata to reject missing or duplicated ISWA
  segments.

Risks:

- CUDA-to-Metal correctness is still unproven until the foreground smoke;
- ISWA base/SWA segments may expose layout or row-size mismatches that require
  another fail-closed fix;
- this is still not a streaming pipeline.

### Per-Layer Copy

Export each affected layer's K/V range and assemble the page payload in a
stable order.

Pros:

- handles mixed memory layouts explicitly;
- gives clear diagnostics when only some layers are unsupported.

Risks:

- more metadata and ordering complexity;
- can reveal layout assumptions that need additional native API support.

### Force Supported KV Memory Type For Spike Only

Run the spike with a configuration that forces CPU-only or otherwise supported
KV memory.

Pros:

- can quickly isolate import/append semantics.

Risks:

- does not prove the desired PGX CUDA source path;
- must be reported as a constrained proof, not as CUDA page handoff pass.

### Explicit Full-State Fallback

If page export remains impossible, report an explicit no-go and keep
large-state full-state framing as the fallback/baseline.

Pros:

- honest failure mode;
- avoids pretending one-shot full-state is page streaming.

Risks:

- does not unblock streaming KV.

## Correctness Gate

The local implementation is only a preparation step. The change is successful
only when the small two-chunk page proof can pass:

1. source exports page/range `0` after chunk `0`;
2. source exports page/range `1` after chunk `1`;
3. Mac imports both pages in position order;
4. final decode start position equals the imported token count;
5. page-path decode matches the one-shot full-state baseline under
   deterministic settings, or records bounded divergence and remains
   non-pass.

The spike must continue to reject full-state blobs as page proof.

## Fail-Closed Semantics

The runtime and harness must fail closed on:

- unsupported memory type;
- missing or unknown memory type label;
- raw pointer/device address exposure attempt;
- partial page export;
- byte-count mismatch;
- checksum mismatch;
- import failure;
- page path falling back to full-state export/import.

Unsupported memory type should become an explicit sanitized reason label, not a
crash or generic read failure.

## Testing Plan

Local tests should cover:

- unsupported memory type returns explicit error;
- supported memory type path produces a page payload in a fake or local
  supported setup;
- memory type labels are bounded enum-like strings;
- reports exclude raw pointers, device addresses, private paths, endpoint
  URLs, prompt text, generated content, token arrays, and KV payload contents;
- full-state blobs cannot satisfy page-handoff proof.

Foreground validation should be the same small two-chunk proof from
`pd-kv-page-handoff-spike`. 4k and 8k remain out of scope until the small proof
passes.

## Relationship To Streaming KV

`pd-streaming-kv-handoff` depends on reliable page export/import. A streaming
pipeline would multiply this failure across chunks and add overlap, queueing,
and cancellation complexity.

Therefore this change must pass, or clearly no-go, before streaming KV apply
resumes.
