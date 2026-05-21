# Design: PD Long Context Scaling

## Context

The scoped PD serving MVP proves the basic path:

```text
/v1/chat/completions
  -> Mac coordinator/router/decode
  -> PGX prefill/export
  -> pd-handoff/1 native KV/decode-state payload
  -> Mac import/decode
  -> OpenAI-compatible streaming response
```

`pd-long-context-admission` then added a pre-prefill guard so that prompts
exceeding the configured PD envelope are rejected or sent to fallback before
PGX prefill starts.

This change defines how to grow that envelope. It is intentionally staged:
Phase A targets 4k/8k, Phase B prepares 32k with chunked prefill, and 256k
remains a feasibility target until the data says one-shot handoff is viable or
a later transfer design replaces it.

## Evidence Baseline

| Evidence | Value | Source |
|---|---:|---|
| Model metadata context | `gemma4.context_length=262144` | GGUF metadata parser used by `pd-kv-handoff-spike` |
| Current runtime context | `ctx_size=8192` | PD validation stage configs / admission smoke |
| Current prefill batch boundary | `n_batch=2048` | `crates/skippy-runtime/src/lib.rs` default and smoke context |
| Current safe prompt admission | about 1800 tokens | `pd-long-context-admission` smoke report |
| Current measured KV payload | about 900 KB/token | MVP and admission smoke reports |
| Current isolated network transfer | about 115 MB/s | MVP and admission smoke reports |

The metadata proves that the model artifact advertises 256k context. It does
not prove the current PD runtime can prefill, export, transfer, import, and
decode 256k safely.

## Why Direct 256k Is Not The Next Step

Measured one-shot KV payload size is approximately linear in prompt tokens for
the current native handoff path:

```text
kv_bytes ~= prompt_tokens * 900_000
```

Approximate payload sizes:

| Prompt tokens | One-shot KV payload | Network transfer at 115 MB/s |
|---:|---:|---:|
| 8k | ~7.4 GB | ~64 s |
| 32k | ~29.5 GB | ~4.3 min |
| 64k | ~59 GB | ~8.6 min |
| 128k | ~118 GB | ~17 min |
| 256k | ~236 GB | ~34 min |

These estimates do not include export/import copy peaks, model weights,
runtime work buffers, decode KV residency, or retry/cancel overhead. A 256k
one-shot handoff would likely make TTFT and memory pressure unacceptable.

## Admission Ladder

Long-context scaling requires a staged admission ladder, not a single prompt
token threshold.

Recommended gate order:

1. **Token gate**: prompt token count is known from coordinator tokenization.
2. **Context gate**: `prompt_tokens + requested_max_tokens <= ctx_size`.
3. **Prefill batch gate**: without chunked prefill, prompt tokens must fit the
   safe prefill batch envelope.
4. **KV bytes gate**: estimated handoff bytes must fit the configured budget.
5. **Memory gate**: prefill/export and decode/import workers must have enough
   available memory for model, KV, and copy buffers.
6. **Network/SLA gate**: estimated transfer time must fit the configured
   operator budget for TTFT or request latency.
7. **Lifecycle gate**: existing in-flight, busy, cancel, timeout, and cleanup
   constraints must still pass.

The gates should fail before PGX prefill starts. Over-limit outcomes remain
pre-content fallback or documented pre-content rejection.

## Calibration Model

The current configured value `estimated_kv_bytes_per_token=524288` is lower
than measured payloads around 900 KB/token. Scaling must replace or correct
that value before increasing prompt caps.

Recommended calibration behavior:

- derive default bytes/token from measured PD handoff rows for the active
  model/topology when available;
- allow an explicit operator override;
- record the source of the calibration as a bounded value such as
  `measured`, `configured`, or `conservative_default`;
- fail safe when a hard byte cap is configured but bytes/token cannot be
  estimated;
- report both estimated and actual handoff bytes during smoke validation.

The first implementation does not need a full online learner. A static
measured value carried from validation is acceptable if it is visible and
conservative. For the current Gemma4 native full-state PD topology, the
calibrated conservative value is `902000` bytes/token. Other model/topology
combinations must provide an explicit override or fail safe until they have
their own calibration.

## Config Relationships

The effective prompt admission envelope is:

```text
effective_prompt_limit =
  min(
    max_prompt_tokens,
    max_ctx_size - requested_max_tokens,
    max_prefill_batch_or_chunked_prefill_limit,
    max_handoff_bytes / calibrated_kv_bytes_per_token,
    memory_budget_tokens,
    network_budget_tokens
  )
```

Important implications:

- Raising `ctx_size` alone does not allow longer PD prompts.
- Raising `max_prompt_tokens` alone is unsafe if `n_batch` remains 2048 and
  chunked prefill is absent.
- Raising `n_batch` alone may increase memory pressure and does not address
  one-shot KV handoff cost.
- `max_handoff_bytes` becomes the dominant bound as prompt length grows.
- 32k cannot be treated as a config-only change unless chunked prefill and
  measured memory/network budgets prove it safe.

## Phase A: 4k / 8k Safe Scaling

Goal: safely test whether the current PD path can move beyond the 1.8k guard
without destabilizing PGX.

Phase A must:

- calibrate bytes/token to the measured value for the active Gemma topology;
- keep PD default-off;
- keep single Mac coordinator/router/decode and single PGX prefill worker;
- use explicit config for `ctx_size`, prefill batch envelope, handoff byte cap,
  and network/SLA cap;
- run 4k near-threshold and 8k near-threshold smoke only when the prefill
  strategy is safe;
- preserve over-threshold reject/fallback before PGX prefill;
- record actual KV bytes, export/import, isolated network transfer, TTFT,
  decode tokens/sec, and process survival.

If chunked prefill is not yet available, Phase A may only admit prompts that
fit the safe prefill batch envelope. In that case, 4k/8k should be documented
as blocked by `n_batch` until chunking or a proven larger batch is available.

## Phase B: 32k Chunked Prefill Prerequisites

32k requires chunked prefill or an equivalent prefill strategy. The current PD
MVP sends one prefill chunk to PGX. A later implementation must define how
multiple prefill chunks preserve:

- session identity;
- position accounting;
- token range continuity;
- stage ACK/error handling;
- cancellation and cleanup;
- export only after the final prefill chunk;
- deterministic correctness against baseline;
- telemetry for chunk count, chunk size, and per-chunk latency.

Phase B should not add chunked KV handoff. It should first prove that PGX can
prefill 32k safely and export a correct native state at the end.

## 256k Feasibility Boundary

256k should not be accepted as a deliverable until at least one of these is
true:

- one-shot handoff measurements show payload size, transfer time, and memory
  pressure are acceptable for the target machines;
- streaming/chunked KV handoff or paging changes the cost model;
- compression or a backend-native sharing strategy reduces transfer size
  without breaking correctness;
- the product requirement accepts very large TTFT and memory costs.

Without one of those conditions, 256k over the current one-shot handoff path is
a No-Go.

## No-Go Conditions

Long-context scaling should stop and require redesign if:

- calibrated bytes/token remains near 900 KB and one-shot payloads exceed the
  configured handoff byte budget;
- 4k/8k transfer time already exceeds the operator's latency/SLA budget;
- Mac decode cannot reserve enough memory for model plus imported KV plus copy
  buffers;
- PGX cannot prefill above the current envelope without chunked prefill;
- import/export requires full payload materialization and causes memory spikes;
- fallback/reject cannot happen before PGX prefill for over-limit requests;
- telemetry cannot distinguish token, context, batch, byte, memory, and network
  rejections.

## Testing And Reporting

Local tests should validate the admission ladder and calibration math without
starting remote processes.

Foreground smoke, when separately authorized, should run:

- 4k near-threshold request;
- 8k near-threshold request if Phase A config declares it safe;
- one over-threshold request expected to reject or fallback before PGX;
- process survival check for PGX prefill;
- telemetry capture for estimated/actual KV bytes and network transfer.

Reports must exclude prompt text, complete token arrays, generated content, KV
payload contents, credentials, private paths, and private machine labels.

## Non-goals

This change does not directly implement 256k, KV compression,
streaming/chunked KV handoff, multi-worker placement, scheduler behavior,
production concurrency, or performance benefit guarantees. It also does not
remove the existing long-context admission guard.
