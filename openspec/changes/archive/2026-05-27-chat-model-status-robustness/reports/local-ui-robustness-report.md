# Local UI Robustness Report

## Result

`ready_for_validation`

This change is UI-only. It keeps the current model adapter hardening and adds a
minimal request-model resolution rule for live Chat.

## Implemented

- `/api/models` payloads are adapted through one guard that accepts a direct
  array, `{ mesh_models }`, `{ models }`, and OpenAI-style `{ data }`.
- Missing, null, undefined, or partial model catalog payloads return an empty
  model summary instead of throwing.
- Chat and Network pages pass the full model payload to the adapter.
- Warm models remain selectable.
- When Chat visually shows Auto and exactly one warm model is selectable, the
  request uses that model id/name.
- When zero warm models are selectable, Chat keeps the existing blocked-send
  behavior.
- When multiple warm models are selectable, Chat keeps Auto behavior.

## Out Of Scope

- No PD backend, router, or runtime changes.
- No model serving status API changes.
- No full model selection/settings redesign.
- No presentation recovery.
- No old active experiment directory cleanup.

## Privacy

The report and tests use synthetic model ids only. They do not record prompt
text, generated content, private paths, endpoint URLs, or credentials.
