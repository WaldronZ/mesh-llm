# PD-detach Phase 2

Status: requirements scope closed; ready for Phase 3 architecture design

Purpose: requirements and scope definition after Phase 1 Discovery.

Core Phase 2 target: heterogeneous Prefill/Decode separation, with PGX nodes
serving prefill and Mac Studio serving token-by-token decode while preserving
the existing OpenAI-compatible API as much as possible. Requirements are
defined with explicit decoupling boundaries: worker roles, activation policy,
KV handoff contract, compatibility matrix, API compatibility, and observability.

Current MVP inputs: `google_gemma-4-31B-it-bf16`, GGUF-embedded `gemma4`
tokenizer metadata, manual activation/placement, single request in-flight,
single decode worker, normal 16-bit/bf16 precision, no low-precision
quantization, and measurable single-user baseline first.

## Documents

| Document | Purpose |
|---|---|
| `PREFILL_DECODE_REQUIREMENTS.zh.md` | Chinese requirements and scope specification with decision records, requirement IDs, MVP boundary, open questions, and acceptance matrix. |
| `PHASE_2_EXIT_REVIEW.zh.md` | Exit review for deciding whether Phase 2 can close and Phase 3 architecture design can begin. |

Do not move Phase 1 evidence here. Reference `../phase-1/` when a requirement
depends on Discovery findings.

Do not write implementation plans in this directory until requirements and
scope are confirmed.
