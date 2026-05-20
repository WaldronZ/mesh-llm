# ADR-001：MVP 采用集中式 Coordinator

日期：2026-05-19  
状态：Accepted for Phase 3 target architecture  

## 背景

PD 分离需要在一次外部 OpenAI-compatible 请求中协调：

- 请求 eligibility。
- 手动 placement。
- PGX prefill worker。
- Mac decode worker。
- KV handoff。
- fallback。
- streaming response。

Phase 2 已确认 MVP 默认关闭、手动开启、手动 placement、单 request、单 decode worker，并要求 PD 失败回退现有 mesh-llm 正常路径。

## 决策

MVP 采用集中式 Coordinator，推荐由 Mac Studio 节点承担。

Mac Studio 同时承担：

- OpenAI-compatible ingress。
- PD Coordinator。
- Decode worker。

PGX 节点只承担 prefill worker 角色，不直接面对外部客户端。

## 理由

1. **Streaming 简单**：外部 token response 从一个节点发出，避免多节点同时管理 SSE。
2. **Fallback 可控**：首 token 前失败可由 Coordinator 直接回退现有 normal mesh route。
3. **符合 MVP 单请求限制**：集中式 admission gate 足够。
4. **靠近 decode worker**：Mac decode 生成 token 后不需要再跨网络给另一个 Coordinator。
5. **更容易诊断**：请求状态机、fallback reason、handoff metrics 集中记录。

## 替代方案

| 方案 | 未选择原因 |
|---|---|
| 分布式 Coordinator | 第一版复杂度过高，需要 leader election、状态复制、跨节点 streaming ownership。 |
| PGX 做 Coordinator | token streaming 需要回传 Mac decode 或让 PGX 面对客户端，不符合目标分工。 |
| 无 Coordinator，worker 点对点自发协作 | 无法清晰处理 fallback、取消、status 和安全边界。 |

## 后果

正面：

- MVP 架构简单。
- 更容易验证和回滚。
- API 兼容面集中。

代价：

- Mac Studio 可能成为控制面 bottleneck。
- 后续多 decode worker 或多 request 并发需要扩展 Coordinator admission 和 lane 管理。

## 证据

- `docs/PD-detach/phase-2/PREFILL_DECODE_REQUIREMENTS.zh.md`
- `docs/PD-detach/phase-2/PHASE_2_EXIT_REVIEW.zh.md`
- `crates/mesh-llm-host-runtime/src/network/openai/ingress.rs`
- `crates/mesh-llm-host-runtime/src/network/openai/transport.rs`
