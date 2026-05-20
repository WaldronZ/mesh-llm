# PD-detach Phase 3

Status: target architecture design

Purpose: architecture design for heterogeneous Prefill/Decode separation after
Phase 2 closes requirements and scope.

Do not mix Discovery evidence or scope decisions into this directory; reference
`../phase-1/` and `../phase-2/` instead.

This phase is design-only. It does not modify business code, create an OpenSpec
change, deploy remote nodes, or change runtime defaults.

## Documents

| Document | Purpose |
|---|---|
| `TARGET_ARCHITECTURE.zh.md` | Target architecture overview, role boundaries, current-code mapping, and API compatibility. |
| `PD_DATA_FLOW.zh.md` | End-to-end request, prefill, KV handoff, decode, streaming, and failure flows. |
| `KV_HANDOFF_DESIGN.zh.md` | KV handoff candidate options, metadata contract, costs, risks, MVP recommendation, and spikes. |
| `ROLE_AND_SCHEDULING.zh.md` | Manual role binding, worker choice, health, capacity, and future scheduling expansion. |
| `API_AND_PROTOCOL.zh.md` | External API compatibility, internal PD protocol sketch, state machine, errors, and compatibility. |
| `DEPLOYMENT_TOPOLOGY.zh.md` | Two PGX + one Mac Studio topology, process roles, ports, model path variables, and rollback principles. |
| `VALIDATION_PLAN.zh.md` | Correctness, performance, network/KV transfer, regression, and minimum acceptance plan. |
| `ADR/` | Architecture decision records for coordinator, KV handoff, Skippy reuse, and API compatibility. |
| `PHASE_3_EXIT_REVIEW.zh.md` | Exit review for entering OpenSpec propose. |
