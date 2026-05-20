# PD-detach Takeover Package

Generated: 2026-05-18

This directory is a branch-local takeover package for large second-development
work. It builds on `docs/PD-detach/phase-1/DOCS_AUDIT.md` and the existing repo docs,
while treating code as the source of truth where documentation has drifted.

Read in this order:

1. `HANDOFF.md`: first-pass orientation and subsystem map.
2. `ARCHITECTURE_CURRENT.md`: current crate/runtime architecture.
3. `RUNTIME_OPERATIONS.md`: local run, process, port, and log ownership.
4. `CONFIGURATION.md`: config schema, env vars, and plugin config entrypoints.
5. `API_REFERENCE.md`: management and OpenAI-compatible API route map.
6. `PROTOCOL_COMPATIBILITY.md`: ALPNs, stream IDs, protobuf compatibility.
7. `SECURITY_AND_PRIVACY.md`: owner identity, trust, telemetry, artifacts.
8. `TEST_MATRIX.md`: validation by touched area.
9. `UI_ARCHITECTURE.md`: console routes, API adapters, UI test surfaces.
10. `DOCS_MAINTENANCE.md`: how to keep docs honest during takeover.
11. `risk-register.md`: code/docs mismatches and unresolved documentation risk.
12. `MINIMUM_ITEMS_CODE_REVIEW.zh.md`: code review of phase-2 minimum items.

Related audit artifacts:

- `DOCS_AUDIT.md`
- `questions.md`
- `FILES.md`
- `EVIDENCE_MATRIX.zh.md`
- `DISCOVERY_EXIT_REVIEW.zh.md`

Source-of-truth rule:

1. Current code and scripts win.
2. Current docs are reused by reference.
3. Stale or contradictory docs are listed in `risk-register.md`.
