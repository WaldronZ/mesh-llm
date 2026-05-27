# Proposal: PD Streaming KV Cleanup Hardening

## Goal

Harden the default-off production `pd-kv-stream/1` path so a stalled or broken
control/page stream cannot permanently hold the single generation lane.

## Problem

After the 4k production integration pass, manual chat testing exposed a stale
in-flight request: the router entered `pd-kv-stream/1`, observed source export
events, received the first page frame, then blocked while reading a later page
frame. Because the request never returned to the lifecycle cleanup path,
`generation_concurrency=1` stayed occupied and later requests returned `429
rate_limit_exceeded`.

## Scope

This change adds bounded timeout/cleanup behavior for production streaming KV
request IO and failure paths. It validates the fix with a short foreground
cleanup smoke that releases a stale single-lane request. It does not change the
streaming protocol, production scheduler, payload format, model configuration,
or UI.

## Non-Goals

- No cleanup-specific 4k/8k smoke requirement.
- No broader production failure-injection matrix or long-soak validation.
- No multi-request concurrency or scheduler work.
- No full-state fallback as a streaming KV pass.
- No prompt text, generated content, full token arrays, KV payloads, private
  paths, hostnames, endpoint URLs, or credentials in diagnostics.
