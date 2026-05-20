# PD Disaggregated Serving MVP Validation Report

## Result

```yaml
result: pass
scoped_mvp: pass
production_ready: no
hardening_regression: resolved_by_pd-disaggregated-serving-hardening
recommendation: archive_after_hardening_review
change: pd-disaggregated-serving-mvp
```

Foreground validation reached the real MVP path with `--pd-serving-mvp` and
passed the 8-prompt positive suite. A later scoped rerun used the real
`--pd-serving-mvp` path plus the MVP-safe double opt-in test fault mechanism to
validate manifest mismatch, pre-content fallback, and post-content failure
semantics. `--pd-router-validation` was not used to stand in for MVP behavior.

This report intentionally excludes prompt text, complete token arrays, KV
payload contents, credentials, private paths, and private machine details.

## Scope Closure

| Item | Conclusion | Evidence |
|---|---|---|
| Scoped MVP | pass | This report's positive path and failure/fallback sections |
| Production-ready serving | no | MVP scope remains one Mac coordinator/router/decode worker, one PGX prefill/export worker, and single-request validation |
| Hardening/regression follow-up | resolved | `openspec/changes/pd-disaggregated-serving-hardening/tasks.md` records local hardening/regression completion |

The original MVP tasks that remained open after foreground validation were not
completed directly in this change. They were explicitly deferred to and
resolved by `pd-disaggregated-serving-hardening`: normal path regression,
Skippy split serving regression, lifecycle cleanup tests, OpenAI-compatible
streaming tests, telemetry privacy/metric presence tests, status/capability
hardening, and busy/admission behavior.

## Environment

| Item | Sanitized value |
|---|---|
| Coordinator/router/decode role | Mac coordinator/router/decode, foreground |
| Prefill/export role | Single PGX prefill/export, foreground |
| Mac binary sha256 | `810288aa45f0e40a22395f059c7fee740874526f2b4c18c4fffdfd8d9b9fac98` |
| PGX binary sha256 | `eae61b887b6a9a850afccb789cc9df0c40b98624e6bbac00fd0d744c281ed99e` |
| Model artifact sha256 | `96e3d95730b961682fe286a0e52dcda8173c5c2bda49c057801f437281556d01` |
| Tokenizer metadata hash | `6aa0dc8786823d04fb6d953994df47eb4f9382ed07efd8898411659778e0a397` |
| Chat template hash | `f86783fcbe17e6e9bd84d7246344a8a2f8c4d35860ca14edef0fc90559a528a3` |
| PD serving default-off checked | CLI help exposed `--pd-serving-mvp`; router was started with explicit flag |
| MVP test fault guard default-off checked | pass in local tests and true-machine rerun |

## Positive Path

| Prompt ID | HTTP | SSE data lines | Result | Prompt tokens | KV bytes | Export ms | Network transfer ms | Import ms | Router overhead ms | TTFT ms | Decode tok/s |
|---|---:|---:|---|---:|---:|---:|---:|---:|---:|---:|---:|
| short-1 | 200 | 34 | pass | 26 | 23431224 | 330.729 | 207.118 | 1.127 | 3741.946 | 806.727 | 10.526 |
| short-2 | 200 | 34 | pass | 23 | 20727792 | 366.040 | 181.122 | 1.032 | 3736.346 | 798.619 | 10.549 |
| medium-1 | 200 | 34 | pass | 34 | 30640376 | 332.925 | 269.868 | 1.161 | 3885.392 | 946.777 | 10.546 |
| medium-2 | 200 | 34 | pass | 34 | 30640376 | 353.944 | 267.261 | 1.115 | 3846.594 | 907.823 | 10.548 |
| long-1 | 200 | 34 | pass | 92 | 82906728 | 776.195 | 722.551 | 1.754 | 4946.902 | 2004.193 | 10.500 |
| long-2 | 200 | 34 | pass | 78 | 70290712 | 700.880 | 609.349 | 1.567 | 4747.575 | 1806.218 | 10.502 |
| target-long-context-1 | 200 | 34 | pass | 124 | 111743336 | 914.564 | 973.868 | 2.252 | 5522.178 | 2578.507 | 10.497 |
| target-long-context-2 | 200 | 34 | pass | 80 | 72093000 | 667.467 | 627.020 | 1.671 | 4763.357 | 1820.005 | 10.498 |

All positive rows emitted `pd.mvp.result=pass`,
`pd.validation_or_mvp.result=pass`, and `pd.kv_transfer_isolated=true`.

After adding the MVP-safe test fault mechanism, a scoped sanity prompt also
passed on the rebuilt Mac/PGX binaries: HTTP 200, 34 SSE data lines, one
`[DONE]`, no error lines, `pd.mvp.result=pass`, and
`pd.kv_transfer_isolated=true`.

## Failure And Fallback

| Case | Expected behavior | Result | Evidence |
|---|---|---|---|
| manifest mismatch | fail closed before decode continuation | pass | Real `--pd-serving-mvp` path with `--pd-serving-mvp-test-fault manifest-mismatch`; PGX export completed, manifest checksum validation failed before Mac import/decode, SSE emitted service-unavailable error, and no assistant content delta was emitted. Telemetry: `pd.mvp.result=fail`, `pd.mvp.failure_phase=manifest_validation`, `pd.mvp.failure_reason=payload_checksum`. |
| pre-content failure | fallback or documented pre-content error before content | pass | Real `--pd-serving-mvp` path with `--pd-serving-mvp-test-fault pre-content-failure`; telemetry marked `pd.pre_token=true`, `pd.mvp.result=fallback`, `pd.mvp.fallback_reason=pre_content_failure_injected`; response completed with HTTP 200, 34 SSE data lines, one `[DONE]`, and no error lines. |
| post-content failure | content delta then explicit error/partial termination, no transparent fallback | pass | Real `--pd-serving-mvp` path with `--pd-serving-mvp-test-fault post-content-failure`; client received one assistant content delta, then an explicit SSE service-unavailable error, with no transparent fallback. Telemetry: `pd.content_delta_count=1`, `pd.mvp.result=fail`, `pd.mvp.failure_phase=post_content_token_failure`, `pd.mvp.failure_reason=transparent_fallback_blocked_after_content_delta`. |
| cancellation cleanup | request state released | pass | All foreground validation processes were stopped with normal interrupt and Mac/PGX validation ports were released. |

## MVP Test Fault Guard

| Check | Result |
|---|---|
| `--pd-serving-mvp-test-fault` default is `none` | pass in local tests |
| Non-`none` test fault without explicit allow is rejected | pass in local tests |
| Test fault requires real `--pd-serving-mvp` path | pass in local tests |
| `--pd-router-validation` was not used to stand in for MVP | pass |

## Telemetry Privacy

| Check | Result |
|---|---|
| No prompt text in tracked report | pass |
| No complete token arrays in tracked report | pass |
| No KV payload contents in tracked report | pass |
| No credentials in tracked report | pass |
| No private paths in tracked report | pass |
| No private machine details in tracked report | pass |

## Cleanup

| Check | Result |
|---|---|
| Mac router stopped | pass |
| Mac decode/import stopped | pass |
| Tunnel stopped if used | pass |
| PGX prefill/export stopped | pass |
| Mac ports released | pass |
| PGX ports released | pass |

## Decision

The scoped MVP validation is complete for the approved one Mac coordinator /
decode worker plus one PGX prefill worker path. Positive serving, manifest
fail-closed behavior, pre-content fallback, post-content failure semantics,
sanitized telemetry, and foreground cleanup all passed within the approved
single-request validation scope.

This result should be read as scoped MVP pass, not production-ready approval.
The hardening/regression items deferred from this change have been completed by
`pd-disaggregated-serving-hardening`, so this MVP change is ready for archive
after normal OpenSpec review.
