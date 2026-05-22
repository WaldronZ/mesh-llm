# Design: PD KV Page ISWA Trim Support

## Current State

The latest `pd-kv-page-handoff-spike` foreground smoke used a 128-token
synthetic two-chunk proof with `chunk_tokens=64`.

Observed:

- PGX/CUDA source exported native KV page segments;
- Mac/Metal coordinator imported native KV page segments;
- ISWA `base` and `swa` segment kinds were present;
- token ranges were continuous: `0..64` and `64..128`;
- checksums and manifest validation passed;
- full-state handoff was not used as a page pass;
- `trim_replay_last_token` bootstrap started;
- `trim_session(127)` failed with `runtime memory type is not supported for
  trim`;
- replay did not run;
- `logits_ready=false`;
- baseline comparison did not run.

This means the page export/import path is no longer blocked by ISWA page memory
classification. The next missing capability is native trim support for the
imported ISWA page state.

## Trim Bootstrap Requirement

The selected decode bootstrap is:

1. import KV pages for token history `0..N`;
2. trim target runtime state to `N - 1`;
3. replay the final prompt token at position `N - 1` with logits requested;
4. verify decode state returns to `N`;
5. sample/decode from fresh logits;
6. compare output with one-shot full-state baseline.

This avoids treating the last prompt token as a new token at position `N`.
Without trim, replay would duplicate or shift KV state and could produce an
off-by-one decode path.

## Native Trim Problem

The existing native trim path appears to accept only a bounded set of runtime
memory/cache objects. Imported Gemma4 ISWA page state is represented through an
ISWA memory wrapper with two logical sub-caches:

- `base`: non-sliding-window attention layers;
- `swa`: sliding-window attention layers.

For trim to be safe, both sub-caches must agree on the resulting sequence
length and token range. A partial trim is not acceptable.

## Candidate Implementation Shape

The preferred implementation should stay close to the existing trim boundary:

1. Extend native trim memory kind detection to recognize ISWA wrappers.
2. Route ISWA trim to the underlying `base` and `swa` sub-caches.
3. Trim both sub-caches to the same requested token count/position.
4. Verify post-trim state is consistent.
5. Return a sanitized error if either sub-cache cannot trim.

The regular non-ISWA trim path should remain unchanged.

## Fail-Closed Semantics

The change must fail closed when:

- trim memory kind is unknown;
- `base` trim succeeds but `swa` trim fails;
- `swa` trim succeeds but `base` trim fails;
- post-trim token count or sequence position is inconsistent;
- trim would require re-prefilling the whole prompt;
- replay would sample from stale logits;
- full-state fallback is needed to pass.

No partial trim result may count as a pass.

## Telemetry And Reporting

Reports should include only bounded, sanitized fields:

- trim memory kind, such as `regular`, `iswa`, `iswa/base`, `iswa/swa`, or
  `unknown`;
- requested trim position;
- imported token count;
- trim result;
- failed segment kind when bounded and safe;
- `logits_ready`;
- decode start position;
- baseline comparison.

Reports must not include prompt text, generated content, complete token arrays,
KV/native payload contents, credentials, private paths, endpoint URLs, real
machine labels, raw pointers, or device addresses.

## Correctness Gate

This change can pass only when the same small two-chunk foreground smoke
completes:

- page import succeeds;
- trim to `N - 1` succeeds;
- replay last prompt token at `N - 1` succeeds;
- `logits_ready=true`;
- `decode_start_position=N`;
- page-path decode exact-matches the one-shot full-state baseline under
  deterministic settings.

If decode diverges, the report must record bounded divergence metadata and
remain non-pass unless a separate accepted correctness rule explains it.

## Relationship To Other Changes

This change follows:

- `pd-kv-page-memory-type-support`, which unblocked ISWA page export/import;
- `pd-kv-page-decode-bootstrap`, which selected `trim_replay_last_token`.

It still precedes `pd-streaming-kv-handoff`, because streaming KV cannot be
useful until a complete page-imported sequence can be made decode-ready and
match the full-state baseline.
