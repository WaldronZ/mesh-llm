# ADR-003：复用 Skippy 基础设施，但不把现有 Skippy split serving 当作 PD MVP

日期：2026-05-19  
状态：Accepted for Phase 3 target architecture  

## 背景

mesh-llm 已集成 Skippy staged serving。Skippy 现有能力包括：

- stage control/status。
- binary activation transport。
- embedded runtime。
- layer package/topology。
- KV cache / exact state 相关代码。
- OpenAI frontend integration。

但是 Skippy split serving 的核心语义是按层切分，把 activation frames 从 stage0 传到 stageN；decode 每个 token 仍穿过 stage chain。PD 目标则是 PGX 完成 prefill 后，把 KV 或等价状态交给 Mac，由 Mac 独立 decode。

## 决策

复用 Skippy 的基础设施和经验，但不把现有 Skippy activation split path 直接定义为 PD MVP。

可复用：

- `skippy-protocol` 的版本化协议思路。
- stage control/status 和 `/api/runtime/stages` 的状态表达。
- `SkippyRuntimeHandle` / embedded runtime。
- KV manifest/identity/export/import 代码作为 spike 起点。
- telemetry 分段指标思想。

不直接复用为 MVP 语义：

- activation chain 作为 PGX/Mac PD 的最终数据流。
- 按层 topology planner 作为 PD worker scheduler。
- Q8 activation wire dtype 作为 MVP 优化。

## 理由

1. 避免把“按层 split”误认为“prefill/decode split”。
2. 最大化复用已有工程基础。
3. 保留后续把 PD 与 Skippy split 组合的可能性。
4. 降低无边界重写风险。

## 后果

正面：

- 设计更符合目标场景。
- spike 可以利用现有 runtime/KV 代码快速验证。
- 后续 OpenSpec 可以拆小。

代价：

- 不能直接宣称 Skippy 已满足 PD。
- 需要新增 PD handoff 合同或扩展 Skippy 能力。

## 证据

- `crates/skippy-server/README.md`
- `crates/skippy-protocol/README.md`
- `crates/mesh-llm-host-runtime/src/inference/skippy/`
- `docs/PD-detach/phase-1/ARCHITECTURE_CURRENT.md`
- `docs/PD-detach/phase-2/PREFILL_DECODE_REQUIREMENTS.zh.md`
