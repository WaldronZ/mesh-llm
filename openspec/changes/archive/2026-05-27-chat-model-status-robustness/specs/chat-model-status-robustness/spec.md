# chat-model-status-robustness Specification

## ADDED Requirements

### Requirement: Model catalog payloads are shape guarded

The Chat and Network UI SHALL adapt live model catalog payloads defensively so
partial or alternate payload shapes do not render fault.

#### Scenario: Alternate model arrays are accepted

- **GIVEN** the model catalog payload is a direct array, `{ mesh_models: [...] }`,
  `{ models: [...] }`, or OpenAI-style `{ data: [...] }`
- **WHEN** the UI adapts the payload
- **THEN** it SHALL return a model summary list without throwing.

#### Scenario: Partial payloads are safe

- **GIVEN** the model catalog payload is missing a model array, null, undefined,
  or otherwise partial
- **WHEN** the UI adapts the payload
- **THEN** it SHALL return an empty model summary
- **AND** Chat and Network pages SHALL keep rendering.

### Requirement: Single warm model avoids unsupported auto requests

The Chat UI SHALL avoid sending `model=auto` to a live backend when the model
catalog exposes exactly one warm selectable model.

#### Scenario: Auto resolves to the sole warm model

- **GIVEN** Chat is in live mode
- **AND** the visible model selection is Auto
- **AND** exactly one warm model is selectable
- **WHEN** the user sends a chat request
- **THEN** the request SHALL use that model's real id or name.

#### Scenario: Auto remains when resolution is ambiguous

- **GIVEN** Chat is in live mode
- **AND** the visible model selection is Auto
- **WHEN** there are zero warm selectable models or more than one warm selectable
  model
- **THEN** Chat SHALL preserve the existing Auto behavior and SHALL NOT invent a
  model id.

### Requirement: Scope remains UI-only

The change SHALL NOT modify PD backend, router, runtime, model serving status
APIs, presentation files, or old active experiment directories.

#### Scenario: UI robustness does not change backend surfaces

- **WHEN** model catalog robustness or single-warm-model request resolution is
  implemented
- **THEN** changes SHALL remain limited to Chat/Network UI adapter and page
  behavior
- **AND** PD backend, router, runtime, model serving status APIs, presentation
  files, and old active experiment directories SHALL remain unchanged.
