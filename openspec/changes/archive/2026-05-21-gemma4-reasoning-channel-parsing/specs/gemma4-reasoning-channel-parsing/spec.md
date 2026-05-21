# gemma4-reasoning-channel-parsing Specification

## ADDED Requirements

### Requirement: Gemma4 thought channel is parsed as reasoning

The backend SHALL parse Gemma4 thought channel output as reasoning content
instead of ordinary assistant content.

The Gemma4 parser generation path SHALL enable thought-channel extraction when
the template supports thinking, so raw channel markers are not left for later
adapters or UI code to hide.

#### Scenario: Streaming chat separates thought channel

- **GIVEN** a Gemma4 chat completion stream emits a thought channel block using
  `<|channel>thought ... <channel|>`
- **WHEN** the backend converts generated text to OpenAI-compatible streaming
  chunks
- **THEN** the thought text SHALL be emitted through
  `choices.delta.reasoning_content`
- **AND** raw channel markers SHALL NOT be emitted through
  `choices.delta.content`.

#### Scenario: Final content excludes channel markers

- **GIVEN** a Gemma4 generated response contains thought channel markers and
  final answer text
- **WHEN** the backend builds the final chat response
- **THEN** the assistant message content SHALL contain final answer text only
- **AND** the assistant message content SHALL NOT include `<|channel>thought`
  or `<channel|>`.

### Requirement: Responses streaming preserves reasoning deltas

The `/v1/responses` stream adapter SHALL map chat reasoning deltas to Responses
reasoning events.

#### Scenario: Chat reasoning delta becomes Responses reasoning event

- **GIVEN** the chat backend emits `choices.delta.reasoning_content`
- **WHEN** `/v1/responses` translates the stream
- **THEN** it SHALL emit `response.reasoning_text.delta`
- **AND** it SHALL NOT merge the reasoning text into
  `response.output_text.delta`.

#### Scenario: Chat content delta remains output text

- **GIVEN** the chat backend emits `choices.delta.content`
- **WHEN** `/v1/responses` translates the stream
- **THEN** it SHALL emit `response.output_text.delta`.

### Requirement: Non-reasoning behavior is preserved

Models or templates without recognized reasoning channel support SHALL continue
to stream ordinary content as before.

#### Scenario: Plain content remains plain content

- **GIVEN** a chat stream chunk contains only `choices.delta.content`
- **WHEN** the Responses adapter translates it
- **THEN** it SHALL emit only `response.output_text.delta`
- **AND** it SHALL NOT synthesize a reasoning event.

### Requirement: Sensitive content is not logged

Diagnostics and tests for this change SHALL avoid logging full generated text
or sensitive runtime details.

#### Scenario: Marker regression avoids generated-content logging

- **WHEN** tests check for raw channel marker leakage
- **THEN** they SHALL use synthetic fixture text or marker-presence assertions
- **AND** they SHALL NOT write prompt text, generated content, complete token
  arrays, KV/native payloads, credentials, private paths, endpoint URLs, or
  real machine labels to reports.
