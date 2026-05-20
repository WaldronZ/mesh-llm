# Security And Privacy Handoff

Generated: 2026-05-18

This page points to current code and existing docs for security-sensitive
surfaces. It does not replace a security review.

## Owner Identity And Trust

Code source:

- `crates/mesh-llm-host-runtime/src/crypto/ownership.rs`
- `crates/mesh-llm-host-runtime/src/cli/commands/auth.rs`
- `crates/mesh-llm-host-runtime/src/mesh/gossip.rs`

Existing docs:

- `docs/design/NODE_OWNER_IDENTITY.md`
- `docs/design/IDENTITY_INCIDENT_RESPONSE.md`

Risk: these docs still read as proposals/drafts, while code implements owner
keystore, node signing, verification, trust policy, and revocation flows. Track
as `RISK-006`.

## Management And Owner-Control APIs

Management API runs on the console port. Owner-control endpoints in
`routes/runtime.rs` use loopback caller checks before returning or mutating
control-plane config.

Sensitive routes:

- `/api/runtime/control-bootstrap`
- `/api/runtime/control/get-config`
- `/api/runtime/control/refresh-inventory`
- `/api/runtime/control/apply-config`

## Credentials

Do not commit credentials, IPs, passwords, SSH commands, owner keys, trust
stores with private material, or lab secrets. The repo notes point developers
to private local notes outside the repo for test machine details.

## External Services And Test Resources

Code and docs identify the following external resource categories, but not the
actual handoff credentials or ownership:

- Nostr/public-private mesh discovery:
  `crates/mesh-llm-host-runtime/src/network/nostr.rs`, `docs/MESHES.md`
- Hugging Face/model/layer-package resources:
  `docs/LAYER_PACKAGE_REPOS.md`, `docs/specs/layer-package-repos.md`,
  `crates/mesh-llm-host-runtime/src/models/`
- OTLP telemetry endpoints:
  `docs/plugins/telemetry.md`,
  `crates/mesh-llm-host-runtime/src/plugins/telemetry/mod.rs`
- Test machines, SSH details, passwords, and codesign notes:
  private local notes outside the repo, not tracked docs

Before phase-2 implementation, owner must confirm which services are in scope,
which credentials are required, which can be placed in CI secrets, and which
must remain local/private.

## Telemetry

Existing doc: `docs/plugins/telemetry.md`.

Code source:

- `crates/mesh-llm-host-runtime/src/plugins/telemetry/mod.rs`
- `crates/mesh-llm-host-runtime/src/runtime/survey.rs`

Rules for takeover work:

- Treat telemetry attribute changes as privacy-sensitive.
- Keep prompt/content-bearing data out unless explicitly reviewed.
- If changing attributes or exporters, update telemetry docs and tests.

## Artifact Transfer

Existing docs:

- `docs/LAYER_PACKAGE_REPOS.md`
- `docs/specs/layer-package-repos.md`

Important env var:

- `MESH_LLM_ARTIFACT_TRANSFER`

Treat `open` transfer mode as high-risk. Default docs say transfer is disabled
or restricted unless explicitly enabled.

## Plugins

Code source:

- `crates/mesh-llm-host-runtime/src/plugin/`
- `crates/mesh-llm-host-runtime/src/plugins/`
- `crates/mesh-llm-plugin/src/`

Existing docs:

- `docs/plugins/README.md`
- `docs/plugins/telemetry.md`
- `docs/plugins/flash-moe.md`

Review plugin changes for:

- Command/env injection surface.
- Stapled HTTP exposure.
- MCP tool names and payloads.
- Telemetry or object/blob access.
