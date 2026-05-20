# Design: PD Disaggregated Serving Hardening

## Context

The scoped MVP already passed true-machine validation for the approved topology:
one Mac coordinator/router/decode worker and one PGX prefill/export worker. The
validated MVP should now be treated as a working baseline. This hardening change
does not restart architecture design and does not re-run native KV handoff
feasibility work.

The goal is to turn remaining risk into explicit regression coverage and
operator-safe lifecycle behavior:

- existing normal routes must remain stable when PD is disabled, ineligible, or
  busy;
- existing Skippy split serving must remain stable where shared serving code was
  touched;
- request lifecycle state must release cleanly across all terminal paths;
- streaming success and failure behavior must be covered locally;
- telemetry/status must stay sanitized and structurally useful.

## Relationship To The MVP Change

`pd-disaggregated-serving-mvp` provided the functional proof:

```text
OpenAI-compatible /v1/chat/completions
  -> Mac coordinator/router/decode
  -> PGX prefill/export
  -> native KV/decode-state handoff
  -> Mac import/decode
  -> OpenAI-compatible response
```

This change treats that as carry-forward evidence and focuses on hardening
around it:

| MVP result | Hardening focus |
|---|---|
| Positive PGX -> Mac serving passed | Add normal-path, Skippy split, and streaming regression tests |
| Manifest mismatch fail-closed passed | Add local negative tests and cleanup assertions |
| Pre-content fallback passed | Add fallback state release and response-shape tests |
| Post-content failure passed | Add no-transparent-fallback streaming tests |
| Sanitized report passed | Add telemetry privacy and metric presence tests |
| Single in-flight scope validated | Add busy/admission behavior tests |

## Normal Path Regression

Hardening must prove that PD being disabled by default is not only a config
property but a behavioral property. Local tests or smoke checks should cover:

- `--pd-serving-mvp` absent: existing OpenAI-compatible route behaves normally;
- PD config absent or incomplete: startup fails or PD is unavailable as
  documented, without silently changing the normal route;
- PD enabled but request ineligible: request follows documented normal route or
  pre-content error behavior;
- no PD-specific status or telemetry leak changes existing clients depend on.

## Skippy Split Serving Regression

The MVP touches shared Skippy serving/frontend code. Hardening must cover the
existing split-serving lane where practical:

- serving without PD flags still accepts existing split-serving configuration;
- binary transport behavior used by split serving is not broken by PD handoff
  additions;
- shared OpenAI frontend code keeps existing stream shapes;
- PD MVP test fault hooks cannot be activated for normal split serving.

This change does not replace Skippy split serving and does not use it to stand
in for PD MVP behavior.

## Lifecycle Cleanup

The scoped MVP has a single in-flight request limit. That makes cleanup
correctness especially important: stale state can block all later PD requests.

Hardening should assert cleanup for:

- success;
- manifest validation failure;
- pre-content fallback;
- post-content failure;
- timeout;
- client cancellation;
- busy/admission rejection;
- worker connection failure before content.

Cleanup means request/session state is released, admission capacity is restored,
temporary handoff state is dropped, and any recorded status/failure reason is
sanitized.

## Streaming And Error Semantics

The MVP report proved the real-machine behavior. Hardening should add local
coverage so future changes do not regress the OpenAI-compatible surface:

- successful streaming response shape;
- pre-content fallback response shape;
- post-content failure response shape;
- explicit SSE error or documented partial termination after content;
- no duplicate, reordered, or mixed-path output after post-content failure.

Prompt text, complete token arrays, generated content, and KV payload bytes must
not be persisted in test reports.

## Telemetry And Status Hardening

Required telemetry/status checks:

- metric presence for KV bytes, export, network read/write, isolated transfer,
  import, router overhead, TTFT, decode tokens/sec, result, failure phase, and
  fallback reason where applicable;
- `pd.kv_transfer_isolated=true` only when timing boundaries exclude export and
  import work;
- no prompt text, full token arrays, KV payload contents, credentials, private
  paths, or private machine details;
- additive status fields remain backward compatible;
- worker health/capacity/compatibility are reported in sanitized form.

## Busy And Admission Behavior

The scoped MVP keeps `inflight_limit=1`. Hardening must define and test the
second-request behavior:

- if one PD request is already admitted, the next eligible request must use the
  configured busy policy;
- busy policy must be explicit: normal-route fallback or documented pre-content
  rejection;
- busy result must record a sanitized reason;
- releasing the first request must restore admission capacity.

## Minimal Validation Strategy

Expected validation is primarily local:

- targeted Rust tests for config/default-off, admission, lifecycle, streaming,
  telemetry, and status behavior;
- smoke checks for normal route and Skippy split serving where shared code is
  touched;
- `cargo fmt --all -- --check`;
- relevant `cargo test` and `cargo check` commands;
- `openspec validate pd-disaggregated-serving-hardening --strict`.

Foreground Mac/PGX validation is not required by default because the MVP report
already proves the scoped live path. A minimal foreground check is allowed only
if a specific hardening item cannot be validated locally.

## Non-goals

This change does not add multiple decode workers, multiple PGX automatic
placement, production multi-request concurrency, automatic scheduling, KV
compression, incremental transfer, public mesh cross-owner PD, or production
serving behavior.
