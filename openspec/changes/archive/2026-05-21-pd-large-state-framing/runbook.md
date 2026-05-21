# PD Large State Framing Runbook

This runbook is for local protocol validation and a future authorized rerun of
the `pd-chunked-prefill` 4k foreground smoke. Do not start remote processes from
this runbook without explicit operator authorization.

## Local Validation

Run these commands serially:

```bash
cargo fmt --all -- --check
cargo test -p skippy-protocol --lib
cargo test -p skippy-server --lib
cargo check -p skippy-server
openspec validate pd-large-state-framing --strict
```

Expected local coverage:

- small `StateImport` payloads continue to use the legacy frame;
- large-state framing requires explicit capability/flag opt-in;
- large framed payloads round-trip through start/data/end frames;
- truncated, corrupted, out-of-order, or oversized framed payloads fail closed;
- PD manifest validation binds payload bytes, checksum, and large-state frame
  provenance before import/decode;
- telemetry contains only bounded sizes, counts, timings, and result labels.

## Future Foreground Smoke

Only after local validation passes and the operator explicitly authorizes
foreground machine validation, rerun the existing `pd-chunked-prefill` 4k smoke:

```bash
skippy-server serve-openai \
  --pd-serving-mvp \
  --pd-chunked-prefill \
  --pd-prefill-chunk-size 1024 \
  --pd-admission-over-limit reject \
  --pd-prefill-addr 127.0.0.1:<prefill-port> \
  --pd-decode-addr 127.0.0.1:<decode-port> \
  --pd-expected-artifact-sha256 <sha256> \
  --pd-expected-tokenizer-hash <sha256> \
  --pd-expected-chat-template-hash <sha256> \
  --pd-source-node-id <sanitized-source-label> \
  --pd-target-node-id <sanitized-target-label>
```

Do not use `--pd-router-validation`. Do not run 8k until 4k proves Mac
import/decode and SSE normal completion.

## Required Smoke Evidence

Record only sanitized evidence:

- result: pass / fail / inconclusive;
- recommendation: rerun_4k / proceed_to_8k / redesign;
- large-state framing capability/version;
- state payload bytes;
- frame count;
- max frame bytes;
- export/import write/read latency;
- checksum latency;
- manifest validation result;
- Mac import/decode result;
- SSE completion result;
- process survival and port release result.

Do not record prompt text, complete token arrays, generated content,
KV/native state payload contents, credentials, private paths, endpoint URLs, or
real machine labels.
