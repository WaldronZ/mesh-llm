# Discovery Exit Review

生成日期：2026-05-18

目标：判断大型二开第一阶段 Discovery 是否可以结束，是否可以进入第二阶段需求/范围定义。

结论先行：**Conditional Go**。  
理由：第一阶段已经形成足够的项目目标、架构、运行、配置、协议/API、测试、安全和风险梳理，可以进入第二阶段的需求/范围定义；但不能直接进入实现阶段。兼容性边界和协议承诺已由 owner 确认，进入第二阶段前还需要把该口径落地主线文档，并确认测试/benchmark 口径和文档权威来源。

补充复核：`docs/PD-detach/phase-1/MINIMUM_ITEMS_CODE_REVIEW.zh.md` 已基于当前代码把 6 个最小事项拆成“代码事实”和“owner 决策”。这些事项来自 Discovery 对代码/文档冲突的归纳，不是用户预先指定的产品需求。Owner 已确认其中两个兼容性决策：`mesh-llm/1` 是当前正式运行协议，`mesh-llm/0` 是历史/遗留兼容代码；`v0.60.0` 是节点软件版本最低门槛。

## 检查项结论

| # | 检查项 | 结论 | 证据路径 | 说明 |
|---:|---|---|---|---|
| 1 | 项目目标和主要使用场景是否已说明清楚 | pass | `docs/PD-detach/phase-1/项目现状简报.zh.md`、`docs/PD-detach/phase-1/HANDOFF.md`、`docs/USAGE.md`、`docs/MESHES.md` | 已说明项目用于组织多机/GPU mesh，提供 OpenAI-compatible API、管理 API、Web console、模型路由、插件和 Skippy 分层推理。 |
| 2 | 核心模块和目录职责是否已说明清楚 | pass | `docs/PD-detach/phase-1/ARCHITECTURE_CURRENT.md`、`docs/PD-detach/phase-1/项目现状简报.zh.md`、`Cargo.toml`、`crates/mesh-llm-host-runtime/src/` | 已明确 `mesh-llm` wrapper、host runtime、protocol、client/API、plugin、UI、Skippy、model crates 等职责。 |
| 3 | 入口、启动流程、构建命令是否已确认 | partial | `docs/PD-detach/phase-1/ARCHITECTURE_CURRENT.md`、`docs/PD-detach/phase-1/RUNTIME_OPERATIONS.md`、`crates/mesh-llm/src/main.rs`、`crates/mesh-llm-host-runtime/src/cli/mod.rs`、`Justfile`、`AGENTS.md` | 入口和启动流程已确认；构建命令来源已定位到 `Justfile`/`AGENTS.md`，但本轮未实际执行构建。 |
| 4 | 测试命令和当前测试状态是否已确认 | partial | `docs/PD-detach/phase-1/TEST_MATRIX.md`、`docs/design/TESTING.md`、`AGENTS.md`、`scripts/qa-control-plane-mixed-version.sh` | 测试命令和测试矩阵已梳理；当前测试状态未确认，因为本阶段仅做文档 Discovery，未运行测试。 |
| 5 | 配置文件、环境变量、默认值来源是否已梳理 | pass | `docs/PD-detach/phase-1/CONFIGURATION.md`、`crates/mesh-llm-host-runtime/src/plugin/config.rs`、`crates/mesh-llm-host-runtime/src/cli/mod.rs`、`crates/mesh-llm-host-runtime/src/runtime/instance.rs` | 已梳理 `--config`、`MESH_LLM_CONFIG`、默认 config、runtime root、端口默认值、telemetry/artifact env 等。 |
| 6 | 协议/API/CLI 表面是否已梳理 | pass | `docs/PD-detach/phase-1/PROTOCOL_COMPATIBILITY.md`、`docs/PD-detach/phase-1/API_REFERENCE.md`、`docs/CLI.md`、`crates/mesh-llm-protocol/src/protocol/mod.rs`、`crates/mesh-llm-host-runtime/src/api/routes/mod.rs`、`crates/mesh-llm-host-runtime/src/cli/mod.rs` | 协议 ALPN/stream、API route map、主要 CLI surface 已有入口和证据路径。 |
| 7 | 运行时依赖、外部服务、模型/数据路径是否已梳理 | partial | `docs/PD-detach/phase-1/RUNTIME_OPERATIONS.md`、`docs/PD-detach/phase-1/CONFIGURATION.md`、`docs/PD-detach/phase-1/SECURITY_AND_PRIVACY.md`、`docs/LAYER_PACKAGE_REPOS.md`、`docs/SKIPPY_SPLITS.md` | 本地 runtime root、模型配置、artifact transfer、Skippy/layer package 已梳理；外部服务如 Nostr/HF/OTLP/test machines 的实际权限和运维细节仍需 owner 交接。 |
| 8 | 安全和隐私风险是否已列出 | pass | `docs/PD-detach/phase-1/SECURITY_AND_PRIVACY.md`、`docs/PD-detach/phase-1/risk-register.md`、`crates/mesh-llm-host-runtime/src/crypto/ownership.rs`、`crates/mesh-llm-host-runtime/src/plugins/telemetry/mod.rs` | 已列出 owner identity、管理 API、telemetry、artifact transfer、插件、凭据等风险。 |
| 9 | 现有文档与代码不一致处是否已列出 | pass | `docs/PD-detach/phase-1/risk-register.md`、`docs/PD-detach/phase-1/DOCS_AUDIT.md`、`docs/PD-detach/phase-1/EVIDENCE_MATRIX.zh.md` | 已记录协议 `/0` vs `/1`、旧 crate 路径、Skippy 旧计划、benchmark 命令漂移等。 |
| 10 | 最高风险和未知问题是否已进入 risk-register/questions | pass | `docs/PD-detach/phase-1/risk-register.md`、`docs/PD-detach/phase-1/questions.md` | high/medium/low 风险和 unknown 项已进入对应文档。 |
| 11 | 是否存在阻塞第二阶段需求定义的问题 | partial | `docs/PD-detach/phase-1/questions.md`、`docs/PD-detach/phase-1/risk-register.md`、`docs/PD-detach/phase-1/MINIMUM_ITEMS_CODE_REVIEW.zh.md` | 不阻塞开始需求/范围定义；兼容性决策已确认，但主线文档落地、benchmark 口径、API schema 维护方式和外部资源交接仍会影响需求定稿和实现排期。 |
| 12 | 是否只修改了 `docs/PD-detach/phase-1/`，没有改业务代码 | pass | `git status --short` 显示 `?? docs/PD-detach/phase-1/` | 当前工作区只显示 `docs/PD-detach/phase-1/` 未跟踪目录；没有业务代码变更。 |

## Go / No-Go

**Conditional Go**

可以结束第一阶段 Discovery，进入第二阶段“需求/范围定义”。  
不建议直接进入实现阶段，也不建议在未确认关键问题前冻结需求范围。

## Conditional Go 的最小补齐事项

进入第二阶段前，必须先补齐以下最小事项。代码复核已补齐能确认的事实，其中兼容性决策 1-2 已由 owner 确认；事项 3-6 仍需治理/交接口径确认：

1. `mesh-llm/0`：已确认当前正式运行以 `mesh-llm/1` 为准；shared protocol crate 中保留的 `/0` 和 `JsonV0` 是历史/遗留兼容代码。  
   证据：`docs/PD-detach/phase-1/MINIMUM_ITEMS_CODE_REVIEW.zh.md`、`docs/PD-detach/phase-1/PROTOCOL_COMPATIBILITY.md`、`docs/PD-detach/phase-1/risk-register.md`、`crates/mesh-llm-protocol/src/protocol/v0.rs`、`crates/mesh-llm-host-runtime/src/protocol/mod.rs`
2. `v0.60.0` peer floor：已确认 `v0.60.0` 是节点软件版本最低门槛；代码会拒绝可解析且低于 `v0.60.0` 的 peer，并保守放行未知/不可解析版本。  
   证据：`docs/PD-detach/phase-1/MINIMUM_ITEMS_CODE_REVIEW.zh.md`、`docs/PD-detach/phase-1/FILES.md`、`docs/PD-detach/phase-1/risk-register.md`、`crates/mesh-llm-host-runtime/src/mesh/gossip.rs`
3. 架构文档：代码确认当前 repo 是 decomposed workspace，`ARCHITECTURE_CURRENT.md` 更贴近现状。仍需确认主线权威文档是更新旧 `DESIGN.md`，还是提升 `ARCHITECTURE_CURRENT.md`。  
   证据：`docs/PD-detach/phase-1/MINIMUM_ITEMS_CODE_REVIEW.zh.md`、`docs/PD-detach/phase-1/ARCHITECTURE_CURRENT.md`、`docs/PD-detach/phase-1/DOCS_AUDIT.md`、`Cargo.toml`
4. 管理 API schema：代码确认 Rust server payload 和 route handler 是当前实际 schema 来源，UI TS 类型是消费者镜像。仍需确认是否引入生成 schema/OpenAPI 及维护流程。  
   证据：`docs/PD-detach/phase-1/MINIMUM_ITEMS_CODE_REVIEW.zh.md`、`docs/PD-detach/phase-1/API_REFERENCE.md`、`crates/mesh-llm-host-runtime/src/api/status.rs`、`crates/mesh-llm-ui/src/lib/api/types.ts`
5. 测试/benchmark：代码和脚本确认了准入命令入口；Discovery 未运行测试，也未认证历史 benchmark。仍需确认哪些测试是第二阶段准入，哪些 benchmark 必须重跑。  
   证据：`docs/PD-detach/phase-1/MINIMUM_ITEMS_CODE_REVIEW.zh.md`、`docs/PD-detach/phase-1/TEST_MATRIX.md`、`docs/PD-detach/phase-1/risk-register.md`、`docs/design/TESTING.md`、`Justfile`
6. 外部服务和测试资源：代码和文档确认 Nostr/HF/OTLP/test machines 类别；实际账号、凭据、权限和运维范围仍需人工交接。  
   证据：`docs/PD-detach/phase-1/MINIMUM_ITEMS_CODE_REVIEW.zh.md`、`docs/PD-detach/phase-1/SECURITY_AND_PRIVACY.md`、`docs/PD-detach/phase-1/questions.md`

## 第二阶段优先要澄清的 10 个问题

1. 主线协议文档是否已明确 `mesh-llm/1` 是当前正式运行协议、`mesh-llm/0` 是历史/遗留兼容代码？  
   证据：`docs/PD-detach/phase-1/questions.md`、`docs/PD-detach/phase-1/PROTOCOL_COMPATIBILITY.md`、`crates/mesh-llm-protocol/src/protocol/v0.rs`
2. 主线运行/发布文档是否已明确 `v0.60.0` 是节点软件版本最低门槛，并说明未知/不可解析版本的保守放行行为？  
   证据：`docs/PD-detach/phase-1/risk-register.md`、`crates/mesh-llm-host-runtime/src/mesh/gossip.rs`
3. 第二阶段是否要继续推进当前 crate decomposition，还是先冻结边界做功能二开？  
   证据：`docs/design/CRATE_DECOMPOSITION.md`、`docs/PD-detach/phase-1/ARCHITECTURE_CURRENT.md`
4. 管理 API schema 是否继续以 Rust payload 为当前权威，还是需要生成 OpenAPI/JSON schema？  
   证据：`docs/PD-detach/phase-1/API_REFERENCE.md`、`crates/mesh-llm-host-runtime/src/api/status.rs`
5. UI TS 类型是否需要从 Rust payload 生成，还是继续手工维护？  
   证据：`docs/PD-detach/phase-1/UI_ARCHITECTURE.md`、`crates/mesh-llm-ui/src/lib/api/types.ts`
6. owner identity 是可选、推荐、默认启用，还是未来强制？  
   证据：`docs/PD-detach/phase-1/SECURITY_AND_PRIVACY.md`、`crates/mesh-llm-host-runtime/src/crypto/ownership.rs`
7. plugin v2 的稳定合同以代码、`docs/plugins/README.md` 还是 `docs/plugins/PLAN.md` 为准？  
   证据：`docs/PD-detach/phase-1/questions.md`、`docs/PD-detach/phase-1/risk-register.md`
8. Skippy 相关哪些文档是当前 runbook，哪些必须归档为历史计划？  
   证据：`docs/PD-detach/phase-1/DOCS_AUDIT.md`、`docs/PD-detach/phase-1/risk-register.md`
9. 哪些 benchmark 数据可以作为当前验收/对外口径，哪些必须重新跑？  
   证据：`docs/PD-detach/phase-1/risk-register.md`、`docs/PD-detach/phase-1/EVIDENCE_MATRIX.zh.md`
10. `docs/PD-detach/phase-1/` 文档包是分支临时资料，还是要迁移为主线正式接管文档？  
    证据：`docs/PD-detach/phase-1/DOCS_MAINTENANCE.md`、`docs/PD-detach/phase-1/README.md`

## 最终判断

Discovery 阶段的目标已经基本完成：项目现状、关键模块、运行/API/协议/配置/测试/安全风险和文档漂移均已形成可追溯文档。

进入第二阶段的建议方式：

- 可以开始需求/范围定义。
- 需求定义必须引用 `risk-register.md` 和 `questions.md`。
- 在主线兼容性文档落地、API schema 维护方式、测试口径确认前，不应承诺实现排期或进行大范围代码改造。
