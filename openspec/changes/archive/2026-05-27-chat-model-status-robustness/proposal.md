# Proposal: Chat Model Status Robustness

## Why

Manual production chat testing exposed two UI robustness gaps:

- the model adapter assumed `/api/models` always exposed a direct array and
  could render fault when the live payload used another shape;
- the Chat page could keep sending `model=auto` even when the live model catalog
  exposed exactly one warm selectable model and the backend did not accept
  `auto`.

These are UI integration issues. The fix should be narrow and should not change
PD serving, routing, runtime, or model status APIs.

## What Changes

- Accept partial or unknown `/api/models` payloads without throwing.
- Normalize arrays, `{ mesh_models }`, `{ models }`, and OpenAI-style
  `{ data }` through one adapter path.
- Have Chat and Network pages pass the full model payload to the adapter.
- Keep warm models selectable.
- When Chat is visually on Auto and there is exactly one warm selectable model,
  send that model id in the request instead of `auto`.

## Out Of Scope

- No PD backend, router, or runtime changes.
- No model serving status API changes.
- No full model selection/settings redesign.
- No hiding Auto in multi-model scenarios.
- No presentation recovery.
- No old active experiment directory cleanup.
