# pd-kv-page-iswa-trim-support Specification

## ADDED Requirements

### Requirement: ISWA trim support scope

The change SHALL address native/runtime trim support for imported Gemma4 ISWA
KV page state and SHALL NOT implement the streaming KV handoff pipeline.

#### Scenario: Streaming pipeline work is proposed

- **WHEN** work attempts to overlap prefill, page export, network transfer,
  page import, and decode as a streaming KV pipeline
- **THEN** that work SHALL be out of scope for this change.

#### Scenario: Larger prompt validation is requested

- **WHEN** 4k, 8k, 32k, 128k, or 256k validation is requested before the small
  two-chunk page proof exact-matches baseline
- **THEN** that validation SHALL be out of scope for this change.

### Requirement: Exact blocker is preserved

The change SHALL carry forward the latest bounded blocker from
`pd-kv-page-handoff-spike`.

#### Scenario: Imported ISWA page state is bootstrapped

- **GIVEN** PGX has exported ISWA `base` and `swa` page segments
- **AND** Mac has imported those page segments
- **WHEN** the coordinator starts `trim_replay_last_token`
- **THEN** it SHALL attempt to trim imported state to `N - 1`
- **AND** current behavior SHALL be treated as blocked when trim returns
  `runtime memory type is not supported for trim`.

#### Scenario: Replay is attempted without successful trim

- **WHEN** trim to `N - 1` fails
- **THEN** replay SHALL NOT run
- **AND** `logits_ready` SHALL remain false
- **AND** the result SHALL NOT be accepted as a page handoff pass.

### Requirement: Native trim audit

The change SHALL audit native and Rust trim support before selecting an
implementation strategy.

#### Scenario: Support matrix is produced

- **WHEN** the audit completes
- **THEN** it SHALL identify accepted trim memory/cache types
- **AND** it SHALL identify unsupported memory/cache types
- **AND** it SHALL identify whether regular, ISWA, hybrid, or unknown memory
  kinds are involved.

#### Scenario: ISWA state is inspected

- **WHEN** Gemma4 ISWA memory is encountered
- **THEN** the audit SHALL identify how `base` and `swa` sub-caches are
  reached by the trim path
- **AND** whether both sub-caches can be trimmed to the same requested
  position.

### Requirement: ISWA trim behavior

The runtime SHALL trim imported ISWA page state consistently across ISWA
sub-caches or fail closed.

#### Scenario: ISWA trim succeeds

- **GIVEN** imported ISWA page state represents tokens `0..N`
- **WHEN** trim is requested to `N - 1`
- **THEN** the runtime SHALL trim both `base` and `swa` state consistently
- **AND** the target session SHALL report the requested post-trim position.

#### Scenario: Base trim succeeds but SWA trim fails

- **WHEN** `base` trim succeeds but `swa` trim fails
- **THEN** the operation SHALL fail closed
- **AND** the result SHALL NOT be accepted as a page handoff pass.

#### Scenario: SWA trim succeeds but base trim fails

- **WHEN** `swa` trim succeeds but `base` trim fails
- **THEN** the operation SHALL fail closed
- **AND** the result SHALL NOT be accepted as a page handoff pass.

#### Scenario: Regular trim is used

- **WHEN** the runtime memory kind is regular non-ISWA KV cache
- **THEN** existing regular trim behavior SHALL remain backward-compatible.

### Requirement: Decode bootstrap correctness

The change SHALL enable `trim_replay_last_token` to proceed only when trim
produces a safe decode state.

#### Scenario: Trim and replay complete

- **GIVEN** imported token count is `N`
- **WHEN** trim to `N - 1` succeeds
- **AND** the final prompt token is replayed at position `N - 1`
- **THEN** `logits_ready` SHALL be true
- **AND** `decode_start_position` SHALL equal `N`.

#### Scenario: Decode start position is wrong

- **WHEN** bootstrap completes but decode start position does not equal `N`
- **THEN** the result SHALL fail closed
- **AND** no baseline pass SHALL be claimed.

#### Scenario: Stale logits are possible

- **WHEN** the runtime cannot prove logits belong to the replayed token at the
  expected position
- **THEN** sampling SHALL NOT proceed.

### Requirement: Correctness against full-state baseline

The two-chunk page path plus ISWA trim support SHALL be compared against the
one-shot full-state baseline under deterministic settings.

#### Scenario: Two-chunk proof passes

- **GIVEN** a 128-token deterministic two-chunk proof
- **WHEN** PGX exports ISWA page segments
- **AND** Mac imports page segments
- **AND** Mac trims, replays the last prompt token, and decodes
- **THEN** page-path decode SHALL exact-match the one-shot full-state baseline
- **AND** the report MAY claim pass.

#### Scenario: Baseline mismatch occurs

- **WHEN** page-path decode differs from the one-shot full-state baseline
- **THEN** the report SHALL record bounded first divergence metadata
- **AND** it SHALL NOT claim pass unless a separate accepted correctness rule
  explains the divergence.

#### Scenario: Full-state path is used as substitute

- **WHEN** full-state export/import is used to make the proof pass
- **THEN** the result SHALL NOT count as page handoff plus ISWA trim pass.

### Requirement: Sanitized telemetry and reports

The change SHALL report trim behavior using bounded, sanitized metadata.

#### Scenario: Trim report is produced

- **WHEN** the harness writes a report
- **THEN** it SHALL include trim memory kind, requested trim position,
  imported token count, trim result, `logits_ready`, decode start position,
  baseline comparison, result, and recommendation.

#### Scenario: Sensitive data appears

- **WHEN** reports include prompt text, generated content, complete token
  arrays, KV/native payload contents, credentials, private paths, endpoint
  URLs, real machine labels, raw pointers, or device addresses
- **THEN** the report SHALL be invalid.

### Requirement: Relationship to streaming KV

The change SHALL keep streaming KV blocked until the small page import plus
trim/replay proof passes.

#### Scenario: ISWA trim proof passes

- **WHEN** the two-chunk page handoff plus ISWA trim proof exact-matches the
  one-shot full-state baseline
- **THEN** `pd-kv-page-handoff-spike` MAY be revisited for closure
- **AND** `pd-streaming-kv-handoff` MAY be reassessed.

#### Scenario: ISWA trim remains unsupported

- **WHEN** native/runtime trim cannot support imported ISWA page state
- **THEN** streaming KV SHALL remain blocked
- **AND** large-state full-state framing SHALL remain the honest fallback.

### Requirement: Scope boundaries

The change SHALL remain focused on ISWA trim support.

#### Scenario: Out-of-scope work is proposed

- **WHEN** work attempts to add full streaming pipeline behavior, 4k/8k
  validation, 32k/128k/256k support, KV compression, low-precision KV changes,
  multi-worker placement, scheduler behavior, production concurrency, default
  enablement, or Chat UI changes
- **THEN** that work SHALL be out of scope for this change.
