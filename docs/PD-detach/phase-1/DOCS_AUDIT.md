# Docs Audit

Generated: 2026-05-18

Scope: `docs/` was audited read-only, with targeted cross-checks against current code paths and scripts. This file does not attempt to rewrite or normalize the documentation set; it records current discovery findings for a larger second-development handoff.

Confidence labels:

- `current`: safe to use for its stated purpose, or clearly usable as dated historical evidence.
- `likely-stale`: contains outdated paths, describes a plan that appears implemented, or should not be used as current operational truth without refresh.
- `unknown`: needs owner confirmation or a deeper implementation audit.

## High-Level Findings

- The source layout has moved beyond several older docs. Current runtime ownership is mostly under `crates/mesh-llm-host-runtime/src/`, while `crates/mesh-llm/src/` is now a thin wrapper with `lib.rs` and `main.rs`. Docs that cite old `src/api`, `src/network`, or `src/inference` paths are suspect.
- Skippy, layer-package, family-certification, and CUDA release docs have the strongest current code/script backing.
- Protocol docs need a compatibility refresh: owner confirmed `mesh-llm/1` is the current formal runtime protocol and `mesh-llm/0` is historical/legacy compatibility code, while shared protocol code still defines `/0`.
- Many benchmark docs are valuable evidence logs, but only some are suitable for current performance claims.
- The repo is missing a single takeover/handoff document that maps current crates, runtime processes, protocols, APIs, configs, and validation by subsystem.

Follow-up takeover docs have been added under `docs/PD-detach/phase-1/`; see
`docs/PD-detach/phase-1/README.md` and `docs/PD-detach/phase-1/risk-register.md`.

## Directory Map

| Path | Purpose | Type | Confidence | Evidence / Reason |
|---|---|---|---|---|
| `docs/` | Top-level docs, docs site assets, usage and architecture entry points. | 使用文档, 部署文档 | current | `docs/README.md`, `docs/USAGE.md`, `docs/CLI.md`; current runtime code in `crates/mesh-llm-host-runtime/src/`. |
| `docs/design/` | Architecture, ADRs, protocol notes, router/identity/metrics/testing design material. | 设计文档, 计划文档, benchmark | unknown | Mixed freshness: `docs/design/CRATE_DECOMPOSITION.md` matches current crates, while `docs/design/DESIGN.md` still uses older source layout. |
| `docs/specs/` | Focused behavior specs for shipped or planned features. | 规格文档 | current | `docs/specs/layer-package-repos.md` aligns with `crates/skippy-model-package/src/main.rs` and HF package resolution code. |
| `docs/plugins/` | Plugin architecture, plugin roadmap, built-in plugin docs. | 设计文档, 规格文档, 计划文档 | current | Code exists in `crates/mesh-llm-host-runtime/src/plugin/`, `crates/mesh-llm-host-runtime/src/plugins/`, and `crates/mesh-llm-plugin/src/`. |
| `docs/skippy/` | Skippy staged runtime docs, certification, benchmarks, experiment logs, family evidence. | 设计文档, benchmark, 历史记录 | current | Backed by `crates/skippy-*`, `crates/mesh-llm-host-runtime/src/inference/skippy/`, and `scripts/family-certify.sh`. |
| `docs/skippy/family/` | Qwen3.6-specific runbooks and benchmark/result logs. | benchmark, 历史记录 | likely-stale | Contains fixed lab state, generated evidence, and model-specific notes; scripts such as `scripts/qwen-lab-preflight.sh` exist but results are time-sensitive. |
| `docs/plans/` | Standalone implementation plans. | 计划文档 | likely-stale | `docs/plans/gpu-benchmark-cli.md` describes a feature now present in `crates/mesh-llm-host-runtime/src/cli/commands/gpus.rs` and `crates/mesh-llm-system/src/benchmark.rs`. |

## Document Audit

| Path | Purpose | Type | Confidence | Evidence / Reason |
|---|---|---|---|---|
| `docs/README.md` | Documentation hub and topic directory guide. | 使用文档 | current | Links current docs and crate READMEs; code has decomposed crates under `crates/`. |
| `docs/USAGE.md` | User and operator workflows: install, local run, meshes, OpenAI API, control-plane QA. | 使用文档, 部署文档 | current | CLI and runtime paths exist in `crates/mesh-llm-host-runtime/src/cli/mod.rs`, `crates/mesh-llm-host-runtime/src/runtime/instance.rs`, and `scripts/qa-control-plane-mixed-version.sh`. |
| `docs/CLI.md` | CLI reference. | 使用文档 | current | Main Clap definitions and command dispatch are in `crates/mesh-llm-host-runtime/src/cli/mod.rs` and `crates/mesh-llm-host-runtime/src/cli/commands/`. |
| `docs/MESHES.md` | Public/private mesh discovery, join, routing, and publishing flows. | 使用文档, 部署文档 | current | Nostr/gossip/tunnel/API code exists in `crates/mesh-llm-host-runtime/src/network/nostr.rs`, `crates/mesh-llm-host-runtime/src/mesh/gossip.rs`, and `crates/mesh-llm-host-runtime/src/network/tunnel.rs`. |
| `docs/AGENTS.md` | Agent integrations and blackboard usage. | 使用文档 | current | Integration and blackboard code exists in `crates/mesh-llm-host-runtime/src/cli/commands/integrations.rs` and `crates/mesh-llm-host-runtime/src/plugins/blackboard/`. |
| `docs/LAYER_PACKAGE_REPOS.md` | Publishing and consuming layer package repos. | 使用文档, 规格文档 | current | Aligns with `docs/specs/layer-package-repos.md`, `crates/skippy-model-package/src/main.rs`, and `crates/mesh-llm-host-runtime/src/models/remote_catalog.rs`. |
| `docs/SKIPPY_SPLITS.md` | Running large models with Skippy split serving. | 使用文档 | current | Current implementation lives in `crates/mesh-llm-host-runtime/src/inference/skippy/`, `crates/skippy-server/`, and `crates/skippy-model-package/`. |
| `docs/SKIPPY.md` | Older Skippy integration plan and migration queue. | 计划文档, 设计文档 | likely-stale | Describes integration work that now appears implemented in `crates/mesh-llm-host-runtime/src/inference/skippy/` and split docs now live in `docs/SKIPPY_SPLITS.md`. |
| `docs/BENCHMARKS.md` | Older performance claims and reality-check numbers. | benchmark | likely-stale | Should be compared against newer Skippy benchmark docs in `docs/skippy/` and current scripts such as `scripts/family-certify.sh`. |
| `docs/EXO_COMPARISON.md` | mesh-llm vs Exo comparison. | benchmark, 历史记录 | likely-stale | It depends on external project state and dated comparison context; local code cannot verify Exo claims. |
| `docs/cuda-release-lanes.md` | CUDA and Blackwell release lane guidance. | 部署文档 | current | Matches `Justfile` recipes `release-build-cuda*`, `.github/workflows/release.yml`, `install.sh`, and `scripts/package-release.sh`. |
| `docs/index.html` | Static landing page for docs/site hosting. | 使用文档, 部署文档 | likely-stale | It embeds old visual labels like `llama-server` and `rpc-server`; current guidance says embedded runtime is primary. |
| `docs/CNAME` | Static site custom domain. | 部署文档 | unknown | Static artifact only; no deployment source-of-truth found in this audit. |
| `docs/mesh.png` | Static image asset for docs/site. | 历史记录 | unknown | Asset referenced by docs/site context only. |
| `docs/mesh-llm-logo.svg` | Static logo asset. | 历史记录 | unknown | Referenced by `docs/index.html`. |
| `docs/mesh-llm-wordmark.png` | Static wordmark asset. | 历史记录 | unknown | Referenced by docs/site context only. |

## Design Docs

| Path | Purpose | Type | Confidence | Evidence / Reason |
|---|---|---|---|---|
| `docs/design/CRATE_DECOMPOSITION.md` | Current crate split status and target decomposition. | 设计文档, 计划文档 | current | Current crates include `mesh-llm-host-runtime`, `mesh-llm-protocol`, `mesh-api`, `mesh-client`, `mesh-llm-plugin`, and `skippy-*`. |
| `docs/design/DESIGN.md` | Broad architecture and protocol overview. | 设计文档 | likely-stale | Several paths use the old monolithic layout; current code is under `crates/mesh-llm-host-runtime/src/` and other decomposed crates. |
| `docs/design/message_protocol.md` | Wire/control-plane protocol reference. | 规格文档 | likely-stale | Stream/protobuf details align with `crates/mesh-llm-protocol/proto/node.proto`, but compatibility text conflicts with `crates/mesh-llm-protocol/src/protocol/v0.rs`. |
| `docs/design/TESTING.md` | Test playbook, scenarios, remote deploy, mixed-version QA. | 使用文档, 部署文档 | current | Cites active workflows and scripts including `scripts/qa-control-plane-mixed-version.sh`; repo notes require reading it before tests. |
| `docs/design/METRICS.md` | Management API metric groups and routing diagnostics. | 设计文档, 规格文档 | current | Aligns with `crates/mesh-llm-host-runtime/src/network/metrics.rs`. |
| `docs/design/MULTI_MODAL.md` | Multimodal capability, blobstore, console upload, routing design. | 设计文档 | current | Backed by `crates/mesh-llm-host-runtime/src/models/capabilities.rs`, `crates/mesh-llm-host-runtime/src/plugins/blobstore/`, `crates/mesh-llm-protocol/proto/node.proto`, and UI attachment code. |
| `docs/design/NODE_OWNER_IDENTITY.md` | Node owner identity and trust model proposal. | 设计文档, 计划文档 | likely-stale | Feature is implemented in `crates/mesh-llm-host-runtime/src/crypto/ownership.rs`, `crates/mesh-llm-host-runtime/src/mesh/gossip.rs`, and `crates/mesh-llm-host-runtime/src/cli/commands/auth.rs`; doc still reads as proposal. |
| `docs/design/IDENTITY_INCIDENT_RESPONSE.md` | Identity incident response runbook draft. | 使用文档, 计划文档 | likely-stale | Depends on owner identity flows now implemented in `crates/mesh-llm-host-runtime/src/cli/commands/auth.rs`; needs reconciliation with actual UX. |
| `docs/design/EMBEDDED_CLIENT_ADR.md` | ADR for embedded client and API crate boundary. | 设计文档, 历史记录 | likely-stale | Current public client surfaces exist in `crates/mesh-api/`, `crates/mesh-api-ffi/`, and `crates/mesh-client/`, but extraction notes reference older ownership. |
| `docs/design/JAN_MESH_API_INTEGRATION.md` | Jan integration design/spec. | 设计文档, 规格文档 | unknown | Local API crates exist, but external Jan plugin status is not verifiable from this repo. |
| `docs/design/LLAMA_STAGE_INTEGRATION_PLAN.md` | Plan for embedded llama-stage integration. | 计划文档 | likely-stale | Current patch/build lane exists in `third_party/llama.cpp/patches`, `scripts/prepare-llama.sh`, `scripts/build-llama.sh`, and `crates/skippy-*`. |
| `docs/design/MODEL_ROUTER.md` | Model router implementation plan. | 计划文档, 设计文档 | likely-stale | Current routing code is in `crates/mesh-llm-host-runtime/src/network/router.rs` and `crates/mesh-llm-routing/`; doc references older paths and plan state. |
| `docs/design/ROUTER_V2.md` | Router V2 proposal. | 设计文档, 计划文档 | unknown | Needs comparison with `crates/mesh-llm-host-runtime/src/network/router.rs` and `crates/mesh-llm-routing/` before using as current design. |
| `docs/design/ROUTER_BENCHMARKS.md` | Router benchmark notes. | benchmark | likely-stale | Mentions untested or dated benchmark state; current routing metrics are in `docs/design/METRICS.md` and `crates/mesh-llm-host-runtime/src/network/metrics.rs`. |
| `docs/design/PREFIX_AFFINITY_BENCHMARKS.md` | Prefix-affinity benchmark procedure and notes. | benchmark | current | Active `Justfile` has `bench-prefix-affinity`; request-affinity code belongs in networking per repo notes. |
| `docs/design/VIRTUAL_LLM.md` | Virtual LLM and inter-model collaboration design. | 设计文档, 历史记录 | likely-stale | Current code includes `crates/mesh-llm-host-runtime/src/inference/virtual_llm.rs` and mesh hook routes, but the doc references older branches and paths. |

## Specs And Plans

| Path | Purpose | Type | Confidence | Evidence / Reason |
|---|---|---|---|---|
| `docs/specs/layer-package-repos.md` | Layer package repo format and discovery contract. | 规格文档 | current | Matches `crates/skippy-model-package/src/main.rs`, `crates/mesh-llm-host-runtime/src/models/remote_catalog.rs`, and `crates/mesh-llm-host-runtime/src/models/resolve/mod.rs`. |
| `docs/specs/context-and-slots-auto.md` | Automatic context and slot sizing behavior. | 规格文档, 计划文档 | likely-stale | Behavior appears implemented in `crates/mesh-llm-host-runtime/src/runtime/context_planning.rs` and `crates/model-artifact/src/gguf.rs`, but doc cites older paths. |
| `docs/plans/gpu-benchmark-cli.md` | GPU benchmark CLI implementation plan. | 计划文档 | likely-stale | Feature appears implemented through `crates/mesh-llm-host-runtime/src/cli/commands/gpus.rs` and `crates/mesh-llm-system/src/benchmark.rs`. |

## Plugin Docs

| Path | Purpose | Type | Confidence | Evidence / Reason |
|---|---|---|---|---|
| `docs/plugins/README.md` | Plugin architecture, manifest model, MCP/control-plane projection. | 设计文档, 规格文档 | current | Aligns with `crates/mesh-llm-plugin/src/`, `crates/mesh-llm-host-runtime/src/plugin/mcp.rs`, and `crates/mesh-llm-host-runtime/src/plugin/stapler.rs`. |
| `docs/plugins/PLAN.md` | Plugin v2 implementation plan. | 计划文档 | likely-stale | Manifest, stapler/MCP, and built-in plugin code now exist; plan should be archived or converted to status. |
| `docs/plugins/flash-moe.md` | Flash MoE plugin design and behavior. | 设计文档 | current | Backed by `crates/mesh-llm-host-runtime/src/plugins/flash_moe/`. |
| `docs/plugins/telemetry.md` | Telemetry plugin behavior, attribute/privacy rules. | 规格文档, 使用文档 | current | Backed by `crates/mesh-llm-host-runtime/src/plugins/telemetry/mod.rs`, `crates/mesh-llm-host-runtime/src/runtime/survey.rs`, and metrics docs. |

## Skippy Docs

| Path | Purpose | Type | Confidence | Evidence / Reason |
|---|---|---|---|---|
| `docs/skippy/FAMILY_STATUS.md` | Family certification support matrix. | 规格文档, benchmark | current | Uses current capability policy concepts from `crates/skippy-topology/src/lib.rs` and `crates/mesh-llm-host-runtime/src/inference/skippy/family_policy.rs`. |
| `docs/skippy/FAMILY_CERTIFY.md` | Certification workflow/runbook. | 使用文档, benchmark | current | Directly backed by `scripts/family-certify.sh`, `crates/skippy-correctness/`, `crates/skippy-topology/`, and `crates/skippy-prompt/`. |
| `docs/skippy/TOPOLOGY_PLANNER.md` | Topology planner design and evidence model. | 设计文档, 规格文档 | current | Backed by `crates/skippy-topology/src/lib.rs` and `crates/mesh-llm-host-runtime/src/inference/skippy/topology.rs`. |
| `docs/skippy/LLAMA_PARITY.md` | llama.cpp parity and family readiness tracker. | benchmark, 历史记录 | current | Backed by `scripts/skippy-llama-parity.py`, `docs/skippy/llama-parity-candidates.json`, and `scripts/family-certify.sh`. |
| `docs/skippy/llama-parity-candidates.json` | Candidate model data for parity certification. | benchmark, 规格文档 | current | Used by `scripts/skippy-llama-parity.py`. |
| `docs/skippy/BENCHMARK_CORPUS.md` | Benchmark corpus generation and tiers. | benchmark, 使用文档 | current | Backed by `scripts/generate-bench-corpus.py`, `crates/skippy-bench/corpora/bench_corpus_sources.json`, and `Justfile` `bench-corpus`. |
| `docs/skippy/BENCHMARK_TODO.md` | Benchmark roadmap and task list. | 计划文档 | unknown | Mentions `scripts/openai-smoke.sh`, while current script found is `scripts/skippy-openai-smoke.sh`; task state needs owner review. |
| `docs/skippy/LLAMA_BENCHY.md` | OpenAI-compatible llama-benchy benchmark path. | benchmark, 使用文档 | likely-stale | Active script is `scripts/run-llama-benchy-openai.sh`, but some docs mention `scripts/openai-smoke.sh` instead of `scripts/skippy-openai-smoke.sh`. |
| `docs/skippy/EXPERIMENTS.md` | Experiment log for staged runtime behavior and performance. | benchmark, 历史记录 | current | Treat as dated evidence, not default performance claim; related code is under `crates/skippy-bench/` and `crates/skippy-server/`. |
| `docs/skippy/DATA_FLOW.md` | Benchmark/data-flow note for specific staged runtime runs. | benchmark, 历史记录 | current | Usable as dated evidence; should not be used as current architecture alone. |
| `docs/skippy/speculative_decoding.md` | Speculative decoding design and benchmark notes. | 设计文档, benchmark | current | Code exists in `crates/skippy-server/src/frontend/speculative.rs`, `crates/skippy-prompt/src/prompt_cli/speculative.rs`, and `crates/llama-spec-bench/`. |
| `docs/skippy/layer-inventory-pr-description.md` | PR-style writeup for layer inventory work. | 历史记录, 规格文档 | likely-stale | Should be treated as PR history; current protocol/runtime code lives in `crates/skippy-protocol/` and `crates/mesh-llm-host-runtime/src/inference/skippy/`. |
| `docs/skippy/family/qwen.md` | Qwen3.6 readiness, benchmark plan, and lab evidence. | benchmark, 历史记录 | likely-stale | Time-sensitive model and lab state; scripts such as `scripts/qwen-lab-preflight.sh` and `Justfile` `bench-corpus` exist. |
| `docs/skippy/family/qwen-package-runbook.md` | Qwen package-generation runbook. | 使用文档, benchmark | unknown | Needs validation against current package scripts and `crates/skippy-model-package/src/main.rs`. |
| `docs/skippy/family/qwen-results.md` | Qwen benchmark/result evidence. | benchmark, 历史记录 | likely-stale | Large generated result log; useful as historical evidence, not current performance guarantee. |

## Most Relevant Docs For Large Second-Development

Start with these:

1. `docs/README.md` for the docs map.
2. `docs/CLI.md` and `docs/USAGE.md` for operator workflows.
3. `docs/MESHES.md` for discovery, join, public/private mesh behavior.
4. `docs/design/CRATE_DECOMPOSITION.md` for current crate ownership and extraction direction.
5. `docs/design/TESTING.md` for required validation and mixed-version testing.
6. `docs/design/message_protocol.md` for protocol shape, with the `mesh-llm/0` caveat tracked in `docs/PD-detach/phase-1/questions.md`.
7. `docs/design/METRICS.md` for routing/API diagnostics.
8. `docs/plugins/README.md` and `docs/plugins/telemetry.md` for plugin and telemetry surfaces.
9. `docs/SKIPPY_SPLITS.md`, `docs/LAYER_PACKAGE_REPOS.md`, and `docs/specs/layer-package-repos.md` for package-based split serving.
10. `docs/skippy/FAMILY_STATUS.md`, `docs/skippy/FAMILY_CERTIFY.md`, and `docs/skippy/TOPOLOGY_PLANNER.md` for Skippy certification and family policy.

Use with caution:

- `docs/design/DESIGN.md`, `docs/SKIPPY.md`, `docs/design/LLAMA_STAGE_INTEGRATION_PLAN.md`, `docs/design/MODEL_ROUTER.md`, `docs/design/VIRTUAL_LLM.md`: useful history, but path and implementation state drift is likely.
- `docs/BENCHMARKS.md`, `docs/design/ROUTER_BENCHMARKS.md`, and Qwen result docs: useful evidence, but should not be used as current benchmark claims without rerun.

## Missing Takeover Docs

Recommended missing handoff docs, ordered by value:

1. `docs/PD-detach/phase-1/HANDOFF.md`: single entry point for current architecture, crate ownership, subsystem status, and "where to start" for new maintainers.
2. `docs/PD-detach/phase-1/ARCHITECTURE_CURRENT.md`: current runtime and crate architecture, replacing or refreshing stale parts of `docs/design/DESIGN.md`.
3. `docs/PD-detach/phase-1/PROTOCOL_COMPATIBILITY.md`: `mesh-llm/0`, `mesh-llm/1`, `mesh-llm-control/1`, stream IDs, protobuf compatibility rules, and breaking-change checklist.
4. `docs/PD-detach/phase-1/RUNTIME_OPERATIONS.md`: local/remote process lifecycle, ports, runtime roots, logs, TUI/headless/no-console behavior, cleanup, and remote observation.
5. `docs/PD-detach/phase-1/CONFIGURATION.md`: config file schema, owner keys/trust policy, plugin config, telemetry settings, and environment variables.
6. `docs/PD-detach/phase-1/API_REFERENCE.md`: management API and OpenAI-compatible API payloads/examples, including UI-consumed endpoints.
7. `docs/PD-detach/phase-1/TEST_MATRIX.md`: minimum validation by touched area: CLI, UI, routing, gossip, protocol, plugin, Skippy, release.
8. `docs/PD-detach/phase-1/SECURITY_AND_PRIVACY.md`: owner identity, trust/revocation, telemetry allowlists, artifact transfer, and credentials handling.
9. `docs/PD-detach/phase-1/UI_ARCHITECTURE.md`: console routes, data fetching, component ownership, API adapters, and smoke/e2e coverage.
10. `docs/PD-detach/phase-1/DOCS_MAINTENANCE.md`: current/stale/archive policy and document owners.

Open questions and uncertain items are tracked in `docs/PD-detach/phase-1/questions.md`.
