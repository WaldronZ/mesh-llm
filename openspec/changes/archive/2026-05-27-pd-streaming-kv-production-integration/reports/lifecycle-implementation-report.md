# PD Streaming KV Production Lifecycle Implementation Report

## Result

`inconclusive / ready_for_source_integration`

This second apply segment adds the local production lifecycle core for
`pd-kv-stream/1` behind the default-off streaming flag. The lifecycle is wired
through a mockable production backend trait so local tests can prove ordering,
manifest validation, final-gate, bootstrap, decode, and cleanup semantics
without starting Mac/PGX processes.

The live PGX production source protocol is not implemented in this segment.
When the real serving path is invoked with streaming enabled, it still fails
before assistant content with the bounded reason
`source_integration_not_implemented`; it does not fall back to full-state
handoff.

## Implemented Locally

- Added a production lifecycle coordinator function for the streaming KV mode.
- Reused coordinator-owned prompt token ids and the chunked prefill planner for
  chunk ranges.
- Added a `pd-kv-stream/1` production backend trait covering:
  - per-chunk source export;
  - per-segment target import;
  - trim/replay bootstrap;
  - decode after the final gate;
  - cleanup.
- Validated per-chunk manifests before import.
- Kept final decode gated on contiguous imported token ranges.
- Required bootstrap success before decode.
- Ensured lifecycle failures return before assistant content in local tests.
- Ensured the backend cleanup hook is called on success and on tested failure
  paths.
- Preserved the existing full-state PD path when streaming is not explicitly
  requested.

## Not Implemented Yet

- Live production split control/page channel client.
- Production PGX source service for per-chunk `export_kv_page`.
- Production Mac runtime `import_kv_page` integration in the serving path.
- Live request cancellation, timeout, and split-channel cleanup.
- Runtime/source capability handshake.
- Per-chunk live telemetry for prefill/export/transfer/import and overlap.
- Short and 4k production serving foreground smoke.
- 8k validation.

## Safety Notes

- Full-state handoff is not used as a streaming KV pass.
- The current real serving path fails pre-content when streaming is explicitly
  requested and live source integration is unavailable.
- Post-content fallback is not implemented or exercised in this segment.
- Reports and tests do not include prompt text, generated content, full token
  arrays, KV/native payload, credentials, private paths, endpoint URLs, or real
  machine labels.

## Verification Scope

Local tests cover:

- happy path with a mocked source/backend reaches the decode gate;
- source failure before content returns a bounded unavailable error;
- checksum/order/final-gate failures fail closed;
- bootstrap failure fails before decode;
- cleanup is called for tested success and failure paths;
- full-state handoff is not used when streaming is requested.

No Mac/PGX process was started. No foreground smoke, 4k, or 8k validation was
run in this segment.
