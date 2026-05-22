# pd-kv-page-decode-bootstrap Specification

## ADDED Requirements

### Requirement: Decode bootstrap scope

The change SHALL address decode readiness after KV page import and SHALL NOT
implement the streaming KV handoff pipeline.

#### Scenario: Streaming pipeline work is proposed

- **WHEN** work attempts to overlap prefill, page export, network transfer,
  page import, and decode as a streaming pipeline
- **THEN** that work SHALL be out of scope for this change.

#### Scenario: Larger prompt validation is requested

- **WHEN** 4k, 8k, 32k, 128k, or 256k validation is requested before the small
  two-chunk page proof can decode
- **THEN** that validation SHALL be out of scope for this change.

### Requirement: Imported pages are not automatically decode-ready

The page handoff path SHALL treat imported KV pages as incomplete for sampling
until a current logits/output buffer is available.

#### Scenario: Page import succeeds but logits are missing

- **GIVEN** page manifests and payloads have imported successfully
- **WHEN** the target runtime does not have a current logits/output buffer
- **THEN** the harness SHALL NOT sample
- **AND** it SHALL fail closed or run an explicitly selected bootstrap
  strategy.

#### Scenario: KV import is reported as full decode success

- **WHEN** a report claims page handoff pass based only on successful page
  import
- **THEN** the report SHALL be invalid unless decode bootstrap and baseline
  comparison also pass.

### Requirement: Bootstrap strategy evaluation

The change SHALL evaluate candidate bootstrap strategies before claiming a
page handoff pass.

#### Scenario: Mac-side last-token bootstrap is selected

- **WHEN** Mac re-evaluates the final prompt token or equivalent decode seed
  after page import
- **THEN** the runtime SHALL ensure the operation creates current logits
  without duplicating KV entries or shifting token positions
- **AND** the report SHALL record the bootstrap strategy and adjusted decode
  position semantics.

#### Scenario: PGX seed metadata is selected

- **WHEN** PGX provides final token or decode seed metadata
- **THEN** the metadata SHALL be bounded
- **AND** it SHALL NOT include complete prompt token arrays, prompt text,
  generated content, or KV/native payload contents.

#### Scenario: Minimal non-KV decode state is selected

- **WHEN** the system exports/imports additional non-KV decode state
- **THEN** that state SHALL be explicitly described and validated
- **AND** it SHALL NOT be a full native state blob masquerading as page
  handoff.

#### Scenario: Final logits buffer export is selected

- **WHEN** native runtime support allows exporting final logits/output buffer
- **THEN** the report SHALL bind the state to the page manifest and decode
  position
- **AND** it SHALL fail closed on layout or byte-count mismatch.

### Requirement: Correctness against full-state baseline

The page handoff plus bootstrap path SHALL be compared against a one-shot
full-state baseline under deterministic settings.

#### Scenario: Two-chunk proof passes

- **GIVEN** a small deterministic two-chunk prompt
- **WHEN** PGX exports KV page segments
- **AND** Mac imports those page segments in order
- **AND** Mac completes the selected decode bootstrap
- **THEN** `logits_ready` SHALL be true before sampling
- **AND** page-path decode SHALL exact-match the one-shot full-state baseline
  or remain non-pass with bounded divergence evidence.

#### Scenario: Token-level divergence occurs

- **WHEN** page-path decode differs from the full-state baseline
- **THEN** the report SHALL record bounded first divergence metadata
- **AND** it SHALL NOT claim pass unless the divergence is explained by an
  accepted correctness rule.

#### Scenario: Full-state path is used as substitute

- **WHEN** full-state export/import is used to make decode succeed
- **THEN** the result SHALL NOT count as page handoff plus bootstrap pass.

### Requirement: Fail-closed decode safety

The runtime and harness SHALL fail closed rather than sampling from ambiguous
decode state.

#### Scenario: Bootstrap is unavailable

- **WHEN** imported pages lack current logits and no selected bootstrap
  strategy is available
- **THEN** the operation SHALL fail closed
- **AND** it SHALL report a bounded reason.

#### Scenario: Stale logits are possible

- **WHEN** the runtime cannot prove the current logits/output buffer belongs
  to the imported page sequence and decode position
- **THEN** sampling SHALL NOT proceed.

#### Scenario: Bootstrap requires full prompt re-prefill

- **WHEN** the only available bootstrap strategy requires re-prefilling the
  whole prompt on the target
- **THEN** the result SHALL be no-go for streaming KV
- **AND** it SHALL NOT be accepted as the page bootstrap path.

#### Scenario: Decode position is ambiguous

- **WHEN** the target cannot verify decode start position after import and
  bootstrap
- **THEN** the operation SHALL fail closed.

### Requirement: Telemetry and reporting

The change SHALL report decode bootstrap state using sanitized, bounded
metadata.

#### Scenario: Bootstrap report is produced

- **WHEN** the harness writes a report
- **THEN** it SHALL include page import result, bootstrap strategy,
  bootstrap eval latency, `logits_ready`, decode start position, imported
  token count, baseline comparison, result, and recommendation.

#### Scenario: Sensitive or oversized data appears

- **WHEN** reports include prompt text, generated content, complete token
  arrays, KV/native payload contents, credentials, private paths, endpoint
  URLs, real machine labels, raw pointers, or device addresses
- **THEN** the report SHALL be invalid.

### Requirement: Relationship to streaming KV

The change SHALL gate streaming KV work on the small page import plus bootstrap
proof.

#### Scenario: Decode bootstrap proof passes

- **WHEN** the two-chunk page handoff plus bootstrap proof passes
- **THEN** `pd-kv-page-handoff-spike` MAY be revisited for closure
- **AND** `pd-streaming-kv-handoff` MAY be reassessed.

#### Scenario: Bootstrap cannot avoid full-state dependence

- **WHEN** page import cannot become decode-ready without full-state blob
  import
- **THEN** streaming KV SHALL remain blocked
- **AND** large-state full-state framing SHALL remain the honest fallback.

### Requirement: Scope boundaries

The change SHALL remain focused on page-import decode bootstrap.

#### Scenario: Out-of-scope work is proposed

- **WHEN** work attempts to add full streaming pipeline behavior, 4k/8k
  validation, 32k/128k/256k support, KV compression, low-precision KV changes,
  multi-worker placement, scheduler behavior, production concurrency, default
  enablement, or Chat UI changes
- **THEN** that work SHALL be out of scope for this change.
