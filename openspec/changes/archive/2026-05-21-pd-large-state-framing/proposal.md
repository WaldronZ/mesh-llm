# Change: PD Large State Framing

## Why

`pd-chunked-prefill` 4k foreground smoke reached PGX final native state export
but failed before Mac import/decode with:

```text
exported binary state payload exceeds i32 length
```

This shows the next blocker is not chunk planning or admission. It is the
binary state payload framing used by `StateExport` -> `StateImport`. Today the
exported native state length is converted into an `i32` and carried through the
existing wire `token_count` field. A 4k chunked prefill can produce a native
state payload larger than that framing can represent.

## What Changes

Design a large native state payload framing path for the PD serving lane:

1. identify the current `i32` payload length limit in binary transport;
2. compare candidate framing approaches for payloads above the old limit;
3. recommend an MVP-compatible large-state frame stream over the existing
   binary transport connection;
4. define backward compatibility and capability negotiation;
5. bind payload bytes, frame count, and checksum to `pd-handoff/1`;
6. define fail-closed behavior for partial, truncated, or corrupted payloads;
7. define sanitized telemetry and tests;
8. require rerunning `pd-chunked-prefill` 4k smoke after the framing blocker is
   fixed.

## Scope

Must:

- keep PD serving default-off;
- preserve the existing small-payload `StateImport` frame for backward
  compatibility;
- add or specify an explicit large-state framing mode that does not encode
  payload byte length in `i32`;
- require capability negotiation before using large-state framing;
- preserve Mac manifest validation and fail-closed import semantics;
- bind payload byte count and checksum to the handoff manifest;
- record sanitized state payload framing telemetry;
- test old-limit, over-old-limit, truncation, checksum mismatch, and privacy
  behavior.

Should:

- prefer streaming frame chunks over single-allocation `u64` payload reads so
  the implementation can bound memory while transferring large native state;
- use conservative per-frame byte limits and total payload limits;
- emit enough metrics to decide whether 4k/8k chunked prefill remains practical
  before trying larger contexts.

Won't:

- rerun or continue 8k smoke in this change proposal;
- implement 32k, 128k, or 256k context support;
- add KV compression;
- add semantic streaming/chunked KV handoff beyond framing large native state
  payloads;
- add multi-worker placement, scheduler behavior, or production concurrency;
- make PD serving default-on.

## Impact

This proposal is docs/spec only. It does not modify business code, does not
apply the change, and does not start local or remote validation processes.

When applied later, expected implementation areas include
`crates/skippy-protocol/src/binary/codec.rs`,
`crates/skippy-protocol/src/binary/types.rs`,
`crates/skippy-server/src/binary_transport.rs`, PD manifest validation,
telemetry/reporting, and local protocol tests. After local validation passes,
`pd-chunked-prefill` 4k smoke should be rerun before authorizing 8k.
