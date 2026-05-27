# chat-reasoning-output-ux Specification

## Purpose
TBD - created by archiving change chat-reasoning-output-ux. Update Purpose after archive.
## Requirements
### Requirement: Reasoning stream is rendered separately

The Chat UI SHALL preserve reasoning or thinking stream content as a separate
thinking channel when the response stream provides it.

#### Scenario: Reasoning delta creates thinking content

- **GIVEN** a Chat response stream emits `response.reasoning_text.delta`
- **WHEN** the UI consumes the stream
- **THEN** the delta SHALL be rendered in the thinking section
- **AND** it SHALL NOT be merged into the primary final-answer text.

#### Scenario: Output delta remains final answer content

- **GIVEN** a Chat response stream emits `response.output_text.delta`
- **WHEN** the UI consumes the stream
- **THEN** the delta SHALL be rendered as primary assistant answer content.

### Requirement: Think-tag fallback remains supported

The Chat UI SHALL continue to parse explicit `<think>...</think>` text spans as
thinking fallback content for adapters or models that do not emit first-class
reasoning events.

#### Scenario: Explicit think tag is split from answer text

- **GIVEN** assistant text contains a `<think>...</think>` span followed by
  final answer text
- **WHEN** the message is rendered
- **THEN** the think span SHALL render as thinking content
- **AND** the remaining text SHALL render as final answer content.

#### Scenario: Untagged text is not treated as thinking

- **GIVEN** assistant streaming text does not contain an explicit thinking tag
- **WHEN** the message is rendered
- **THEN** the text SHALL remain final answer content
- **AND** the UI SHALL NOT infer thinking from arbitrary untagged text.

#### Scenario: Isolated closing think tag does not create thinking

- **GIVEN** assistant text contains an isolated `</think>` marker without a
  preceding explicit `<think>` marker
- **WHEN** the message is rendered
- **THEN** the text before the closing marker SHALL remain final answer content
- **AND** the UI SHALL NOT retroactively classify it as thinking.

### Requirement: Completed thinking is inspectable but not primary

The Chat UI SHALL show thinking while it is active and collapse completed
thinking by default.

#### Scenario: Thinking is visible while streaming

- **GIVEN** an assistant response is streaming reasoning content
- **WHEN** the thinking section is active
- **THEN** the UI SHALL show a visible thinking area.

#### Scenario: Completed thinking defaults collapsed

- **GIVEN** thinking content has completed and final answer content is present
- **WHEN** the message is displayed
- **THEN** the thinking content SHALL default to collapsed or hidden
- **AND** the user SHALL be able to expand it for inspection.

### Requirement: Length truncation is visible

The Chat UI SHALL surface token-limit truncation when response metadata
indicates `finish_reason=length`.

#### Scenario: Length finish reason shows warning

- **GIVEN** a completed response includes `finish_reason=length`
- **WHEN** the assistant message is rendered
- **THEN** the UI SHALL show a clear warning that the answer may be truncated.

#### Scenario: Truncation warning leaves follow-up actions deferred

- **GIVEN** an assistant message is marked truncated
- **WHEN** the user views the warning
- **THEN** the UI SHALL show the truncation warning
- **AND** continue-generation, output-limit settings, and context budget
  guidance SHALL remain deferred follow-up work for this change.

### Requirement: Output token limit is explicit

The Chat request builder SHALL include `max_output_tokens` in the
`/api/responses` request payload.

#### Scenario: Request payload includes max output tokens

- **WHEN** the UI builds a Chat request
- **THEN** the payload SHALL include `max_output_tokens`
- **AND** the temporary manual-testing default value SHALL be 4096 until a
  settings UI lands.

#### Scenario: Future setting values are supported by design

- **WHEN** a later UI setting chooses 256, 512, 1024, 2048, or 4096
- **THEN** the request builder SHALL be able to pass that selected value without
  changing PD runtime or protocol behavior.

### Requirement: Serving-layer changes remain out of scope

This change SHALL NOT modify PD runtime, PD protocol, chunked prefill,
large-state framing, or long-context guarantees.

#### Scenario: PD serving is unaffected

- **WHEN** Chat UI reasoning/output UX changes are implemented
- **THEN** they SHALL NOT change PD runtime state handoff, chunked prefill,
  large-state framing, or server-side admission semantics.

#### Scenario: Sensitive content is not logged

- **WHEN** tests, diagnostics, reports, or warnings are produced
- **THEN** they SHALL NOT include prompt text, generated content, complete token
  arrays, KV/native payloads, credentials, private paths, endpoint URLs, or real
  machine labels.
