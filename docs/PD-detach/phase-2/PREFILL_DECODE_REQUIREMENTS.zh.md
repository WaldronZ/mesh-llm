# 异构 Prefill/Decode 分离需求与范围说明书

文档状态：Phase 2 需求与范围定义  
生成日期：2026-05-18  
更新日期：2026-05-19  
适用范围：`PD-detach` 大型二开  

本文是需求与范围说明书，不是架构设计，不是实现计划，不要求修改业务代码。  
本文基于 `docs/PD-detach/phase-1/` Discovery 文档，并加入 owner 对本次二开目标的确认。

## 1. 执行摘要

本次大型二开的核心目标是实现**异构 Prefill/Decode 分离**：

- 两台 PGX 作为 prefill workers，负责 prompt/prefill 阶段。
- 一台 Mac Studio 作为 decode worker，负责 token-by-token decode 阶段。
- 对外尽量保持现有 OpenAI-compatible API。
- 基于 mesh-llm 现有架构扩展，不做无边界重写。

高级需求判断：

1. 完整目标可以一次定义清楚，但交付必须分阶段。
2. MVP 必须先证明跨机器 KV handoff、模型/tokenizer/backend 兼容和端到端可测量性；若收益不足，必须明确瓶颈。
3. MVP 默认关闭 PD 分离，需要手动开启，不做全自动 worker 调度。
4. 能力发现第一阶段用于识别、诊断和校验，不用于无约束自动 placement。
5. KV cache 或等价 decode 初始状态必须跨机器交接，否则不满足目标场景。
6. `mesh-llm/1` 是当前正式 runtime protocol，`mesh-llm/0` 只是历史/遗留兼容代码。
7. `v0.60.0` 是节点软件版本最低门槛。
8. 速度/效率优化是本项目的核心收益目标之一，但 MVP 不承诺最终性能最优；MVP 必须量化收益和瓶颈，为后续优化提供依据。
9. MVP 验收口径先以“功能打通、单用户可用、可测量 baseline”为准，不在第一版承诺固定收益阈值。
10. MVP 测试模型为 `google_gemma-4-31B-it-bf16`；tokenizer 以该 GGUF artifact 内嵌 tokenizer 元数据为权威来源。

## 2. Phase 1 证据基线

| 结论 | Phase 1 证据 |
|---|---|
| 主业务运行时在 `crates/mesh-llm-host-runtime/`，`crates/mesh-llm/` 是薄入口。 | `docs/PD-detach/phase-1/ARCHITECTURE_CURRENT.md` |
| 对外已有 OpenAI-compatible API，默认端口 `9337`。 | `docs/PD-detach/phase-1/API_REFERENCE.md` |
| 管理 API / Web console 默认端口 `3131`。 | `docs/PD-detach/phase-1/API_REFERENCE.md` |
| 当前正式 mesh 协议是 `mesh-llm/1`。 | `docs/PD-detach/phase-1/PROTOCOL_COMPATIBILITY.md` |
| `mesh-llm/0` 是历史/遗留兼容代码。 | `docs/PD-detach/phase-1/PROTOCOL_COMPATIBILITY.md` |
| `v0.60.0` 是节点软件版本最低门槛。 | `docs/PD-detach/phase-1/PROTOCOL_COMPATIBILITY.md` |
| Skippy split serving 已有相关 crate 和 host runtime 集成。 | `docs/PD-detach/phase-1/ARCHITECTURE_CURRENT.md` |
| API schema 当前以 Rust server payload 为事实来源，UI TS 类型是消费端镜像。 | `docs/PD-detach/phase-1/API_REFERENCE.md` |
| benchmark 旧数据不能直接作为当前性能承诺。 | `docs/PD-detach/phase-1/TEST_MATRIX.md`、`docs/PD-detach/phase-1/risk-register.md` |
| KV/prompt/telemetry/凭据均属于安全与隐私敏感面。 | `docs/PD-detach/phase-1/SECURITY_AND_PRIVACY.md` |

## 3. 术语

| 术语 | 定义 |
|---|---|
| Prefill | 对 prompt tokens 执行初始前向计算并生成可供 decode 使用的 KV cache 或等价状态。 |
| Decode | 基于 prefill 后状态逐 token 生成输出。 |
| Prefill worker | 执行 prefill 阶段的节点。本目标场景中是 PGX。 |
| Decode worker | 执行 decode 阶段的节点。本目标场景中是 Mac Studio。 |
| Coordinator / router | 接收请求、选择 worker、校验兼容性、协调请求生命周期的逻辑角色。本文不规定具体实现位置。 |
| KV handoff | 将 prefill 后的 KV cache 或等价 decode 初始状态交接给 decode worker。 |
| Capability discovery | 节点暴露自身模型、tokenizer、backend、dtype、角色候选、KV 格式等能力，供校验和诊断使用。 |
| Placement | 为某个请求选择 prefill worker 和 decode worker 的策略。MVP 使用手动 placement。 |
| Backend | 模型实际执行的运行后端/硬件后端，例如 PGX 上可能使用 CUDA 路径，Mac Studio 上可能使用 Metal 路径；具体以后续架构验证为准。 |
| dtype | 计算或状态中使用的数据类型。MVP 采用正常 16-bit 口径，本次测试 artifact 是 bf16。 |
| quantization | 低精度量化格式，例如 Q4/Q8 等。MVP 不做低精度量化适配，保留为后续优化。 |

## 4. 设计前决策记录

这些是需求阶段已确认或已收敛的产品/范围决策，不是实现设计：

| ID | 决策 | 状态 | 影响 |
|---|---|---|---|
| D-001 | 本次核心功能是异构 Prefill/Decode 分离。 | 已确认 | 全局目标 |
| D-002 | 目标拓扑是两台 PGX prefill workers + 一台 Mac Studio decode worker。 | 已确认 | MVP 范围 |
| D-003 | 对外尽量保持 OpenAI-compatible API。 | 已确认 | API 兼容 |
| D-004 | 基于现有 mesh-llm 架构扩展，不做无边界重写。 | 已确认 | 范围控制 |
| D-005 | `mesh-llm/1` 是当前正式 runtime protocol。 | 已确认 | 协议兼容 |
| D-006 | `mesh-llm/0` 是历史/遗留兼容代码。 | 已确认 | 协议文档 |
| D-007 | `v0.60.0` 是节点软件版本最低门槛。 | 已确认 | mesh 兼容 |
| D-008 | MVP 激活策略建议采用默认关闭、手动开启、手动 placement，不做全自动调度。 | 架构推荐 | 风险控制 |
| D-009 | Capability discovery 在 MVP 只用于识别、诊断和校验。 | 保持现有口径 | 解耦和排障 |
| D-010 | KV handoff 必须跨机器验证。 | 已确认 | 功能验收 |
| D-011 | 性能优化目标进入需求范围，但具体优化算法和实现方案留到后续架构/性能专项。 | 已确认 | 验收与后续规划 |
| D-012 | PD 分离是独立分支的新功能；PD 失败时回退到现有正常 mesh-llm 路径。 | 已确认 | API 兼容、风险控制 |
| D-013 | 本次功能迭代的 MVP 测试模型选择 `google_gemma-4-31B-it-bf16`；该模型用于验证适配，不代表最终成品只支持 Gemma。 | 已确认 | 验收环境、模型适配 |
| D-014 | MVP 建议允许只支持单 request in-flight 和单 decode worker，作为第一版打通路径的范围限制。 | 架构推荐 | MVP 范围 |
| D-015 | MVP 性能验收口径是先功能打通、单用户可用、建立可测量 baseline；固定收益阈值不作为第一版准入。 | 已确认 | 验收 |
| D-016 | MVP dtype/precision 口径采用正常 16-bit；本次测试 artifact 为 bf16；低精度 quantization 不进 MVP。 | 已确认 | 兼容性、后续优化 |
| D-017 | MVP 测试环境和模型路径来源于 operator-local private env file；仓库文档只记录变量名和角色，不复制 credentials。 | 已确认 | 安全、环境交接 |

## 5. 需求解耦边界

需求必须从一开始按合同和职责拆分。后续架构可以变化，但需求边界不应混在一起。

| 边界 | 需求关注点 | 不应混入 |
|---|---|---|
| Worker role | prefill worker、decode worker 分别声明和承担什么能力。 | 调度算法、API 入口实现细节。 |
| Activation policy | PD 分离如何开启、默认是否关闭、按什么粒度启用。 | KV 格式、网络传输实现。 |
| Placement policy | 哪些 worker 被允许参与请求。 | 模型内部计算实现。 |
| KV handoff contract | KV 状态如何描述、版本化、校验、失败。 | UI 展示、benchmark 报告格式。 |
| Compatibility matrix | 模型、tokenizer、dtype、quantization、backend 是否兼容。 | 自动调度策略。 |
| External API compatibility | 哪些现有 API 不能破坏。 | worker 内部协议细节。 |
| Observability | 哪些阶段、指标、错误码必须可见。 | 具体日志库或 metrics exporter 设计。 |
| Performance optimization | 需要衡量哪些效率指标、MVP 要证明哪些收益和瓶颈。 | KV 压缩算法、调度算法、网络协议实现、batching 实现。 |

## 6. 范围总览

### 6.1 In Scope

| 编号 | 范围项 | MVP |
|---|---|---|
| S-001 | PGX prefill + Mac Studio decode 的跨机器请求路径。 | 是 |
| S-002 | 跨机器 KV handoff 或等价 decode 初始状态交接。 | 是 |
| S-003 | 手动开启 PD 分离。 | 是 |
| S-004 | 手动指定 prefill workers 和 decode worker。 | 是 |
| S-005 | 单测试模型、单 tokenizer、单兼容 backend/dtype/quantization 组合；MVP 测试模型为 `google_gemma-4-31B-it-bf16`，dtype/precision 按正常 16-bit/bf16 口径，低精度 quantization 不进 MVP。 | 是 |
| S-006 | 单 request in-flight。 | 是 |
| S-007 | 单 decode worker。 | 是 |
| S-008 | Capability discovery 用于校验和诊断。 | 是 |
| S-009 | OpenAI-compatible API 基本兼容。 | 是 |
| S-010 | 关键性能与故障指标记录。 | 是 |
| S-011 | 评估复用现有 Skippy split serving 能力。 | 是 |
| S-012 | 建立速度/效率 baseline 和瓶颈数据。 | 是 |
| S-013 | 定义后续性能优化方向和验收指标。 | 是 |

### 6.2 Out of Scope for MVP

| 编号 | 非目标 | 说明 |
|---|---|---|
| N-001 | 全自动 worker 调度。 | 后续阶段。 |
| N-002 | 多 decode workers。 | 后续阶段。 |
| N-003 | 多 request 并发调度。 | 后续阶段。 |
| N-004 | 多模型、多 tokenizer 任意组合。 | 后续阶段；MVP 只用 `google_gemma-4-31B-it-bf16` 验证，但设计不能写死 Gemma。 |
| N-005 | 跨低精度 quantization 或任意 backend 的 KV handoff。 | 必须先有兼容性证据；低精度量化属于后续优化。 |
| N-006 | 公共 mesh 跨 owner PD 分离。 | 安全和信任边界未定义。 |
| N-007 | 替换现有 OpenAI-compatible API。 | 违背兼容目标。 |
| N-008 | 把 `mesh-llm/0` 恢复为正式协议。 | 已明确为历史兼容代码。 |
| N-009 | 使用旧 benchmark 作为性能承诺。 | 必须重跑并标日期。 |
| N-010 | 无边界重写 host runtime、protocol、routing 或 Skippy。 | 违背二开约束。 |
| N-011 | 在 MVP 中承诺最终性能最优。 | MVP 先验证可行性、收益和瓶颈。 |
| N-012 | 在需求文档中指定具体 KV 压缩、调度、batching 或网络传输实现。 | 属于后续架构设计或性能专项。 |

## 7. 目标层级

| 层级 | 目标 | 验收重点 |
|---|---|---|
| Target Requirement | 完整异构 PD 分离能力：多 worker、可观测、可回退、可扩展，外部 API 尽量不变。 | 长期方向，不等于第一版全部交付。 |
| MVP | 固定两台 PGX + 一台 Mac Studio，默认关闭、手动开启，单模型，单 tokenizer，单 decode worker，单 request。 | 跨机器 KV handoff、正确性、可测量 baseline。 |
| Phase 2.x | 半自动 worker 发现、兼容性诊断、多 request、fallback policy、管理 API/status 展示。 | 在 MVP 证明可行后扩展。 |
| Later | 自动调度、多 decode workers、多模型、多 backend/quantization、KV 压缩/增量传输、UI 产品化。 | 规模化和产品化。 |

## 8. 功能需求

优先级说明：

- Must：MVP 必须满足。
- Should：强烈建议进入 MVP 或紧随其后。
- Could：后续扩展。

### 8.1 Prefill Worker

| ID | 优先级 | 需求 | 验收方式 |
|---|---|---|---|
| FR-PW-001 | Must | PGX prefill worker 必须能接收被明确分配的 prefill 工作。 | 指定 PGX worker 后，请求进入 prefill 阶段。 |
| FR-PW-002 | Must | Prefill worker 必须使用与 decode worker 兼容的模型、tokenizer、上下文参数和推理配置。 | 不兼容配置 fail closed。 |
| FR-PW-003 | Must | Prefill worker 必须生成可交接的 KV cache 或等价 decode 初始状态。 | Mac decode worker 能从 handoff 状态继续 decode。 |
| FR-PW-004 | Must | Prefill worker 必须输出 handoff 元数据，包括模型、tokenizer、dtype、quantization、backend、KV 格式版本、context/position 信息。 | Handoff 元数据可被 decode worker 校验。 |
| FR-PW-005 | Must | Prefill worker 失败时不得交接不完整或不可验证 KV。 | 模拟 prefill 失败，请求失败或明确回退。 |
| FR-PW-006 | Should | 两台 PGX 中至少一个可被手动指定为 active prefill worker。 | 手动切换 PGX worker 可诊断。 |

### 8.2 Decode Worker

| ID | 优先级 | 需求 | 验收方式 |
|---|---|---|---|
| FR-DW-001 | Must | Mac Studio decode worker 必须接收并校验 handoff 状态。 | 不兼容 handoff 被拒绝。 |
| FR-DW-002 | Must | Decode worker 必须从 handoff 状态开始 token-by-token decode。 | 输出不重新从 prompt 全量 prefill 开始。 |
| FR-DW-003 | Must | Decode worker 必须校验模型、tokenizer、dtype、backend、KV 格式版本和 context/position 元数据。 | 任一 mismatch fail closed。 |
| FR-DW-004 | Must | Decode worker 必须按现有 API 行为返回生成结果。 | `/v1/*` 主路径调用方不需要理解内部分离。 |
| FR-DW-005 | Must | Decode worker 在请求取消、超时、失败后必须释放临时状态。 | 取消和失败场景无残留请求状态。 |

### 8.3 Coordinator / Router

| ID | 优先级 | 需求 | 验收方式 |
|---|---|---|---|
| FR-CR-001 | Must | Coordinator/router 必须接收现有 OpenAI-compatible 请求入口。 | 现有 `/v1/*` 主路径继续可用。 |
| FR-CR-002 | Must | Coordinator/router 必须只在显式开启 PD 分离时进入 PD 路径。 | 默认关闭时走现有路径。 |
| FR-CR-003 | Must | MVP 必须尊重手动 placement 配置。 | 未配置允许的 worker 不参与请求。 |
| FR-CR-004 | Must | Coordinator/router 必须校验 prefill/decode worker 兼容性。 | 不兼容 worker pair 被拒绝。 |
| FR-CR-005 | Must | Coordinator/router 必须管理请求状态：queued、prefilling、handoff、decoding、completed、failed、cancelled。 | 管理/status surface 可诊断阶段。 |
| FR-CR-006 | Must | 不满足 PD 条件或 PD 执行失败时，默认回退到现有正常 mesh-llm 路径。 | PD 失败不破坏现有请求路径。 |
| FR-CR-007 | Should | Capability discovery 结果应可用于诊断 worker 不可用或不兼容原因。 | status 或诊断输出可见原因。 |

### 8.4 Activation / Placement

| ID | 优先级 | 需求 | 验收方式 |
|---|---|---|---|
| FR-AP-001 | Must | MVP 默认关闭 PD 分离。 | 未配置时现有路径不受影响。 |
| FR-AP-002 | Must | MVP 必须通过手动配置或显式管理入口开启 PD 分离。 | 手动开启后请求进入 PD 路径。 |
| FR-AP-003 | Must | MVP 必须手动指定 prefill workers 和 decode worker。 | 未指定 worker 时不自动参与。 |
| FR-AP-004 | Must | MVP 能力发现只用于识别、诊断和校验，不用于全自动调度。 | 发现到候选 worker 但未配置允许时不参与请求。 |
| FR-AP-005 | Should | 后续阶段可支持半自动 placement，系统发现候选 worker 后仍需 policy 允许。 | Phase 2.x 范围。 |
| FR-AP-006 | Could | 后续阶段可支持基于健康状态、兼容性和网络指标的自动 placement。 | Later 范围。 |

## 9. KV Handoff 合同需求

### 9.1 必须跨机器交接

| ID | 优先级 | 需求 | 验收方式 |
|---|---|---|---|
| FR-KV-001 | Must | 目标场景要求 KV cache 或等价 decode 初始状态从 PGX 跨机器交接到 Mac Studio。 | PGX prefill + Mac decode 的端到端请求成功。 |
| FR-KV-002 | Must | 只在单机内完成 handoff 不能视为 MVP 完成。 | 验收环境必须包含至少一台 PGX 和一台 Mac Studio。 |

### 9.2 格式版本化

KV handoff 必须有显式格式版本和兼容性元数据。

| ID | 优先级 | 需求 | 验收方式 |
|---|---|---|---|
| FR-KV-003 | Must | Handoff 元数据必须包含 KV 格式版本。 | 版本缺失 fail closed。 |
| FR-KV-004 | Must | Handoff 元数据必须包含模型 artifact identity。 | 模型 mismatch fail closed。 |
| FR-KV-005 | Must | Handoff 元数据必须包含 tokenizer identity。 | tokenizer mismatch fail closed。 |
| FR-KV-006 | Must | Handoff 元数据必须包含 dtype、quantization、backend。 | 不兼容组合 fail closed。 |
| FR-KV-007 | Must | Handoff 元数据必须包含 context、position、sequence 相关信息。 | position/context mismatch fail closed。 |
| FR-KV-008 | Must | Handoff 格式版本不匹配时必须 fail closed。 | 构造版本 mismatch 测试。 |
| FR-KV-009 | Should | 新增 handoff 字段应尽量 additive。 | 与 `mesh-llm/1` 兼容原则一致。 |

### 9.3 安全级别

| ID | 优先级 | 需求 | 验收方式 |
|---|---|---|---|
| FR-KV-010 | Must | KV cache 视为 prompt-derived 敏感数据。 | 安全文档和 telemetry allowlist 覆盖 KV。 |
| FR-KV-011 | Must | 日志和 telemetry 不得记录 KV 内容。 | 日志审计和测试。 |
| FR-KV-012 | Must | 请求取消、失败、超时后必须清理临时 KV 状态。 | 取消/失败场景验证。 |

## 10. 兼容性需求

### 10.1 模型一致性

| ID | 优先级 | 需求 | 验收方式 |
|---|---|---|---|
| FR-MC-001 | Must | MVP 测试模型为 `google_gemma-4-31B-it-bf16`，并以该模型完成 PD 分离适配验证。 | 非指定模型不进入 MVP PD 路径。 |
| FR-MC-002 | Must | Prefill 和 decode 必须使用同一模型 artifact，或使用已证明兼容的 artifact identity。 | artifact mismatch fail closed。 |
| FR-MC-003 | Must | context length、position、rope 等配置必须一致或显式证明兼容。 | mismatch fail closed。 |
| FR-MC-004 | Must | MVP 只允许一组已验证 dtype/quantization/backend 组合。 | 非允许组合不进入 PD 路径。 |
| FR-MC-005 | Must | `google_gemma-4-31B-it-bf16` 只是测试适配模型，需求和后续架构不得把 PD 能力写死为 Gemma-only。 | Phase 3 架构设计需保留模型适配扩展边界。 |
| FR-MC-006 | Should | 后续阶段可扩展兼容性矩阵，但必须有 correctness 和性能证据。 | Phase 2.x 或 Later。 |

### 10.2 Tokenizer 一致性

| ID | 优先级 | 需求 | 验收方式 |
|---|---|---|---|
| FR-TK-001 | Must | MVP 只支持与指定模型绑定的单 tokenizer。 | tokenizer mismatch fail closed。 |
| FR-TK-002 | Must | Tokenizer identity 必须进入 handoff 元数据。 | 元数据校验。 |
| FR-TK-003 | Must | Decode worker 不得用不同 tokenizer 重新解释 prompt。 | 构造 mismatch 测试。 |
| FR-TK-004 | Must | 错误信息可诊断 tokenizer mismatch，但不能泄露 prompt。 | 错误响应审计。 |

### 10.3 MVP 测试模型、tokenizer 与兼容组合事实

| 项 | 当前口径 | 证据路径 |
|---|---|---|
| 测试模型 | `google_gemma-4-31B-it-bf16`。该模型只作为 MVP 适配和验收模型，后续成品不能写死为 Gemma-only。 | `D-013`、`FR-MC-001`、`FR-MC-005`；operator-local env 中的 `MESH_GEMMA_MODEL_NAME` |
| 本地模型 artifact | 当前 operator-local 模型目录存在 split GGUF 分片。Phase 3 需要定义 artifact identity/hash 的正式规则。 | `MESH_GEMMA_MODEL_MAC_PATH` 指向的本地 GGUF artifact |
| GGUF 架构元数据 | shard 1 元数据显示 `general.architecture=gemma4`、`general.name=Gemma 4 31B It`、`general.size_label=31B`、`gemma4.block_count=60`、`gemma4.context_length=262144`。 | `scripts/skippy-llama-parity.py` 的 `gguf_metadata()`；`google_gemma-4-31B-it-bf16-00001-of-00002.gguf` |
| Tokenizer 权威来源 | MVP 使用该 GGUF artifact 内嵌 tokenizer 元数据，不另行假设外部 tokenizer 文件。 | GGUF 元数据：`tokenizer.ggml.model=gemma4`、`tokenizer.ggml.tokens=list[262144]`、`tokenizer.ggml.merges=list[514906]`、`tokenizer.chat_template` 存在 |
| Tokenizer 特征 | 已读到 `tokenizer.ggml.add_bos_token=True`、`tokenizer.ggml.add_space_prefix=False`、`bos_token_id=2`、`eos_token_id=1`、`padding_token_id=0`、`unknown_token_id=3`、`mask_token_id=4`。 | `scripts/skippy-llama-parity.py` 的 `gguf_metadata()`；本地 GGUF shard 1 |
| dtype / precision | MVP 按正常 16-bit 口径推进；本次测试 artifact 名称和运行约束采用 bf16。Phase 3 需验证实际 KV handoff dtype/布局。 | `D-016`；`MESH_GEMMA_MODEL_NAME`；本地 GGUF 目录名 |
| quantization | MVP 不做低精度量化适配，不支持 Q4/Q8 等低精度组合；量化是后续性能/容量优化方向。 | `D-016`、`N-005`、`NFR-COMP-004` |
| backend 兼容 | backend 指运行后端/硬件后端。MVP 目标是 PGX prefill 后端与 Mac decode 后端能通过同一版本化 KV 合同交接；具体 CUDA/Metal/llama.cpp/skippy 路径由 Phase 3 设计和验证确认。 | `FR-KV-006`、`FR-MC-004`、`FR-FT-007` |

测试环境来源：

- operator-local private env file 记录 Mac Studio、两台 PGX、Gemma 模型名称和各机器模型路径相关变量。
- 仓库文档只记录变量名和角色，例如 `MESH_MACSTUDIO_*`、`MESH_PGX_30BE_*`、`MESH_PGX_3030_*`、`MESH_GEMMA_*`。
- 该 env 文件中的 credentials、密码、token 或连接细节不得复制到任何 tracked 文档。

## 11. API 与协议兼容需求

### 11.1 不可破坏的 API

| ID | 优先级 | API surface | 需求 |
|---|---|---|---|
| FR-API-001 | Must | `/v1/*` | 现有 OpenAI-compatible 主入口不得被替换。 |
| FR-API-002 | Must | `/models` | 现有模型列表语义不得因 PD 分离破坏。 |
| FR-API-003 | Must | `/api/status` | 管理状态 API 必须继续可用。 |
| FR-API-004 | Must | `/api/models` | 管理模型 API 必须继续可用。 |
| FR-API-005 | Must | `/api/runtime/*` | runtime 管理 API 不得被 PD 分离破坏。 |
| FR-API-006 | Must | `/api/events` | 事件流不得被 PD 分离破坏。 |
| FR-API-007 | Should | `/api/model-targets` | 应能表达或诊断 PD 相关 target 状态。 |

### 11.2 外部行为

| ID | 优先级 | 需求 | 验收方式 |
|---|---|---|---|
| FR-API-008 | Must | 调用方默认仍通过 OpenAI-compatible API 发请求。 | 客户端不需要换 endpoint。 |
| FR-API-009 | Must | 非 PD 路径必须保留，作为 fallback 和 baseline。 | 默认关闭 PD 时现有路径正常。 |
| FR-API-010 | Should | 如果需要 PD 模式开关，应使用配置、管理 API 或非破坏性扩展字段。 | 不替换现有 endpoint。 |
| FR-API-011 | Must | 错误响应必须可解释，但不能泄露 prompt、KV、凭据、内部路径。 | 错误响应审计。 |

### 11.3 Mesh 协议

| ID | 优先级 | 需求 |
|---|---|---|
| FR-PR-001 | Must | 当前正式 runtime protocol 仍是 `mesh-llm/1`。 |
| FR-PR-002 | Must | 本阶段不得扩大 `mesh-llm/0` 支持范围。 |
| FR-PR-003 | Must | 节点软件版本最低门槛仍是 `v0.60.0`。 |
| FR-PR-004 | Must | 新增协议字段或 handoff 元数据必须遵循 additive / fail-closed 原则。 |

## 12. 故障处理需求

| ID | 优先级 | 故障 | 期望行为 |
|---|---|---|---|
| FR-FT-001 | Must | 无可用 PGX prefill worker | 回退现有路径或返回明确错误；不能挂起请求。 |
| FR-FT-002 | Must | 无可用 Mac decode worker | 回退现有路径或返回明确错误；不能生成不可信结果。 |
| FR-FT-003 | Must | prefill worker 中途失败 | 不交接不完整 KV；清理 PD 状态并回退现有路径。 |
| FR-FT-004 | Must | decode worker 在产生对外 token 前失败 | 清理 PD 状态并回退现有路径。 |
| FR-FT-005 | Must | KV 格式版本不匹配 | 拒绝 PD 路径，回退现有路径，并记录可诊断原因。 |
| FR-FT-006 | Must | 模型/tokenizer 不匹配 | 拒绝 PD 路径，回退现有路径，并记录可诊断原因。 |
| FR-FT-007 | Must | dtype/quantization/backend 不兼容 | 拒绝 PD 路径，回退现有路径，并记录可诊断原因。 |
| FR-FT-008 | Must | 网络超时或带宽不足 | 回退现有路径；必须记录 handoff latency 或 timeout 原因。 |
| FR-FT-009 | Must | 请求取消 | prefill 和 decode worker 均释放临时状态。 |
| FR-FT-010 | Must | worker 版本低于门槛 | 不参与请求，诊断可见。 |
| FR-FT-011 | Must | 发现候选 worker 但未被配置允许 | MVP 不参与请求，只作为诊断信息。 |
| FR-FT-012 | Must | 手动配置与能力发现不一致 | 拒绝 PD 路径，回退现有路径，并提示配置/能力不一致。 |
| FR-FT-013 | Should | decode worker 在已经向外 streaming token 后失败 | streaming partial output 策略仍需单独确认；不得静默伪造后续 token。 |

## 13. 非功能需求

### 13.1 性能与容量

Phase 1 未重跑 benchmark，因此旧结果不能作为验收标准。MVP 必须建立新 baseline。

性能优化是 PD 分离的核心收益目标之一。本阶段需求要定义“为什么优化、衡量什么、MVP 必须证明什么”，但不定义具体优化算法或实现方案。KV 压缩、增量 handoff、prefill batching、自动 placement、多 worker 并发、网络协议优化等属于后续架构设计或性能专项。

| ID | 优先级 | 指标 | 说明 |
|---|---|---|---|
| NFR-PERF-001 | Must | End-to-end latency | 总请求耗时。 |
| NFR-PERF-002 | Must | Prefill latency | PGX prefill 耗时。 |
| NFR-PERF-003 | Must | KV handoff bytes | 跨机器传输字节数。 |
| NFR-PERF-004 | Must | KV handoff latency | KV 传输和接收耗时。 |
| NFR-PERF-005 | Must | Decode tokens/sec | Mac decode 吞吐。 |
| NFR-PERF-006 | Must | Time to first token | 首 token 延迟。 |
| NFR-PERF-007 | Should | p50/p95/p99 latency | 稳定性分布。 |
| NFR-PERF-008 | Must | failure rate | 请求失败率。 |
| NFR-PERF-009 | Must | fallback rate | 回退比例。 |

Owner 已确认 MVP 性能验收先以“功能打通、单用户可用、可测量 baseline”为准，不把固定收益阈值作为第一版准入。MVP 至少建立以下 baseline：

1. Mac Studio 单机完整推理 baseline。
2. PGX 单机完整推理 baseline。
3. 现有 mesh-llm 非 PD 路径 baseline。
4. PGX prefill + Mac decode 路径 baseline。

### 13.2 效率优化边界

| ID | 优先级 | 需求 |
|---|---|---|
| NFR-EFF-001 | Must | MVP 必须证明 PD 分离在目标长 prompt 场景下的收益或明确瓶颈。 |
| NFR-EFF-002 | Must | MVP 必须记录并区分 prefill 计算耗时、KV handoff 耗时、decode 耗时，不能只给总耗时。 |
| NFR-EFF-003 | Must | MVP 必须记录 KV handoff bytes，用于判断网络是否成为瓶颈。 |
| NFR-EFF-004 | Must | MVP 必须保留 Mac-only、PGX-only、现有 mesh path 作为性能对照。 |
| NFR-EFF-005 | Should | 需求应为后续 KV 压缩、增量传输、batching、自动 placement 预留可观测指标。 |
| NFR-EFF-006 | Should | 短 prompt 和长 prompt 应分开评估，避免平均值掩盖收益或退化。 |
| NFR-EFF-007 | Could | 后续可定义按 prompt length、KV size、worker health、network latency 选择 PD 路径的策略。 |

当前不进入需求文档的具体优化方案：

1. KV 压缩格式或压缩算法。
2. KV 分块、流水线、增量传输算法。
3. prefill batching 或 decode batching 调度算法。
4. worker 自动 placement 算法。
5. 网络传输协议内部实现。
6. KV cache 生命周期和内存管理实现细节。

### 13.3 正确性

| ID | 优先级 | 需求 |
|---|---|---|
| NFR-COR-001 | Must | 对同一模型/tokenizer/采样设置，PD 路径输出必须与基线路径具备可解释一致性。 |
| NFR-COR-002 | Must | 不兼容组合必须 fail closed，不能输出疑似正确但未验证 token。 |
| NFR-COR-003 | Should | correctness 验收应覆盖短 prompt、长 prompt、边界上下文长度和 streaming 场景。 |

### 13.4 可观测性

| ID | 优先级 | 需求 |
|---|---|---|
| NFR-OBS-001 | Must | 请求阶段必须可诊断：queued、prefilling、handoff、decoding、completed、failed、cancelled。 |
| NFR-OBS-002 | Must | 错误必须有分类：worker unavailable、compatibility mismatch、handoff timeout、decode failure、cancelled。 |
| NFR-OBS-003 | Must | 观测数据不得包含 prompt 内容、KV 内容或凭据。 |
| NFR-OBS-004 | Should | status surface 应能展示 PD 模式是否启用、参与 worker、最近失败原因和关键指标。 |
| NFR-OBS-005 | Should | status/diagnostic surface 应能区分“功能可用但性能无收益”和“功能不可用”。 |

### 13.5 安全与隐私

| ID | 优先级 | 需求 |
|---|---|---|
| NFR-SEC-001 | Must | KV cache 视为敏感数据，安全级别接近 prompt。 |
| NFR-SEC-002 | Must | 未经手动配置允许的节点不得参与 MVP PD 请求。 |
| NFR-SEC-003 | Must | 日志、telemetry、错误响应不得泄露 prompt/KV/凭据/内部私有路径。 |
| NFR-SEC-004 | Must | 外部服务、测试机器、HF token、Nostr/OTLP endpoint 和 credentials 不得进入仓库。 |
| NFR-SEC-005 | Should | 若未来支持公共 mesh 或跨 owner PD，必须重新定义 trust boundary。 |

### 13.6 兼容性与可回退

| ID | 优先级 | 需求 |
|---|---|---|
| NFR-COMP-001 | Must | 默认关闭 PD 时，现有请求路径行为不变。 |
| NFR-COMP-002 | Must | PD 失败策略已确认：回退到现有正常 mesh-llm 路径。 |
| NFR-COMP-003 | Must | 非 PD baseline 必须保留用于对照和回退。 |
| NFR-COMP-004 | Should | 兼容性矩阵应可扩展到更多模型、backend 和 quantization。 |

## 14. MVP 定义

MVP 是最小可验收版本，不是最终产品形态。

| 维度 | MVP 口径 |
|---|---|
| 拓扑 | 两台 PGX prefill workers + 一台 Mac Studio decode worker。 |
| 测试环境来源 | operator-local private env file 中的 `MESH_MACSTUDIO_*`、`MESH_PGX_30BE_*`、`MESH_PGX_3030_*`、`MESH_GEMMA_*` 变量；不复制 credentials。 |
| 激活 | 推荐默认关闭，手动开启。 |
| Placement | 推荐手动指定 prefill workers 和 decode worker。 |
| 能力发现 | 只用于识别、诊断、校验，不做全自动调度。 |
| 并发 | 允许只支持单 request in-flight。 |
| Decode | 允许只支持单 decode worker。 |
| 模型 | MVP 测试模型为 `google_gemma-4-31B-it-bf16`。 |
| Tokenizer | 只支持该 GGUF artifact 内嵌的 `gemma4` tokenizer 元数据。 |
| Backend/dtype/quantization | 只支持一组已验证兼容组合；dtype/precision 按正常 16-bit/bf16 口径；不做低精度 quantization。 |
| Handoff | 必须跨机器完成 KV handoff 或等价 decode 初始状态交接。 |
| API | 保持现有 OpenAI-compatible 主路径，不破坏非 PD 路径。 |
| 观测 | 必须记录 prefill latency、KV bytes、handoff latency、decode tokens/sec、end-to-end latency。 |
| 效率目标 | 必须量化长 prompt 场景收益和瓶颈；不承诺 MVP 达到最终性能最优。 |
| 失败处理 | PD 失败时回退到现有正常 mesh-llm 路径；必须覆盖 worker 不可用、KV mismatch、模型/tokenizer mismatch、网络超时、请求取消、配置与能力不一致。 |

## 15. 后续扩展范围

MVP 之后可以扩展，但不进入第一版必做：

1. 半自动 placement。
2. 全自动 placement。
3. 多 decode workers。
4. 多 request 并发。
5. 多模型、多 tokenizer、多 artifact。
6. 多 dtype / quantization / backend 兼容矩阵。
7. KV cache 压缩、分块、增量传输。
8. KV cache 跨请求复用。
9. speculative decoding 与 PD 分离组合。
10. 更细粒度 streaming 策略。
11. 管理 UI 展示 PD 拓扑、队列和性能。
12. OpenAPI/JSON schema 生成。
13. 自动 benchmark/report。
14. 公共 mesh 或跨 owner 的 PD 分离。
15. per-request 策略覆盖，例如特定请求显式要求或禁止 PD 分离。
16. prefill batching、decode batching 或 mixed batching。
17. 基于 prompt length、KV size、network latency 和 worker health 的自动性能策略。
18. KV 压缩算法、增量 handoff 算法和传输协议优化。

## 16. 开放问题

这些问题必须在进入实现设计前明确，或被记录为 MVP 限制。

| ID | 问题 | 建议默认口径 | 影响 |
|---|---|---|---|
| OQ-001 | MVP 是否确认手动开启 PD 分离？ | 架构推荐：默认关闭，手动开启，手动 placement。 | 范围、安全、排障 |
| OQ-002 | PD 开启粒度是什么？ | MVP 建议 per-model 或显式运行配置；Phase 3 设计具体 surface。 | 配置/API |
| OQ-003 | PD 失败时默认回退还是返回错误？ | 已确认：回退到现有正常 mesh-llm 路径。 | API 行为、正确性 |
| OQ-004 | streaming decode 中途失败是否允许返回 partial output？ | 待 owner 决策。 | API 行为 |
| OQ-005 | MVP 指定哪个模型 artifact 和 tokenizer？ | 已确认：`google_gemma-4-31B-it-bf16`；tokenizer 使用该 GGUF artifact 内嵌 `gemma4` tokenizer 元数据。 | 验收环境 |
| OQ-006 | PGX 与 Mac 的 backend/dtype/quantization 兼容组合是什么？ | 已收敛：正常 16-bit/bf16 口径；不做低精度量化；backend 以版本化 KV 合同校验，具体组合由 Phase 3 验证。 | 正确性 |
| OQ-007 | 性能验收阈值是什么？ | 已确认：先功能打通、单用户可用、建立可测量 baseline；固定收益阈值后续再定。 | 验收 |
| OQ-008 | 网络带宽是否足够支撑 KV handoff？ | 必须实测 KV bytes 和 handoff latency。 | 可行性 |
| OQ-009 | 是否复用现有 Skippy split serving 代码？ | 保持现有口径：必须优先评估复用，但不在需求中指定实现。 | 实施范围 |
| OQ-010 | 管理 API/status 是否必须展示 PD 状态？ | 建议 MVP 至少有诊断状态。 | 可观测性 |
| OQ-011 | Capability discovery 字段是否进入 mesh gossip 或其他 surface？ | 保持现有口径：MVP 用于识别、诊断、校验；具体 surface 待 Phase 3 设计。 | 协议/API |
| OQ-012 | 是否允许公共 mesh 或跨 owner 节点参与 PD？ | MVP 不允许。 | 安全 |
| OQ-013 | MVP 性能目标是“功能打通 + 可测量”，还是必须达到某个收益阈值？ | 已确认：先功能打通、单用户可用、可测量 baseline。 | 性能验收 |
| OQ-014 | 长 prompt 和短 prompt 是否采用不同验收标准？ | 建议分开验收。 | 性能验收 |

## 17. 验收矩阵

| 验收项 | 对应需求 | MVP 必须 |
|---|---|---|
| 默认关闭 PD，不影响现有路径。 | FR-AP-001、NFR-COMP-001 | 是 |
| 手动开启后，PGX prefill + Mac decode 端到端成功。 | FR-PW-001、FR-DW-002、FR-KV-001 | 是 |
| MVP 使用 `google_gemma-4-31B-it-bf16` 完成适配验证。 | D-013、FR-MC-001 | 是 |
| MVP tokenizer 使用该 GGUF artifact 内嵌 `gemma4` tokenizer 元数据。 | FR-TK-001、10.3 节 | 是 |
| PD 设计不得写死为 Gemma-only。 | D-013、FR-MC-005 | 是 |
| KV handoff 缺失版本或版本不匹配时 fail closed。 | FR-KV-003、FR-KV-008 | 是 |
| 模型/tokenizer mismatch fail closed。 | FR-MC-002、FR-TK-001 | 是 |
| backend/dtype/quantization 不兼容 fail closed；MVP 不做低精度 quantization。 | D-016、FR-MC-004、FR-FT-007 | 是 |
| 未配置允许的候选 worker 不参与请求。 | FR-AP-003、FR-AP-004、FR-FT-011 | 是 |
| 现有 `/v1/*` 主路径不被替换。 | FR-API-001、FR-API-008 | 是 |
| 非 PD baseline 保留。 | FR-API-009、NFR-COMP-003 | 是 |
| PD 失败时回退到现有正常 mesh-llm 路径。 | D-012、FR-CR-006、NFR-COMP-002 | 是 |
| 记录 prefill latency、KV bytes、handoff latency、decode tokens/sec、end-to-end latency。 | NFR-PERF-001 到 NFR-PERF-006 | 是 |
| 记录并区分 prefill、handoff、decode 三段耗时。 | NFR-EFF-002 | 是 |
| 建立 Mac-only、PGX-only、现有 mesh path 和 PD path baseline。 | NFR-EFF-004 | 是 |
| 日志/telemetry 不记录 prompt 或 KV 内容。 | FR-KV-011、NFR-SEC-003 | 是 |

## 18. Phase 2 输出物

后续 Phase 2 文档仍应保持需求/范围粒度，不进入实现计划：

1. `OPEN_QUESTIONS.zh.md`：owner 未确认问题。
2. `ACCEPTANCE_CRITERIA.zh.md`：验收指标、测试场景、阈值。
3. `API_COMPATIBILITY_SCOPE.zh.md`：不能破坏的 API 行为。
4. `MODEL_KV_COMPATIBILITY_SCOPE.zh.md`：模型、tokenizer、KV、dtype、backend 兼容性边界。
5. `ACTIVATION_PLACEMENT_SCOPE.zh.md`：PD 分离如何开启、worker 如何被允许参与、能力发现如何用于校验。
6. `PERFORMANCE_EFFICIENCY_SCOPE.zh.md`：速度/效率目标、baseline、指标、MVP 与后续优化边界。

## 19. 进入架构设计前的关口

在进入架构设计或实现计划前，至少需要完成：

1. MVP 激活策略按架构推荐进入 Phase 3：默认关闭、手动开启、手动 placement。
2. MVP 范围按架构推荐进入 Phase 3：允许只支持单 request 和单 decode worker。
3. Fallback 策略已确认：PD 失败时回退到现有正常 mesh-llm 路径；第三阶段需设计其触发边界和可观测性。
4. MVP 模型 artifact 已确认：`google_gemma-4-31B-it-bf16`；tokenizer 来源已收敛为该 GGUF artifact 内嵌 tokenizer 元数据。
5. MVP 兼容组合已收敛：正常 16-bit/bf16 口径，不做低精度 quantization；backend 兼容性由 Phase 3 的 KV 合同和验证矩阵确认。
6. MVP 性能口径已确认：先功能打通、单用户可用、建立可测量 baseline；固定收益阈值后续再定。
7. 测试环境来源已确认：operator-local private env file；Phase 3 只引用变量名/角色，不复制 credentials。
8. Skippy split serving 复用评估保持现有口径：优先评估复用，但需求文档不指定实现。
9. Capability discovery 保持现有口径：MVP 用于识别、诊断、校验；具体 API/gossip/config surface 由 Phase 3 决定。
10. 短 prompt 和长 prompt 建议分开验收；Phase 3 需定义 baseline 场景。
