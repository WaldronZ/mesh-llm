# pd-long-context-scaling Specification

## ADDED Requirements

### Requirement: Calibrated KV handoff estimation

PD long-context scaling SHALL use measured or explicitly configured KV
bytes-per-token estimates before increasing the PD admission envelope.

#### Scenario: Measured bytes per token are used

- **GIVEN** previous PD MVP or admission smoke reports contain actual KV payload bytes and prompt token counts
- **WHEN** the long-context scaling policy is built
- **THEN** the policy SHALL use a conservative measured bytes-per-token estimate or an explicit operator override
- **AND** it SHALL NOT silently rely on the previously low estimate if measured data shows a larger value.

#### Scenario: Calibration source is visible

- **WHEN** bytes-per-token estimation is reported
- **THEN** telemetry or reports SHALL include a bounded calibration source such as `measured`, `configured`, or `conservative_default`
- **AND** the value SHALL NOT include prompt text, token arrays, private paths, machine names, credentials, or KV payload contents.

#### Scenario: Missing calibration fails safe

- **GIVEN** a hard maximum handoff byte budget is configured
- **AND** bytes-per-token cannot be measured, configured, or conservatively defaulted
- **WHEN** PD admission evaluates a request
- **THEN** the request SHALL fail safe before PGX prefill
- **AND** missing estimation SHALL NOT be treated as unlimited capacity.

### Requirement: Staged admission ladder

PD long-context scaling SHALL evaluate token, context, prefill batch, KV byte,
memory, network/SLA, and lifecycle constraints before PGX prefill starts.

#### Scenario: Gate order is deterministic

- **GIVEN** PD serving is explicitly enabled
- **AND** a request is otherwise eligible for the scoped PD lane
- **WHEN** the coordinator has normalized and tokenized the request
- **THEN** it SHALL evaluate the request using a deterministic admission ladder
- **AND** the ladder SHALL include token, context, prefill batch, KV byte, memory, network/SLA, and lifecycle gates.

#### Scenario: Effective prompt limit is bounded by all gates

- **WHEN** the effective prompt limit is computed
- **THEN** it SHALL be bounded by max prompt tokens, context size minus requested generation budget, prefill batch or chunked prefill strategy, handoff byte budget, memory budget, and network/SLA budget
- **AND** raising `ctx_size` alone SHALL NOT make a request eligible for the PD path.

#### Scenario: Over-limit request remains pre-content

- **GIVEN** any admission ladder gate rejects a request
- **WHEN** the response is returned
- **THEN** the request SHALL fallback or reject before assistant content is visible
- **AND** no PGX prefill work SHALL start for that request.

### Requirement: Phase A 4k and 8k scaling plan

PD long-context scaling SHALL define a bounded Phase A plan for 4k and 8k
validation before attempting larger contexts.

#### Scenario: 4k validation is planned or implemented

- **WHEN** Phase A is evaluated
- **THEN** the change SHALL define a 4k near-threshold validation plan or implementation
- **AND** it SHALL record calibrated KV bytes, export time, isolated network transfer time, import time, TTFT, decode tokens/sec, and PGX process survival.

#### Scenario: 8k validation is planned or implemented

- **WHEN** Phase A is evaluated
- **THEN** the change SHALL define an 8k near-threshold validation plan or implementation
- **AND** the plan SHALL identify whether the current prefill strategy can safely admit 8k or whether chunked prefill is required first.

#### Scenario: Over-threshold guard remains active

- **WHEN** Phase A smoke runs an over-threshold request
- **THEN** the request SHALL reject or fallback before PGX prefill
- **AND** the report SHALL record that PGX prefill did not receive the request or that the PGX prefill process remained alive.

### Requirement: 32k chunked prefill prerequisites

PD long-context scaling SHALL treat 32k support as dependent on chunked prefill
or an equivalent safe prefill strategy.

#### Scenario: Chunked prefill prerequisites are documented

- **WHEN** the change describes 32k support
- **THEN** it SHALL document prerequisites for session identity, position continuity, token range accounting, per-chunk ACK/error handling, cancellation, cleanup, and final export after the last prefill chunk.

#### Scenario: 32k is not treated as config-only

- **GIVEN** chunked prefill has not been implemented or validated for the PD path
- **WHEN** a 32k request is considered
- **THEN** the request SHALL NOT become eligible only because `ctx_size` or `max_prompt_tokens` was raised.

#### Scenario: Chunked KV handoff remains separate

- **WHEN** 32k chunked prefill prerequisites are documented
- **THEN** the change SHALL NOT require streaming/chunked KV handoff
- **AND** streaming/chunked KV handoff SHALL remain a separate future change.

### Requirement: 256k remains a feasibility target

PD long-context scaling SHALL NOT promise direct 256k support over the current
one-shot native KV handoff path.

#### Scenario: 256k is classified as future feasibility

- **GIVEN** the model metadata advertises a 256k context length
- **WHEN** the long-context scaling roadmap is documented
- **THEN** 256k SHALL be described as future feasibility rather than an implementation commitment
- **AND** the roadmap SHALL state that runtime, memory, network, and handoff strategy must be proven before 256k can be admitted.

#### Scenario: No-Go conditions are explicit

- **WHEN** long-context scaling is reviewed
- **THEN** the change SHALL list No-Go conditions including excessive one-shot KV payload size, unacceptable transfer time, insufficient Mac memory, PGX prefill inability without chunking, and unsafe import/export memory peaks.

### Requirement: Sanitized runbook and reporting

PD long-context scaling SHALL provide sanitized runbook/reporting guidance for
4k/8k validation.

#### Scenario: Required metrics are reported

- **WHEN** a scaling smoke report is produced
- **THEN** it SHALL include prompt token count, estimated KV bytes, actual KV bytes when available, export latency, isolated network transfer latency, import latency, TTFT, decode tokens/sec, admission result, and admission reason.

#### Scenario: Sensitive data is excluded

- **WHEN** scaling telemetry or reports are produced
- **THEN** they SHALL NOT include prompt text, complete token arrays, generated content, KV payload contents, credentials, private paths, or private machine labels.

#### Scenario: Foreground smoke requires explicit authorization

- **GIVEN** a smoke plan requires Mac/PGX foreground processes
- **WHEN** validation is requested
- **THEN** those processes SHALL only be started after separate explicit authorization
- **AND** the runbook SHALL include cleanup and port-release checks.

### Requirement: Existing behavior is preserved

PD long-context scaling SHALL preserve the existing admission guard and
default-off scoped PD behavior.

#### Scenario: Existing admission guard remains active

- **WHEN** long-context scaling is applied
- **THEN** the existing over-threshold admission guard SHALL remain active
- **AND** over-limit prompts SHALL NOT be silently sent into the PD path.

#### Scenario: Normal and Skippy split paths are unaffected

- **GIVEN** PD serving is disabled or a request uses the existing Skippy split path
- **WHEN** local regression tests run
- **THEN** normal path behavior and Skippy split behavior SHALL remain unaffected by long-context scaling changes.
