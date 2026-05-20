# Runbook: PD Disaggregated Serving MVP Foreground Validation

This runbook is the planned foreground validation procedure for the scoped MVP.
Do not run these steps without separate machine authorization.

## Scope

Validate one Mac coordinator/router/decode worker with one PGX prefill/export
worker. This runbook does not cover multiple decode workers, automatic
placement, production multi-request concurrency, public mesh PD, KV
compression, low precision KV, or production scheduling.

## Preconditions

- Current Mac and PGX binaries are built from the same intended source state.
- PD serving is explicitly enabled through the MVP config; default-off behavior
  has already been tested locally.
- The model artifact sha256, tokenizer metadata hash, and chat template hash
  are known and match on Mac and PGX.
- Required ports are checked read-only before startup.
- All processes run in foreground observable terminals or SSH TTY sessions.
- Prompt text, complete token arrays, KV payload contents, credentials, and
  private paths must not be copied into tracked reports.

## Sanitized Variables

Use environment-specific values from private operator configuration. Tracked
docs may refer to variable names only:

- `PD_MODEL_ID`
- `PD_MODEL_ARTIFACT_SHA256`
- `PD_TOKENIZER_HASH`
- `PD_CHAT_TEMPLATE_HASH`
- `PD_PREFILL_ADDR`
- `PD_DECODE_ADDR`
- `PD_OPENAI_ADDR`
- `PD_SOURCE_NODE_ID`
- `PD_TARGET_NODE_ID`
- `PD_MAX_TOKENS`

## Startup Plan

1. Read-only check Mac and PGX ports.
2. Start PGX prefill/export in a foreground SSH TTY.
3. Start any required local tunnel in the foreground.
4. Start Mac decode/import in the foreground.
5. Start Mac OpenAI-compatible router with explicit MVP enablement.

The router command must include the scoped MVP flag and required identity
fields. It must not use validation fault injection unless running a test-only
validation mode.

For MVP failure checks, use only the MVP-safe test fault mechanism on the real
`--pd-serving-mvp` path:

```text
--pd-serving-mvp \
--pd-serving-mvp-allow-test-faults \
--pd-serving-mvp-test-fault <none|manifest-mismatch|pre-content-failure|post-content-failure>
```

The test fault mechanism is default-off, hidden from normal production help,
and must not be used with `--pd-router-validation` to stand in for MVP
validation. Operators may alternatively set
`SKIPPY_ALLOW_PD_MVP_TEST_FAULTS=1` for the foreground validation shell, but the
tracked runbook should prefer the explicit allow flag.

## Positive Path

Run the sanitized prompt suite through:

```text
POST /v1/chat/completions
```

Required request settings for reproducibility:

- `temperature=0`
- fixed `seed` if supported
- bounded `max_tokens`
- streaming enabled for SSE checks

Record only prompt IDs, status, event counts, bytes, and timing fields.

## Failure Checks

Run the following checks in foreground validation:

| Check | Expected behavior |
|---|---|
| Manifest mismatch | Restart the router with `--pd-serving-mvp-test-fault manifest-mismatch`; PGX export must complete first, then Mac import must fail closed before decode continuation. |
| Pre-content failure | Restart the router with `--pd-serving-mvp-test-fault pre-content-failure`; fallback must happen before client-visible assistant content or return documented pre-content error. |
| Post-content failure | Restart the router with `--pd-serving-mvp-test-fault post-content-failure`; client receives assistant content delta before failure; transparent fallback is blocked; SSE error or documented partial termination is visible. |
| Cancel/cleanup | Temporary sessions are released. |

## Required Metrics

- `pd.kv_payload_bytes`
- `pd.kv_export_ms`
- `pd.kv_export_roundtrip_ms`
- `pd.kv_network_read_ms`
- `pd.kv_network_write_ms`
- `pd.kv_transfer_network_ms`
- `pd.kv_transfer_isolated`
- `pd.kv_import_ms`
- `pd.router_overhead_ms`
- `pd.ttft_ms`
- `pd.decode_tokens_per_sec`
- `pd.mvp.result`
- `pd.mvp.fallback_reason`
- `pd.mvp.failure_phase`

## Cleanup

Stop foreground processes with normal interrupt/shutdown in this order:

1. Mac router.
2. Mac decode/import.
3. Tunnel, if used.
4. PGX prefill/export.

After shutdown, check all validation ports are released. If a port remains
occupied, pause and report; do not kill existing processes without separate
authorization.

## Report

Use `reports/mvp-validation-report-template.md` and
`reports/mvp-validation-report-template.json` as the sanitized evidence shape.
