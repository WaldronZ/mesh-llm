# PD Streaming KV Production Source Listener Lifetime Report

## Result

`inconclusive / ready_for_short_foreground_serving_smoke_rerun`

This apply segment fixes the local production source listener lifetime blocker
found by the short foreground serving smoke audit. It does not start Mac/PGX
processes and does not run short, 4k, or 8k foreground smoke.

## Root Cause

The production `serve-binary --pd-streaming-kv-source` source initially bound
the control and page listeners, but accepted only one control/page stream pair.
If that pair reached EOF, received a bad frame, or stopped the current request,
the source listener function returned while the main `serve-binary` process
remained alive. Later router connections therefore had no streaming source
listener to connect to.

## Implemented Locally

- Changed the production source to use a serial request-scoped accept loop:
  accept control stream, accept page stream, process one request/session, clean
  up, then return to accept the next control/page pair.
- Kept single active request behavior; no scheduler or multi-request
  concurrency was added.
- EOF and bad control frames now clean up the current request scope and continue
  listening.
- `Stop` now ends the current request scope and continues listening instead of
  ending the source listener thread.
- Bind and fatal listener accept errors remain fail-closed.
- Added bounded diagnostics for listener start, request start, request EOF,
  request error, request cleanup, listener continue, and listener shutdown.
- Preserved the existing full-state PD path behavior.

## Local Test Coverage

- Source remains default-off through existing option tests.
- Control/page accept can continue after a dropped EOF pair.
- Empty control input is classified as EOF, not a fatal source exit.
- Malformed control input is classified as a bad frame, not a fatal source exit.
- Existing full-state PD path tests remain covered by the skippy-server test
  suite.

## Not Run

- No Mac/PGX foreground process was started for this segment.
- No short serving smoke was rerun.
- No 4k or 8k validation was run.

## Privacy

Diagnostics and this report do not include prompt text, generated content, full
token arrays, KV/native payload, credentials, private paths, endpoint URLs, or
real machine labels.
