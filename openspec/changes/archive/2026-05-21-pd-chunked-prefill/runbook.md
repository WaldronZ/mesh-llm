# PD Chunked Prefill Runbook

This runbook is for a future foreground 4k/8k smoke only. It must not be used
without explicit operator authorization.

## Activation

Use the scoped PD serving path only:

```bash
skippy-server serve-openai \
  --pd-serving-mvp \
  --pd-chunked-prefill \
  --pd-prefill-chunk-size 1800 \
  --pd-admission-over-limit reject \
  --pd-max-prompt-tokens 8192 \
  --pd-max-prefill-batch 1800 \
  --pd-max-ctx-size 8192 \
  --pd-max-handoff-bytes <bytes> \
  --pd-estimated-kv-bytes-per-token <bytes-per-token> \
  --pd-prefill-addr 127.0.0.1:<prefill-port> \
  --pd-decode-addr 127.0.0.1:<decode-port> \
  --pd-expected-artifact-sha256 <sha256> \
  --pd-expected-tokenizer-hash <sha256> \
  --pd-expected-chat-template-hash <sha256> \
  --pd-source-node-id <sanitized-source-label> \
  --pd-target-node-id <sanitized-target-label>
```

Do not use `--pd-router-validation` for MVP smoke. Do not make PD serving
default-on.

## Prompt Suite

Construct synthetic prompts by target token count and record only:

- prompt id
- estimated token count
- request class: baseline, 4k, 8k, over-policy

Do not record prompt text, complete token arrays, generated content, KV payload
contents, credentials, private paths, endpoint URLs, or real machine labels.

## Foreground Process Checks

Before starting:

1. Confirm the Mac and PGX binaries support `--pd-serving-mvp` and
   `--pd-chunked-prefill`.
2. Confirm model artifact, tokenizer, and chat template hashes.
3. Check planned ports on Mac and PGX.
4. If a port is occupied, pause. Do not kill any process without a separate
   authorization.

Start only foreground observable processes:

1. PGX prefill/export binary stage.
2. SSH tunnel if required.
3. Mac decode/import binary stage.
4. Mac OpenAI router with `--pd-serving-mvp --pd-chunked-prefill`.

Stop in reverse order and confirm planned ports are released.

## Expected Tests

1. Baseline below the current one-shot envelope: admitted and PD path pass.
2. 4k prompt: admitted through chunked prefill, final export, Mac import/decode.
3. 8k prompt: admitted through chunked prefill, final export, Mac import/decode.
4. Over-policy prompt: rejected or fallback before PGX.
5. Manual interruption or configured test harness: chunk error, timeout, cancel,
   and cleanup behavior.

## Required Report Fields

- result: pass / fail / inconclusive
- recommendation: proceed_to_next_scale / redesign / run_more_validation
- chunked prefill enabled
- chunk size
- chunk count
- per-chunk token counts
- per-chunk prefill latency
- total prefill latency
- final decode start position
- final KV payload bytes
- export/import/network timing
- TTFT
- decode tokens/sec
- PGX process survival
- failure reason
- privacy review result
