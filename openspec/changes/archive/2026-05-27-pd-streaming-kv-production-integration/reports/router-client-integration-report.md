# PD Streaming KV Production Router Client Integration Report

## Result

`inconclusive / ready_for_short_foreground_serving_smoke`

This apply segment connects the Mac router production backend to the
`pd-kv-stream/1` split-channel source protocol in code and local tests. It does
not start Mac/PGX processes and does not run short, 4k, or 8k foreground smoke.

## Implemented Locally

- Replaced the unavailable production backend with a real router-side
  `pd-kv-stream/1` client when `--pd-streaming-kv-handoff` is explicitly
  enabled.
- Connected the router backend to `--pd-stream-control-addr` and
  `--pd-stream-page-addr`.
- Sent per-chunk token IDs as binary payload bytes over the control channel.
  The implementation does not log prompt text or token arrays.
- Read source control events and page frames.
- Validated protocol version, request id, chunk index, token range, identity,
  checksum, segment kind, payload bytes, and full-state-frame misuse.
- Imported each page segment into a request-scoped Mac runtime session with
  `import_kv_page`.
- Kept final decode gated on contiguous imported chunks and manifest
  validation.
- Added `trim-replay-last-token` bootstrap before decode.
- Routed decoded tokens through the existing OpenAI token callback, so SSE can
  begin only after the final gate and bootstrap pass.
- Added sanitized pass telemetry fields for segment count, payload bytes,
  import time, bootstrap time, and decode start position.
- Sent source stop and dropped the local runtime session during cleanup.

## Fail-Closed Coverage

Local tests cover:

- mocked source happy path reaches import, bootstrap, and decode;
- source error before content fails closed and calls cleanup;
- checksum mismatch fails before decode;
- identity/order/gap/overlap/missing segment failures remain covered by shared
  manifest validation;
- import failure fails closed and calls cleanup;
- bootstrap failure fails closed before assistant content;
- full-state frames are rejected in streaming mode;
- existing non-streaming PD MVP/full-state tests continue to pass.

## Remaining Gaps

- No foreground short serving smoke has been run.
- No 4k production serving smoke has been run.
- No 8k validation has been run.
- Live timeout/cancel behavior has not been exercised with real processes.
- Production capability handshake is still pending.
- Fine-grained per-chunk prefill/export/transfer timing is not fully emitted in
  the serving path yet.

## Safety Notes

- Full-state handoff is not used as a streaming KV pass.
- Streaming mode remains default-off and only selected when
  `--pd-streaming-kv-handoff` is set.
- Existing `--pd-serving-mvp` full-state behavior remains unchanged when the
  streaming flag is absent.
- Reports and tests do not include prompt text, generated content, full token
  arrays, KV/native payload, credentials, private paths, endpoint URLs, or real
  machine labels.
