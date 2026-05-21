# Change: PD Chunked Prefill

## Why

The scoped PD MVP and hardening changes passed, and long-context admission now
keeps over-limit prompts out of PGX prefill. The current one-shot path is still
bounded by `n_batch=2048` and a calibrated KV byte gate: 4k/8k prompts are
safely rejected before PGX instead of admitted. The next step is not raising
thresholds. It is adding a bounded chunked prefill lifecycle so 4k/8k prompts
can be admitted without exceeding the PGX batch envelope.

## What Changes

Define and later validate a chunked prefill path for the default-off PD serving
lane:

1. split token IDs into prefill chunks that fit the configured batch envelope;
2. keep one logical PD request/session across chunks;
3. advance token positions and runtime state monotonically across chunks;
4. export native KV/decode state only after the final prefill chunk;
5. import/decode on Mac using the existing `pd-handoff/1` native handoff path;
6. extend manifest provenance and telemetry so operators can verify the chunked
   path without exposing prompts or KV payloads.

## Scope

Must:

- support 4k/8k prompt admission when chunked prefill capability is explicitly
  configured and healthy;
- preserve `--pd-serving-mvp` default-off activation;
- keep single Mac coordinator/router/decode plus single PGX prefill worker;
- keep chunk size bounded by `n_batch`, admission policy, and configured
  safety margin;
- define per-chunk ACK, error, timeout, cancel, cleanup, and fail-closed
  semantics;
- record chunked provenance in `pd-handoff/1`;
- emit sanitized chunk telemetry and reports;
- add local correctness/lifecycle tests and a 4k/8k foreground smoke plan.

Should:

- make chunk sizing configurable with a conservative default derived from the
  known prefill batch envelope;
- support pre-content fallback or rejection when chunked prefill is unavailable;
- compare 4k/8k chunked output against the existing deterministic baseline
  rules where feasible.

Won't:

- implement 32k+ production support;
- implement 128k or 256k context;
- add streaming/chunked KV handoff;
- add KV compression;
- add multi-worker placement, scheduler behavior, or production concurrency;
- make PD serving default-on.

## Impact

This proposal is docs/spec only. It does not modify business code, does not
apply the change, and does not start local or remote validation processes.

When applied later, expected implementation areas include the binary stage
prefill request protocol, PD router lifecycle, admission policy, handoff
manifest metadata, telemetry/reporting, local tests, and a separately
authorized 4k/8k foreground smoke.
