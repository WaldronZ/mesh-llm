# pd-kv-page-memory-type-support Specification

## Purpose
TBD - created by archiving change pd-kv-page-memory-type-support. Update Purpose after archive.
## Requirements
### Requirement: Memory type support scope

The change SHALL address runtime/native KV page memory type diagnostics and
support, and SHALL NOT implement the streaming KV handoff pipeline.

#### Scenario: Streaming pipeline work is proposed

- **WHEN** work attempts to overlap prefill, page export, network transfer,
  and import as a streaming pipeline
- **THEN** that work SHALL be out of scope for this change
- **AND** it SHALL remain blocked until the two-chunk page handoff proof
  succeeds.

#### Scenario: 4k or 8k validation is requested

- **WHEN** 4k or 8k prompt validation is requested before the small two-chunk
  page proof passes
- **THEN** that validation SHALL be out of scope for this change.

### Requirement: Sanitized KV memory type diagnostics

The runtime SHALL expose bounded, sanitized memory type diagnostics at KV page
export and import boundaries.

#### Scenario: Export boundary is reached

- **WHEN** `export_kv_page` inspects a KV page or range
- **THEN** the runtime SHALL identify a bounded memory object label such as
  `llama_kv_cache`, `llama_kv_cache_iswa`, `llama_memory_hybrid`,
  `llama_memory_hybrid_iswa`, or `unknown`
- **AND** it SHALL record the affected layer range and token range.

#### Scenario: Import boundary is reached

- **WHEN** `import_kv_page` receives a page manifest and payload
- **THEN** the runtime SHALL identify whether the target memory type is
  supported
- **AND** it SHALL fail closed before import when the target memory type is
  unsupported or unknown.

#### Scenario: Diagnostics include unsafe data

- **WHEN** memory diagnostics or reports include raw pointers, device
  addresses, endpoint URLs, private paths, credentials, prompt text, generated
  content, complete token arrays, or KV/native payload contents
- **THEN** the diagnostics SHALL be considered invalid.

### Requirement: Native support audit

The change SHALL audit native and Rust KV page support before selecting an
implementation strategy.

#### Scenario: Support matrix is produced

- **WHEN** the audit completes
- **THEN** it SHALL list supported and unsupported memory types for
  `stage_export_kv_page`
- **AND** it SHALL list supported and unsupported memory types for
  `stage_import_kv_page`
- **AND** it SHALL identify whether CPU-only, CUDA device, Metal device,
  unified, host-mapped, split, or mixed memory is supported.

#### Scenario: Unsupported memory type is encountered

- **WHEN** a page export or import sees an unsupported memory type
- **THEN** the runtime SHALL return an explicit sanitized error reason
- **AND** the harness SHALL NOT continue as if page handoff succeeded.

### Requirement: Memory support strategy

The implementation SHALL define a correctness-first strategy for supported and
unsupported KV memory types.

#### Scenario: Direct device page copy is supported

- **WHEN** the native backend can safely copy the requested page/range from
  device memory
- **THEN** it MAY produce a page payload directly
- **AND** it SHALL still bind byte count and checksum into the page manifest.

#### Scenario: CPU staging is selected

- **WHEN** direct device copy is unsafe or unavailable but a staged copy is
  possible
- **THEN** the runtime MAY copy page data into a CPU staging buffer
- **AND** the manifest SHALL report that staging was used using a sanitized
  label
- **AND** the page payload SHALL still be validated by byte count and checksum.

#### Scenario: Per-layer copy is required

- **WHEN** a page spans layers with different memory placement
- **THEN** the runtime MAY export the page using per-layer copies
- **AND** it SHALL preserve deterministic layer and K/V ordering in the
  payload.

#### Scenario: ISWA memory is used

- **WHEN** the runtime memory object is ISWA attention KV
- **THEN** export SHALL represent the page as explicit `base` and `swa`
  segments
- **AND** import SHALL route each segment to the matching ISWA sub-cache
- **AND** the regular non-ISWA page descriptor path SHALL remain unchanged.

#### Scenario: Forced supported memory type is used for spike-only validation

- **WHEN** the spike forces CPU-only or another supported KV memory type to
  isolate import correctness
- **THEN** the report SHALL label the result as constrained
- **AND** it SHALL NOT claim PGX CUDA page handoff pass unless the PGX CUDA
  memory path is actually used.

#### Scenario: Full-state fallback is used

- **WHEN** page export remains impossible and the system falls back to
  full-state framing
- **THEN** the result SHALL NOT count as a page-handoff pass.

### Requirement: Fail-closed page memory behavior

The runtime and harness SHALL fail closed rather than producing ambiguous page
state.

#### Scenario: Unknown memory type

- **WHEN** export or import cannot classify the KV memory type
- **THEN** the operation SHALL fail closed
- **AND** the report SHALL include a bounded reason label.

#### Scenario: Partial export

- **WHEN** only part of the requested page/range can be copied
- **THEN** export SHALL fail closed
- **AND** it SHALL NOT emit a page manifest that appears complete.

#### Scenario: Byte count or checksum mismatch

- **WHEN** the copied page bytes do not match the manifest byte count or
  checksum
- **THEN** import SHALL fail closed before decode.

#### Scenario: ISWA segment is missing or duplicated

- **WHEN** a page proof for ISWA memory omits a required `base` or `swa`
  segment, duplicates a segment, or changes the segment kind
- **THEN** validation SHALL fail closed
- **AND** the result SHALL NOT be accepted as page handoff proof.

### Requirement: Two-chunk proof gates streaming KV

The change SHALL require the small two-chunk page proof to pass before
`pd-streaming-kv-handoff` resumes.

#### Scenario: Two-chunk proof passes

- **GIVEN** the source pre-fills two chunks
- **WHEN** source exports page/range `0` and page/range `1`
- **AND** Mac imports both pages in token-position order
- **THEN** final decode start position SHALL equal the imported token count
- **AND** page-path decode SHALL match the one-shot full-state baseline under
  deterministic settings, or remain non-pass with bounded divergence evidence.

#### Scenario: Page records are not produced

- **WHEN** source cannot produce page records because memory type support is
  missing
- **THEN** the result SHALL remain `inconclusive` or `fail`
- **AND** streaming KV handoff SHALL NOT proceed to implementation.

#### Scenario: Full-state path is used as substitute

- **WHEN** the system uses full-state export/import instead of page export and
  import
- **THEN** the result SHALL NOT be accepted as proof for this change.

### Requirement: Scope boundaries

The change SHALL remain focused on memory type support for KV page APIs.

#### Scenario: Out-of-scope work is proposed

- **WHEN** work attempts to add overlap pipelining, 4k/8k validation,
  32k/128k/256k support, KV compression, multi-worker placement, scheduler
  behavior, production concurrency, default-on PD serving, or Chat UI changes
- **THEN** that work SHALL be out of scope for this change.
