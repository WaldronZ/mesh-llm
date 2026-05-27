# Design: Chat Model Status Robustness

## Adapter Shape Guard

The model adapter should be the only place that understands model catalog
payload variants. It accepts unknown input and returns a safe `ModelSummary[]`.
Supported forms:

- a direct model array;
- `{ mesh_models: [...] }`;
- `{ models: [...] }`;
- OpenAI-style `{ data: [...] }`.

Malformed or missing arrays return an empty summary instead of throwing. Valid
mesh model rows keep their existing behavior. OpenAI-style rows can contribute
model ids but are not treated as warm unless they carry a recognized warm
status.

## Chat Request Model Resolution

The UI can continue to display Auto by default. Before sending a live chat
request, Chat resolves the request model:

- if the visible selection is Auto;
- and there is exactly one warm selectable model;
- then send that model's real id/name;
- otherwise preserve the existing selected model behavior.

This keeps multi-model Auto semantics intact while avoiding `model=auto` for
single-model backends that only accept explicit ids.

## Privacy And Scope

Tests and fixtures use synthetic model ids only. This change does not record
prompt text, generated content, private paths, endpoint URLs, or credentials.
It does not alter backend/server behavior.
