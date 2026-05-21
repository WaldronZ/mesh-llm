# PD Long Context Admission Runbook

This runbook is for the scoped PD serving MVP long-context admission guard.
It is not a production deployment guide and it does not cover chunked prefill,
multi-worker placement, or scheduler behavior.

## Required PD Admission Flags

`--pd-serving-mvp` remains default-off. When it is enabled, the router must be
started with explicit admission limits:

```bash
--pd-max-prompt-tokens <tokens>
--pd-max-prefill-batch <tokens>
--pd-max-ctx-size <tokens>
--pd-max-handoff-bytes <bytes>
--pd-estimated-kv-bytes-per-token <bytes>
--pd-admission-over-limit fallback
```

Use `--pd-admission-over-limit reject` only when clients should receive a
pre-content OpenAI-compatible error instead of normal-route fallback.

Missing or zero admission limits fail closed. Missing limits are never treated
as unlimited.

## Admission Behavior

The Mac coordinator/router computes prompt token count after tokenization and
before any PGX prefill/export work starts.

The effective token limit is:

```text
min(max_prompt_tokens, max_prefill_batch, max_ctx_size - requested_max_tokens)
```

The estimated KV handoff hard guard is:

```text
estimated_kv_bytes = prompt_token_count * estimated_kv_bytes_per_token
estimated_kv_bytes <= max_handoff_bytes
```

If a request exceeds any limit, it must either fallback or reject before the
first assistant content delta. It must not enter PGX prefill.

## Manual Smoke Shape

Only run this section after explicit foreground validation authorization.

1. Start the normal scoped PD MVP foreground processes with `--pd-serving-mvp`.
2. Include all required admission flags.
3. Send one synthetic near-threshold request whose token count is at or below
   the configured effective limit.
4. Send one synthetic over-threshold request whose token count exceeds the
   configured effective limit.
5. Confirm the over-threshold request records fallback or rejection before
   PGX prefill.
6. Confirm the PGX prefill process remains alive after the over-threshold
   request.
7. Stop foreground processes and confirm ports are released.

Do not record prompt text, full token arrays, KV payload contents, credentials,
private paths, private machine names, or private endpoint values in reports.

## Sanitized Evidence Fields

Record only bounded fields:

- `pd.admission.result`
- `pd.admission.reason`
- `pd.prompt_token_count`
- `pd.estimated_kv_bytes`
- `pd.max_prompt_tokens`
- `pd.max_prefill_batch`
- `pd.max_ctx_size`
- `pd.max_handoff_bytes`

Acceptable result values are bounded labels such as `admitted`, `fallback`,
`rejected`, and `pd_unavailable`.
