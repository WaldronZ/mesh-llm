# Change: PD Streaming KV Handoff

## Why

The default-off PD serving path has now proven the scoped MVP, large-state
framing, and 4k chunked prefill. The remaining dominant cost is the final
native state/KV handoff. The 4k foreground smoke showed that PGX prefill
finishes much earlier than first token delivery because the system waits for a
final one-shot export, network transfer, and Mac import before decode can
begin.

Observed 4k timing shape:

- native state payload: about 3.5 GB;
- prefill: about 4.9 s;
- export: about 15.4 s;
- network transfer: about 30.7 s;
- import: about 14.1 s;
- TTFT: about 79.2 s.

The next useful question is whether the handoff can become a pipeline: export
and transfer each chunk's KV/native-state delta while PGX computes later
prefill chunks, then let Mac import verified chunks in order before final
decode.

## What Changes

This proposal defines a streaming/chunked KV handoff validation path for the
default-off PD serving lane:

1. PGX processes prefill chunk `N`;
2. PGX exports the KV/native-state delta or page range for chunk `N`;
3. the router/transporter sends that delta while PGX computes chunk `N+1`;
4. Mac validates and imports chunk `N` in position order;
5. final decode starts only after all chunks are contiguous, verified, and
   imported.

The change should start as a capability audit and spike because the current
native runtime may only expose full-state export/import. If delta/page export
or incremental import is unavailable, the correct outcome is redesign or a
new runtime API spike, not pretending the existing one-shot path is streaming.

## Scope

Must:

- define the streaming/chunked KV handoff lifecycle for 4k first and optional
  8k after 4k passes;
- define a versioned per-chunk delta/page manifest;
- validate ordering, position continuity, checksums, identity binding, and
  fail-closed behavior;
- define overlap telemetry for prefill, export, network, import, idle time,
  TTFT, and bytes per token;
- define memory and network capacity gates such as max in-flight KV chunks,
  queued bytes, frame bytes, timeout, cancel, and cleanup;
- compare streaming handoff correctness with the existing final one-shot
  handoff baseline;
- keep large-state framing as the fallback/baseline path;
- preserve default-off PD serving behavior.

Should:

- reuse the existing chunked prefill planner;
- reuse large-state frame stream concepts where possible;
- start with 4k and run optional 8k only after 4k passes;
- allow spike result labels: `pass`, `inconclusive`, or `redesign`.

Won't:

- promise or implement 32k/128k/256k production support;
- add KV compression;
- change KV precision or low-precision compatibility;
- add multi-worker placement;
- add scheduler behavior;
- add production concurrency;
- make PD serving default-on;
- change Chat UI behavior.

## Impact

This proposal is docs/spec only. It does not modify business code, does not
apply the change, and does not start local or remote validation processes.

When applied later, expected implementation areas include native runtime
capability discovery, binary transport framing, PD router lifecycle, manifest
validation, telemetry/reporting, local tests, and separately authorized
foreground smoke.
