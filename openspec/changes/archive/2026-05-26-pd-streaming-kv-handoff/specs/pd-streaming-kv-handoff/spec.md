# pd-streaming-kv-handoff Specification

## ADDED Requirements

### Requirement: Streaming KV handoff lifecycle

The PD serving path SHALL support a capability-gated streaming/chunked
KV/native-state handoff lifecycle before it is used for decode.

#### Scenario: Chunk delta transfers overlap later prefill

- **GIVEN** PD serving is explicitly enabled
- **AND** both peers advertise streaming KV handoff capability
- **AND** a prompt is split into multiple prefill chunks
- **WHEN** PGX completes prefill chunk `N`
- **THEN** PGX SHALL export the KV/native-state delta or page range for chunk
  `N`
- **AND** that delta SHOULD be transmitted while PGX computes chunk `N+1`
  where runtime capability permits.

#### Scenario: Decode waits for contiguous verified chunks

- **GIVEN** Mac receives streamed KV/native-state chunks
- **WHEN** not all expected chunks have been received, verified, and imported
  in position order
- **THEN** Mac SHALL NOT start decode.

#### Scenario: Final decode start position is continuous

- **GIVEN** all streamed chunks have been received
- **WHEN** Mac validates the final request state
- **THEN** the final decode start position SHALL equal the total prompt token
  count
- **AND** chunk token ranges SHALL be contiguous with no gaps or overlaps.

### Requirement: Delta/page manifest

Each streamed KV/native-state unit SHALL carry enough manifest metadata to
validate identity, position, layout, and integrity.

#### Scenario: Per-chunk manifest binds identity and position

- **WHEN** PGX exports a chunk delta or page range
- **THEN** the manifest SHALL include protocol version, chunk index, token
  start/end positions, total expected chunks, total expected prompt tokens,
  artifact identity, tokenizer metadata hash, chat template hash, dtype/layout,
  KV byte range or page ids, payload byte count, checksum algorithm, and
  checksum.

#### Scenario: Final manifest binds all chunks

- **WHEN** the final handoff is validated
- **THEN** the final manifest SHALL bind the ordered per-chunk manifests
- **AND** it SHALL record final decode start position and total imported
  prompt tokens.

### Requirement: Ordering and fail-closed behavior

Streaming KV handoff SHALL fail closed when transfer state is ambiguous,
corrupted, or incomplete.

#### Scenario: Out-of-order or duplicate chunk fails closed

- **GIVEN** Mac expects chunk `N`
- **WHEN** it receives chunk `N+1`, a duplicate chunk, or a chunk with an
  unexpected frame index
- **THEN** Mac SHALL reject the handoff
- **AND** it SHALL NOT decode from the ambiguous state.

#### Scenario: Missing chunk fails closed

- **GIVEN** the final manifest declares an expected chunk count
- **WHEN** one or more chunks are missing at final validation
- **THEN** Mac SHALL reject the handoff before decode.

#### Scenario: Position gap or overlap fails closed

- **GIVEN** adjacent chunk manifests have token ranges
- **WHEN** the ranges have a gap or overlap
- **THEN** Mac SHALL reject the handoff before import/decode.

#### Scenario: Checksum or layout mismatch fails closed

- **GIVEN** a chunk manifest declares checksum, dtype, layout, or page identity
- **WHEN** the received bytes or metadata do not match
- **THEN** Mac SHALL reject the handoff.

#### Scenario: Import failure fails closed

- **GIVEN** a chunk payload passes manifest validation
- **WHEN** Mac incremental import fails
- **THEN** the PD path SHALL fail closed
- **AND** it SHALL NOT transparently fallback after assistant content is
  visible.

### Requirement: Capacity gates

Streaming KV handoff SHALL be bounded by explicit memory, network, and cleanup
limits.

#### Scenario: In-flight chunks are bounded

- **WHEN** streaming KV handoff is enabled
- **THEN** the router or transport SHALL enforce max in-flight KV chunks
- **AND** it SHALL enforce max queued bytes for transfer and import.

#### Scenario: Frame and timeout limits are enforced

- **WHEN** a KV delta/page frame exceeds max frame bytes or timeout limits
- **THEN** the transfer SHALL fail with a bounded error reason
- **AND** cleanup SHALL run for PGX, transport, and Mac importer state.

#### Scenario: Existing single-request boundary remains

- **WHEN** this change is applied
- **THEN** it SHALL NOT introduce multi-request production concurrency or a
  production scheduler.

### Requirement: Overlap telemetry

Streaming KV handoff SHALL report sanitized timing and pipeline utilization
metrics.

#### Scenario: Required overlap metrics are emitted

- **WHEN** a streaming handoff succeeds or fails
- **THEN** telemetry or reports SHALL include per-chunk prefill latency,
  KV delta export latency, KV delta network latency, KV delta import latency,
  overlap time, pipeline idle time, TTFT, bytes per token, chunk count, and a
  bounded result or failure reason label.

#### Scenario: Sensitive data is excluded

- **WHEN** telemetry, logs, or reports are produced
- **THEN** they SHALL NOT include prompt text, complete token arrays, generated
  content, KV/native payload contents, credentials, private paths, endpoint
  URLs, or real machine labels.

### Requirement: Correctness validation against one-shot handoff

Streaming KV handoff SHALL be validated against the existing one-shot
large-state framing path before it is considered pass.

#### Scenario: Streaming output is compared to one-shot baseline

- **GIVEN** a deterministic request suite
- **WHEN** both one-shot handoff and streaming KV handoff are available
- **THEN** the report SHALL compare streaming output with the one-shot baseline
- **AND** it SHALL record token/position continuity and any first divergence
  metadata available.

#### Scenario: Existing large-state framing remains fallback or baseline

- **WHEN** streaming KV handoff capability is missing, disabled, or fails before
  visible assistant content
- **THEN** the system SHALL use existing policy to fail closed or fall back
  before content
- **AND** the known-good large-state framing path SHALL remain available as a
  baseline.

### Requirement: Scope boundaries

This change SHALL remain focused on streaming/chunked handoff validation for
4k first and optional 8k after 4k pass.

#### Scenario: Larger production context remains out of scope

- **WHEN** work attempts to add 32k/128k/256k production support, KV
  compression, low-precision KV changes, multi-worker placement, scheduler
  behavior, production concurrency, default-on PD serving, or UI changes
- **THEN** that work SHALL be out of scope for this change.

#### Scenario: Runtime capability absence leads to spike outcome

- **GIVEN** the runtime cannot export per-chunk deltas/page ranges or import
  incremental state
- **WHEN** the capability audit completes
- **THEN** the change outcome SHALL be `inconclusive` or `redesign`
- **AND** implementation SHALL NOT claim streaming handoff by reusing only the
  final one-shot full-state export path.
