# Design: PD KV Page Decode Bootstrap

## Current State

The latest small two-chunk page foreground smoke advanced the page handoff
proof beyond the previous memory-kind blocker:

1. PGX source prefilled chunk `0`.
2. PGX source exported ISWA `base` and `swa` page segments for `0..64`.
3. PGX source prefilled chunk `1`.
4. PGX source exported ISWA `base` and `swa` page segments for `64..128`.
5. Mac coordinator validated checksums and manifests.
6. Mac coordinator reached `import_kv_page`.
7. First decode/sample failed with
   `imported_page_state_has_no_current_logits_output_buffer`.

This is not a memory-type failure anymore. It is also not a pass. Page import
success does not imply the runtime is ready to sample.

## Decode Bootstrap Problem

Autoregressive decode needs more than KV cache/history. The sampler needs a
current logits/output buffer representing the next-token distribution at the
current decode position.

Full-state handoff can decode because full native state import restores a
larger set of runtime/session state. KV page handoff restores only page/range
KV data. After page import, the target runtime may know the imported token
history but still lack:

- current logits/output buffer;
- last evaluated token metadata;
- decode cursor or current position metadata;
- sampler-ready state tied to the imported KV sequence.

The page path must explicitly establish this state, or fail closed before
sampling.

## Candidate Strategies

### A. Mac-Side Last-Token Bootstrap

After importing all pages, the Mac runtime re-evaluates a small bootstrap input
to create current logits for sampling. The first candidate is the final prompt
token or an equivalent decode seed token with carefully controlled position
semantics.

Benefits:

- keeps PGX page payloads focused on KV data;
- avoids shipping large logits tensors;
- can reuse target backend evaluation behavior.

Risks:

- re-evaluating the last prompt token might duplicate KV entries or shift
  positions if the runtime cannot evaluate for logits without appending state;
- using the wrong position could produce an off-by-one decode state;
- exact-match baseline comparison is mandatory before claiming pass.

This is the recommended first spike path only if a read-only audit confirms
the runtime has a safe way to evaluate for logits without corrupting imported
KV continuity.

### B. PGX Decode Seed Token Or Logits Metadata

The PGX source could send bounded decode seed metadata with the page records.
Examples include final prompt token id, final position, and bootstrap mode.

Benefits:

- preserves source-side knowledge of the final prefill boundary;
- avoids guessing the bootstrap token on Mac.

Risks:

- seed token metadata alone may still require Mac-side evaluation;
- full logits are too large and sensitive to treat casually;
- metadata must not include complete prompt token arrays or generated content.

This strategy may complement A by making the bootstrap token/position explicit.

### C. Minimal Non-KV Decode State

Export a small, explicit non-KV decode-state record in addition to page
segments. This could include only the state required to make the target runtime
logits-ready, not the full native state blob.

Benefits:

- directly addresses the missing current output buffer;
- avoids relying on last-token re-evaluation semantics.

Risks:

- native runtime APIs may not expose a stable minimal decode-state boundary;
- the state may be backend-specific or too close to full-state handoff;
- correctness and privacy boundaries must be defined carefully.

This is a fallback if A cannot preserve correctness.

### D. Final Logits Buffer Export

If the native runtime can export the final logits/output buffer safely, page
handoff could include a bounded final logits state record.

Benefits:

- target can sample without a bootstrap eval;
- mirrors the exact source prefill endpoint.

Risks:

- logits tensors may be large and backend/layout sensitive;
- copying logits across heterogeneous backends may not match target-side
  numerical behavior;
- the state may be insufficient without sampler/session metadata.

This is a candidate only if the native ABI already supports or can safely
support it.

## Recommended Spike Path

Start with a read-only runtime audit, then implement only the smallest
correctness proof that the audit supports.

Recommended sequence:

1. Audit target runtime APIs for logits readiness, decode position, and
   evaluation-without-append semantics.
2. Confirm whether `import_kv_page` sets decode position or only writes KV
   page bytes.
3. Determine whether Mac can re-evaluate the final prompt token to populate
   current logits without duplicating KV or shifting positions.
4. If safe, implement Mac-side last-token bootstrap in the existing
   `kv-page-handoff` coordinator.
5. Compare page path decode with the one-shot full-state baseline under
   deterministic settings.
6. If exact match fails, record first divergence metadata and mark the result
   `inconclusive` or `fail`; do not claim pass.

If last-token bootstrap would require re-prefilling the whole prompt, or if it
cannot avoid duplicate KV semantics, switch to C or D instead of forcing a
false pass.

## Correctness Criteria

The change can pass only when:

- page import completes without full-state fallback;
- bootstrap strategy is recorded;
- `logits_ready = true` before sampling;
- decode start position equals the imported token count or the explicitly
  documented bootstrap-adjusted position;
- page-path decode exact-matches the one-shot full-state baseline under
  deterministic settings;
- no raw prompt text, generated content, complete token arrays, KV/native
  payload contents, credentials, private paths, endpoint URLs, or real machine
  labels appear in reports.

If backend nondeterminism appears, the report must record bounded first
divergence metadata and remain non-pass unless the divergence is explained by
an accepted correctness rule.

## Safety

The runtime and harness must fail closed when:

- imported KV pages are present but logits are not ready;
- bootstrap strategy is unsupported;
- bootstrap would require full prompt re-prefill;
- bootstrap would duplicate prompt tokens in KV;
- decode position cannot be verified;
- stale logits are detected or suspected;
- full-state blob is required to pass the page proof.

The page path must never sample from stale or missing logits.

## Telemetry And Report Shape

Reports should include sanitized fields:

- `page_import_result`;
- `bootstrap_strategy`;
- `bootstrap_eval_ms`;
- `logits_ready`;
- `decode_start_position`;
- `imported_token_count`;
- `baseline_comparison`;
- `first_divergence_token_index` when applicable;
- `result`: `pass`, `fail`, or `inconclusive`;
- `recommendation`: `resume_page_handoff_spike`, `redesign`, or
  `run_more_spike`.

Reports must not include prompt text, generated content, complete token arrays,
KV/native payload contents, credentials, private paths, endpoint URLs, real
machine labels, raw pointers, or device addresses.

## Relationship To Prior Changes

This change follows `pd-kv-page-memory-type-support` because that change
unblocked ISWA page export/import far enough to reveal the next missing state:
current logits/output buffer after import.

It also precedes `pd-streaming-kv-handoff`, because streaming page transfer is
not useful until a fully imported page sequence can bootstrap decode and match
the full-state baseline.
