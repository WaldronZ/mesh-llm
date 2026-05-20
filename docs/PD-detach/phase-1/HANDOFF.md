# Large Second-Development Handoff

Generated: 2026-05-18

This is the entry point for taking over the branch. It intentionally links to
existing docs instead of copying their content.

## First Pass

1. Read `docs/PD-detach/phase-1/DOCS_AUDIT.md`.
2. Read `docs/PD-detach/phase-1/risk-register.md` before trusting older architecture
   or benchmark docs.
3. Use `docs/README.md`, `docs/CLI.md`, `docs/USAGE.md`, and `docs/MESHES.md`
   for normal user/operator behavior.
4. Use `docs/design/CRATE_DECOMPOSITION.md`, then this package's
   `ARCHITECTURE_CURRENT.md`, for current code ownership.
5. Use `docs/design/TESTING.md` plus `TEST_MATRIX.md` before changing runtime,
   routing, gossip, protocol, plugin, UI, or Skippy behavior.

## Source Of Truth

When code and docs disagree, use code and log the mismatch in
`risk-register.md`.

Primary code anchors:

| Area | Current owner |
|---|---|
| Binary wrapper | `crates/mesh-llm/src/main.rs`, `crates/mesh-llm/src/lib.rs` |
| Host runtime | `crates/mesh-llm-host-runtime/src/` |
| CLI | `crates/mesh-llm-host-runtime/src/cli/` |
| Runtime orchestration | `crates/mesh-llm-host-runtime/src/runtime/` |
| Management API | `crates/mesh-llm-host-runtime/src/api/` |
| Mesh/gossip | `crates/mesh-llm-host-runtime/src/mesh/` |
| Wire protocol | `crates/mesh-llm-protocol/`, `crates/mesh-llm-host-runtime/src/protocol/` |
| Routing/proxying | `crates/mesh-llm-host-runtime/src/network/`, `crates/mesh-llm-routing/` |
| Models and resolution | `crates/mesh-llm-host-runtime/src/models/`, `crates/model-*` |
| Skippy split serving | `crates/skippy-*`, `crates/mesh-llm-host-runtime/src/inference/skippy/` |
| Plugins | `crates/mesh-llm-host-runtime/src/plugin/`, `crates/mesh-llm-plugin/` |
| UI console | `crates/mesh-llm-ui/src/` |
| Build/release scripts | `Justfile`, `scripts/`, `.github/workflows/release.yml` |

## Current Branch Delta

See `FILES.md`. The committed branch delta against local `main` touches:

- `AGENTS.md`
- `crates/mesh-llm-host-runtime/src/mesh/gossip.rs`

The branch-specific behavior is gossip filtering for peers with parseable
versions below `v0.60.0`, plus transitive idle-client filtering. Unknown or
unparseable versions are conservatively allowed. Owner confirmed `v0.60.0` is
the minimum supported node software version. Treat this as a protocol/mesh
behavior change and validate with mixed-version tests.

## Do Not Start With

- Do not use old `src/api`, `src/network`, or `src/inference` paths from stale
  docs as current paths. Use `crates/mesh-llm-host-runtime/src/...`.
- Do not use benchmark docs as current performance claims unless rerun.
- Do not assume `--headless` means "quiet background mode"; it disables the
  embedded web UI but keeps the management API.
- Do not change wire fields, stream IDs, capability semantics, or plugin
  protocol behavior without checking `PROTOCOL_COMPATIBILITY.md`.

## Existing Docs To Reuse

| Need | Reuse |
|---|---|
| User quickstart | `README.md`, `docs/USAGE.md` |
| CLI reference | `docs/CLI.md`, `crates/mesh-llm-host-runtime/src/cli/mod.rs` |
| Mesh join/discovery | `docs/MESHES.md` |
| Testing playbook | `docs/design/TESTING.md` |
| Crate split | `docs/design/CRATE_DECOMPOSITION.md` |
| Metrics | `docs/design/METRICS.md` |
| Multimodal | `docs/design/MULTI_MODAL.md` |
| Plugins | `docs/plugins/README.md`, `docs/plugins/telemetry.md` |
| Skippy split serving | `docs/SKIPPY_SPLITS.md`, `docs/skippy/FAMILY_CERTIFY.md` |
| Layer packages | `docs/LAYER_PACKAGE_REPOS.md`, `docs/specs/layer-package-repos.md` |
