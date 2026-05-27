# Tasks

## 1. Adapter Robustness

- [x] Accept direct arrays, `{ mesh_models }`, `{ models }`, and `{ data }`.
- [x] Return an empty model summary for undefined, null, and partial payloads.
- [x] Preserve valid mesh model mapping behavior.
- [x] Keep OpenAI-style rows safe without inventing warm status.

## 2. Page Integration

- [x] Route full model payloads through `adaptModelsToSummary` in Chat.
- [x] Route full model payloads through `adaptModelsToSummary` in Network.
- [x] Keep warm models selectable in Chat.
- [x] Resolve Auto to the sole warm model id only when exactly one warm model is
      selectable.
- [x] Preserve Auto behavior for zero or multiple warm models.

## 3. Tests

- [x] Cover valid mesh model payloads.
- [x] Cover OpenAI-style model payloads.
- [x] Cover undefined, null, and missing model arrays.
- [x] Cover Chat page partial payload render safety.
- [x] Cover Auto + one warm model request resolution.
- [x] Cover Auto + zero/multiple warm model behavior.

## 4. Deferred

- [x] Backend/router changes are out of scope.
- [x] Model serving status API changes are out of scope.
- [x] Full model selection/settings redesign is out of scope.
- [x] Presentation and old active experiment directories are out of scope.
