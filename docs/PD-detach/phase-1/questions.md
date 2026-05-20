# Documentation Questions

Generated: 2026-05-18

These are the audit items that need owner confirmation before the docs can be treated as authoritative for a large second-development handoff.

## Origin And Current Code Review Status

These questions were not provided by the user as product requirements. They
were derived during Discovery from code/doc mismatches, stale-looking docs, and
missing source-of-truth decisions.

Code review on 2026-05-18 resolved part of the factual uncertainty:

- `mesh-llm/0`: shared protocol code still carries `/0` and `JsonV0`, but the
  inspected host runtime path advertises/connects mesh `mesh-llm/1` only.
  Owner confirmed `/0` is historical/legacy compatibility code and `mesh-llm/1`
  is the current formal runtime protocol.
- `v0.60.0` peer floor: `PD-detach` code rejects parseable versions below
  `v0.60.0` in direct/transitive ingest and outbound rebroadcast. Unknown or
  unparseable versions are allowed. Owner confirmed `v0.60.0` is the minimum
  supported node software version.
- Architecture source of truth: current code matches
  `docs/PD-detach/phase-1/ARCHITECTURE_CURRENT.md` better than old
  `docs/design/DESIGN.md`; promotion/update policy remains an owner decision.
- Management API schema: server Rust payloads in
  `crates/mesh-llm-host-runtime/src/api/status.rs` are the current
  serialization anchors; UI TypeScript types are consumer mirrors. Generated
  schema/OpenAPI policy remains undecided.
- Test/benchmark commands are identifiable from `Justfile`, scripts, and
  `docs/design/TESTING.md`, but this Discovery did not run them and did not
  certify historical benchmark claims.
- External service categories are known, but actual Nostr/HF/OTLP/test machine
  access and credentials still require human handoff.

Owner confirmation on 2026-05-18 resolved two compatibility decisions:

- `mesh-llm/1` is the current formal runtime protocol.
- `mesh-llm/0` is retained historical/legacy compatibility code.
- `v0.60.0` is the minimum supported node software version for peers that
  advertise a parseable version.

1. Resolved: `mesh-llm/0` is retained historical/legacy compatibility code; `mesh-llm/1` is the current formal runtime protocol. Remaining doc action: update main protocol docs so this is explicit.
2. Should `docs/design/DESIGN.md` be updated to the current decomposed crate layout, or should a new `docs/ARCHITECTURE_CURRENT.md` become the source of truth?
3. Is `docs/SKIPPY.md` still authoritative for anything, or should it be archived now that embedded Skippy and split-serving docs exist elsewhere?
4. Should old plan docs such as `docs/plans/gpu-benchmark-cli.md`, `docs/design/LLAMA_STAGE_INTEGRATION_PLAN.md`, `docs/design/MODEL_ROUTER.md`, and `docs/design/ROUTER_V2.md` move under a history/archive section?
5. What is the current plugin protocol/API source of truth: `docs/plugins/README.md`, `docs/plugins/PLAN.md`, code in `crates/mesh-llm-plugin/`, or host code in `crates/mesh-llm-host-runtime/src/plugin/`?
6. Are `docs/skippy/family/qwen.md`, `docs/skippy/family/qwen-package-runbook.md`, and `docs/skippy/family/qwen-results.md` current customer runbooks, or historical evidence that should be split from active procedure?
7. Which benchmark docs are allowed for current performance claims? Candidates include `docs/BENCHMARKS.md`, `docs/design/ROUTER_BENCHMARKS.md`, `docs/design/PREFIX_AFFINITY_BENCHMARKS.md`, and the Skippy benchmark docs.
8. `docs/skippy/BENCHMARK_TODO.md` and `docs/skippy/LLAMA_BENCHY.md` mention `scripts/openai-smoke.sh`, but the current script found in this audit is `scripts/skippy-openai-smoke.sh`. Is this a rename drift?
9. Is the Jan integration in `docs/design/JAN_MESH_API_INTEGRATION.md` actively maintained from this repo, or is it an external handoff spec only?
10. Should owner identity docs be retitled from proposal/draft to current runbook now that code exists in `crates/mesh-llm-host-runtime/src/crypto/ownership.rs` and `crates/mesh-llm-host-runtime/src/cli/commands/auth.rs`?
11. Where should the management API schema live long term? Current code uses Rust server payloads in `crates/mesh-llm-host-runtime/src/api/status.rs` as serialization anchors, while UI adapters mirror the contract; generated OpenAPI/JSON schema is not yet established.
12. Should static site files `docs/index.html`, `docs/CNAME`, and image assets remain mixed with source documentation, or move to a site-specific directory?
13. Does `docs/specs/context-and-slots-auto.md` describe shipped behavior after `crates/mesh-llm-host-runtime/src/runtime/context_planning.rs`, and should old path references be updated?
14. Is `docs/design/VIRTUAL_LLM.md` current for mesh hooks and virtual LLM behavior, or should it be reconciled with `crates/mesh-llm-host-runtime/src/inference/virtual_llm.rs` and current patch queue state?
15. What is the accepted release-lane source of truth: `docs/cuda-release-lanes.md`, `RELEASE.md`, `install.sh`, `Justfile`, or `.github/workflows/release.yml`?
