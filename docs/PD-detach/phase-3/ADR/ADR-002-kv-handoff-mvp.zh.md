# ADR-002：MVP 以 Native KV Page Handoff 为目标方案，但必须 spike

日期：2026-05-19  
状态：Accepted as spike-gated target  

## 背景

目标场景要求 PGX 执行 prompt/prefill，Mac Studio 执行 token-by-token decode。只有把 KV cache 或等价 decode 初始状态从 PGX 交给 Mac，才满足目标场景。

现有代码中已有 Skippy KV manifest、prefix identity、exact state export/import 相关能力，但尚未证明跨机器、跨 backend 可用。

## 决策

MVP 目标方案选择 Native KV Page Handoff：

1. PGX prefill 后导出 KV page bytes。
2. PGX 生成 manifest，包含模型、tokenizer、runtime ABI、layout、codec、token range、checksum。
3. Mac 校验 manifest。
4. PGX 分块传输 KV payload。
5. Mac 导入 KV 到 decode session 后继续 decode。

该方案必须先通过 spike。若 PGX->Mac import 失败或输出不一致，不能直接进入完整 MVP 实现。

## 理由

1. 最符合 PD 语义：decode worker 不重算 prompt。
2. 可复用现有 Skippy KV 相关设计。
3. 能直接测量 prefill/export/transfer/import/decode 成本。
4. 保留后续 KV 压缩、分块、增量传输优化空间。

## 替代方案

| 方案 | 结论 |
|---|---|
| Full runtime state handoff | 更大、更不透明，跨 backend 风险更高，只作为备选 spike。 |
| Skippy activation chain | 可复用基础设施，但语义是按层 split，不是完整 PD handoff。 |
| Decode 端 prompt replay | 不满足 KV handoff，不算 MVP。 |
| 外部对象存储 | 引入额外依赖和安全风险，不进 MVP。 |

## 后果

正面：

- 架构目标清晰。
- 验证指标可量化。
- 和未来性能优化方向一致。

风险：

- `ggml-native-kv` 可能不能跨 CUDA/Metal 直接导入。
- KV bytes 可能过大。
- bf16 模型权重与 f16 KV codec 需要明确区分。

## 证据

- `docs/PD-detach/phase-3/KV_HANDOFF_DESIGN.zh.md`
- `crates/skippy-server/src/kv_proto.rs`
- `crates/skippy-server/src/kv_integration/identity.rs`
- `crates/skippy-server/src/kv_integration/exact_state.rs`
- `crates/skippy-cache/src/identity.rs`
