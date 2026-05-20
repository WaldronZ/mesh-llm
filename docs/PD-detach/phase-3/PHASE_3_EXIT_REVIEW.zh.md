# Phase 3 Exit Review

生成日期：2026-05-19

目标：判断第三阶段“异构 Prefill/Decode 分离目标架构设计”是否可以结束，是否可以进入 OpenSpec propose 阶段。

结论先行：**Conditional Go**。

理由：目标架构、数据流、KV handoff 候选和 MVP 推荐、角色调度、API/协议兼容、部署拓扑、验证计划和 ADR 已形成完整架构输入，可以进入 OpenSpec propose；但第一份 OpenSpec change 建议明确为 `pd-kv-handoff-spike`，先验证 PGX native KV 到 Mac decode 的跨 backend 可行性，再推进完整 MVP 实现 proposal。

## 检查项结论

| # | 检查项 | 结论 | 证据路径 | 说明 |
|---:|---|---|---|---|
| 1 | 目标架构总览是否明确 | pass | `docs/PD-detach/phase-3/TARGET_ARCHITECTURE.zh.md` | 已定义 Mac Coordinator/Decode + PGX Prefill 的目标架构，并映射到当前 mesh-llm 模块。 |
| 2 | Prefill worker / Decode worker / Coordinator 职责是否明确 | pass | `TARGET_ARCHITECTURE.zh.md`、`ROLE_AND_SCHEDULING.zh.md` | 三类角色职责、MVP 绑定、health/capacity/backpressure 已定义。 |
| 3 | 与现有 mesh-llm 模块映射是否明确 | pass | `TARGET_ARCHITECTURE.zh.md` 第 5 节 | 已映射到 OpenAI ingress/proxy、mesh/gossip、Skippy runtime、status API、config 入口。 |
| 4 | 外部 OpenAI-compatible API 兼容方式是否明确 | pass | `API_AND_PROTOCOL.zh.md`、ADR-004 | 明确不替换 `/v1/*`，PD 是内部 execution lane，默认关闭并保留 normal path。 |
| 5 | 请求数据流和时序是否明确 | pass | `PD_DATA_FLOW.zh.md` | 已覆盖入口、tokenization、prefill、KV handoff、decode、streaming。 |
| 6 | 正常路径和失败路径是否明确 | pass | `PD_DATA_FLOW.zh.md`、`API_AND_PROTOCOL.zh.md` | 已定义首 token 前 fallback，首 token 后不能透明 fallback。 |
| 7 | KV handoff 方案是否明确 | partial | `KV_HANDOFF_DESIGN.zh.md`、ADR-002 | 已选 Native KV Page Handoff 作为 spike-gated MVP 目标；但跨 PGX/Mac 可行性必须 spike。 |
| 8 | KV metadata、dtype、layout、一致性约束是否明确 | pass | `KV_HANDOFF_DESIGN.zh.md` | 已列出 manifest 字段、16-bit/f16 KV 口径、layout、artifact/tokenizer/position/ABI 约束。 |
| 9 | 网络传输成本和风险是否列出 | pass | `KV_HANDOFF_DESIGN.zh.md`、`VALIDATION_PLAN.zh.md` | 已给出成本公式、必须实测指标和 high-risk 项。 |
| 10 | 调度、health、capacity、backpressure 是否明确 | pass | `ROLE_AND_SCHEDULING.zh.md` | MVP 单 request、单 decode worker、手动 placement、busy fallback 已定义。 |
| 11 | 内部协议、错误码、状态机、版本兼容是否明确 | pass | `API_AND_PROTOCOL.zh.md` | 已提出 `pd-handoff/1` 内部协议草案、错误码和兼容策略。 |
| 12 | 部署拓扑是否明确且未泄露 credentials | pass | `DEPLOYMENT_TOPOLOGY.zh.md` | 只记录 env 变量名和角色，不复制密码/token/私有连接细节。 |
| 13 | 验证计划是否覆盖正确性、性能、网络/KV、回归 | pass | `VALIDATION_PLAN.zh.md` | 已定义分阶段验证、baseline、failure/fallback 和最小验收清单。 |
| 14 | ADR 是否齐全 | pass | `docs/PD-detach/phase-3/ADR/` | 已生成集中式 Coordinator、KV handoff、Skippy 复用、外部 API 兼容四份 ADR。 |
| 15 | 是否标记 spike/prototype 问题 | pass | `TARGET_ARCHITECTURE.zh.md`、`KV_HANDOFF_DESIGN.zh.md`、`VALIDATION_PLAN.zh.md` | KV export/import、network bytes/latency、post-token streaming failure 等已标记。 |
| 16 | 是否未修改业务代码、未启动部署、未创建 OpenSpec | pass | `git status` 本地检查 | 本阶段只新增/更新 `docs/PD-detach/phase-3/` 文档。 |

## Go / No-Go

**Conditional Go**

可以进入 OpenSpec propose，但建议拆成两级：

1. **OpenSpec Spike Proposal**：创建 `pd-kv-handoff-spike`，先定义并验证 KV export/import、manifest、PGX->Mac handoff、correctness baseline。
2. **OpenSpec MVP Implementation Proposal**：只有 spike 证明可行后，再提出完整 PD MVP 实现。

## 进入 OpenSpec 前的推荐决策

1. 第一份 OpenSpec change：`pd-kv-handoff-spike`，不是完整 MVP implementation。
2. 内部协议边界：逻辑上定义 `pd-handoff/1`；spike 阶段可以复用 Skippy transport / KV export-import 代码，但不要把 PD 语义永久混进 `skippy-stage/1`。
3. Artifact identity：MVP 最小规则使用模型文件内容 `sha256`，Mac/PGX 的 `sha256` 必须一致；模型名和本地路径都不能单独作为一致性依据。
4. Tokenizer / chat template identity：使用 GGUF tokenizer metadata hash + `tokenizer.chat_template` hash；Coordinator 统一 tokenization，PGX 接收 token IDs。
5. Native KV export/import spike：固定 prompt，deterministic decode，例如 `temperature=0` 或固定 seed；PGX prefill 后导出 KV / decode state，Mac 校验 manifest 并导入，输出与 baseline 达到可解释一致。
6. Post-token streaming failure policy：首 token 前可以 fallback normal mesh path；首 token 后终止 SSE 并返回明确 error/partial 结束状态，不透明 fallback。

## OpenSpec propose 建议范围

第一份 OpenSpec 不建议一次覆盖完整多阶段性能优化。建议先包含：

- PD enable/placement config。
- PD worker capability/status。
- Coordinator request lifecycle。
- `pd-kv-handoff-spike`。
- Baseline validation matrix。
- Fallback and safety requirements。

暂不纳入第一份 OpenSpec：

- 多 decode workers。
- 多 request 并发。
- 自动 placement。
- KV 压缩/增量传输。
- 公共 mesh 跨 owner PD。
- 低精度 quantization。

## 结论

第三阶段架构设计可以结束。  
下一步可以进入 OpenSpec propose，但应以 spike-gated 的方式推进，避免在 KV 跨 backend 可行性尚未确认前承诺完整 MVP 实现。
