# Configuration Handoff

Generated: 2026-05-18

Configuration source of truth is
`crates/mesh-llm-host-runtime/src/plugin/config.rs`, with user guidance in
`docs/USAGE.md`, `docs/plugins/README.md`, and `docs/plugins/telemetry.md`.

## Config Path

Resolution order:

1. CLI `--config <path>`
2. `MESH_LLM_CONFIG`
3. `~/.mesh-llm/config.toml`

## Top-Level Schema

| Section | Important fields | Code source |
|---|---|---|
| `version` | Optional; when set must be `1`. | `MeshConfig.version` |
| `[gpu]` | `assignment`, `parallel` | `GpuConfig` |
| `[owner_control]` | `bind`, `advertise_addr` | `OwnerControlConfig` |
| `[telemetry]` | `enabled`, `service_name`, `endpoint`, `headers`, `export_interval_secs`, `queue_size`, `metrics.endpoint` | `TelemetryConfig` |
| `[[models]]` | `model`, `mmproj`, `ctx_size`, `gpu_id`, `parallel`, cache/batch/flash fields | `ModelConfigEntry` |
| `[[plugin]]` | `name`, `enabled`, `command`, `args`, `url` | `PluginConfigEntry` |

## Validation Rules To Remember

- `gpu.parallel` and per-model `parallel` must be at least `1`.
- `gpu.assignment = "auto"` forbids per-model `gpu_id`.
- `gpu.assignment = "pinned"` requires every model to set non-empty `gpu_id`.
- `models[].batch` and `models[].ubatch` cannot be `0`.
- `telemetry.prompt_shape_metrics` is not supported and must remain false.
- Built-in plugins such as `blackboard`, `blobstore`, `openai-endpoint`, and
  `telemetry` have restricted config fields.

## Plugin Defaults

Important defaults in code:

- `blackboard`: enabled unless disabled.
- `blobstore`: enabled unless disabled.
- `telemetry`: enabled unless disabled.
- `openai-endpoint`: disabled unless configured.
- `flash-moe`: enabled only when configured and valid.

Read `docs/plugins/README.md`, `docs/plugins/flash-moe.md`, and
`docs/plugins/telemetry.md` for behavior. Use code for allowed config fields.

## Important Environment Variables

| Env var | Purpose | Source |
|---|---|---|
| `MESH_LLM_CONFIG` | Config file override. | `plugin/config.rs` |
| `MESH_LLM_RUNTIME_ROOT` | Runtime instance root override. | `runtime/instance.rs` |
| `XDG_RUNTIME_DIR` | Runtime root fallback on Linux/systemd. | `runtime/instance.rs` |
| `MESH_LLM_ARTIFACT_TRANSFER` | Layer package/artifact transfer policy. | `docs/specs/layer-package-repos.md`, `models/artifact_transfer.rs` |
| `MESH_LLM_OPENAI_ENDPOINT_URL` | OpenAI endpoint plugin URL fallback. | `docs/USAGE.md`, plugin code |
| `OTEL_EXPORTER_OTLP_METRICS_ENDPOINT` | Telemetry metrics endpoint. | `docs/plugins/telemetry.md` |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | Telemetry OTLP endpoint fallback. | `docs/plugins/telemetry.md` |
| `MESH_TOKIO_STACK_SIZE` | Tokio worker stack override. | `crates/mesh-llm/src/main.rs` |

## Existing Docs To Reuse

- User config examples: `docs/USAGE.md`
- Plugin model: `docs/plugins/README.md`
- Telemetry config: `docs/plugins/telemetry.md`
- Layer package transfer modes: `docs/LAYER_PACKAGE_REPOS.md` and
  `docs/specs/layer-package-repos.md`

