# pd-kv-page-handoff-spike Specification

## Purpose
TBD - created by archiving change pd-kv-page-handoff-spike. Update Purpose after archive.
## Requirements
### Requirement: Spike-only page handoff scope

The change SHALL validate page-level KV handoff correctness and SHALL NOT
implement production streaming KV handoff.

#### Scenario: Full streaming pipeline is requested

- **WHEN** work attempts to overlap prefill, export, network transfer, and
  import as a production pipeline
- **THEN** that work SHALL be out of scope for `pd-kv-page-handoff-spike`
- **AND** it SHALL be deferred to `pd-streaming-kv-handoff` or a later change.

#### Scenario: Full-state framing is used as proof

- **WHEN** the implementation uses final full-state export/import to pass the
  spike
- **THEN** the spike SHALL NOT be considered a page-level handoff pass.

### Requirement: Minimal KV page handoff path

The spike SHALL define a deterministic path that exports KV pages after
prefill chunks and imports them in token-position order before decode.

#### Scenario: Two chunk page handoff succeeds

- **GIVEN** a deterministic prompt is split into at least two prefill chunks
- **WHEN** PGX completes chunk `0`
- **THEN** PGX SHALL export a KV page or range for chunk `0`
- **AND** the page manifest SHALL describe the token range for chunk `0`.
- **WHEN** PGX completes chunk `1`
- **THEN** PGX SHALL export a KV page or range for chunk `1`
- **AND** Mac SHALL import page `0` and page `1` in token-position order.

#### Scenario: Decode start position equals imported tokens

- **GIVEN** all expected pages have been imported
- **WHEN** Mac prepares to decode
- **THEN** the decode start position SHALL equal the total imported token count
- **AND** the imported token ranges SHALL be contiguous with no gaps or
  overlaps.

### Requirement: Page manifest metadata

Each KV page handoff unit SHALL carry enough manifest metadata to validate
identity, layout, position, and integrity.

#### Scenario: Page manifest is generated

- **WHEN** PGX exports a KV page or range
- **THEN** the manifest SHALL include protocol version, page index, total
  expected pages, token start/end positions, layer start/end range,
  dtype/layout metadata, native page flags, payload byte count, checksum
  algorithm, checksum, model artifact identity, tokenizer metadata hash, chat
  template hash, and source/target capability labels.

#### Scenario: Page manifest lacks required metadata

- **WHEN** Mac receives a page without required position, layout, identity,
  byte count, or checksum metadata
- **THEN** Mac SHALL reject the page before import.

### Requirement: Ordered import and fail-closed behavior

The spike SHALL reject ambiguous, corrupted, or incompatible page streams
before decode.

#### Scenario: Missing page fails closed

- **GIVEN** the final manifest declares an expected page count
- **WHEN** one or more pages are missing
- **THEN** Mac SHALL reject the handoff
- **AND** it SHALL NOT decode from the partial state.

#### Scenario: Duplicate or out-of-order page fails closed

- **GIVEN** Mac expects page `N`
- **WHEN** it receives a duplicate page or page `N+1` before page `N`
- **THEN** Mac SHALL reject the handoff before decode.

#### Scenario: Position gap or overlap fails closed

- **GIVEN** adjacent page manifests declare token ranges
- **WHEN** the ranges have a gap or overlap
- **THEN** Mac SHALL reject the handoff before import or decode.

#### Scenario: Checksum or layout mismatch fails closed

- **WHEN** the received page bytes, dtype, layout, layer range, token range, or
  checksum do not match the manifest
- **THEN** Mac SHALL reject the page before import.

#### Scenario: Import failure fails closed

- **WHEN** Mac fails to import a manifest-validated page
- **THEN** the spike SHALL fail
- **AND** it SHALL NOT claim correctness for that run.

### Requirement: Correctness comparison against one-shot baseline

The spike SHALL compare page-handoff decode with the existing one-shot
full-state handoff baseline under deterministic settings.

#### Scenario: Page handoff matches baseline

- **GIVEN** one-shot full-state handoff output is available for the same
  deterministic request
- **WHEN** Mac decodes after ordered page import
- **THEN** the spike SHALL prefer exact token match with the baseline
- **AND** exact final text match MAY be used only when token IDs are
  unavailable.

#### Scenario: Page handoff diverges

- **WHEN** page-handoff output diverges from the baseline
- **THEN** the spike report SHALL record first divergence token metadata when
  available
- **AND** it SHALL NOT mark the result as pass without a bounded explanation.

#### Scenario: Subjective similarity is used

- **WHEN** the only correctness evidence is that outputs look similar
- **THEN** the spike SHALL fail correctness.

### Requirement: Telemetry and report

The spike SHALL produce a sanitized report that captures page handoff timing,
size, and decision evidence.

#### Scenario: Report is produced

- **WHEN** the spike completes or fails
- **THEN** the report SHALL include result, recommendation, total pages, page
  token ranges, page bytes, page export latency, page transfer/read/write
  latency, page import latency, decode TTFT after final import, correctness
  comparison, and bounded failure reason if any.

#### Scenario: Sensitive data appears in report

- **WHEN** telemetry, logs, or reports include prompt text, generated content,
  complete token arrays, KV/native payload contents, credentials, private
  paths, endpoint URLs, or real machine labels
- **THEN** the report SHALL be considered invalid for this spike.

### Requirement: Spike outcome gates streaming handoff work

The result of this spike SHALL determine whether `pd-streaming-kv-handoff`
can proceed to implementation.

#### Scenario: Page handoff passes

- **WHEN** page export, ordered import, and deterministic correctness pass
- **THEN** the spike MAY recommend proceeding to a scoped
  `pd-streaming-kv-handoff` implementation.

#### Scenario: Page handoff cannot be proven

- **WHEN** page import cannot append, CUDA-exported pages cannot import into
  Metal, recurrent or non-KV state is missing, metadata is insufficient, or
  the implementation falls back to full-state blobs
- **THEN** the spike SHALL recommend `redesign` or `run_more_spike`
- **AND** streaming KV handoff SHALL NOT claim pass.

### Requirement: Scope boundaries

The spike SHALL remain focused on KV page correctness.

#### Scenario: Out-of-scope work is proposed

- **WHEN** work attempts to add 8k/32k/128k/256k promises, KV compression,
  low-precision KV changes, multi-worker placement, scheduler behavior,
  production concurrency, default-on PD serving, or Chat UI changes
- **THEN** that work SHALL be out of scope for this change.
