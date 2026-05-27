# PD Streaming KV Production Integration Local Report

## Result

`inconclusive / ready_for_lifecycle_implementation`

This first apply segment adds the default-off production configuration and
fail-closed skeleton for `pd-kv-stream/1`. It does not implement the live
production split-channel lifecycle, does not start Mac/PGX processes, and does
not run short, 4k, or 8k foreground smoke.

## Implemented Locally

- Added explicit streaming KV serving enablement:
  `--pd-streaming-kv-handoff`.
- Added required production skeleton channel config:
  `--pd-stream-control-addr` and `--pd-stream-page-addr`.
- Added bounded capacity config:
  `--pd-stream-max-in-flight-chunks`,
  `--pd-stream-max-in-flight-bytes`,
  `--pd-stream-max-frame-bytes`, and
  `--pd-stream-max-queue-depth`.
- Preserved existing `--pd-serving-mvp` full-state behavior when streaming is
  not enabled.
- Rejected streaming config without `--pd-streaming-kv-handoff`.
- Rejected `--pd-streaming-kv-handoff` without `--pd-serving-mvp`.
- Rejected missing stream channel config and invalid capacity limits.
- Added a distinct streaming handoff mode that reports protocol
  `pd-kv-stream/1`.
- Added a production skeleton that fails before assistant content with a
  service-unavailable error instead of silently using full-state handoff.
- Added local manifest validation for chunk/token range, ISWA `base`/`swa`
  segments, dtype/layout identity, checksum, and full-state frame rejection.
- Added sanitized status/telemetry skeleton fields for streaming enabled,
  protocol, lifecycle state, channel configured booleans, capacity settings,
  failure phase, and failure reason.

## Not Implemented Yet

- Live production split control/page channels.
- Production PGX per-chunk `export_kv_page`.
- Production Mac per-chunk `import_kv_page`.
- Production final contiguous gate.
- Production `trim-replay-last-token` bootstrap.
- Runtime capability detection.
- Live import/bootstrap failure cleanup.
- Short and 4k production serving foreground smoke.
- 8k validation.

## Safety Notes

- The skeleton fails pre-content and does not emit assistant content.
- Full-state handoff is not used as a streaming KV pass.
- Existing `--pd-serving-mvp` full-state behavior remains the default when the
  streaming flag is absent.
- Reports and tests do not include prompt text, generated content, full token
  arrays, KV/native payload, credentials, private paths, endpoint URLs, or real
  machine labels.

## Verification

Initial local verification:

- `cargo test -p skippy-server --lib`: pass.

Final command results are recorded in the assistant turn summary for this
apply segment.
