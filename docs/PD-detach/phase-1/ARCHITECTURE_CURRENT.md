# Current Architecture

Generated: 2026-05-18

This is a compact source map for takeover work. It complements
`docs/design/CRATE_DECOMPOSITION.md` and supersedes older monolithic path
references in stale docs.

## Runtime Entry

- `crates/mesh-llm/src/main.rs` builds the Tokio runtime and calls
  `mesh_llm::run_main()`.
- `crates/mesh-llm/src/lib.rs` re-exports `mesh_llm_host_runtime`.
- `crates/mesh-llm-host-runtime/src/lib.rs` owns `VERSION`, `run()`, and
  `run_main()`.
- `crates/mesh-llm-host-runtime/src/runtime/mod.rs` owns top-level startup,
  serve/client normalization, API listener startup, model startup, console
  state, and long-running orchestration.

## Crate Ownership

| Crate/path | Responsibility |
|---|---|
| `crates/mesh-llm-host-runtime/` | Main host binary behavior: CLI, mesh, API, runtime, plugins, local inference. |
| `crates/mesh-llm-protocol/` | Shared protobuf types, ALPN/stream constants, protocol conversion helpers. |
| `crates/mesh-client/` | Client-side control plane, mesh/network/model helpers. |
| `crates/mesh-api/`, `crates/mesh-api-ffi/` | Public API/FFI client surfaces. |
| `crates/mesh-llm-routing/` | Reusable routing logic. |
| `crates/mesh-llm-plugin/` | Plugin manifest/runtime protocol surface. |
| `crates/mesh-llm-system/` | System and build/runtime-adjacent concerns such as GPU benchmark and updates. |
| `crates/mesh-llm-ui/` | React/Vite console embedded into the binary. |
| `crates/skippy-*` | Split-serving protocol, runtime, server, prompt, cache, topology, correctness, benchmarks. |
| `crates/model-*` | Model references, artifacts, packages, Hugging Face and resolution support. |

## Control And Data Flow

1. CLI parses in `crates/mesh-llm-host-runtime/src/cli/mod.rs`.
2. Runtime startup flows through `runtime::run()` and active/passive client
   orchestration in `crates/mesh-llm-host-runtime/src/runtime/mod.rs`.
3. Mesh membership and gossip live under `crates/mesh-llm-host-runtime/src/mesh/`.
4. Wire encoding is shared through `crates/mesh-llm-protocol/`, with host-side
   conversion in `crates/mesh-llm-host-runtime/src/protocol/convert.rs`.
5. Management API routes are dispatched by
   `crates/mesh-llm-host-runtime/src/api/routes/mod.rs`.
6. OpenAI-compatible ingress is handled through API chat passthrough and
   network proxy/routing code in `crates/mesh-llm-host-runtime/src/network/`.
7. Local model/runtime state feeds the management API through
   `crates/mesh-llm-host-runtime/src/runtime_data/`.
8. UI consumes management API types and adapters under `crates/mesh-llm-ui/src/`.

## Subsystem Map

| Subsystem | Code | Existing docs |
|---|---|---|
| CLI | `crates/mesh-llm-host-runtime/src/cli/` | `docs/CLI.md`, `docs/USAGE.md` |
| Mesh discovery and gossip | `crates/mesh-llm-host-runtime/src/mesh/`, `crates/mesh-llm-host-runtime/src/network/nostr.rs` | `docs/MESHES.md` |
| Protocol | `crates/mesh-llm-protocol/`, `crates/mesh-llm-host-runtime/src/protocol/` | `docs/design/message_protocol.md` with risk notes |
| API | `crates/mesh-llm-host-runtime/src/api/` | `API_REFERENCE.md`, `docs/USAGE.md` |
| Routing | `crates/mesh-llm-host-runtime/src/network/router.rs`, `crates/mesh-llm-routing/` | `docs/design/METRICS.md`, router docs with risk notes |
| Models | `crates/mesh-llm-host-runtime/src/models/`, `crates/model-*` | `docs/LAYER_PACKAGE_REPOS.md`, `docs/specs/layer-package-repos.md` |
| Skippy | `crates/skippy-*`, `crates/mesh-llm-host-runtime/src/inference/skippy/` | `docs/SKIPPY_SPLITS.md`, `docs/skippy/*` |
| Plugins | `crates/mesh-llm-host-runtime/src/plugin/`, `crates/mesh-llm-host-runtime/src/plugins/`, `crates/mesh-llm-plugin/` | `docs/plugins/README.md` |
| UI | `crates/mesh-llm-ui/src/` | `UI_ARCHITECTURE.md`, `crates/mesh-llm-ui/README.md` if present |

## Architecture Risks

See `risk-register.md` for stale source-layout docs, protocol compatibility
drift, older Skippy integration plans, benchmark drift, and branch-specific
gossip compatibility implications.
