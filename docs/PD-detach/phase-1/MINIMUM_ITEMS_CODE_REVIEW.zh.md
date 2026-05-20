# 第二阶段最小事项代码复核

生成日期：2026-05-18

这份文件专门回答 `DISCOVERY_EXIT_REVIEW.zh.md` 中列出的 6 个进入第二阶段前最小事项。

这些事项不是用户预先定下的产品需求，而是 Discovery 审计根据当前代码、旧文档和 `docs/PD-detach/phase-1/` 接管文档之间的冲突推导出来的待确认项。代码可以确认“现在实际怎么写”，但不能替 owner 确认“未来正式承诺什么”。

Owner 确认补充，2026-05-18：

- 当前正式运行以正在使用的 host runtime 协议为准，也就是 `mesh-llm/1`。
- `mesh-llm/0` 属于保留的历史/遗留兼容代码，不作为当前正式运行协议承诺。
- `v0.60.0` 确认为节点软件版本最低门槛。

## 结论总览

| 事项 | 代码能确认的事实 | 仍需人工确认的决策 | 当前状态 |
|---|---|---|---|
| `mesh-llm/0` 支持状态 | shared protocol crate 仍保留 `/0`；host runtime 当前 mesh 连接/监听只看到 `/1` | 已确认：`/0` 是历史/遗留兼容代码，当前正式运行以 `/1` 为准 | pass |
| `v0.60.0` peer floor | `PD-detach` 代码会拒绝可解析且低于 `v0.60.0` 的 peer；未知/不可解析版本保守放行 | 已确认：`v0.60.0` 是节点软件版本最低门槛 | pass |
| 架构文档权威来源 | 当前代码已经是 decomposed workspace；`ARCHITECTURE_CURRENT.md` 更贴近代码现状 | 更新旧 `DESIGN.md`，还是提升 `ARCHITECTURE_CURRENT.md` | partial |
| 管理 API schema | Rust server payload 是当前序列化锚点；UI TS 类型是消费者镜像 | 是否引入生成 schema/OpenAPI，以及维护流程 | partial |
| 测试/benchmark 口径 | 测试脚本和命令入口可以从代码/脚本确认 | 哪些 benchmark 结果可作为验收或对外口径 | partial |
| 外部服务/测试资源 | Nostr/HF/OTLP/test machines 类别可定位 | 账号、凭据、机器、权限、运营边界 | partial |

## 1. `mesh-llm/0` 的真实支持状态

### 代码确认

shared protocol crate 仍保留 legacy `/0` 兼容痕迹：

- `crates/mesh-llm-protocol/src/protocol/v0.rs` 定义 `ALPN_V0 = b"mesh-llm/0"`。
- `crates/mesh-llm-protocol/src/protocol/mod.rs` 仍有 `ControlProtocol::JsonV0`。
- `crates/mesh-llm-protocol/src/protocol/mod.rs` 的 `protocol_from_alpn()` 会把 `ALPN_V0` 判为 `JsonV0`。
- `crates/mesh-llm-protocol/src/protocol/mod.rs` 的 `connect_mesh()` 使用 `with_additional_alpns(vec![ALPN_V0.to_vec()])`。

但当前 host runtime 的运行路径只看到 `/1`：

- `crates/mesh-llm-host-runtime/src/protocol/mod.rs` 定义 `ALPN_V1 = b"mesh-llm/1"`，未定义 `ALPN_V0`。
- `crates/mesh-llm-host-runtime/src/protocol/mod.rs` 的 `ControlProtocol` 只有 `ProtoV1`。
- `crates/mesh-llm-host-runtime/src/protocol/mod.rs` 的 `connect_mesh()` 使用 `endpoint.connect(addr, ALPN_V1)`。
- `crates/mesh-llm-host-runtime/src/mesh/mod.rs` 的 mesh endpoint ALPN 列表包含 `ALPN_V1` 和 Skippy stage ALPN，没有看到 `ALPN_V0`。

### 当前判断

代码能确认的是：`mesh-llm/0` 在 shared protocol crate 中仍存在，但当前 host runtime 运行路径未证明它仍作为 mesh 协议正式服务。

Owner 已确认：当前正式运行以正在使用的 host runtime 协议为准，即 `mesh-llm/1`；`mesh-llm/0` 是保留的历史/遗留兼容代码，不作为当前正式运行协议承诺。

### 后续文档动作

- 主线协议文档应明确：`mesh-llm/1` 是当前正式运行协议。
- `/0` 可以记录为 legacy compatibility code，避免接手者误认为当前 host runtime 仍正式服务 `/0`。

## 2. `PD-detach` 拒绝可解析且低于 `v0.60.0` peer 的边界

### 代码确认

`crates/mesh-llm-host-runtime/src/mesh/gossip.rs` 中已经明确实现版本地板：

- `MIN_REBROADCAST_VERSION_MAJOR = 0`
- `MIN_REBROADCAST_VERSION_MINOR = 60`
- `version_allowed_for_rebroadcast()` 会拒绝 `0.60.0` 以下的可解析版本。
- `add_peer()` 对直接 peer ingest 执行该检查。
- `update_transitive_peer()` 对 transitive peer ingest 执行该检查。
- `collect_announcements()` 对 outbound rebroadcast 执行该检查。
- 单测覆盖 floor、metadata/prerelease、unknown version、direct reject、transitive reject。

一个重要细节：`None`、空字符串和不可解析版本会被保守放行。也就是说，当前代码不是“拒绝所有未知旧节点”，而是拒绝“明确声明低于 `v0.60.0` 的节点”。

### 当前判断

这是 `PD-detach` 分支的代码事实，可信度 high。Owner 已确认：`v0.60.0` 是节点软件版本最低门槛。

### 后续文档动作

- 在协议/运行/发布文档中明确最低节点软件版本是 `v0.60.0`。
- 保留实现细节说明：未知、空、不可解析版本会保守放行。
- 混版 QA 仍应验证低于、等于、高于 `v0.60.0` 的节点行为。

## 3. 第二阶段架构文档权威来源

### 代码确认

当前 repo 已经是 decomposed workspace：

- `Cargo.toml` 列出多个 workspace members。
- `crates/mesh-llm/src/main.rs` 是薄入口。
- `crates/mesh-llm/src/lib.rs` re-export host runtime。
- `crates/mesh-llm-host-runtime/src/lib.rs` 拥有 `VERSION`、`run()`、`run_main()`。
- 实际业务目录位于 `crates/mesh-llm-host-runtime/src/`、`crates/mesh-llm-protocol/`、`crates/mesh-client/`、`crates/skippy-*`、`crates/model-*` 等。

### 当前判断

`docs/PD-detach/phase-1/ARCHITECTURE_CURRENT.md` 比旧 `docs/design/DESIGN.md` 更贴近当前代码现状。第二阶段讨论架构时，应先引用 `ARCHITECTURE_CURRENT.md` 作为 Discovery 期间的事实基线。

### 仍需 owner 确认

- 是把 `ARCHITECTURE_CURRENT.md` 提升为正式架构文档？
- 还是把旧 `docs/design/DESIGN.md` 更新到当前 crate decomposition？
- 哪份文档在主线长期维护？

## 4. 管理 API schema 的权威来源和维护方式

### 代码确认

当前服务端 payload 权威更接近 Rust 代码：

- `crates/mesh-llm-host-runtime/src/api/status.rs` 文件头写明它是 public status/model payloads 和 serialization compatibility anchors。
- `status.rs` 中的 `RuntimeStatusPayload`、`StatusPayload`、`PeerPayload`、`OwnershipPayload`、`MeshModelPayload`、`ModelTargetPayload` 等结构体派生 `Serialize`。
- `crates/mesh-llm-host-runtime/src/api/routes/mod.rs` 和 `routes/*.rs` 持有实际路由分发和 response 构造。
- `crates/mesh-llm-ui/src/lib/api/types.ts` 是 UI 消费端类型镜像，不能视为服务端 schema 权威。

### 当前判断

当前事实基线：Rust server payload 与 route handler 是 API schema 的实际来源；UI TS 类型是消费者合同，需要同步维护。

### 仍需 owner 确认

- 是否要生成 OpenAPI/JSON schema？
- schema diff 是否进入 CI？
- UI 类型是继续手写，还是从 Rust/schema 生成？

## 5. 测试/benchmark 验收口径

### 代码和脚本确认

当前可确认入口包括：

- Rust basic：`cargo fmt --all -- --check`、`cargo check -p mesh-llm`
- protocol/gossip/API serialization：`cargo test -p mesh-llm --lib`
- mixed-version QA：`scripts/qa-control-plane-mixed-version.sh`
- UI：`docs/design/TESTING.md` 提到 `npm run test:run`、`npm run typecheck`；`Justfile` 使用 `pnpm run typecheck`。
- build：`just build`
- benchmark corpus：`just bench-corpus`
- family certification：`just family-certify` / `scripts/family-certify.sh`
- OpenAI smoke：`just skippy-openai-smoke` / `scripts/skippy-openai-smoke.sh`
- benchy：`scripts/run-llama-benchy-openai.sh`

同时发现命名漂移风险：

- `docs/skippy/BENCHMARK_TODO.md` 仍提到 `scripts/openai-smoke.sh`。
- 当前脚本是 `scripts/skippy-openai-smoke.sh`。

### 当前判断

测试命令入口已经可以作为第二阶段初始验收清单，但当前 Discovery 没有运行测试，不能声称测试通过。benchmark 文档中的历史结果只能作为 evidence log，不能直接作为当前验收或对外性能口径。

### 仍需 owner 确认

- 第二阶段准入必须跑哪些测试？
- 哪些 benchmark 必须重跑并打日期？
- 哪些历史 benchmark 只保留为历史证据？

## 6. 外部服务和测试资源交接范围

### 代码和文档确认

可定位的外部/环境依赖类别包括：

- Nostr/public-private mesh discovery：`crates/mesh-llm-host-runtime/src/network/nostr.rs`、`docs/MESHES.md`
- Hugging Face / layer packages / model artifacts：`docs/LAYER_PACKAGE_REPOS.md`、`docs/specs/layer-package-repos.md`、`crates/mesh-llm-host-runtime/src/models/`
- OTLP telemetry：`docs/plugins/telemetry.md`、`crates/mesh-llm-host-runtime/src/plugins/telemetry/mod.rs`
- runtime root / local process metadata：`crates/mesh-llm-host-runtime/src/runtime/instance.rs`
- test machines and credentials：repo notes point to an operator-local private note outside repo，且明确不能提交 credentials。

### 当前判断

代码能确认资源类别和部分配置入口；不能确认实际账号、令牌、机器、权限、运营 SLA 或现网归属。

### 仍需 owner 确认

- Nostr relay / public mesh 服务边界和可用性承诺。
- Hugging Face token、model repo、layer package 发布权限。
- OTLP endpoint、数据保留和隐私审查要求。
- test machines、SSH/codesign/password 交接方式。
- 哪些凭据只允许本地私有保存，哪些可以放入 CI secret。
