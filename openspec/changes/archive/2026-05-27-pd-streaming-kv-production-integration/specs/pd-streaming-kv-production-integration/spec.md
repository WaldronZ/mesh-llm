# pd-streaming-kv-production-integration Specification

## ADDED Requirements

### Requirement: Default-off production streaming KV mode

The PD serving path SHALL only use production streaming KV handoff when it is
explicitly enabled.

#### Scenario: Existing PD MVP behavior remains unchanged by default

- **GIVEN** `--pd-serving-mvp` is enabled
- **AND** production streaming KV handoff is not explicitly enabled
- **WHEN** the router admits a PD request
- **THEN** the router SHALL use the existing PD MVP behavior
- **AND** it SHALL NOT silently replace final full-state handoff with
  `pd-kv-stream/1`.

#### Scenario: Streaming KV requires explicit PD serving enablement

- **GIVEN** production streaming KV handoff is enabled
- **AND** PD serving is not enabled
- **WHEN** the server starts or validates config
- **THEN** config validation SHALL fail with a bounded error.

#### Scenario: Invalid streaming combinations are rejected

- **GIVEN** production streaming KV handoff is enabled
- **WHEN** required peer capabilities, chunking/admission settings, or capacity
  limits are missing or incompatible
- **THEN** the server SHALL reject the configuration or request before entering
  the streaming path.

### Requirement: Production split-channel streaming lifecycle

The production PD serving path SHALL support a request-scoped split control and
page stream lifecycle for `pd-kv-stream/1`.

#### Scenario: Router request uses per-chunk KV export and import

- **GIVEN** production streaming KV handoff is enabled
- **AND** a request is admitted to the PD path
- **WHEN** the coordinator splits the prompt into prefill chunks
- **THEN** PGX SHALL prefill each chunk
- **AND** PGX SHALL export KV page segments for each chunk
- **AND** Mac SHALL import KV page segments for each chunk
- **AND** decode SHALL wait for the final contiguous gate.

#### Scenario: Split channels are used for lifecycle and page payloads

- **GIVEN** a production streaming KV request is active
- **WHEN** the source and coordinator exchange handoff data
- **THEN** lifecycle events SHALL use a control channel
- **AND** page frame headers and raw payload bytes SHALL use a page stream
  channel or an equivalently separated transport that prevents page payloads
  from obscuring lifecycle state.

#### Scenario: Final decode starts only after bootstrap

- **GIVEN** all expected chunks have been imported
- **WHEN** the final contiguous gate passes
- **THEN** Mac SHALL run the configured decode bootstrap
- **AND** the initial production strategy SHALL be `trim-replay-last-token`
- **AND** Mac SHALL verify `logits_ready=true`
- **AND** Mac SHALL verify final `decode_start_position` equals the imported
  prompt token count before decode begins.

### Requirement: Production streaming manifest and provenance

Production streaming KV handoff SHALL bind every page segment to request,
identity, layout, and integrity metadata.

#### Scenario: Page manifest records identity and integrity

- **WHEN** PGX exports a KV page segment
- **THEN** its manifest SHALL include protocol version, stream/request id,
  chunk index, total expected chunks, total expected prompt tokens, token
  start/end positions, cache kind, segment kind, layer range, dtype/layout,
  payload byte count, checksum algorithm, checksum, artifact identity,
  tokenizer metadata hash, and chat template hash.

#### Scenario: ISWA segments are explicit

- **GIVEN** the runtime uses Gemma4 ISWA KV cache
- **WHEN** page segments are exported
- **THEN** manifests SHALL distinguish `base` and `swa` segment kinds
- **AND** Mac SHALL validate and import the expected segment kinds before the
  chunk can be considered complete.

#### Scenario: Full-state handoff cannot satisfy streaming proof

- **WHEN** a production streaming KV request is validated
- **THEN** final full-state handoff frames SHALL NOT be accepted as proof that
  per-chunk streaming KV handoff occurred.

### Requirement: Production failure semantics

Production streaming KV handoff SHALL fail closed when state is ambiguous,
corrupt, incomplete, or unsafe to decode.

#### Scenario: Manifest mismatch fails before decode

- **GIVEN** a page frame declares identity, layout, checksum, payload length,
  cache kind, segment kind, or token range metadata
- **WHEN** received data or local expectations do not match that metadata
- **THEN** the request SHALL fail closed
- **AND** Mac SHALL NOT decode from that state.

#### Scenario: Ordering mismatch fails before decode

- **GIVEN** the coordinator expects chunks to be contiguous
- **WHEN** a chunk is missing, duplicated, out of order under the configured
  policy, or has a position gap or overlap
- **THEN** the request SHALL fail closed before decode.

#### Scenario: Import or bootstrap failure fails closed

- **WHEN** Mac import, final gate validation, trim, replay, logits readiness,
  or decode start position validation fails
- **THEN** the request SHALL fail closed
- **AND** it SHALL NOT sample from stale or ambiguous logits.

#### Scenario: No transparent fallback after content

- **GIVEN** assistant content has been emitted to the client
- **WHEN** production streaming KV handoff fails
- **THEN** the router SHALL NOT transparently switch to a different serving
  path
- **AND** it SHALL surface a bounded stream error and clean up request state.

#### Scenario: Pre-content fallback is explicit

- **GIVEN** assistant content has not been emitted
- **WHEN** production streaming KV handoff cannot start or fails before decode
- **THEN** the router MAY reject the request or use an explicitly configured
  fallback path
- **AND** that fallback SHALL NOT be reported as a streaming KV pass.

### Requirement: Capacity, cleanup, and cancellation

Production streaming KV handoff SHALL enforce bounded resource usage and clean
up request-scoped state.

#### Scenario: Capacity limits are enforced

- **WHEN** production streaming KV handoff is enabled
- **THEN** the router/coordinator SHALL enforce max frame bytes, max in-flight
  chunks, max in-flight bytes or queued bytes, and timeout limits.

#### Scenario: Source failure cancels importer

- **WHEN** PGX source prefill/export/control fails
- **THEN** Mac importer/page stream work SHALL be cancelled
- **AND** request-scoped resources SHALL be cleaned up.

#### Scenario: Importer failure cancels source

- **WHEN** Mac page validation/import/final-gate work fails
- **THEN** PGX source work SHALL be cancelled where possible
- **AND** request-scoped resources SHALL be cleaned up.

### Requirement: Production lifecycle diagnostics

Production streaming KV handoff SHALL emit sanitized, bounded lifecycle
diagnostics sufficient to prove the configured production correctness path and
diagnose bounded failures.

#### Scenario: Streaming diagnostics prove the production lifecycle

- **WHEN** a production streaming KV request completes or fails
- **THEN** diagnostics SHALL include protocol version, streaming enabled
  status, chunk count, bounded token ranges, page byte counts, cache and
  segment kinds, checksum and identity validation booleans, final gate result,
  bootstrap logits-ready result, final decode start position, cleanup events,
  and bounded result/failure labels.

#### Scenario: Performance readiness is not claimed without timing telemetry

- **WHEN** production diagnostics do not include aligned per-phase timing,
  overlap, TTFT, control-lag, writer-wait, backpressure, and queue-depth
  metrics
- **THEN** reports SHALL mark those metrics as deferred or not recorded
- **AND** they SHALL NOT claim production performance readiness.

#### Scenario: Sensitive data is excluded

- **WHEN** production streaming KV telemetry, logs, or reports are emitted
- **THEN** they SHALL NOT contain prompt text, generated content, complete
  token arrays, KV/native payload contents, credentials, private paths,
  endpoint URLs, or real machine labels.

### Requirement: Regression safety

Production streaming KV integration SHALL preserve existing serving paths.

#### Scenario: Non-PD serving path is unaffected

- **GIVEN** PD serving is disabled
- **WHEN** a normal OpenAI-compatible request is handled
- **THEN** it SHALL use the existing non-PD serving path.

#### Scenario: Full-state PD path is unaffected without streaming flag

- **GIVEN** `--pd-serving-mvp` is enabled
- **AND** production streaming KV handoff is disabled
- **WHEN** a request enters the PD path
- **THEN** it SHALL use the existing full-state PD handoff behavior.

### Requirement: Foreground validation boundary

This change SHALL validate production integration with short and 4k serving
smokes before claiming production streaming KV serving pass.

#### Scenario: Short serving smoke precedes 4k

- **WHEN** production streaming KV integration is implemented
- **THEN** a short `/v1/chat/completions` foreground smoke SHOULD run before a
  4k foreground smoke.

#### Scenario: 4k serving smoke is the closure target

- **WHEN** the change is validated for closure
- **THEN** the required foreground scope SHALL be a 4k production serving smoke
  with `pd-kv-stream/1`, final gate pass, bootstrap pass, correct
  `decode_start_position`, SSE completion, no full-state pass, and no fallback.

#### Scenario: Larger and unrelated work remains out of scope

- **WHEN** work attempts to require 8k, 32k/128k/256k validation, KV
  compression, low-precision KV, multi-worker placement, scheduler behavior,
  production concurrency, public mesh PD serving, default-on PD serving, or UI
  changes
- **THEN** that work SHALL be out of scope for this change.
