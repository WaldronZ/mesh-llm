# PD Chunked Prefill Smoke Report

## Result

- result: pass
- recommendation: request_8k_optional_smoke
- scope: 4k-only chunked prefill foreground smoke after large-state framing
- conclusion: 4k chunked prefill completed the real PD path:
  PGX chunked prefill -> final large-state export -> Mac import/decode -> SSE
  normal completion.

## Environment

- Mac binary sha256: `b607aeb69ddf56fe880d8d107358eeda2d87ef491b476c262f15d733a7904902`
- PGX binary sha256: `40a88f75a753305fec58e2d643ce135fb5867f69d69164b8992440713833b84b`
- model entry shard basename: `google_gemma-4-31B-it-bf16-00001-of-00002.gguf`
- model entry shard sha256: `df3ddef50bfc84938ee22387d4c96fb1a549a1d4f8f425fe84cbf558c41dfd29`
- tokenizer metadata hash: `6aa0dc8786823d04fb6d953994df47eb4f9382ed07efd8898411659778e0a397`
- chat template hash: `f86783fcbe17e6e9bd84d7246344a8a2f8c4d35860ca14edef0fc90559a528a3`
- real machine names recorded: no
- private paths recorded: no

## Prompt Suite

| prompt id | target class | tokenizer prompt tokens | prefill tokens | expected admission | observed admission |
|---|---:|---:|---:|---|---|
| chunked-4k | 4k | 3903 | 3902 | admitted | admitted |

Prompt text and complete token arrays were not recorded.

## Chunked Prefill Metrics

| prompt id | chunked enabled | chunk size | observed chunk count | chunk tokens | total prefill ms | final decode start position |
|---|---|---:|---:|---|---:|---:|
| chunked-4k | true | 1024 | 4 | 1024, 1024, 1024, 830 | 4917.711 | 3902 |

Position continuity was verified from PGX telemetry:

| chunk index | tokens | KV tokens after chunk |
|---:|---:|---:|
| 0 | 1024 | 1024 |
| 1 | 1024 | 2048 |
| 2 | 1024 | 3072 |
| 3 | 830 | 3902 |

The final decode start position matched the total prefill tokens. No fallback
path was used.

## Large-State Handoff And Decode Metrics

| prompt id | state payload bytes | large-state protocol | frame count | frame bytes | export ms | network ms | import ms | TTFT ms | decode tok/s |
|---|---:|---|---:|---:|---:|---:|---:|---:|---:|
| chunked-4k | 3516265368 | large-state-framing/1 | 210 | 16777216 | 15385.536 | 30685.686 | 14098.603 | 79249.602 | 9.860 |

The final PGX export used large-state frame streaming. Mac import/decode
completed successfully and the client stream reached normal SSE completion.

## Request Result And Cleanup

- HTTP status: 200
- SSE normal completion: yes
- SSE error observed: no
- fallback used: no
- Mac import/decode completed: yes
- PGX process survived until manual stop: yes
- Mac decode process survived until manual stop: yes
- Mac router process survived until manual stop: yes
- port release confirmed: yes

## Privacy Review

Confirmed absent from this report:

- prompt text
- complete token arrays
- generated content
- KV payload contents
- native state payload contents
- credentials
- private paths
- endpoint URLs
- real machine labels

## Remaining Smoke Scope

- Baseline below the current one-shot limit was not rerun in this smoke.
- 8k remains intentionally unrun and requires separate authorization.
- Over-policy rejection/fallback was not rerun in this smoke.

## Scope Closure

- current scope: 4k chunked prefill pass
- 8k: future validation, not part of this archive scope
- baseline below one-shot limit: deferred; not required for the 4k chunked
  prefill pass
- over-policy smoke: deferred; prior admission changes cover pre-PGX
  rejection/fallback semantics and it was not rerun here
- UI reasoning/final-answer separation: out of scope; defer to a future Chat
  UI/UX change
- output truncation UX for `finish_reason=length`: out of scope; defer to a
  future Chat UI/UX change
- configurable `max_output_tokens`: out of scope; defer to a future Chat UI/UX
  change
- finish reason / completion token count reporting gap: future UI/UX or
  reporting improvement, not a chunked prefill correctness blocker

## Recommendation

The 4k chunked prefill scope is ready to archive. The 4k blocker introduced by
the previous i32 state payload framing limit is resolved for this foreground
smoke. Request separate authorization before running any optional 8k foreground
smoke.
