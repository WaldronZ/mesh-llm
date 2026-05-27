# pd-streaming-kv-page-subframing Specification

## Purpose
TBD - created by archiving change pd-streaming-kv-page-subframing. Update Purpose after archive.
## Requirements
### Requirement: Page segment subframing schema

Production `pd-kv-stream/1` page streaming SHALL support splitting one logical
KV page segment payload into bounded subframes.

#### Scenario: Logical segment is represented by ordered subframes

- **GIVEN** PGX exports a logical KV page segment
- **AND** the segment payload is larger than the configured page subframe size
- **WHEN** the source writes the page stream
- **THEN** it SHALL split the logical segment into bounded subframes
- **AND** every subframe SHALL include request id, request-scoped session id,
  chunk index, segment index, subframe index, subframe count or final marker,
  byte offset, payload byte count, cache kind, segment kind, dtype/layout, and
  identity reference metadata.

#### Scenario: Subframes carry integrity metadata

- **WHEN** a subframe is emitted
- **THEN** it SHALL include a per-subframe checksum or equivalent integrity
  value
- **AND** it SHALL reference the logical segment checksum and total logical
  segment payload byte count.

### Requirement: Router reassembles logical segments before import

The router SHALL reconstruct a logical KV page segment from subframes before
calling the existing native `import_kv_page` API unless native streaming import
support is introduced by a separate change.

#### Scenario: Complete segment is imported once

- **GIVEN** all expected subframes for a logical segment have arrived
- **AND** subframe metadata, offsets, byte counts, and checksums are valid
- **WHEN** the router reassembles the logical segment payload
- **THEN** it SHALL validate the logical segment manifest and checksum
- **AND** it SHALL call `import_kv_page` once for that logical segment.

#### Scenario: Decode waits for reassembled imports

- **GIVEN** production streaming KV handoff is active
- **WHEN** one or more logical segments are still missing or not imported
- **THEN** the final contiguous gate SHALL fail closed or remain blocked
  within bounded timeout
- **AND** decode, SSE assistant content, and bootstrap SHALL NOT begin from
  incomplete page state.

### Requirement: Subframe validation fails closed

Production streaming KV handoff SHALL fail closed when subframe or segment
state is ambiguous, corrupt, incomplete, duplicated, or unsafe.

#### Scenario: Missing, duplicate, or out-of-order subframe fails

- **GIVEN** the initial production policy is fail closed on subframe ordering
- **WHEN** a subframe is missing, duplicated, or arrives out of order
- **THEN** the router SHALL fail the streaming request before import or decode
- **AND** it SHALL clean up request-scoped source and router state.

#### Scenario: Offset or length mismatch fails

- **WHEN** subframe byte offsets contain a gap or overlap
- **OR** the reassembled byte count does not equal the declared logical segment
  byte count
- **THEN** the router SHALL fail closed before import.

#### Scenario: Checksum or identity mismatch fails

- **WHEN** a per-subframe checksum, logical segment checksum, request id,
  session id, chunk index, segment index, cache kind, segment kind, dtype,
  layout, artifact identity, tokenizer identity, or chat template identity does
  not match expectations
- **THEN** the router SHALL fail closed before import.

### Requirement: Bounded frame capacity

Production `pd-kv-stream/1` SHALL avoid relying on a 1 GiB single-frame cap for
logical KV segments.

#### Scenario: Default frame size is bounded

- **WHEN** production streaming KV source and router are started with default
  subframing settings
- **THEN** the maximum page stream frame size SHALL be in a bounded range such
  as `16 MiB` to `64 MiB`
- **AND** large logical KV segments SHALL be split into subframes rather than
  requiring a one-frame payload.

#### Scenario: Capacity limits remain enforced

- **WHEN** a request exceeds max frame bytes, max logical segment bytes, max
  in-flight bytes, or max subframes per segment
- **THEN** the source or router SHALL fail closed with a bounded reason
- **AND** full-state handoff SHALL NOT be used as a streaming pass fallback.

### Requirement: Control errors remain visible during page reads

The router SHALL not report a known source-side control error as a misleading
page read timeout when a bounded control error is available.

#### Scenario: Source frame-size error is surfaced directly

- **GIVEN** the source cannot write a page segment or subframe because of a
  frame-size or capacity error
- **WHEN** it emits a bounded control error
- **THEN** the router SHALL observe and report that error as a source/control
  failure
- **AND** it SHALL NOT wait until page read timeout and report only
  `page_read_timeout`.

### Requirement: Subframing lifecycle diagnostics

Production streaming KV subframing SHALL emit sanitized diagnostics sufficient
to prove source write, router receive, reassembly, import, and cleanup.

#### Scenario: Successful subframe lifecycle is observable

- **WHEN** a logical segment is split into subframes and imported
- **THEN** diagnostics SHALL include bounded events for source subframe write
  start/end, router subframe received, segment reassembled, import start/end,
  final gate, bootstrap, decode start, and cleanup.

#### Scenario: Failure diagnostics are bounded and sanitized

- **WHEN** subframe streaming fails
- **THEN** diagnostics SHALL distinguish source frame-size errors, subframe
  validation failures, segment reassembly failures, page stream timeouts, and
  control errors using bounded labels
- **AND** diagnostics SHALL NOT contain prompt text, generated content,
  complete token arrays, KV/native payload contents, private paths, real
  hostnames, endpoint URLs, or credentials.

### Requirement: Foreground validation boundary

Page subframing SHALL be validated with local tests and bounded foreground
smokes before any 8k claim.

#### Scenario: Regression smoke covers large ISWA segment

- **WHEN** page subframing is implemented
- **THEN** a short production serving smoke SHOULD run first
- **AND** a medium prompt regression smoke SHOULD verify an ISWA `swa` logical
  segment larger than the old `64 MiB` frame cap transfers through subframes
- **AND** reports SHALL compare the result to the temporary 1 GiB cap
  workaround without claiming production performance readiness.

#### Scenario: 8k remains out of scope

- **WHEN** short and medium prompt subframing smokes pass
- **THEN** 4k may be considered separately
- **AND** 8k SHALL remain deferred for this change.

