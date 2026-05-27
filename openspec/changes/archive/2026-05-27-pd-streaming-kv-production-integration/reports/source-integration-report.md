# PD Streaming KV Production Source Integration Report

## Result

`inconclusive / ready_for_router_client_integration`

This third apply segment adds the production source-side `pd-kv-stream/1`
wire and `serve-binary` integration. It does not start Mac/PGX processes and
does not run short, 4k, or 8k foreground smoke.

## Implemented Locally

- Added default-off `serve-binary` source enablement:
  `--pd-streaming-kv-source`.
- Added source bind flags:
  `--pd-stream-control-bind-addr` and `--pd-stream-page-bind-addr`.
- Reused the existing bounded capacity flags for source validation:
  `--pd-stream-max-in-flight-chunks`,
  `--pd-stream-max-in-flight-bytes`,
  `--pd-stream-max-frame-bytes`, and
  `--pd-stream-max-queue-depth`.
- Added a skippy-server internal `pd-kv-stream/1` source wire module with
  versioned control requests/events and page stream frames.
- Required chunk requests to carry token IDs as binary payload bytes; range-only
  requests fail closed.
- Added source-side prefill using `RuntimeState::prefill_kv_stream_chunk`.
- Added source-side per-chunk `export_kv_page_segments`.
- Added page frame manifests with protocol version, token range, cache kind,
  segment kind, layer range, dtype/layout identity, checksum, payload bytes,
  and native page descriptor metadata.
- Preserved Gemma4 ISWA `base`/`swa` segment expression.
- Added source stop/error cleanup for the request-scoped runtime session.
- Kept the Mac router production backend unavailable; this segment does not
  implement production import/decode.

## Fail-Closed Coverage

Local tests cover:

- `serve-binary` streaming source is default-off.
- Missing control/page bind addresses are rejected.
- Invalid capacity limits are rejected.
- Source wire parser rejects the wrong protocol version.
- Prefill chunk requests without token payload fail closed.
- Page frames reject full-state manifests.
- Page frames reject identity mismatch.
- Error events use bounded sanitized labels.
- Frame size caps are enforced.
- Existing full-state PD path tests continue to pass.

## Not Implemented Yet

- Mac router production client for the source control/page channels.
- Production Mac-side per-chunk `import_kv_page`.
- Production final contiguous gate, trim/replay bootstrap, and decode/SSE path.
- End-to-end cancellation between router, source, and importer workers.
- Foreground short serving smoke.
- 4k production serving smoke.
- 8k validation.

## Safety Notes

- Full-state handoff is not used as a streaming KV pass.
- The production router backend still fails pre-content while the live router
  client/import/decode integration is pending.
- Reports and tests do not include prompt text, generated content, full token
  arrays, KV/native payload, credentials, private paths, endpoint URLs, or real
  machine labels.
