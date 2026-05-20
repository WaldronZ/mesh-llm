# Test Matrix

Generated: 2026-05-18

Always read `docs/design/TESTING.md` before running tests. This page narrows
that playbook by touched area for takeover work.

Cargo commands must run serially.

| Touched area | Minimum local validation | Broader validation |
|---|---|---|
| Docs only | No build required; check links/paths manually. | Optional markdown review. |
| Rust-only narrow change | `cargo fmt --all -- --check`; `cargo check -p mesh-llm` | Targeted tests for changed module. |
| CLI behavior | `cargo check -p mesh-llm`; targeted CLI tests if present | Smoke command with `--help` or relevant command path. |
| API payload/routes | `cargo test -p mesh-llm --lib` | UI adapter tests if payload is consumed by console. |
| Gossip/protocol/mesh | `cargo test -p mesh-llm --lib` | `scripts/qa-control-plane-mixed-version.sh`; two-node validation. |
| Routing/proxy/OpenAI surface | `cargo test -p mesh-llm --lib` | Chat/OpenAI smoke against `/v1/models` and `/v1/chat/completions`. |
| Skippy split serving | Relevant `skippy-*` tests | `docs/skippy/FAMILY_CERTIFY.md`; `scripts/family-certify.sh` for family work. |
| Layer packages | `skippy-model-package` validation path | `docs/specs/layer-package-repos.md`; package materialization smoke. |
| Plugin protocol/runtime | Plugin unit tests if touched | MCP/stapled HTTP smoke; telemetry privacy review when telemetry changes. |
| Telemetry | Targeted telemetry tests | Check `docs/plugins/telemetry.md` and attribute allowlist. |
| UI-only | `just build` | Relevant Vitest/Playwright tests where available. |
| Mixed Rust and UI | `just build` | Feature-specific runtime/API smoke. |
| Release/build lanes | `just build`; relevant release recipe dry run if available | `.github/workflows/release.yml`, `docs/cuda-release-lanes.md`. |

## Code-Confirmed Command Entrypoints

The following commands and scripts are present in the current tree and can be
used to define phase-2 acceptance. This Discovery pass did not run them, so
this section confirms entrypoints, not pass/fail status.

- Basic Rust: `cargo fmt --all -- --check`; `cargo check -p mesh-llm`
- Protocol/gossip/API serialization: `cargo test -p mesh-llm --lib`
- Mixed-version QA: `scripts/qa-control-plane-mixed-version.sh`
- Full supported build path: `just build`
- UI typecheck: `pnpm run typecheck` through `Justfile`; older testing docs
  also mention `npm run test:run` and `npm run typecheck`
- Benchmark corpus: `just bench-corpus`
- Skippy family certification: `just family-certify` and
  `scripts/family-certify.sh`
- OpenAI smoke: `just skippy-openai-smoke` and
  `scripts/skippy-openai-smoke.sh`
- Benchy/OpenAI benchmark helper: `scripts/run-llama-benchy-openai.sh`

Benchmark results in existing docs should be treated as evidence logs unless
they are rerun, dated, and explicitly accepted for the phase-2 scope.

## Branch-Specific Test Focus

For `PD-detach`, prioritize:

- Gossip unit tests around version floor and transitive idle-client filtering.
- Mixed-version public/private mesh flow.
- `/api/status` peer list sanity after gossip filtering.
- Routing/inference smoke through every advertised `/v1/models` entry.

## Existing Docs To Reuse

- Full playbook: `docs/design/TESTING.md`
- Deploy checklist: repo `AGENTS.md`
- Skippy family certification: `docs/skippy/FAMILY_CERTIFY.md`
- CUDA release lanes: `docs/cuda-release-lanes.md`
