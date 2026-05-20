# Phase 3 架构决策矩阵

生成日期：2026-05-19  
适用范围：`PD-detach` Phase 3 目标架构设计  
阅读对象：项目负责人、技术负责人、范围决策人  

本文把 Phase 3 的关键架构选择压缩成决策矩阵，用于进入 OpenSpec propose 前确认范围。本文只总结已有 Phase 3 文档，不引入新的实现设计。

## 决策矩阵

| 决策 | 推荐方案 | 被拒绝方案 | 推荐原因 | 代价/风险 | 证据文档 | 是否需要我确认 |
|---|---|---|---|---|---|---|
| 是否采用集中式 Coordinator | MVP 采用集中式 Coordinator，推荐由 Mac Studio 承担；Mac 同时负责 OpenAI ingress、Coordinator 和 Decode worker。 | 分布式 Coordinator；PGX 做 Coordinator；无 Coordinator、worker 点对点自发协作。 | streaming ownership、fallback、单请求 admission、诊断都集中在一个节点，第一版复杂度最低。 | Mac 控制面可能成为瓶颈；后续多 decode worker、多请求并发需要扩展 Coordinator。 | `docs/PD-detach/phase-3/ADR/ADR-001-centralized-coordinator.zh.md`、`docs/PD-detach/phase-3/ROLE_AND_SCHEDULING.zh.md` | 不需要重新确认，除非你希望第一版就支持分布式控制面或多 decode worker。 |
| KV handoff MVP 方案 | 以 Native KV Page Handoff 为 MVP 目标；第一份 OpenSpec change 建议为 `pd-kv-handoff-spike`，PGX prefill 后导出 KV / decode state + manifest，Mac 校验并导入后继续 decode。 | Full runtime state handoff；Skippy activation chain 直接当 PD；Decode 端 prompt replay；外部对象存储。 | 最符合 PD 语义，Mac 不重算 prompt；可复用现有 Skippy KV manifest、identity、export/import 代码；性能指标可直接测量。 | 跨 CUDA/Metal backend 可能不可导入；KV bytes 可能过大；bf16 权重与 f16 KV codec 必须区分。 | `docs/PD-detach/phase-3/ADR/ADR-002-kv-handoff-mvp.zh.md`、`docs/PD-detach/phase-3/KV_HANDOFF_DESIGN.zh.md` | 建议已明确：先做 `pd-kv-handoff-spike`，不直接承诺完整 MVP。 |
| 是否复用 Skippy | 复用 Skippy 的 runtime embedding、protocol/versioning 思路、stage status、KV manifest/identity/export/import 和 telemetry 经验；spike 可以复用 Skippy transport / KV 代码。 | 把现有 Skippy layer/activation split serving 直接定义为 PD MVP；完全绕开 Skippy 重写。 | 既利用已有工程资产，又避免把“按层切分”误认为“Prefill/Decode 分离”。 | 需要新增或明确 PD handoff 合同；不能直接宣称现有 Skippy 已经满足 PD。 | `docs/PD-detach/phase-3/ADR/ADR-003-skippy-reuse.zh.md`、`docs/PD-detach/phase-3/KV_HANDOFF_DESIGN.zh.md` | 建议已明确：复用工程基础，但 PD 语义不并入 Skippy split serving。 |
| 外部 API 兼容策略 | 外部继续使用 OpenAI-compatible API；不替换 `/v1/*`，不要求客户端使用新的 PD endpoint；status 只做 additive 扩展。 | 新增必需 `/v1/pd/*`；通过 request 必填字段开启 PD；用 PD route 替换 normal route。 | 客户端无需迁移；normal mesh path 保留为 fallback 和 baseline；PD 作为内部 feature lane 演进。 | Coordinator 需要承担更多 eligibility、fallback 和错误映射逻辑；post-token failure 不能透明回退。 | `docs/PD-detach/phase-3/ADR/ADR-004-external-api-compatibility.zh.md`、`docs/PD-detach/phase-3/API_AND_PROTOCOL.zh.md` | 不需要重新确认，这是 Phase 2/3 的核心约束。 |
| 单请求还是并发 | MVP 只支持单个 PD request in-flight、单 decode worker；新的请求在 busy 时走 normal mesh path 或记录 `pd_busy` 后 fallback。 | 第一版支持多请求队列；多 decode lanes；自动并发调度。 | 先打通功能和正确性，避免把并发队列、capacity、backpressure、取消清理混进第一版。 | 第一版吞吐有限；不能证明生产并发能力；性能优化要进入后续阶段。 | `docs/PD-detach/phase-3/ROLE_AND_SCHEDULING.zh.md`、`docs/PD-detach/phase-3/PHASE_3_EXIT_REVIEW.zh.md` | 建议已明确：MVP 以单用户/单请求可用为准，并发留到后续扩展。 |
| streaming 是否进 MVP | MVP 保持 OpenAI-compatible response，支持 SSE/JSON 路径；Coordinator 负责 streaming ownership。 | 第一版只做离线非 streaming；让 PGX 或多个 worker 直接对客户端 streaming；首 token 后失败时静默切换 normal path。 | 外部 API 兼容目标要求不破坏现有 streaming 语义；集中式 Coordinator 可以统一 token 输出顺序。 | 首 token 后失败不能透明 fallback。 | `docs/PD-detach/phase-3/PD_DATA_FLOW.zh.md`、`docs/PD-detach/phase-3/API_AND_PROTOCOL.zh.md`、`docs/PD-detach/phase-3/ADR/ADR-001-centralized-coordinator.zh.md` | 建议已明确：首 token 后终止 SSE，并返回明确 error/partial 结束状态。 |
| fallback 行为 | 首 token 前 PD 失败回退现有 normal mesh path；首 token 后失败不得透明 fallback，只能明确失败或终止 partial。 | PD 失败直接报错不回退；首 token 后继续静默切换 normal path；PD 替换 normal route。 | 既保护现有可用路径，又避免首 token 后生成重复或矛盾 token。 | fallback 逻辑集中在 Coordinator；post-token failure 需要被客户端可理解地表达。 | `docs/PD-detach/phase-3/PD_DATA_FLOW.zh.md`、`docs/PD-detach/phase-3/API_AND_PROTOCOL.zh.md`、`docs/PD-detach/phase-3/VALIDATION_PLAN.zh.md` | 建议已明确：首 token 前 fallback，首 token 后明确 error/partial termination。 |
| PD 开启和 placement 策略 | MVP 默认关闭、手动开启、手动 placement；两台 PGX 中选择一个 active prefill worker，另一个作为 fallback 候选。 | 默认自动开启；根据实时性能自动选 PGX；公共 mesh 自动发现并参与 PD。 | 避免影响现有 normal path；先把功能、正确性和可观测性打通。 | 需要人工配置；第一版不自动优化性能；配置错误会导致 fallback 或不可用。 | `docs/PD-detach/phase-3/ROLE_AND_SCHEDULING.zh.md`、`docs/PD-detach/phase-3/DEPLOYMENT_TOPOLOGY.zh.md` | 建议已明确：MVP 坚持手动开启/手动 placement，自动识别放后续。 |
| 内部协议边界 | 逻辑上定义独立内部协议 `pd-handoff/1`；spike 阶段可以复用 Skippy transport / KV 代码。 | 直接扩展外部 OpenAI API；把 PD 完全塞进现有 `mesh-llm/1` 字段；无版本化 handoff 协议。 | PD 语义不同于 Skippy layer split；独立协议能清楚表达 KV manifest、worker roles、fallback reason 和 lifecycle。 | 新协议需要兼容和版本治理；若 spike 复用 `skippy-stage/1` transport，需要避免语义混淆。 | `docs/PD-detach/phase-3/API_AND_PROTOCOL.zh.md`、`docs/PD-detach/phase-3/PHASE_3_EXIT_REVIEW.zh.md` | 建议已明确：目标是 `pd-handoff/1`，spike 可借用 Skippy 工程能力。 |
| 模型、tokenizer、artifact 一致性 | 模型 artifact 用文件内容 `sha256`；tokenizer 用 GGUF tokenizer metadata hash；chat template 单独 hash；runtime ABI、KV layout/dtype 作为 handoff 前置校验；mismatch fail closed。 | 只比较模型名；只比较本地路径；PGX/Mac 各自解析 prompt；mismatch 时继续尝试 decode。 | KV 是 decode state，必须保证 token 序列、position、layout 和 ABI 一致，否则可能生成看似正常但错误的 token。 | 需要实现 hash 采集和 manifest 校验；会增加 OpenSpec 和实现复杂度。 | `docs/PD-detach/phase-3/KV_HANDOFF_DESIGN.zh.md`、`docs/PD-detach/phase-3/PD_DATA_FLOW.zh.md`、`docs/PD-detach/phase-3/DEPLOYMENT_TOPOLOGY.zh.md` | 建议已明确：OpenSpec 前采用这组最小 identity 规则。 |
| OpenSpec 进入方式 | 采用 Conditional Go：第一份 OpenSpec change 为 `pd-kv-handoff-spike`，验证 KV export/import、manifest、PGX->Mac handoff 和 correctness baseline；再做完整 MVP proposal。 | 直接写完整 MVP 实现 proposal；把性能优化、并发、自动 placement 一次纳入。 | 最大未知是 KV 跨 backend 可行性，必须先用 spike 消除架构风险。 | 阶段会多一步；但可以避免在核心技术未验证前承诺完整功能。 | `docs/PD-detach/phase-3/PHASE_3_EXIT_REVIEW.zh.md`、`docs/PD-detach/phase-3/VALIDATION_PLAN.zh.md` | 建议已明确：先 spike，再 MVP。 |

## 需要你优先确认的事项

1. 第一份 OpenSpec change 按 `pd-kv-handoff-spike` 创建。
2. MVP 坚持单请求、单 decode worker、手动开启、手动 placement。
3. 目标内部协议边界为 `pd-handoff/1`；spike 可复用 Skippy transport / KV 代码。
4. post-token streaming failure 采用明确 SSE error/partial termination，不透明 fallback。
5. artifact identity 使用模型文件内容 `sha256`；tokenizer/chat template 使用 GGUF metadata hash + `tokenizer.chat_template` hash。

证据：`docs/PD-detach/phase-3/PHASE_3_EXIT_REVIEW.zh.md`、`docs/PD-detach/phase-3/API_AND_PROTOCOL.zh.md`、`docs/PD-detach/phase-3/KV_HANDOFF_DESIGN.zh.md`、`docs/PD-detach/phase-3/VALIDATION_PLAN.zh.md`
