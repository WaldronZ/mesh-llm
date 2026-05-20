# UI Architecture

Generated: 2026-05-18

The console lives in `crates/mesh-llm-ui/` and is embedded into the host binary
as static assets. Build and workflow details are in `CONTRIBUTING.md`,
`docs/USAGE.md`, and repo build recipes.

## Router

Source: `crates/mesh-llm-ui/src/app/router/router.tsx`.

Routes:

| Route | Purpose |
|---|---|
| `/` | Dashboard surface. |
| `/chat` | Chat workspace. |
| `/configuration` | Configuration workspace. |
| `/configuration/$configurationTab` | Configuration tab deep link. |
| `/__playground` | Dev-only component playground. |
| `/__meshviz-perf` | Perf route when dev or `VITE_ENABLE_PERF_ROUTE=true`. |

There is older route helper code that maps dashboard/chat/playground paths in
`crates/mesh-llm-ui/src/features/app-shell/lib/routes.ts`. Check current
router usage before relying on helper paths.

## API Consumers

| UI area | Code |
|---|---|
| Shared API types | `crates/mesh-llm-ui/src/lib/api/types.ts` |
| Status/runtime/model queries | `crates/mesh-llm-ui/src/features/network/api/` |
| Chat streaming and attachments | `crates/mesh-llm-ui/src/features/chat/api/` |
| Configuration adapters | `crates/mesh-llm-ui/src/features/configuration/api/` |
| Query keys/provider | `crates/mesh-llm-ui/src/lib/query/` |

Server payload source remains Rust under `crates/mesh-llm-host-runtime/src/api/`.
Update both sides when changing serialized fields.

## Feature Areas

| Feature | Code path |
|---|---|
| App shell | `src/app/`, `src/features/app-shell/` |
| Network/dashboard | `src/features/network/`, `src/features/dashboard/` |
| Chat | `src/features/chat/` |
| Configuration | `src/features/configuration/` |
| Drawers/status/shared UI | `src/features/drawers/`, `src/features/status/`, `src/components/ui/` |
| Developer playground | `src/features/developer/` |

## Design And Testing Notes

- Prefer existing UI primitives in `src/components/ui/`.
- Treat the console as an operator tool: dense, scannable, and stateful.
- UI-only change: run `just build`.
- If changing API payloads, run relevant UI adapter tests and Rust API tests.
- For visual or routing regressions, use existing e2e files under
  `crates/mesh-llm-ui/e2e/`.

