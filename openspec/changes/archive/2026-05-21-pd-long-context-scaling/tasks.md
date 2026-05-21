# Tasks: PD Long Context Scaling

## 1. Measurement Model And Bytes/Token Calibration

- [x] 1.1 Extract measured KV bytes/token from existing MVP and admission smoke
      reports for the active Gemma PD topology.
- [x] 1.2 Replace or correct the currently low estimate
      `estimated_kv_bytes_per_token=524288` with a conservative calibrated
      value or explicit operator override.
- [x] 1.3 Record calibration source as a bounded sanitized value such as
      `measured`, `configured`, or `conservative_default`.
- [x] 1.4 Emit estimated and actual KV bytes in reports so drift is visible.
- [x] 1.5 Fail safe when a hard byte limit is configured but bytes/token cannot
      be estimated.

## 2. Config / Admission Ladder Design

- [x] 2.1 Define the relationship among `ctx_size`, `n_batch`,
      `max_prompt_tokens`, `max_handoff_bytes`, and requested generation
      tokens.
- [x] 2.2 Implement or document the staged gate order: token, context, prefill
      batch, KV bytes, memory, network/SLA, and lifecycle.
- [x] 2.3 Ensure over-limit outcomes still happen before PGX prefill starts.
- [x] 2.4 Preserve default-off PD serving behavior.
- [x] 2.5 Preserve existing normal path, Skippy split path, and admission guard
      behavior.

## 3. 4k / 8k Scaling Implementation Or Smoke Plan

- [x] 3.1 Define Phase A target configurations for 4k and 8k validation.
- [x] 3.2 Deferred: this change does not admit 4k/8k prompts on the current
      one-shot handoff path; 4k/8k admission must wait for chunked prefill or
      an equivalent bounded long-context execution strategy.
- [x] 3.3 If implementation is deferred, document exactly which missing
      runtime capability blocks 4k or 8k admission.
- [x] 3.4 Add or update a report template for 4k/8k near-threshold and
      over-threshold results.
- [x] 3.5 Ensure over-threshold validation proves PGX prefill does not receive
      the request.

## 4. Chunked Prefill Design Notes For 32k

- [x] 4.1 Document why 32k requires chunked prefill or an equivalent strategy.
- [x] 4.2 Define chunked prefill requirements for session identity, position
      continuity, token range accounting, ACK/error handling, cancel, cleanup,
      and final export.
- [x] 4.3 Define required telemetry for chunk count, chunk size, per-chunk
      latency, final export bytes, transfer time, and TTFT.
- [x] 4.4 Mark chunked KV handoff, KV compression, and 256k implementation as
      separate future changes.

## 5. Runbook / Reporting

- [x] 5.1 Add a runbook or smoke plan for 4k near-threshold validation.
- [x] 5.2 Add a runbook or smoke plan for 8k near-threshold validation.
- [x] 5.3 Add over-threshold reject/fallback validation that checks PGX process
      survival.
- [x] 5.4 Report actual KV bytes, estimated KV bytes, export latency, isolated
      network transfer latency, import latency, TTFT, and decode tokens/sec.
- [x] 5.5 Exclude prompt text, complete token arrays, generated content, KV
      payload contents, credentials, private paths, and private machine labels.

## 6. Tests

- [x] 6.1 Add local tests for calibrated bytes/token selection.
- [x] 6.2 Add local tests for effective prompt limit calculation.
- [x] 6.3 Add local tests for token, context, prefill batch, KV byte, memory,
      and network/SLA gate ordering.
- [x] 6.4 Add local tests that missing calibration data fails safe.
- [x] 6.5 Add local tests that over-limit requests do not reach PGX prefill.
- [x] 6.6 Add local regression tests that normal path and Skippy split behavior
      remain unaffected.
- [x] 6.7 Run relevant cargo tests/checks serially.
- [x] 6.8 Run `openspec validate pd-long-context-scaling --strict`.

## 7. Optional Foreground Smoke

- [x] 7.1 Start Mac/PGX foreground observable validation processes only with
      separate explicit authorization.
- [x] 7.2 Run a baseline near-safe request below the current effective limit and
      confirm it is admitted with PD path pass.
- [x] 7.3 Run a 4k over-limit request and confirm it is rejected before PGX
      prefill.
- [x] 7.4 Run an 8k over-limit request and confirm it is rejected before PGX
      prefill.
- [x] 7.5 Confirm PGX prefill remains alive after over-threshold validation.
- [x] 7.6 Stop foreground validation processes and confirm ports are released.
- [x] 7.7 Record sanitized smoke evidence without prompt text or private
      environment details.
