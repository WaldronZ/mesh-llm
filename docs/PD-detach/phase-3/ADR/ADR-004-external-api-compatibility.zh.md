# ADR-004：外部 API 保持 OpenAI-compatible

日期：2026-05-19  
状态：Accepted  

## 背景

Phase 2 明确要求对外尽量保持 OpenAI-compatible API。现有代码中 `/v1/*`、`/models`、`/api/chat*`、`/api/responses*` 已经由 host runtime 的 API routes 和 OpenAI ingress/proxy 处理。

PD 分离是内部执行路径，不应迫使调用方理解 PGX/Mac worker、KV handoff 或内部协议。

## 决策

外部 API 保持兼容：

- 不新增必需的 PD endpoint。
- 不替换 `/v1/*`。
- 不改变 `/models` 语义。
- PD 默认关闭。
- PD 失败在首 token 前回退 normal mesh path。
- Management/status API 只做 additive 扩展。

## 理由

1. 降低客户端迁移成本。
2. 保留 normal mesh path 作为 fallback 和 baseline。
3. 允许 PD 作为独立 feature lane 演进。
4. 避免把内部 worker 拓扑暴露成外部产品契约。

## 替代方案

| 方案 | 未选择原因 |
|---|---|
| 新增 `/v1/pd/*` | 会要求客户端理解内部架构，破坏兼容目标。 |
| 通过 request 必填字段开启 PD | 破坏现有客户端；MVP 应手动配置开启。 |
| 替换 normal route | 风险过高，且失去 fallback/baseline。 |

## 后果

正面：

- 现有客户端无需修改。
- PD 可灰度、可关闭、可回退。
- OpenSpec 可聚焦内部协议和配置。

代价：

- Coordinator 需要在内部处理更多 eligibility/fallback 逻辑。
- Post-token failure 不能透明 fallback，需要明确 streaming policy。

## 证据

- `docs/PD-detach/phase-1/API_REFERENCE.md`
- `docs/PD-detach/phase-2/PREFILL_DECODE_REQUIREMENTS.zh.md`
- `crates/mesh-llm-host-runtime/src/api/routes/chat.rs`
- `crates/mesh-llm-host-runtime/src/network/openai/ingress.rs`
- `crates/mesh-llm-host-runtime/src/network/openai/transport.rs`
