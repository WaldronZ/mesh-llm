# Branch File Record: PD-detach

Generated: 2026-05-18

## Branch Snapshot

- Branch: `PD-detach`
- Upstream: `origin/PD-detach`
- Head: `4b66e89e` (`mesh: drop peers below v0.60.0 from gossip ingest and re-broadcast (#576)`)
- Comparison used for committed branch files: `main...HEAD`
- Merge base with local `main`: `0e268d4f19641eef4b2f8f6ba5797589131dd0c9`

Note: at audit time, `origin/main` and `origin/PD-detach` both pointed at
`4b66e89e`. This record therefore captures the files that differ from the
local `main` branch, not files that differ from `origin/main`.

## Committed Files Related To This Branch

| File | Branch relevance |
|---|---|
| `AGENTS.md` | Adds local run guidance for `mesh-llm client --auto`, `--log-format json`, `--headless`, `--no-console`, and avoiding quiet background TUI launches through `nohup`. |
| `crates/mesh-llm-host-runtime/src/mesh/gossip.rs` | Implements the owner-confirmed `v0.60.0` minimum node software version for peers that advertise parseable versions, rejects older peers at direct and transitive ingest, omits old-version peers from outbound rebroadcast, conservatively allows unknown/unparseable versions, filters idle transitive clients with no useful identity/reachability/demand signal, and adds tests for those behaviors. |

## Branch-Local Documentation Files

The following files are branch-local takeover documentation. They are not part
of the committed `PD-detach` branch delta shown by
`git diff --name-status main...HEAD` unless they are committed separately:

- `docs/PD-detach/README.md`
- `docs/PD-detach/phase-1/API_REFERENCE.md`
- `docs/PD-detach/phase-1/ARCHITECTURE_CURRENT.md`
- `docs/PD-detach/phase-1/CONFIGURATION.md`
- `docs/PD-detach/phase-1/DISCOVERY_EXIT_REVIEW.zh.md`
- `docs/PD-detach/phase-1/DOCS_MAINTENANCE.md`
- `docs/PD-detach/phase-1/DOCS_AUDIT.md`
- `docs/PD-detach/phase-1/EVIDENCE_MATRIX.zh.md`
- `docs/PD-detach/phase-1/questions.md`
- `docs/PD-detach/phase-1/FILES.md`
- `docs/PD-detach/phase-1/HANDOFF.md`
- `docs/PD-detach/phase-1/MINIMUM_ITEMS_CODE_REVIEW.zh.md`
- `docs/PD-detach/phase-1/PROTOCOL_COMPATIBILITY.md`
- `docs/PD-detach/phase-1/README.md`
- `docs/PD-detach/phase-1/RUNTIME_OPERATIONS.md`
- `docs/PD-detach/phase-1/SECURITY_AND_PRIVACY.md`
- `docs/PD-detach/phase-1/TEST_MATRIX.md`
- `docs/PD-detach/phase-1/UI_ARCHITECTURE.md`
- `docs/PD-detach/phase-1/risk-register.md`
- `docs/PD-detach/phase-1/中文审阅摘要.md`
- `docs/PD-detach/phase-1/项目现状简报.zh.md`
- `docs/PD-detach/phase-2/README.md`
- `docs/PD-detach/phase-3/README.md`

## Commands Used

- `git status --short --branch`
- `git branch --show-current`
- `git diff --name-status main...HEAD`
- `git diff --stat main...HEAD`
- `git merge-base main HEAD`
