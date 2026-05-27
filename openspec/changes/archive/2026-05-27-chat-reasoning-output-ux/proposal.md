# Change: Chat Reasoning Output UX

## Why

The backend Gemma4 reasoning channel parser now separates model reasoning from
final answer content: `/v1/chat/completions` emits `reasoning_content`, and
`/v1/responses` maps that to `response.reasoning_text.delta`. The remaining
manual Chat experience issues are UI/API-adapter concerns, not PD runtime
blockers:

1. reasoning or thinking text can appear mixed with the final answer;
2. completed thinking traces remain too prominent instead of becoming
   inspectable supporting context;
3. token-limit truncation can feel like an abrupt answer stop because the UI
   does not clearly surface `finish_reason=length`;
4. `max_output_tokens` must be explicit in the request payload, with a path
   toward user configuration.

This change keeps those concerns out of the PD chunked prefill and large-state
framing changes.

## What Changes

Implement a minimal Chat UX layer for reasoning-aware streaming and
output-limit feedback:

1. preserve first-class reasoning stream events as the thinking channel when
   available;
2. continue supporting explicit `<think>...</think>` text parsing as a
   compatibility fallback;
3. show thinking while it streams, then collapse it by default after completion;
4. keep the final answer as the primary visible assistant content;
5. surface `finish_reason=length` as a truncation warning;
6. include `max_output_tokens` in `/api/responses` request payloads with a
   temporary manual-testing default of 4096;
7. document user-configurable output limits, continue-generation UX, and
   context budget guidance as deferred follow-up work.

## Scope

Must:

- preserve `response.reasoning_text.delta` as a separate thinking channel when
  the stream provides it;
- support explicit `<think>...</think>` fallback parsing for models or adapters
  that emit thinking as tagged text;
- keep orphan or isolated `</think>` markers in final answer content rather
  than treating preceding untagged text as thinking;
- show active thinking during streaming;
- collapse completed thinking by default while keeping it expandable;
- keep final answer text as the primary visible content;
- surface `finish_reason=length` as a clear truncation warning;
- include `max_output_tokens` in the request payload with a temporary
  manual-testing default of 4096 until a settings UI lands;
- add tests for thinking parsing/rendering and `max_output_tokens` payloads.

Should:

- keep the output-limit path compatible with a future settings panel using
  common values such as 256, 512, 1024, 2048, and 4096;
- keep continue-generation and context budget guidance as explicit deferred
  follow-ups rather than implemented actions in this change;
- preserve the existing Chat visual style and message rendering structure as
  much as possible.

Won't:

- change PD runtime, binary protocol, or server-side state handoff;
- change chunked prefill or large-state framing;
- promise 8k, 32k, 128k, or 256k behavior;
- enforce a separate reasoning token budget;
- log prompt text, generated content, complete token arrays, KV/native payloads,
  credentials, private paths, or real machine labels.

## Closure Notes

This change is implemented in the Chat UI request builder, streaming parser,
response metadata mapping, assistant message renderer, thinking segment
renderer, and related UI tests. It does not archive, commit, push, start local
or remote serving processes, restore presentation files, or change PD/server
runtime behavior.

Deferred follow-ups:

- generation settings panel for common output limits;
- continue-generation workflow;
- context budget estimator and warning;
- changing the temporary 4096 manual-testing default into a user-configurable
  setting.
