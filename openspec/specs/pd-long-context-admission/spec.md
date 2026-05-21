# pd-long-context-admission Specification

## Purpose
TBD - created by archiving change pd-long-context-admission. Update Purpose after archive.
## Requirements
### Requirement: PD long-context admission gate

PD serving SHALL evaluate long-context admission before starting PGX prefill
work.

#### Scenario: Request is evaluated before PGX prefill

- **GIVEN** PD serving is explicitly enabled
- **AND** a request is otherwise eligible for the scoped PD MVP path
- **WHEN** the coordinator has normalized and tokenized the request
- **THEN** the coordinator SHALL evaluate the request against the long-context admission policy
- **AND** it SHALL complete admission before sending prefill work to the PGX worker.

#### Scenario: Over-limit request is not sent to PGX prefill

- **GIVEN** a request exceeds any configured prompt, context, prefill batch, or handoff byte limit
- **WHEN** the request reaches the admission gate
- **THEN** the coordinator SHALL NOT send that request to PGX prefill
- **AND** the request SHALL use pre-content fallback or documented pre-content rejection.

#### Scenario: Admission preserves default-off behavior

- **GIVEN** PD serving is not explicitly enabled
- **WHEN** a client calls `/v1/chat/completions`
- **THEN** the request SHALL use the existing normal route
- **AND** long-context admission SHALL NOT alter normal-route behavior.

### Requirement: Token counting and capacity estimation

PD admission SHALL use coordinator-owned token count and bounded capacity
estimates to decide whether a request may enter the PD lane.

#### Scenario: Prompt token count is computed

- **GIVEN** PD serving is enabled and the request is otherwise eligible
- **WHEN** the coordinator tokenizes the request
- **THEN** the coordinator SHALL compute `pd.prompt_token_count`
- **AND** it SHALL NOT rely on PGX-side prompt text tokenization as the authority for admission.

#### Scenario: Prompt token count is below threshold

- **GIVEN** `pd.prompt_token_count` is below `pd.max_prompt_tokens`
- **AND** it is below or equal to `pd.max_prefill_batch`
- **AND** the request fits `pd.max_ctx_size`
- **AND** estimated KV handoff bytes are below or equal to the configured byte budget
- **WHEN** admission runs
- **THEN** the request SHALL be admitted to the scoped PD path if other MVP eligibility checks pass.

#### Scenario: Prompt token count is exactly at threshold

- **GIVEN** `pd.prompt_token_count` is exactly equal to `pd.max_prompt_tokens`
- **AND** it is below or equal to `pd.max_prefill_batch`
- **AND** the request fits `pd.max_ctx_size`
- **AND** estimated KV handoff bytes are below or equal to the configured byte budget
- **WHEN** admission runs
- **THEN** the request SHALL be admitted to the scoped PD path if other MVP eligibility checks pass.

#### Scenario: Prompt exceeds prefill batch

- **GIVEN** `pd.prompt_token_count` is greater than `pd.max_prefill_batch`
- **AND** chunked prefill is not implemented
- **WHEN** admission runs
- **THEN** the request SHALL NOT be admitted to PGX prefill
- **AND** `pd.admission.reason` SHALL identify the bounded prefill batch limit.

#### Scenario: Estimated KV handoff bytes exceed limit

- **GIVEN** `pd.estimated_kv_bytes` is greater than the configured maximum handoff byte limit
- **WHEN** admission runs
- **THEN** the request SHALL NOT be admitted to PGX prefill
- **AND** `pd.admission.reason` SHALL identify the handoff byte limit.

### Requirement: Admission configuration fails safe

PD admission SHALL fail safe when required long-context admission policy data is
missing or unknown.

#### Scenario: Required admission config is missing

- **GIVEN** PD serving is explicitly enabled
- **AND** required admission policy data such as max prompt tokens, max prefill batch, max context size, max handoff bytes, or an equivalent safe policy is missing
- **WHEN** the serving process starts or evaluates request eligibility
- **THEN** PD serving SHALL fail startup, mark PD unavailable, or use a documented conservative default
- **AND** it SHALL NOT treat missing limits as unlimited.

#### Scenario: Runtime batch limit is known

- **GIVEN** the PGX prefill runtime exposes or is configured with a bounded prefill batch limit
- **WHEN** admission policy is built
- **THEN** `pd.max_prefill_batch` SHALL be derived from that limit or an operator-configured lower limit
- **AND** the effective limit SHALL be visible through sanitized status or telemetry.

#### Scenario: Runtime batch limit is unknown

- **GIVEN** the PGX prefill batch limit is unknown
- **WHEN** PD serving evaluates a long prompt
- **THEN** the coordinator SHALL fail safe by marking PD unavailable, using a documented conservative default, or rejecting/falling back before PGX prefill
- **AND** it SHALL NOT send a long prompt into PD under an unbounded assumption.

### Requirement: Pre-content fallback or rejection for over-limit prompts

Requests rejected by long-context admission SHALL exit the PD path before any
assistant content is visible to the client.

#### Scenario: Over-limit request falls back

- **GIVEN** an over-limit request reaches long-context admission
- **AND** policy selects normal-route fallback
- **WHEN** admission rejects the PD path
- **THEN** the request SHALL use the existing normal route
- **AND** fallback SHALL occur before any assistant content delta reaches the client.

#### Scenario: Over-limit request is rejected

- **GIVEN** an over-limit request reaches long-context admission
- **AND** policy selects pre-content rejection
- **WHEN** admission rejects the PD path
- **THEN** the client SHALL receive a documented pre-content error response
- **AND** no PGX prefill work SHALL be started for that request.

#### Scenario: Mixed-path output is prevented

- **GIVEN** long-context admission rejects a request before PGX prefill
- **WHEN** fallback or rejection is returned
- **THEN** the response SHALL NOT contain PD-generated assistant content
- **AND** it SHALL NOT contain mixed PD and normal-route output.

### Requirement: Admission telemetry and status are sanitized

PD admission SHALL emit bounded diagnostic fields without exposing sensitive
request, model path, machine, credential, token, or KV data.

#### Scenario: Admission fields are emitted

- **WHEN** long-context admission evaluates a request
- **THEN** telemetry or status SHALL include `pd.admission.result`
- **AND** it SHALL include `pd.admission.reason`
- **AND** it SHALL include `pd.prompt_token_count`
- **AND** it SHALL include `pd.estimated_kv_bytes`
- **AND** it SHALL include `pd.max_prompt_tokens`
- **AND** it SHALL include `pd.max_prefill_batch`.

#### Scenario: Admission result is bounded

- **WHEN** `pd.admission.result` is emitted
- **THEN** it SHALL use a bounded result value such as `admitted`, `fallback`, `rejected`, or `pd_unavailable`
- **AND** it SHALL NOT include free-form prompt text or private environment details.

#### Scenario: Admission reason is bounded

- **WHEN** `pd.admission.reason` is emitted
- **THEN** it SHALL use a bounded reason value such as `within_limits`, `prompt_tokens_exceeded`, `prefill_batch_exceeded`, `ctx_size_exceeded`, `estimated_handoff_bytes_exceeded`, or `admission_config_missing`
- **AND** it SHALL NOT include raw prompt text, complete token arrays, KV payload contents, credentials, private paths, or private machine details.

### Requirement: Long-context admission regression coverage

The change SHALL include local regression coverage and an operator smoke plan
for long-context admission behavior.

#### Scenario: Below-threshold request is tested

- **WHEN** local tests run with a prompt below the configured admission thresholds
- **THEN** the request SHALL be admitted to the PD path if all other MVP eligibility checks pass.

#### Scenario: Exactly-at-threshold request is tested

- **WHEN** local tests run with a prompt exactly at the configured threshold
- **THEN** the request SHALL be admitted to the PD path if all other MVP eligibility checks pass.

#### Scenario: Above-threshold request is tested

- **WHEN** local tests run with a prompt above the configured threshold
- **THEN** the request SHALL fallback or reject before PGX prefill
- **AND** the test SHALL assert that PGX prefill is not started for that request.

#### Scenario: Missing config is tested

- **WHEN** local tests run with missing admission policy data
- **THEN** PD admission SHALL fail closed or use the documented conservative default
- **AND** the test SHALL assert that missing limits are not treated as unlimited.

#### Scenario: Normal path and Skippy split are unaffected

- **WHEN** regression tests run with PD disabled or non-PD Skippy split behavior
- **THEN** normal routing SHALL remain unchanged
- **AND** existing Skippy split serving behavior SHALL remain unaffected.

#### Scenario: Foreground smoke proves PGX does not crash

- **GIVEN** foreground Mac/PGX validation is explicitly authorized
- **WHEN** the operator runs a near-threshold prompt and an over-threshold prompt
- **THEN** the near-threshold prompt SHALL behave according to admission policy
- **AND** the over-threshold prompt SHALL fallback or reject before PGX prefill
- **AND** the PGX prefill process SHALL remain alive after the over-threshold request.
