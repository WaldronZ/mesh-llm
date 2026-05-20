# Docs Maintenance

Generated: 2026-05-18

Use this policy while turning branch-local takeover docs into durable project
docs.

## Source Hierarchy

1. Code, tests, and scripts.
2. Current docs explicitly backed by code paths.
3. Branch-local takeover docs in `docs/PD-detach/phase-1/`.
4. Older design/plan docs, only after checking `risk-register.md`.

## How To Update Docs

- Prefer links and short summaries over copied content.
- Cite code paths for operational claims.
- If a doc describes a shipped behavior, include the owning code path.
- If a doc is a plan, label it as a plan or move it to history/archive.
- If code and docs disagree, update code-backed docs and add or close a risk
  entry in `risk-register.md`.

## Freshness Labels

Use the same labels as `DOCS_AUDIT.md`:

- `current`
- `likely-stale`
- `unknown`

`current` does not mean perfect. It means safe enough for its stated purpose.

## Promotion Path

When the branch-local package is ready to become mainline docs:

1. Move durable docs from `docs/PD-detach/phase-1/` to stable `docs/` paths.
2. Update relative links.
3. Resolve or intentionally carry open risk entries.
4. Keep `risk-register.md` or replace it with a maintained project risk log.
5. Archive `FILES.md` if it is only branch-delta metadata.

## Suggested Durable Targets

| Branch-local file | Durable target |
|---|---|
| `HANDOFF.md` | `docs/HANDOFF.md` |
| `ARCHITECTURE_CURRENT.md` | `docs/ARCHITECTURE_CURRENT.md` |
| `PROTOCOL_COMPATIBILITY.md` | `docs/PROTOCOL_COMPATIBILITY.md` |
| `RUNTIME_OPERATIONS.md` | `docs/RUNTIME_OPERATIONS.md` |
| `CONFIGURATION.md` | `docs/CONFIGURATION.md` |
| `API_REFERENCE.md` | `docs/API_REFERENCE.md` |
| `TEST_MATRIX.md` | `docs/TEST_MATRIX.md` |
| `SECURITY_AND_PRIVACY.md` | `docs/SECURITY_AND_PRIVACY.md` |
| `UI_ARCHITECTURE.md` | `docs/UI_ARCHITECTURE.md` |
| `risk-register.md` | `docs/risk-register.md` or kept branch-local |

