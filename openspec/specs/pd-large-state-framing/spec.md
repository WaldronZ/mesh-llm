# pd-large-state-framing Specification

## Purpose
TBD - created by archiving change pd-large-state-framing. Update Purpose after archive.
## Requirements
### Requirement: Large native state framing

The binary state transport SHALL support capability-gated native state payloads
that exceed the legacy `i32` `StateImport` payload length limit.

#### Scenario: Large payload does not use legacy i32 length

- **GIVEN** a PGX state export payload exceeds the legacy `i32` length envelope
- **AND** both peers advertise large-state framing capability
- **WHEN** PGX exports native state
- **THEN** the exported payload SHALL NOT be encoded as legacy `StateImport`
  `token_count`
- **AND** the payload SHALL be sent through explicit large-state framing that
  can represent the full payload byte count.

#### Scenario: Small payload remains backward-compatible

- **GIVEN** a state payload fits the legacy `StateImport` envelope
- **WHEN** large-state framing support exists
- **THEN** the implementation SHALL preserve the existing small-payload wire
  behavior unless a negotiated policy explicitly selects the new framing
- **AND** existing non-PD binary serving SHALL remain compatible.

#### Scenario: Capability missing fails before partial large payload

- **GIVEN** a payload exceeds the legacy envelope
- **AND** the peer does not advertise large-state framing capability
- **WHEN** export is requested
- **THEN** the exporter SHALL reject with a bounded explicit error
- **AND** it SHALL NOT send a partial large-state payload.

### Requirement: Large-state frame stream

Large-state framing SHALL bound individual frames while preserving total
payload identity and ordering.

#### Scenario: Start frame declares payload identity

- **WHEN** large-state transfer starts
- **THEN** the first frame SHALL declare total payload bytes, frame count,
  maximum frame bytes, checksum algorithm, checksum or checksum commitment,
  request id, session id, and import-relevant state metadata.

#### Scenario: Data frames are ordered and bounded

- **WHEN** payload bytes are transferred
- **THEN** each frame SHALL include a bounded frame index and byte offset
- **AND** no frame SHALL exceed the configured maximum frame byte size
- **AND** the receiver SHALL validate that frames are contiguous with no gaps
  or overlaps.

#### Scenario: Completion requires full payload

- **WHEN** the receiver reaches the completion frame
- **THEN** the receiver SHALL have read exactly the declared total payload bytes
- **AND** it SHALL validate the checksum before attempting Mac import/decode.

### Requirement: Manifest integrity binding

`pd-handoff/1` validation SHALL bind large-state payload metadata before Mac
import/decode.

#### Scenario: Manifest records large-state metadata

- **GIVEN** a handoff uses large-state framing
- **WHEN** the handoff manifest is validated
- **THEN** the manifest SHALL include payload byte count, frame count, checksum
  algorithm, checksum, and framing protocol/version metadata
- **AND** these fields SHALL match the received large-state payload metadata.

#### Scenario: Checksum mismatch fails closed

- **GIVEN** a large-state payload checksum does not match the manifest or start
  frame metadata
- **WHEN** Mac validates the handoff
- **THEN** Mac SHALL reject the handoff before import/decode
- **AND** the response SHALL follow existing fail-closed semantics.

### Requirement: Failure semantics

Large-state framing SHALL fail closed on ambiguous or corrupted transfer state.

#### Scenario: Truncated stream fails closed

- **GIVEN** a large-state transfer starts
- **WHEN** the stream closes before all declared bytes arrive
- **THEN** the receiver SHALL reject the payload
- **AND** it SHALL NOT import partial state.

#### Scenario: Frame ordering mismatch fails closed

- **GIVEN** a received frame has an unexpected index, duplicate index, offset
  mismatch, or oversized payload
- **WHEN** the receiver validates the frame
- **THEN** it SHALL reject the transfer
- **AND** it SHALL NOT continue decode from ambiguous state.

#### Scenario: Import failure fails closed

- **GIVEN** payload transfer and checksum validation succeed
- **WHEN** Mac native state import fails
- **THEN** the PD path SHALL fail closed
- **AND** it SHALL not transparently fallback after assistant content is
  visible.

### Requirement: Telemetry and privacy

Large-state framing SHALL emit enough sanitized telemetry to debug framing
without exposing request or payload contents.

#### Scenario: Required metrics are emitted

- **WHEN** large-state framing succeeds or fails
- **THEN** telemetry or reports SHALL include state payload bytes, frame count,
  bounded frame byte sizes, write latency, read latency, checksum latency, and
  a bounded result or failure reason label.

#### Scenario: Sensitive data is excluded

- **WHEN** large-state framing telemetry, logs, or reports are produced
- **THEN** they SHALL NOT include prompt text, complete token arrays, generated
  content, KV/native state payload contents, credentials, private paths,
  endpoint URLs, or real machine labels.

### Requirement: Follow-up validation boundary

This change SHALL unblock `pd-chunked-prefill` 4k validation before any larger
smoke is attempted.

#### Scenario: 4k rerun comes before 8k

- **GIVEN** large-state framing implementation and local tests pass
- **WHEN** foreground validation is authorized
- **THEN** the next smoke SHALL rerun `pd-chunked-prefill` 4k first
- **AND** 8k SHALL remain unrun until 4k proves Mac import/decode and SSE normal
  completion.

#### Scenario: Explicit non-goals remain excluded

- **WHEN** implementation or review attempts to add 32k/128k/256k support, KV
  compression, multi-worker placement, scheduler behavior, production
  concurrency, or default-on PD serving
- **THEN** that work SHALL be out of scope for this change.
