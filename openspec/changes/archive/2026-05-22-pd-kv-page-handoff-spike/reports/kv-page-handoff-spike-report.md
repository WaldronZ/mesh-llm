# PD KV Page Handoff Spike Report

## Result

`result`: `pass`

`recommendation`: `proceed_to_streaming_handoff_design_or_apply`

## Summary

The local implementation phases added two pieces:

- a test-only page handoff manifest validator in `skippy-server`;
- a `skippy-correctness kv-page-handoff` source/coordinator harness for the
  future PGX/Mac foreground smoke.

The harness validates page manifests, ordered token ranges, checksum binding,
identity binding, source/coordinator frame serialization, and sanitized report
shape. When `--model` is provided, the source role can load a runtime, prefill
token chunks in one session, and call `export_kv_page`; the coordinator role
can tokenize synthetic input, request pages, validate manifests, call
`import_kv_page`, decode, and compare with a one-shot full-state baseline.

The later foreground phases proved heterogeneous runtime correctness for the
128-token two-chunk scope: PGX/CUDA exported KV page segments, Mac/Metal
imported them, trim/replay bootstrap produced logits, and page-path decode
exact-matched the local one-shot baseline.

## Completed Local Checks

- Positive two-page manifest validation.
- Source/coordinator CLI role parsing.
- Test-only JSON header plus raw payload frame roundtrip.
- Runtime-loop code path returns `inconclusive`, not `pass`, when a foreground
  model/runtime is not provided.
- Missing page fails closed.
- Duplicate page fails closed.
- Out-of-order page fails closed.
- Position gap fails closed.
- Position overlap fails closed.
- Checksum mismatch fails closed.
- Dtype/layout mismatch fails closed.
- Artifact, tokenizer, and chat template mismatch fail closed.
- Full-state blob is rejected as page-handoff proof.
- Report shape excludes prompt text, generated content, complete token arrays,
  KV/native payload contents, credentials, private paths, endpoint URLs, and
  real machine labels.

## Foreground Runtime Checks

- Observed PGX native `export_kv_page` after prefill chunk `0`.
- Observed PGX native `export_kv_page` after prefill chunk `1`.
- Observed Mac native ordered `import_kv_page` / append behavior.
- Final decode start position from real imported pages: `128`.
- Deterministic output comparison against local one-shot baseline:
  `exact_token_match`.
- Full-state path was not used as page-path pass.
- 4k/8k validation is deferred to future changes.

## Telemetry Shape

The local report model includes:

- `page_count`
- `total_page_bytes`
- `page_export_ms`
- `page_transfer_ms`
- `page_import_ms`
- `validation_result`
- `failure_reason`
- `recommendation`

`decode_ttft_after_import_ms` remains unset until a real decode follows native
page import.

## Scope Closure For This Phase

Current closure scope is a 128-token two-chunk page-level KV handoff
correctness pass. It is intentionally not a 4k/8k pass and not a full
`pd-streaming-kv-handoff` implementation.

The next phase may resume `pd-streaming-kv-handoff` using this correctness
evidence, but still needs a separate overlap/pipeline implementation and
validation plan.
