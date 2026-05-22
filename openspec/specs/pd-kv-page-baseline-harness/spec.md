# pd-kv-page-baseline-harness Specification

## Purpose
TBD - created by archiving change pd-kv-page-baseline-harness. Update Purpose after archive.
## Requirements
### Requirement: Baseline harness scope

The change SHALL address deterministic baseline comparison for the
`kv-page-handoff` correctness harness and SHALL NOT implement streaming KV
handoff.

#### Scenario: Streaming work is proposed

- **WHEN** work attempts to overlap prefill, page export, network transfer,
  page import, or decode as a streaming pipeline
- **THEN** that work SHALL be out of scope for this change.

#### Scenario: Larger prompt validation is requested

- **WHEN** 4k, 8k, 32k, 128k, or 256k validation is requested before the
  128-token two-chunk baseline comparison passes
- **THEN** that validation SHALL be out of scope for this change.

### Requirement: Baseline availability

The harness SHALL treat baseline availability as mandatory for a correctness
pass.

#### Scenario: Baseline restore session is unavailable

- **GIVEN** page-path decode has been observed
- **WHEN** the one-shot baseline cannot be created or restored
- **THEN** the result SHALL be `inconclusive`
- **AND** the report SHALL record a bounded failure reason.

#### Scenario: Baseline comparison is skipped

- **WHEN** the harness skips baseline token comparison
- **THEN** it SHALL NOT report page handoff pass.

### Requirement: Baseline strategy separation

The harness SHALL clearly separate baseline execution from page-path execution.

#### Scenario: Full-state handoff is used as baseline

- **WHEN** full-state export/import is used to produce baseline tokens
- **THEN** the report SHALL label it as baseline-only
- **AND** it SHALL NOT count full-state handoff as page-path pass.

#### Scenario: Local one-shot baseline is used

- **WHEN** the coordinator runs a local one-shot prefill/decode baseline
- **THEN** it SHALL use the same prompt tokens and deterministic settings as
  the page path
- **AND** it SHALL keep baseline session state separate from page-path state.

### Requirement: Deterministic comparison

The harness SHALL compare page-path decoded tokens with baseline decoded
tokens under deterministic settings.

#### Scenario: Exact match occurs

- **GIVEN** page import, trim/replay bootstrap, and page-path decode complete
- **AND** baseline decode completes
- **WHEN** page-path decoded tokens exactly match baseline decoded tokens
- **THEN** the result MAY be `pass`.

#### Scenario: Token mismatch occurs

- **WHEN** page-path decoded tokens differ from baseline decoded tokens
- **THEN** the report SHALL record bounded first divergence metadata
- **AND** it SHALL NOT include generated text or complete token arrays
- **AND** the result SHALL be `fail` or `inconclusive`.

### Requirement: Fail-closed reporting

The harness SHALL fail closed when correctness cannot be proven.

#### Scenario: Page path decodes but baseline fails

- **WHEN** page-path decode produces tokens
- **AND** baseline decode cannot run
- **THEN** the result SHALL remain `inconclusive`.

#### Scenario: Baseline strategy is ambiguous

- **WHEN** the report cannot distinguish page path from baseline path
- **THEN** the report SHALL be invalid.

### Requirement: Sanitized report

The report SHALL contain only bounded metadata.

#### Scenario: Sensitive or oversized data appears

- **WHEN** reports include prompt text, generated content, complete token
  arrays, KV/native payload contents, credentials, private paths, endpoint
  URLs, real machine labels, raw pointers, or device addresses
- **THEN** the report SHALL be invalid.

### Requirement: Relationship to page and streaming changes

The change SHALL gate streaming KV work on baseline-backed page correctness.

#### Scenario: Baseline-backed page proof passes

- **WHEN** the two-chunk page path exact-matches the deterministic baseline
- **THEN** `pd-kv-page-handoff-spike` MAY be revisited for closure
- **AND** `pd-streaming-kv-handoff` MAY be reassessed.

#### Scenario: Baseline remains unavailable

- **WHEN** no reliable deterministic baseline can be produced
- **THEN** `pd-streaming-kv-handoff` SHALL remain paused
- **AND** the proof strategy SHALL be redesigned.
