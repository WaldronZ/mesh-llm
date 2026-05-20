# API Reference Map

Generated: 2026-05-18

This is a route ownership map, not a full payload reference. Use code as source
of truth for payload fields.

Primary sources:

- Dispatch: `crates/mesh-llm-host-runtime/src/api/routes/mod.rs`
- Public payloads: `crates/mesh-llm-host-runtime/src/api/status.rs`
- UI API types: `crates/mesh-llm-ui/src/lib/api/types.ts`
- UI adapters: `crates/mesh-llm-ui/src/features/*/api/`

## Schema Authority

Current code review confirms that the management API schema is not generated
from a standalone OpenAPI/JSON schema file. The strongest server-side source is
Rust code:

- `crates/mesh-llm-host-runtime/src/api/status.rs` describes itself as public
  status/model payloads and serialization compatibility anchors.
- `StatusPayload`, `PeerPayload`, `OwnershipPayload`, `RuntimeStatusPayload`,
  `MeshModelPayload`, `ModelTargetPayload`, and related structs derive
  `serde::Serialize`.
- `crates/mesh-llm-host-runtime/src/api/routes/mod.rs` and `routes/*.rs` own
  route dispatch and response construction.
- `crates/mesh-llm-ui/src/lib/api/types.ts` is a UI consumer mirror, not the
  authoritative server schema.

Open question for phase 2: whether to keep this code-first contract or add a
generated schema/OpenAPI workflow.

## Surfaces

| Surface | Default port | Purpose |
|---|---:|---|
| Management API / web console | `3131` | Status, runtime, config, plugin, object, UI assets. |
| OpenAI-compatible API | `9337` | `/v1/*`, `/models`, chat/responses passthrough. |

## Management Routes

| Route | Method | Owner |
|---|---|---|
| `/api/status` | `GET` | `routes/runtime.rs`, `api/status.rs` |
| `/api/models` | `GET` | `routes/runtime.rs`, `api/status.rs` |
| `/api/events` | `GET` | `routes/runtime.rs` |
| `/api/discover` | `GET` | `routes/discover.rs` |
| `/api/search` | `GET` | `routes/search.rs` |
| `/api/runtime` | `GET` | `routes/runtime.rs` |
| `/api/runtime/llama` | `GET` | `routes/runtime.rs`, runtime metrics/slots |
| `/api/runtime/events` | `GET` | `routes/runtime.rs` |
| `/api/runtime/endpoints` | `GET` | `routes/runtime.rs` |
| `/api/runtime/processes` | `GET` | `routes/runtime.rs` |
| `/api/runtime/stages` | `GET` | `routes/runtime.rs` |
| `/api/runtime/models` | `POST` | Runtime model load |
| `/api/runtime/models/<model>` | `DELETE` | Runtime model unload |
| `/api/runtime/instances/<id>` | `DELETE` | Runtime instance unload |
| `/api/model-interests` | `GET`, `POST` | `routes/model_interests.rs` |
| `/api/model-interests/<model-ref>` | `DELETE` | `routes/model_interests.rs` |
| `/api/model-targets` | `GET` | `routes/model_targets.rs`, `api/model_targets.rs` |
| `/api/runtime/control-bootstrap` | `GET` | Owner-control bootstrap, loopback guarded |
| `/api/runtime/control/get-config` | `POST` | Owner-control config read, loopback guarded |
| `/api/runtime/control/refresh-inventory` | `POST` | Owner-control inventory refresh, loopback guarded |
| `/api/runtime/control/apply-config` | `POST` | Owner-control config apply, loopback guarded |

## Plugin And Object Routes

| Route | Method | Owner |
|---|---|---|
| `/api/plugins` | `GET` | `routes/plugins.rs` |
| `/api/plugins/endpoints` | `GET` | Plugin runtime endpoint listing |
| `/api/plugins/providers` | `GET` | Provider discovery |
| `/api/plugins/providers/<capability>` | `GET` | Provider discovery by capability |
| `/api/plugins/<plugin>/manifest` | `GET` | Plugin manifest |
| `/api/plugins/<plugin>/tools` | `GET` | Tool listing |
| `/api/plugins/<plugin>/tools/<tool>` | `POST` | Tool call |
| `/api/plugins/<plugin>/http/*` | many | Stapled plugin HTTP bindings |
| `/api/blackboard/feed` | `GET` | Legacy alias to blackboard plugin HTTP binding |
| `/api/blackboard/search` | `GET` | Legacy alias |
| `/api/blackboard/post` | `POST` | Legacy alias |
| `/api/objects` | `POST` | Blob/object upload |
| `/api/objects/complete` | `POST` | Blob/object complete |
| `/api/objects/abort` | `POST` | Blob/object abort |

## OpenAI And Chat Routes

| Route | Method | Owner |
|---|---|---|
| `/v1/*` | `GET`, `POST`, `OPTIONS` | `routes/chat.rs`, network/OpenAI proxy |
| `/models` | `GET`, `POST`, `OPTIONS` | Legacy OpenAI model surface |
| `/api/chat*` | many | Rewritten to `/v1/chat/completions` |
| `/api/responses*` | many | Rewritten to `/v1/responses` |
| `/mesh/hook` | `POST` | Local serving-runtime hook callbacks |

## Payload Guidance

- Treat `api/status.rs` as the Rust serialization source.
- Treat `crates/mesh-llm-ui/src/lib/api/types.ts` as a UI consumer contract,
  not an independent server spec.
- When changing payload fields, update server tests, UI adapters, and this
  route map together.
