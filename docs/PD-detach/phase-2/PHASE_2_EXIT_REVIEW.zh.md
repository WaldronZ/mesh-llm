# Phase 2 Exit Review

生成日期：2026-05-19

目标：判断第二阶段“需求与范围定义”是否可以结束，是否可以进入第三阶段“架构设计”。

结论先行：**Go**。

理由：异构 Prefill/Decode 分离的核心目标、角色职责、MVP 边界、非目标、兼容性、安全、故障和可测量性能指标已经形成可追踪需求规格，可以进入第三阶段架构设计；owner 已确认 PD 失败时回退到现有正常 mesh-llm 路径，PD 分离是独立分支的新功能，且 MVP 测试模型为 `google_gemma-4-31B-it-bf16`。原先阻塞 Phase 3 的最小问题已收敛为明确输入：激活/placement 采用架构推荐，MVP 允许单 request / 单 decode worker，tokenizer 以 GGUF 内嵌元数据为权威来源，dtype/precision 采用正常 16-bit/bf16 口径且不做低精度量化，性能验收先以功能打通、单用户可用、可测量 baseline 为准，测试环境来源为 operator-local private env file 且不复制 credentials。

## 检查项结论

| # | 检查项 | 结论 | 证据路径 | 说明 |
|---:|---|---|---|---|
| 1 | PD 分离的目标是否明确 | pass | `docs/PD-detach/phase-2/PREFILL_DECODE_REQUIREMENTS.zh.md` 第 1、4、7 节 | 已明确核心目标是两台 PGX 做 prefill、Mac Studio 做 decode，对外尽量保持 OpenAI-compatible API，并基于现有 mesh-llm 架构扩展。决策 `D-001` 到 `D-004` 已记录。 |
| 2 | Prefill worker 职责是否明确 | pass | `docs/PD-detach/phase-2/PREFILL_DECODE_REQUIREMENTS.zh.md` 第 8.1 节 | `FR-PW-001` 到 `FR-PW-006` 已定义 prefill worker 接收工作、保证兼容配置、生成 handoff 状态、输出元数据、失败时不得交接不完整 KV 等职责。 |
| 3 | Decode worker 职责是否明确 | pass | `docs/PD-detach/phase-2/PREFILL_DECODE_REQUIREMENTS.zh.md` 第 8.2 节 | `FR-DW-001` 到 `FR-DW-005` 已定义 decode worker 接收并校验 handoff、从 handoff 状态 decode、保持 API 输出行为、清理失败/取消状态等职责。 |
| 4 | Coordinator/router 职责是否明确 | pass | `docs/PD-detach/phase-2/PREFILL_DECODE_REQUIREMENTS.zh.md` 第 8.3、8.4 节 | `FR-CR-*` 和 `FR-AP-*` 已定义请求入口、手动 placement、兼容性校验、状态生命周期、默认关闭、能力发现仅用于诊断/校验等职责。 |
| 5 | MVP 范围是否明确 | pass | `docs/PD-detach/phase-2/PREFILL_DECODE_REQUIREMENTS.zh.md` 第 6、7、14 节 | 已明确 MVP 是两台 PGX + 一台 Mac、推荐默认关闭、手动开启、手动 placement、单 request、单 decode worker、单模型、单 tokenizer、单兼容 backend/dtype/quantization 组合。 |
| 6 | 明确不做的内容是否列出 | pass | `docs/PD-detach/phase-2/PREFILL_DECODE_REQUIREMENTS.zh.md` 第 6.2、15 节 | 已列出 MVP 不做全自动调度、多 decode workers、多 request、多模型、多 tokenizer、任意 backend/quantization、公共 mesh 跨 owner、替换 OpenAI API、恢复 `/0`、旧 benchmark 承诺、无边界重写等。 |
| 7 | 模型/tokenizer/KV 一致性要求是否列出 | pass | `docs/PD-detach/phase-2/PREFILL_DECODE_REQUIREMENTS.zh.md` 第 9、10 节 | 已定义 KV 格式版本、模型 artifact identity、tokenizer identity、dtype/quantization/backend、context/position/sequence 元数据和 fail closed 行为；MVP 测试模型已确认为 `google_gemma-4-31B-it-bf16`，tokenizer 来源已收敛为该 GGUF artifact 内嵌 `gemma4` tokenizer 元数据，且需求要求后续架构不得写死为 Gemma-only。 |
| 8 | API 兼容性要求是否列出 | pass | `docs/PD-detach/phase-2/PREFILL_DECODE_REQUIREMENTS.zh.md` 第 11 节；`docs/PD-detach/phase-1/API_REFERENCE.md` | 已列出不可破坏的 `/v1/*`、`/models`、`/api/status`、`/api/models`、`/api/runtime/*`、`/api/events`、`/api/model-targets` 等 surface，并要求保留非 PD baseline。 |
| 9 | 性能验收指标是否可测量 | pass | `docs/PD-detach/phase-2/PREFILL_DECODE_REQUIREMENTS.zh.md` 第 13.1、13.2、17 节 | 已定义可测量指标：end-to-end latency、prefill latency、KV handoff bytes、KV handoff latency、decode tokens/sec、TTFT、p50/p95/p99、failure rate、fallback rate，并要求建立 Mac-only、PGX-only、现有 mesh path、PD path baseline。owner 已确认 MVP 先以功能打通、单用户可用、可测量 baseline 为准，固定收益阈值后续再定。 |
| 10 | 故障处理需求是否定义 | pass | `docs/PD-detach/phase-2/PREFILL_DECODE_REQUIREMENTS.zh.md` 第 12、17 节 | 已覆盖 worker 不可用、prefill/decode 中途失败、KV 格式 mismatch、模型/tokenizer mismatch、dtype/backend 不兼容、网络超时、请求取消、版本低于门槛、未配置 worker、配置与能力发现不一致等；owner 已确认 PD 失败时回退到现有正常 mesh-llm 路径。 |
| 11 | 安全/隐私风险是否记录 | pass | `docs/PD-detach/phase-2/PREFILL_DECODE_REQUIREMENTS.zh.md` 第 9.3、13.4 节；`docs/PD-detach/phase-1/SECURITY_AND_PRIVACY.md` | 已将 KV cache 视为 prompt-derived 敏感数据，要求日志/telemetry/错误响应不得泄露 prompt、KV、凭据和内部路径，且 MVP 未经手动配置允许的节点不得参与。 |
| 12 | 是否还有阻塞架构设计的问题 | pass | `docs/PD-detach/phase-2/PREFILL_DECODE_REQUIREMENTS.zh.md` 第 16、19 节 | 不存在阻塞第三阶段架构设计的问题。tokenizer、dtype/precision、quantization、性能口径和测试环境来源已收敛；Skippy 复用、capability discovery surface、fallback 触发边界属于 Phase 3 架构输入，不再阻塞需求阶段关闭。 |

## Go / No-Go

**Go**

可以结束第二阶段需求与范围定义，进入第三阶段架构设计。  
第三阶段仍不能直接进入实现计划；应先把下面的已收敛输入转化为架构方案、接口合同、验证矩阵和风险回退设计。

## 第三阶段开始时必须带入的设计输入

1. **MVP 激活策略**  
   架构推荐：默认关闭、手动开启、手动 placement。  
   证据：`PREFILL_DECODE_REQUIREMENTS.zh.md` 的 `D-008`、`FR-AP-001` 到 `FR-AP-004`、`OQ-001`。

2. **MVP 并发和 decode 范围**  
   架构推荐：允许第一版只支持单 request in-flight 和单 decode worker。  
   证据：`PREFILL_DECODE_REQUIREMENTS.zh.md` 的 `D-014`、第 14 节和第 19 节。

3. **MVP tokenizer 获取方式**  
   已收敛：tokenizer 以 `google_gemma-4-31B-it-bf16` GGUF artifact 内嵌 tokenizer 元数据为权威来源。Phase 3 需要定义 tokenizer identity/hash 规则。  
   证据：`PREFILL_DECODE_REQUIREMENTS.zh.md` 的 `D-013`、`FR-MC-001`、`FR-TK-001`、`OQ-005`、第 10.3 节。

4. **PGX 与 Mac 的 backend / dtype / quantization 兼容组合**  
   已收敛：MVP 按正常 16-bit/bf16 口径推进，不做低精度 quantization；backend 兼容性由版本化 KV handoff 合同和 Phase 3 验证矩阵确认。  
   证据：`PREFILL_DECODE_REQUIREMENTS.zh.md` 的 `D-016`、`FR-MC-004`、`FR-KV-006`、`OQ-006`。

5. **性能验收口径**  
   已确认：第一版先功能打通、单用户可用、建立可测量 baseline；固定收益阈值后续再定。  
   证据：`PREFILL_DECODE_REQUIREMENTS.zh.md` 的 `D-015`、`NFR-PERF-*`、`NFR-EFF-*`、`OQ-007`、`OQ-013`、`OQ-014`。

6. **测试环境和资源交接**  
   已确认来源：operator-local private env file。Phase 3 只引用 `MESH_MACSTUDIO_*`、`MESH_PGX_30BE_*`、`MESH_PGX_3030_*`、`MESH_GEMMA_*` 等变量名和角色，不复制 credentials。  
   证据：`PREFILL_DECODE_REQUIREMENTS.zh.md` 的 `D-017`、第 10.3 节、第 14 节、第 19 节；`docs/PD-detach/phase-1/SECURITY_AND_PRIVACY.md`。

7. **Skippy split serving 复用评估**  
   保持现有口径：第三阶段优先评估可复用边界，但需求文档不指定实现。  
   证据：`PREFILL_DECODE_REQUIREMENTS.zh.md` 的 `S-011`、`OQ-009`、第 19 节；`docs/PD-detach/phase-1/ARCHITECTURE_CURRENT.md`。

8. **Capability discovery 架构归属**  
   保持现有口径：MVP 只用于识别、诊断、校验；具体进入 mesh gossip、管理 API、配置还是其他 surface，由第三阶段架构设计明确。  
   证据：`PREFILL_DECODE_REQUIREMENTS.zh.md` 的 `D-009`、`FR-AP-004`、`OQ-011`。

9. **设计 fallback 的触发边界和可观测性**  
   策略已确认：PD 失败回退现有正常 mesh-llm 路径。第三阶段仍需定义哪些错误可安全回退、哪些 streaming 场景不能回退、以及如何记录 fallback reason。  
   证据：`PREFILL_DECODE_REQUIREMENTS.zh.md` 的 `D-012`、`FR-CR-006`、`FR-FT-*`、`NFR-COMP-002`。

## 第三阶段建议入口

第三阶段架构设计应从以下问题开始，而不是直接写实现任务：

1. PD request lifecycle 和状态机。
2. KV handoff 合同和版本化边界。
3. Worker capability / placement / activation surface。
4. 模型、tokenizer、dtype、backend 兼容性校验边界。
5. API 兼容和 fallback 策略。
6. 可观测指标和性能 baseline 采集边界。
7. Skippy split serving 可复用性评估。
