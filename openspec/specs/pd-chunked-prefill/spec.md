# pd-chunked-prefill Specification

## Purpose
TBD - created by archiving change pd-chunked-prefill. Update Purpose after archive.
## Requirements
### Requirement: Chunked prefill lifecycle

PD serving SHALL support a bounded chunked prefill lifecycle that lets eligible
4k/8k prompts prefill on PGX without requiring a single prefill batch to contain
the whole prompt.

#### Scenario: Chunked request runs under one logical session

- **GIVEN** PD serving is explicitly enabled
- **AND** the configured PGX worker advertises chunked prefill capability
- **AND** a 4k or 8k request passes admission
- **WHEN** the coordinator dispatches prefill work
- **THEN** it SHALL create one logical chunked prefill session
- **AND** it SHALL send bounded token chunks to PGX in order
- **AND** PGX SHALL advance one persistent runtime state across those chunks
- **AND** PGX SHALL export native KV/decode state only after the final chunk.

#### Scenario: Chunk size respects prefill batch limits

- **GIVEN** the PGX worker has a configured prefill batch envelope
- **WHEN** the coordinator plans chunks
- **THEN** every chunk SHALL be bounded by `n_batch`, `max_prefill_batch`, and
  configured safety margin
- **AND** a prompt SHALL NOT be admitted merely because `max_prompt_tokens` or
  `ctx_size` was raised.

### Requirement: Position continuity

Chunked prefill SHALL preserve monotonic token position and decode start
position across all chunks.

#### Scenario: Chunk ACK advances position

- **GIVEN** a chunked prefill session is active
- **WHEN** PGX acknowledges a chunk
- **THEN** the ACK SHALL identify the consumed token range and resulting
  position
- **AND** the next chunk SHALL start at that acknowledged position.

#### Scenario: Final decode position matches prompt length

- **WHEN** the final chunk is acknowledged
- **THEN** the final `decode_start_position` SHALL equal the total consumed
  prompt token count
- **AND** the final handoff manifest SHALL record the same position.

#### Scenario: Position mismatch fails closed

- **GIVEN** a chunk ACK reports an unexpected range or position
- **WHEN** the coordinator validates the ACK
- **THEN** it SHALL fail the PD path before final export
- **AND** it SHALL clean up the chunked session
- **AND** it SHALL NOT continue decode from ambiguous state.

### Requirement: Chunk ACK, error, timeout, and cancel semantics

Chunked prefill SHALL define terminal behavior for each chunk and SHALL avoid
unsafe retry when state advancement is ambiguous.

#### Scenario: Chunk succeeds

- **WHEN** PGX consumes a chunk successfully
- **THEN** it SHALL return an ACK with a bounded success status and consumed
  range metadata.

#### Scenario: Chunk fails after possible state advancement

- **GIVEN** a chunk may have partially advanced runtime state
- **WHEN** PGX reports an error or the coordinator cannot determine whether the
  chunk advanced state
- **THEN** the coordinator SHALL fail closed and clean up
- **AND** it SHALL NOT retry that chunk unless a later change proves retry is
  idempotent.

#### Scenario: Client cancels during chunked prefill

- **WHEN** the client cancels before assistant content is visible
- **THEN** the coordinator SHALL stop sending chunks, request cleanup where
  possible, release admission capacity, and avoid writing sensitive request
  content to logs or reports.

### Requirement: Manifest provenance for chunked prefill

`pd-handoff/1` manifest validation SHALL include additive chunked prefill
provenance before Mac import/decode.

#### Scenario: Chunked manifest validates

- **GIVEN** PGX completed chunked prefill and exported native state
- **WHEN** Mac validates the manifest
- **THEN** the manifest SHALL include bounded provenance for prefill mode,
  chunk count, token ranges, chunk size policy, total prompt tokens, and final
  decode start position
- **AND** existing model, tokenizer, chat template, dtype/layout/ABI, byte
  count, checksum, and position checks SHALL still pass before import.

#### Scenario: Chunked provenance mismatch fails closed

- **GIVEN** chunked provenance conflicts with coordinator expectations
- **WHEN** Mac or the coordinator validates the handoff
- **THEN** the handoff SHALL be rejected before decode
- **AND** the response SHALL follow existing pre-content fallback/rejection
  semantics.

### Requirement: Chunked admission

PD admission SHALL allow 4k/8k prompts only when chunked prefill capability and
all existing safety gates pass.

#### Scenario: Capability is missing

- **GIVEN** a request exceeds the one-shot prefill envelope
- **AND** the PGX worker does not advertise chunked prefill capability
- **WHEN** admission runs
- **THEN** the request SHALL reject or fallback before PGX prefill
- **AND** current long-context admission safety SHALL remain unchanged.

#### Scenario: Capability is present

- **GIVEN** chunked prefill capability is configured and healthy
- **WHEN** a 4k or 8k prompt is evaluated
- **THEN** admission SHALL evaluate token, context, chunk size, KV bytes,
  memory, network/SLA, lifecycle, and in-flight gates
- **AND** the request SHALL be admitted only if all gates pass.

### Requirement: Chunked telemetry and reporting

Chunked prefill SHALL emit sanitized telemetry sufficient to validate 4k/8k
behavior and diagnose admission or lifecycle failures.

#### Scenario: Successful chunked request emits metrics

- **WHEN** a chunked PD request completes
- **THEN** telemetry or reports SHALL include chunk count, per-chunk token
  counts, per-chunk prefill latency, total prefill latency, final KV payload
  bytes, export latency, network transfer latency, import latency, TTFT, and
  decode tokens/sec.

#### Scenario: Sensitive data is excluded

- **WHEN** chunked telemetry or reports are produced
- **THEN** they SHALL NOT include prompt text, complete token arrays, generated
  content, KV payload contents, credentials, private paths, endpoint URLs, or
  real machine labels.

### Requirement: Existing paths remain compatible

Chunked prefill SHALL not break default-off PD behavior, normal route behavior,
the one-shot PD path, or existing Skippy split serving.

#### Scenario: PD is disabled

- **WHEN** PD serving is not explicitly enabled
- **THEN** chunked prefill SHALL NOT affect request routing or serving behavior.

#### Scenario: One-shot PD request remains eligible

- **GIVEN** a request still fits the existing one-shot PD envelope
- **WHEN** chunked prefill support exists
- **THEN** the implementation MAY use one-shot or chunked prefill according to
  policy
- **AND** external OpenAI-compatible response semantics SHALL remain unchanged.

#### Scenario: Skippy split path is used

- **WHEN** an existing Skippy split serving path is selected
- **THEN** chunked PD prefill changes SHALL NOT alter split serving protocol,
  routing, or response semantics.

### Requirement: 4k and 8k validation boundary

This change SHALL target 4k/8k validation and SHALL NOT claim broader
long-context production readiness.

#### Scenario: Foreground smoke requires authorization

- **GIVEN** 4k/8k smoke requires Mac/PGX foreground processes
- **WHEN** smoke validation is requested
- **THEN** those processes SHALL only start after separate explicit
  authorization
- **AND** the runbook SHALL include startup, stop, cleanup, and port-release
  checks.

#### Scenario: Larger contexts remain out of scope

- **WHEN** 32k, 128k, or 256k support is discussed
- **THEN** this change SHALL describe them as future work
- **AND** it SHALL NOT implement or validate them as production support.

#### Scenario: Explicit non-goals remain excluded

- **WHEN** implementation or review attempts to add streaming/chunked KV
  handoff, KV compression, multi-worker placement, scheduler behavior,
  production concurrency, or default-on PD serving
- **THEN** that work SHALL be out of scope for this change.
