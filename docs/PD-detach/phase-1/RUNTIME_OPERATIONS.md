# Runtime Operations

Generated: 2026-05-18

This page is the takeover map for local process lifecycle, ports, logs, and
runtime observation. It references `docs/USAGE.md`, `docs/MESHES.md`, and
`docs/design/TESTING.md`.

## Normal Local Starts

| Goal | Command |
|---|---|
| Join best discovered mesh as a client | `mesh-llm client --auto` |
| Serve configured models | `mesh-llm serve` |
| Serve one model | `mesh-llm serve --model <model-or-path>` |
| Use JSON logs | add `--log-format json` |
| Disable embedded web UI only | add `--headless` |

`--headless` does not mean background or quiet mode. It disables the embedded
web UI while keeping the management API on the console port.

## Ports

| Port | Default | Owner |
|---|---:|---|
| OpenAI-compatible API | `9337` | CLI `--port`, runtime startup |
| Management API / console | `3131` | CLI `--console`, API server |
| Mesh QUIC bind | dynamic unless `--bind-port` | mesh/network runtime |

Code sources:

- `crates/mesh-llm-host-runtime/src/cli/mod.rs`
- `crates/mesh-llm-host-runtime/src/runtime/mod.rs`
- `crates/mesh-llm-host-runtime/src/api/routes/mod.rs`

## Runtime Directory

Per-instance runtime metadata is owned by
`crates/mesh-llm-host-runtime/src/runtime/instance.rs`.

Runtime root resolution:

1. `MESH_LLM_RUNTIME_ROOT`
2. `$XDG_RUNTIME_DIR/mesh-llm/runtime`
3. `$HOME/.mesh-llm/runtime`

Allowed per-instance content:

- `lock`
- `owner.json`
- `logs/`

Native Skippy/llama.cpp logs are under the active runtime instance's
`logs/` directory.

## Stop And Cleanup

Preferred:

- `mesh-llm stop`
- `just stop`

Emergency only:

- `pkill -f mesh-llm`

The clean stop path uses runtime metadata under the runtime root. Prefer it
before process-wide kill commands.

## Observation

Primary local endpoints:

- `GET http://127.0.0.1:3131/api/status`
- `GET http://127.0.0.1:3131/api/runtime`
- `GET http://127.0.0.1:3131/api/runtime/llama`
- `GET http://127.0.0.1:3131/api/runtime/processes`
- `GET http://127.0.0.1:3131/api/runtime/stages`
- `GET http://127.0.0.1:9337/v1/models`

For scripted observation, prefer `--log-format json` and API polling over
backgrounding a TUI process.

## Remote Test Machines

Follow `docs/design/TESTING.md` and repo-level deploy instructions. Credentials
must stay outside tracked files. Never commit IPs, passwords, or SSH details.

