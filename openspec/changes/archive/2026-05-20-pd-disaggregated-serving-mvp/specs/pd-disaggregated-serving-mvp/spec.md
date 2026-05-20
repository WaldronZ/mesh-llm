# pd-disaggregated-serving-mvp Specification

## ADDED Requirements

### Requirement: Scoped MVP boundaries

The MVP SHALL implement a bounded Prefill/Decode serving lane based on the
validated PGX prefill/export -> Mac import/decode path and SHALL NOT define
production-scale PD serving.

#### Scenario: MVP topology is configured

- **GIVEN** PD serving is explicitly enabled
- **AND** one Mac coordinator/router/decode worker is configured
- **AND** one PGX prefill/export worker is configured
- **WHEN** an eligible request enters the MVP path
- **THEN** the request SHALL use that configured Mac/PGX worker pair
- **AND** it SHALL NOT require automatic placement.

#### Scenario: Excluded scale features are requested

- **WHEN** a proposal, implementation, or task attempts to include multiple decode workers, automatic PGX placement, production multi-request concurrency, public mesh cross-owner PD, KV compression, incremental transfer, low precision KV compatibility, automatic model selection, or a production-grade scheduler
- **THEN** that behavior SHALL be out of scope for this MVP
- **AND** it SHALL be deferred to a later OpenSpec change.

#### Scenario: Prior validation evidence exists

- **GIVEN** `pd-kv-handoff-spike` records native handoff feasibility as passing
- **AND** `pd-router-validation` records a real OpenAI-compatible router validation path
- **AND** `pd-router-validation-followup` records isolated network timing and post-content-token failure semantics as passing
- **WHEN** this MVP is planned
- **THEN** it SHALL reuse those results as carry-forward evidence
- **AND** it SHALL focus on hardening scoped serving behavior rather than re-proving native handoff.

### Requirement: Default-off activation and explicit configuration

The MVP SHALL be disabled by default and SHALL only run when explicitly
configured by the operator.

#### Scenario: PD is not enabled

- **GIVEN** the operator has not explicitly enabled PD serving
- **WHEN** a client calls `/v1/chat/completions`
- **THEN** the request SHALL use the existing normal route
- **AND** existing Skippy split serving behavior SHALL remain available.

#### Scenario: Required PD config is missing

- **GIVEN** PD serving is enabled
- **BUT** required worker, model identity, tokenizer identity, chat template identity, KV ABI/layout, or timeout config is missing
- **WHEN** the serving process starts or evaluates request eligibility
- **THEN** PD serving SHALL fail startup or mark PD unavailable before serving that request
- **AND** it SHALL NOT silently run a partially configured PD path.

#### Scenario: Request is ineligible for PD

- **GIVEN** PD serving is enabled
- **BUT** the request model or runtime capability does not match the configured MVP policy
- **WHEN** the request is routed
- **THEN** it SHALL use the existing normal route or return a documented pre-content error according to policy
- **AND** it SHALL NOT attempt best-effort PD.

### Requirement: OpenAI-compatible request lifecycle

The MVP SHALL preserve the external OpenAI-compatible
`/v1/chat/completions` surface while routing eligible requests through the
scoped internal PD lifecycle.

#### Scenario: Positive PD request succeeds

- **GIVEN** PD serving is enabled and eligible
- **AND** the configured PGX prefill worker and Mac decode worker are healthy
- **WHEN** a client calls `/v1/chat/completions`
- **THEN** the coordinator SHALL normalize the request and perform tokenization
- **AND** the PGX prefill worker SHALL receive token IDs plus metadata
- **AND** the PGX prefill worker SHALL export native KV/decode state
- **AND** the Mac decode worker SHALL validate and import the handoff
- **AND** the client SHALL receive an OpenAI-compatible response.

#### Scenario: Concurrency is bounded

- **GIVEN** the MVP in-flight limit is reached
- **WHEN** another eligible request arrives
- **THEN** the coordinator SHALL route it to the normal path or reject it according to configured MVP policy
- **AND** it SHALL record a sanitized busy/fallback reason.

#### Scenario: Request is cancelled

- **WHEN** the client cancels an in-flight PD request
- **THEN** the coordinator SHALL stop prefill/handoff/decode work where possible
- **AND** temporary request state SHALL be cleaned up
- **AND** sensitive request data SHALL NOT be written to logs or reports.

### Requirement: Manifest validation and fail-closed import

The MVP SHALL validate the handoff manifest before Mac decode continuation and
SHALL fail closed on identity, layout, position, byte count, or checksum
mismatch.

#### Scenario: Manifest validates

- **GIVEN** PGX exports native KV/decode state
- **WHEN** Mac receives the manifest and payload
- **THEN** Mac SHALL validate schema version, model artifact identity, tokenizer metadata hash, chat template hash, context/position metadata, runtime ABI, KV dtype/codec/layout, byte count, and payload checksum
- **AND** Mac SHALL import the payload only after validation passes.

#### Scenario: Manifest validation fails

- **GIVEN** any required manifest field, identity, position, byte count, or checksum does not match
- **WHEN** Mac evaluates the handoff
- **THEN** Mac SHALL reject the handoff
- **AND** it SHALL NOT continue decode from that payload
- **AND** the coordinator SHALL record a sanitized fail-closed reason.

#### Scenario: Partial handoff is received

- **WHEN** the handoff payload is truncated, corrupt, or interrupted
- **THEN** the decode worker SHALL NOT import partial state
- **AND** the request SHALL follow pre-content fallback or documented error semantics.

### Requirement: Fallback and post-content failure semantics

The MVP SHALL permit normal-route fallback before client-visible assistant
content and SHALL block transparent fallback after client-visible assistant
content.

#### Scenario: Failure occurs before assistant content

- **GIVEN** no assistant content delta has reached the client
- **WHEN** PD fails due to worker unavailability, prefill failure, export failure, transfer failure, import failure, or decode failure before content
- **THEN** the coordinator MAY fall back to the existing normal path
- **AND** it SHALL record a sanitized fallback reason.

#### Scenario: Failure occurs after assistant content

- **GIVEN** at least one assistant content delta has reached the client
- **WHEN** PD fails
- **THEN** the coordinator SHALL NOT transparently fall back to the normal path
- **AND** the client SHALL receive explicit SSE error behavior or documented partial termination
- **AND** the stream SHALL NOT contain mixed-path output.

#### Scenario: Mixed-path output is prevented

- **WHEN** post-content failure handling runs
- **THEN** the response SHALL NOT contain duplicate content
- **AND** it SHALL NOT contain reordered content
- **AND** it SHALL NOT combine PD output with fallback decode output.

### Requirement: Telemetry, status, and report privacy

The MVP SHALL record sanitized timing/status/report data needed to evaluate the
PD path without exposing sensitive request or environment content.

#### Scenario: Positive request metrics are emitted

- **WHEN** a PD request completes successfully
- **THEN** telemetry/reporting SHALL include KV payload bytes, export timing, network read/write timing, isolated transfer timing, import timing, router overhead, TTFT, and decode tokens/sec
- **AND** isolated transfer timing SHALL exclude PGX export and Mac import work.

#### Scenario: Fallback or failure metrics are emitted

- **WHEN** a PD request falls back or fails
- **THEN** telemetry/reporting SHALL include sanitized result, fallback reason, or failure phase
- **AND** it SHALL avoid prompt text, full token arrays, KV payload contents, credentials, private paths, and private machine details.

#### Scenario: Status is exposed

- **WHEN** status or diagnostics are requested
- **THEN** status SHALL include additive PD availability, role, state, last result, and recent timing summary fields
- **AND** existing status clients SHALL remain compatible.

### Requirement: Normal path and Skippy split path compatibility

The MVP SHALL not break existing normal routing or existing Skippy split
serving.

#### Scenario: PD is disabled

- **WHEN** PD is disabled
- **THEN** normal OpenAI-compatible routing SHALL behave as it did before the MVP
- **AND** existing Skippy split serving SHALL remain usable.

#### Scenario: PD is enabled but request is not eligible

- **WHEN** a request is outside the configured PD MVP policy
- **THEN** the request SHALL use the existing normal path or documented pre-content error behavior
- **AND** it SHALL NOT affect Skippy split serving state.

#### Scenario: Regression validation runs

- **WHEN** MVP validation is performed
- **THEN** tests or smoke checks SHALL cover PD default-off behavior, normal route behavior, and Skippy split path behavior where shared code was touched.

### Requirement: MVP runbook and foreground validation

The MVP SHALL include a runbook and report template for foreground validation
on the configured Mac/PGX topology.

#### Scenario: Foreground validation is run

- **GIVEN** current binaries and matching model/tokenizer identities are prepared
- **WHEN** the operator runs the MVP validation runbook
- **THEN** the runbook SHALL start PGX prefill/export and Mac decode/router as foreground observable processes
- **AND** it SHALL run positive, manifest mismatch, pre-content fallback, and post-content failure checks
- **AND** it SHALL record isolated timing and sanitized result evidence
- **AND** it SHALL stop validation processes and confirm ports are released.

#### Scenario: MVP report is produced

- **WHEN** foreground validation completes
- **THEN** the report SHALL include `result: pass | fail | inconclusive`
- **AND** it SHALL include a recommendation for the next change
- **AND** it SHALL avoid prompt text, full token arrays, KV payload contents, credentials, private paths, and private machine details.
