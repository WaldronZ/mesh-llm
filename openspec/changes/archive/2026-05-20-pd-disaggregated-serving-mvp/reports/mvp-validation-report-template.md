# PD Disaggregated Serving MVP Validation Report

## Result

```yaml
result: pending
recommendation: pending
change: pd-disaggregated-serving-mvp
```

This report template is for foreground MVP validation after separate machine
authorization. Do not include prompt text, complete token arrays, KV payload
contents, credentials, private paths, or private machine details.

## Environment

| Item | Sanitized value |
|---|---|
| Coordinator/router/decode role | pending |
| Prefill/export role | pending |
| Mac binary sha256 | pending |
| PGX binary sha256 | pending |
| Model artifact sha256 | pending |
| Tokenizer metadata hash | pending |
| Chat template hash | pending |
| PD serving default-off checked | pending |
| MVP test fault guard default-off checked | pending |

## Positive Path

| Prompt ID | HTTP | SSE data lines | Result | Prompt tokens | KV bytes | Export ms | Network transfer ms | Import ms | Router overhead ms | TTFT ms | Decode tok/s |
|---|---:|---:|---|---:|---:|---:|---:|---:|---:|---:|---:|
| pending | pending | pending | pending | pending | pending | pending | pending | pending | pending | pending | pending |

## Failure And Fallback

| Case | Expected behavior | Result | Evidence |
|---|---|---|---|
| manifest mismatch | fail closed before decode continuation | pending | pending |
| pre-content failure | fallback or documented pre-content error before content | pending | pending |
| post-content failure | content delta then explicit error/partial termination, no transparent fallback | pending | pending |
| cancellation cleanup | request state released | pending | pending |

## MVP Test Fault Guard

| Check | Result |
|---|---|
| `--pd-serving-mvp-test-fault` default is `none` | pending |
| Non-`none` test fault without explicit allow is rejected | pending |
| Test fault requires real `--pd-serving-mvp` path | pending |
| `--pd-router-validation` was not used to stand in for MVP | pending |

## Telemetry Privacy

| Check | Result |
|---|---|
| No prompt text | pending |
| No complete token arrays | pending |
| No KV payload contents | pending |
| No credentials | pending |
| No private paths | pending |
| No private machine details | pending |

## Cleanup

| Check | Result |
|---|---|
| Mac router stopped | pending |
| Mac decode/import stopped | pending |
| Tunnel stopped if used | pending |
| PGX prefill/export stopped | pending |
| Mac ports released | pending |
| PGX ports released | pending |

## Decision

Pending foreground validation.
