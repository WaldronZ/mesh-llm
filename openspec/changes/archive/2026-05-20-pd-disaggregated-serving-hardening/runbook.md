# Runbook: PD Disaggregated Serving Hardening

This runbook is for local hardening and regression validation after the scoped
MVP has already passed foreground Mac/PGX validation. It does not re-prove
native PGX -> Mac KV handoff.

## Scope

Use this runbook to validate:

- normal OpenAI-compatible routing when PD is disabled;
- existing Skippy split serving guardrails where shared code was touched;
- lifecycle cleanup for success, failure, fallback, timeout, cancellation, and
  busy/admission paths;
- OpenAI-compatible streaming success and failure shapes;
- telemetry privacy and required metric presence;
- additive sanitized status/capability fields;
- `inflight_limit=1` busy/admission behavior.

Do not use this hardening runbook for multiple decode workers, automatic PGX
placement, production multi-request concurrency, automatic scheduling, KV
compression, incremental transfer, public mesh cross-owner PD, or production
serving validation.

## Local Validation Commands

Run cargo commands serially:

```text
cargo fmt --all -- --check
cargo test -p skippy-protocol --lib
cargo test -p skippy-server --lib
cargo check -p skippy-server
openspec validate pd-disaggregated-serving-hardening --strict
```

If a hardening change touches mesh-owned embedded Skippy integration, also run
the relevant `mesh-llm` Skippy tests before closing the change.

## Expected Evidence

The local test evidence should show:

- PD is default-off for normal and split-serving paths.
- MVP test faults require the explicit MVP double opt-in.
- MVP serving cannot be combined with existing Skippy split serving flags.
- MVP serving requires the scoped single-request lane.
- Busy admission under `inflight_limit=1` rejects without queueing and restores
  capacity after terminal paths.
- Streaming success, done, and explicit error shapes remain distinct.
- Status/capability fields are additive and sanitized.
- Telemetry summaries contain required timing/result fields without prompt
  text, complete token arrays, generated content, KV payload contents,
  credentials, private paths, or private machine details.

## Manual Or Foreground Smoke

Foreground Mac/PGX validation is not required by default for this change
because `pd-disaggregated-serving-mvp` already records scoped MVP pass.

Request separate authorization before any foreground machine smoke. A minimal
smoke may be justified only when a specific hardening item cannot be validated
locally, for example:

- a Skippy split-serving regression requires a real model fixture;
- a cancellation or timeout path depends on foreground process behavior;
- a runbook command shape changed and needs operator confirmation.

The minimal smoke must not restart the full MVP positive suite unless that
specific hardening item needs it. Do not record prompt text, generated content,
full token arrays, KV payload contents, credentials, private paths, or private
machine details.
