# Design: PD Large State Framing

## Current Limitation

The current binary stage wire format uses a fixed v4 prefix:

- `kind`, `pos_start`, `token_count`, `token_sideband_count`,
  `position_sideband_count` as `i32`;
- `StageStateHeader`;
- request/session ids;
- optional sampling/chat metadata;
- then token sidebands, raw state bytes, or activation bytes.

For `StateImport`, `token_count` is interpreted as the number of raw state bytes
to read. `read_stage_message_timed()` allocates exactly `token_count` bytes and
then `read_exact()` fills that buffer. During `StateExport`, the server exports
native runtime state, sets `state_payload_bytes = exported.len()`, and then
casts `exported.len()` to `i32` before sending a `StateImport` reply.

Evidence paths:

- `crates/skippy-protocol/src/binary/codec.rs`: fixed prefix and `StateImport`
  raw payload read use `token_count`.
- `crates/skippy-server/src/binary_transport.rs`: `StateExport` casts
  `exported.len()` to `i32` and currently emits the observed
  `exported binary state payload exceeds i32 length` error.
- `openspec/changes/pd-chunked-prefill/reports/chunked-prefill-smoke-report.md`:
  records the 4k smoke failure at PGX final binary state export.

This means payloads larger than `i32::MAX` cannot be represented by the current
state import/export frame. Even below that bound, a single giant allocation and
`read_exact()` is operationally risky.

## Candidate Options

### Option A: `u64` length framing in-place

Replace the `StateImport` length interpretation with a `u64` length field.

Pros:

- conceptually simple;
- low per-payload overhead;
- supports payloads larger than `i32::MAX`.

Cons:

- not backward-compatible if the same `StateImport` kind is reused;
- older readers will parse the wrong bytes and fail unpredictably unless the
  new format is distinguished before read;
- still encourages a single huge allocation and single-frame transfer.

### Option B: Large-state frame stream

Add an explicit capability-gated large-state framing mode over the existing
binary transport connection. The exporter sends a bounded metadata frame and a
sequence of bounded payload frames, followed by a completion frame. The receiver
validates frame order, total byte count, checksum, and manifest binding before
import/decode.

Pros:

- avoids encoding payload length in `i32`;
- can bound each frame size while supporting large total payloads;
- naturally detects partial/truncated streams and frame order mismatch;
- can keep old `StateImport` for small payloads;
- does not require shared filesystem or sidecar artifact management.

Cons:

- more protocol and state-machine work than an in-place `u64`;
- requires explicit capability negotiation and tests;
- does not reduce the total bytes transferred.

### Option C: Sidecar file / mmap / temp artifact

Write native state to a local temp artifact, transfer or expose the artifact,
and import from that artifact on Mac.

Pros:

- can avoid holding the full payload in one wire frame;
- could enable resumable or out-of-band transfer later.

Cons:

- introduces cleanup, file permission, path privacy, and cross-machine artifact
  transfer concerns;
- harder to keep private paths out of logs/reports;
- larger operational surface than needed for the scoped PD path.

### Option D: Explicit reject limit

Keep the current framing and reject payloads above a configured maximum with a
clear pre-import error.

Pros:

- safest short-term behavior;
- prevents ambiguous partial imports;
- easy to test.

Cons:

- does not unblock 4k chunked prefill;
- only improves error quality.

## Recommended MVP

Use **Option B: capability-gated large-state frame stream**, plus Option D as a
safe fallback while capability or limits are unavailable.

Recommended protocol shape:

- keep existing `StateImport` for small payloads that fit the legacy framing;
- add explicit large-state message kinds or an equivalent explicit versioned
  envelope, so old readers fail closed instead of misparsing;
- negotiate capability such as `large-state-framing/1` before using it;
- send a start/manifest frame with total bytes, frame count, frame size limit,
  checksum algorithm, checksum, request/session ids, and state header metadata;
- send ordered data frames with `frame_index`, `offset`, and bounded payload
  bytes;
- send an end frame only after all bytes are written and checksum is known;
- receiver validates order, total bytes, checksum, and manifest binding before
  import;
- Mac import/decode only starts after validation passes.

This is transport framing for large native state payloads. It is not a semantic
streaming/chunked KV handoff design and should not claim context scaling beyond
the next `pd-chunked-prefill` 4k rerun.

## Backward Compatibility

Small payloads continue to use the current `StateImport` frame and existing
tests.

Large payloads require both peers to advertise the new capability. If either
peer lacks the capability, the exporter must reject with a sanitized explicit
limit/error before sending any partial large-state payload. The implementation
must not reuse the old `StateImport` kind with a changed layout unless a reader
can unambiguously distinguish the new layout before consuming payload bytes.

Using new message kinds is preferred because old binaries will fail with
`unknown stage message kind`, which is fail-closed and easier to diagnose.

## Integrity

The large-state envelope must bind:

- total `state_payload_bytes`;
- `frame_count`;
- `frame_bytes` / max frame bytes;
- checksum algorithm and checksum;
- request id and session id;
- state header fields relevant to import;
- `pd-handoff/1` manifest identity fields.

The checksum should cover the concatenated payload bytes in order. The
manifest should record payload bytes and checksum without recording payload
contents.

## Failure Semantics

The receiver must fail closed when it sees:

- missing start frame;
- frame count mismatch;
- out-of-order frame;
- duplicate frame;
- offset mismatch;
- frame larger than configured max;
- stream closed before all bytes arrive;
- total bytes mismatch;
- checksum mismatch;
- import failure after successful transfer.

Before assistant content is visible, the router may use existing pre-content
fallback/rejection policy. After assistant content is visible, it must not
perform transparent fallback or mix outputs from another path.

## Telemetry

Sanitized telemetry should include:

- `pd.state_payload_bytes`;
- `pd.large_state_framing.enabled`;
- `pd.large_state_framing.protocol`;
- `pd.large_state.frame_count`;
- `pd.large_state.frame_bytes`;
- `pd.large_state.write_ms`;
- `pd.large_state.read_ms`;
- `pd.large_state.checksum_ms`;
- `pd.large_state.result`;
- bounded failure reason labels.

Telemetry and reports must not include prompt text, complete token arrays,
generated content, KV/native state payload contents, credentials, private
paths, endpoint URLs, or real machine labels.

## Validation Plan

Local tests should cover:

- small payload stays backward-compatible through old `StateImport`;
- payload just below old `i32` limit uses the expected path or rejects according
  to policy without allocation-heavy fixtures;
- payload over old limit uses large-state framing in tests with bounded fake
  data or fake readers/writers;
- truncated stream fails closed;
- frame order/offset mismatch fails closed;
- checksum mismatch fails closed;
- import failure after valid transfer fails closed;
- telemetry privacy and required metric presence.

Foreground validation should rerun only the `pd-chunked-prefill` 4k smoke after
local protocol tests pass. Do not proceed to 8k until 4k completes Mac
import/decode and SSE normal completion.
