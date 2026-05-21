# Tasks: PD Long Context Admission

## 1. Config / Admission Policy

- [x] 1.1 Define the admission policy inputs for the PD MVP lane:
      `max_prompt_tokens`, `max_prefill_batch`, `max_ctx_size`,
      `max_handoff_bytes`, or equivalent fields.
- [x] 1.2 Ensure admission policy is only active for explicit PD serving and
      does not change the default-off normal path.
- [x] 1.3 Define safe behavior when admission policy fields are missing:
      startup failure, PD unavailable, or documented conservative defaults.
- [x] 1.4 Ensure missing limits are never interpreted as unlimited.
- [x] 1.5 Preserve existing `inflight_limit=1` busy/admission behavior.

## 2. Token Counting And Estimation

- [x] 2.1 Count prompt tokens after coordinator-owned tokenization and before
      PGX prefill starts.
- [x] 2.2 Evaluate prompt token count against `max_prompt_tokens`.
- [x] 2.3 Evaluate prompt token count against `max_prefill_batch` or the known
      PGX prefill batch limit.
- [x] 2.4 Evaluate prompt plus requested generation budget against
      `max_ctx_size`.
- [x] 2.5 Estimate KV handoff bytes before export using configured or measured
      bytes-per-token data.
- [x] 2.6 Evaluate estimated KV bytes against `max_handoff_bytes`.

## 3. Pre-content Fallback / Rejection Semantics

- [x] 3.1 Ensure over-limit requests do not start PGX prefill.
- [x] 3.2 Route over-limit requests to normal path fallback or documented
      pre-content rejection according to policy.
- [x] 3.3 Ensure fallback/rejection happens before any assistant content delta.
- [x] 3.4 Record bounded sanitized admission reason codes.
- [x] 3.5 Ensure admission rejection cannot become a post-content mixed-path
      response.

## 4. Telemetry / Status

- [x] 4.1 Emit `pd.admission.result`.
- [x] 4.2 Emit `pd.admission.reason`.
- [x] 4.3 Emit `pd.prompt_token_count`.
- [x] 4.4 Emit `pd.estimated_kv_bytes`.
- [x] 4.5 Emit `pd.max_prompt_tokens`.
- [x] 4.6 Emit `pd.max_prefill_batch`.
- [x] 4.7 Emit `pd.max_ctx_size`.
- [x] 4.8 Emit `pd.max_handoff_bytes`.
- [x] 4.9 Ensure telemetry/status excludes prompt text, complete token arrays,
      generated content, KV payload contents, credentials, private paths, and
      private machine details.

## 5. Tests

- [x] 5.1 Add local test: prompt below threshold is admitted.
- [x] 5.2 Add local test: prompt exactly at threshold is admitted.
- [x] 5.3 Add local test: prompt above threshold falls back or rejects before
      PGX prefill.
- [x] 5.4 Add local test: missing admission config fails closed or uses a safe
      documented default.
- [x] 5.5 Add local test: normal path is unaffected when PD is disabled.
- [x] 5.6 Add local test: existing Skippy split behavior is unaffected.
- [x] 5.7 Add local test: required admission telemetry/status fields are
      present.
- [x] 5.8 Add local test: admission telemetry/status does not contain sensitive
      data.
- [x] 5.9 Run relevant cargo tests/checks serially.
- [x] 5.10 Run `openspec validate pd-long-context-admission --strict`.

## 6. Docs / Runbook

- [x] 6.1 Document admission policy fields and safe defaults.
- [x] 6.2 Document pre-content fallback/rejection behavior for over-limit
      prompts.
- [x] 6.3 Document the manual smoke procedure for near-threshold and
      over-threshold prompt cases.
- [x] 6.4 Document required sanitized evidence and telemetry fields.
- [x] 6.5 Avoid recording prompt text, full token arrays, KV payload contents,
      credentials, private paths, or private machine details.

## 7. Optional Foreground Smoke

- [x] 7.1 Build or stage current binaries only if foreground validation is
      explicitly authorized.
- [x] 7.2 Start Mac/PGX foreground observable processes only with explicit
      authorization.
- [x] 7.3 Run one near-threshold prompt that should be admitted.
- [x] 7.4 Run one over-threshold prompt that should fallback or reject before
      PGX prefill.
- [x] 7.5 Confirm PGX prefill process remains alive after the over-threshold
      request.
- [x] 7.6 Stop foreground validation processes and confirm ports are released.
- [x] 7.7 Record sanitized smoke evidence without prompt text or private
      environment details.
