# Protocol Compatibility

Generated: 2026-05-18

Use code as the source of truth for protocol constants and protobuf fields.
The most relevant files are:

- `crates/mesh-llm-protocol/src/protocol/mod.rs`
- `crates/mesh-llm-protocol/src/protocol/v0.rs`
- `crates/mesh-llm-protocol/proto/node.proto`
- `crates/mesh-llm-host-runtime/src/protocol/mod.rs`
- `crates/mesh-llm-host-runtime/src/protocol/convert.rs`

Owner confirmation, 2026-05-18:

- Current formal runtime protocol follows the host runtime path in active use:
  `mesh-llm/1`.
- `mesh-llm/0` is retained historical/legacy compatibility code, not the
  current formal runtime protocol commitment.
- `v0.60.0` is the minimum supported node software version for peers that
  advertise a parseable version.

## ALPNs

| ALPN | Code source | Notes |
|---|---|---|
| `mesh-llm/1` | `ALPN_V1` in `crates/mesh-llm-protocol/src/protocol/mod.rs` and `crates/mesh-llm-host-runtime/src/protocol/mod.rs` | Current formal host-runtime mesh protocol. Host endpoint setup advertises `ALPN_V1` plus Skippy stage ALPN in `crates/mesh-llm-host-runtime/src/mesh/mod.rs`. |
| `mesh-llm/0` | `ALPN_V0` in `crates/mesh-llm-protocol/src/protocol/v0.rs` | Retained historical/legacy JSON compatibility code in the shared protocol crate, including `ControlProtocol::JsonV0` and additional ALPN negotiation. Current host runtime protocol module does not define/advertise `/0` in the inspected branch. |
| `mesh-llm-control/1` | `ALPN_CONTROL_V1` | Owner/config control plane. |

Documentation note: if `mesh-llm/0` is mentioned in current docs, describe it
as legacy compatibility code retained in shared protocol code. Do not describe
it as the current formal host-runtime protocol.

## Stream IDs

| Stream | ID | Purpose |
|---|---:|---|
| `STREAM_GOSSIP` | `0x01` | Peer announcement exchange. |
| `STREAM_TUNNEL` | `0x02` | QUIC tunnel data. |
| `STREAM_TUNNEL_MAP` | `0x03` | Tunnel map/control. |
| `STREAM_TUNNEL_HTTP` | `0x04` | HTTP tunnel path. |
| `STREAM_ROUTE_REQUEST` | `0x05` | Route table request/response. |
| `STREAM_PEER_DOWN` | `0x06` | Peer-down notice. |
| `STREAM_PEER_LEAVING` | `0x07` | Peer leaving notice. |
| `STREAM_PLUGIN_CHANNEL` | `0x08` | Plugin channel. |
| `STREAM_PLUGIN_BULK_TRANSFER` | `0x09` | Plugin bulk transfer. |
| `STREAM_CONFIG_SUBSCRIBE` | `0x0b` | Reserved on mesh protocol; config now uses `mesh-llm-control/1`. |
| `STREAM_CONFIG_PUSH` | `0x0c` | Reserved on mesh protocol; config now uses `mesh-llm-control/1`. |
| `STREAM_SUBPROTOCOL` | `0x0d` | Advertised subprotocol opening, including Skippy/artifact lanes. |

## Protobuf Rules

- `PeerAnnouncement` is the main gossip payload. Add fields rather than
  repurposing existing fields.
- Capability fields are consumed by routing, API, and UI. Treat semantic
  changes as mesh-wide behavior changes.
- Control frames include `gen`; mismatched generation is rejected.
- Current generation is `NODE_PROTOCOL_GENERATION = 1`.

## Branch-Specific Compatibility Note

`PD-detach` changes `crates/mesh-llm-host-runtime/src/mesh/gossip.rs` to reject
peers that advertise a parseable version below `v0.60.0` from
direct/transitive ingest and outbound rebroadcast. Unknown, empty, or
unparseable versions are conservatively allowed by
`version_allowed_for_rebroadcast()`.

Owner confirmed that `v0.60.0` is the minimum supported node software version
for peers that advertise a parseable version. Validate with
`scripts/qa-control-plane-mixed-version.sh` before merging or deploying.

## Required Validation For Protocol Work

Use `docs/design/TESTING.md` plus `TEST_MATRIX.md`. At minimum:

- `cargo fmt --all -- --check`
- `cargo check -p mesh-llm`
- For protocol/gossip/API serialization changes: `cargo test -p mesh-llm --lib`
- Mixed-version flow: `scripts/qa-control-plane-mixed-version.sh` when the
  change affects gossip, routing, owner-control, or compatibility.
