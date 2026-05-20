# pd-disaggregated-serving-hardening Specification

## ADDED Requirements

### Requirement: Hardening-only scope

The change SHALL harden the scoped PD serving MVP and SHALL NOT redefine or
expand the MVP into production-grade PD serving.

#### Scenario: Scoped MVP evidence is carried forward

- **GIVEN** `pd-disaggregated-serving-mvp` records `result: pass`
- **AND** it records real `--pd-serving-mvp` positive serving, manifest mismatch fail-closed behavior, pre-content fallback, post-content failure semantics, isolated timing, sanitized telemetry, and foreground cleanup
- **WHEN** this hardening change is evaluated
- **THEN** those results SHALL be treated as carry-forward evidence
- **AND** this change SHALL NOT re-prove native PGX -> Mac handoff feasibility.

#### Scenario: Excluded production features are proposed

- **WHEN** a proposal, implementation, or task attempts to include multiple decode workers, multiple PGX automatic placement, production multi-request concurrency, an automatic scheduler, KV compression, incremental transfer, public mesh cross-owner PD, or production-grade serving
- **THEN** that behavior SHALL be out of scope
- **AND** it SHALL be deferred to a later OpenSpec change.

#### Scenario: Full MVP foreground positive suite is requested

- **WHEN** validation asks to re-run the full MVP foreground positive prompt suite
- **THEN** it SHALL be unnecessary for this change by default
- **AND** it MAY only be performed as a minimal targeted check when required by a specific hardening item.

### Requirement: Normal path regression coverage

The hardening change SHALL prove that existing normal OpenAI-compatible routing
remains unchanged when PD serving is disabled, incomplete, or ineligible.

#### Scenario: PD is disabled

- **GIVEN** PD serving is not explicitly enabled
- **WHEN** a client calls `/v1/chat/completions`
- **THEN** the request SHALL use the existing normal route
- **AND** PD-specific handoff, import, and test fault behavior SHALL NOT run.

#### Scenario: PD configuration is incomplete

- **GIVEN** PD serving configuration is missing required worker, model identity, tokenizer identity, chat template identity, KV ABI/layout, or timeout fields
- **WHEN** the process starts or evaluates request eligibility
- **THEN** PD SHALL fail startup or mark PD unavailable according to the MVP policy
- **AND** it SHALL NOT silently change normal-route behavior.

#### Scenario: Request is not eligible for PD

- **GIVEN** PD serving is enabled
- **BUT** the request is outside the configured MVP policy
- **WHEN** the request is routed
- **THEN** the request SHALL use the existing normal path or a documented pre-content error according to policy
- **AND** a sanitized reason SHALL be recorded if PD is skipped.

### Requirement: Skippy split serving regression coverage

The hardening change SHALL prove that existing Skippy split serving remains
usable where PD MVP changes touch shared serving or binary transport behavior.

#### Scenario: Existing split serving runs without PD

- **GIVEN** the operator uses existing Skippy split serving configuration
- **AND** PD MVP serving is not explicitly enabled
- **WHEN** split serving starts or handles a request
- **THEN** PD MVP routing and test fault hooks SHALL remain inactive
- **AND** existing split-serving behavior SHALL remain available.

#### Scenario: Shared binary transport is exercised

- **WHEN** regression coverage exercises binary transport paths shared by Skippy split serving and PD handoff
- **THEN** existing split-serving message flow SHALL remain compatible
- **AND** PD-specific metadata SHALL NOT be required by non-PD split-serving messages.

#### Scenario: Split-serving smoke cannot be fully automated

- **WHEN** a split-serving smoke check cannot be automated locally
- **THEN** the runbook or task evidence SHALL document the manual check, required environment, and reason it remains manual.

### Requirement: Lifecycle cleanup across terminal paths

The hardening change SHALL ensure request/session/admission state is released
for all scoped MVP terminal paths.

#### Scenario: PD request succeeds

- **WHEN** a PD MVP request completes successfully
- **THEN** request/session state SHALL be released
- **AND** admission capacity SHALL be restored.

#### Scenario: PD request fails or falls back before content

- **WHEN** a PD MVP request fails before content, fails manifest validation, or falls back before content
- **THEN** prefill/decode session state SHALL be released
- **AND** temporary handoff state SHALL be dropped
- **AND** admission capacity SHALL be restored.

#### Scenario: PD request fails after content

- **GIVEN** at least one assistant content delta has reached the client
- **WHEN** a PD MVP request fails
- **THEN** transparent fallback SHALL remain blocked
- **AND** request/session state SHALL be released after explicit SSE error behavior or documented partial termination.

#### Scenario: PD request times out or is cancelled

- **WHEN** a PD MVP request times out or the client cancels the request
- **THEN** the coordinator SHALL stop or abandon prefill/handoff/decode work where possible
- **AND** temporary request state SHALL be cleaned up
- **AND** admission capacity SHALL be restored.

### Requirement: OpenAI-compatible streaming regression coverage

The hardening change SHALL add regression coverage for the scoped MVP streaming
surface.

#### Scenario: Streaming success

- **WHEN** an eligible PD MVP streaming request succeeds
- **THEN** the client SHALL receive an OpenAI-compatible streaming response
- **AND** the stream SHALL complete with the documented terminal event shape.

#### Scenario: Pre-content fallback streaming

- **GIVEN** no assistant content delta has reached the client
- **WHEN** PD falls back before content
- **THEN** the client SHALL receive the documented normal-route streaming response or documented pre-content error
- **AND** a sanitized fallback reason SHALL be recorded.

#### Scenario: Post-content failure streaming

- **GIVEN** at least one assistant content delta has reached the client
- **WHEN** PD fails
- **THEN** the client SHALL receive explicit SSE error behavior or documented partial termination
- **AND** the stream SHALL NOT include duplicate, reordered, or mixed-path output.

### Requirement: Telemetry privacy and required metric tests

The hardening change SHALL test telemetry/reporting privacy and required metric
presence for scoped MVP outcomes.

#### Scenario: Positive metrics are present

- **WHEN** a PD MVP request completes successfully
- **THEN** telemetry/reporting SHALL include KV payload bytes, export timing, network read/write timing, isolated transfer timing, import timing, router overhead, TTFT, decode tokens/sec, and result.

#### Scenario: Fallback or failure metrics are present

- **WHEN** a PD MVP request falls back, is busy, or fails
- **THEN** telemetry/reporting SHALL include sanitized result, fallback reason, failure phase, or busy/admission reason as applicable.

#### Scenario: Sensitive data is excluded

- **WHEN** telemetry, status, reports, or validation artifacts are produced
- **THEN** they SHALL NOT include prompt text, full token arrays, generated content, KV payload contents, credentials, private paths, or private machine details.

#### Scenario: Isolated transfer timing is labeled

- **WHEN** transfer timing excludes PGX export and Mac import work
- **THEN** telemetry MAY mark `pd.kv_transfer_isolated=true`
- **AND** if timing boundaries are ambiguous or include export/import work, telemetry SHALL NOT mark the transfer as isolated.

### Requirement: Status and capability hardening

The hardening change SHALL expose additive, sanitized status/capability fields
needed to operate the scoped MVP.

#### Scenario: Worker health is reported

- **WHEN** status is requested
- **THEN** status SHALL include sanitized worker health and availability fields for the configured coordinator, prefill worker, and decode worker
- **AND** it SHALL NOT expose private paths, credentials, or raw private machine details.

#### Scenario: Capacity is reported

- **WHEN** status is requested
- **THEN** status SHALL include sanitized capacity and current in-flight state
- **AND** it SHALL make the `inflight_limit=1` MVP behavior visible.

#### Scenario: Compatibility is reported

- **WHEN** status is requested
- **THEN** status SHALL include sanitized compatibility readiness for model artifact identity, tokenizer metadata hash, chat template hash, and KV ABI/layout expectations
- **AND** existing status clients SHALL remain compatible with the additive fields.

### Requirement: Busy and admission behavior

The hardening change SHALL define and validate scoped MVP busy behavior under
`inflight_limit=1`.

#### Scenario: First eligible request is admitted

- **GIVEN** no PD MVP request is in flight
- **WHEN** an eligible request arrives
- **THEN** the coordinator SHALL admit it to the scoped MVP path
- **AND** it SHALL mark the MVP lane as occupied until the request reaches a terminal path.

#### Scenario: Second eligible request arrives while busy

- **GIVEN** one PD MVP request is already in flight
- **WHEN** another eligible request arrives
- **THEN** the coordinator SHALL use the configured busy policy
- **AND** that policy SHALL be normal-route fallback or documented pre-content rejection
- **AND** a sanitized busy/admission reason SHALL be recorded.

#### Scenario: Terminal path releases capacity

- **WHEN** the in-flight request succeeds, fails, falls back, times out, or is cancelled
- **THEN** admission capacity SHALL be restored for a later eligible request.

### Requirement: Docs and runbook polish

The hardening change SHALL update docs or runbooks only where needed to reflect
the hardened scoped MVP behavior.

#### Scenario: Operator-facing behavior changes

- **WHEN** hardening changes operator-facing validation, status, telemetry, or runbook steps
- **THEN** the relevant runbook or report template SHALL be updated
- **AND** private paths, credentials, prompt text, generated content, full token arrays, and KV payload contents SHALL remain excluded.

#### Scenario: Manual checks remain

- **WHEN** a regression check remains manual-only
- **THEN** the docs SHALL describe the minimum command shape, required environment, and pass/fail evidence without including private configuration.
