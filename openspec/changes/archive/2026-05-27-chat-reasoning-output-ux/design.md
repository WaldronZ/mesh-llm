# Design: Chat Reasoning Output UX

## Problem Boundary

This change addresses Chat presentation and `/api/responses` stream metadata.
It does not alter PD serving correctness. The backend Gemma4 reasoning channel
parser now separates reasoning from final answer content, so the UI should
prefer first-class reasoning events and keep tag parsing as a compatibility
fallback rather than hiding backend raw-marker bugs.

## Stream Model

The preferred path is first-class streaming metadata:

- `response.reasoning_text.delta` feeds the thinking channel;
- `response.output_text.delta` feeds the final answer channel;
- `response.completed` carries response metadata such as usage and, when
  available, finish reason.

If first-class reasoning deltas are unavailable, the UI may parse explicit
`<think>...</think>` spans from assistant text as a compatibility fallback.
Fallback parsing must not treat arbitrary untagged streaming text as thinking,
and an isolated `</think>` marker must not retroactively convert preceding
answer text into thinking.

## Thinking Presentation

During streaming, active thinking should be visible as a separate section with a
small status label. When thinking ends and final answer content starts, the
thinking section should become completed supporting context and default to
collapsed. Users can expand it when they want to inspect the reasoning trace.

The final answer remains the primary assistant message content. Thinking should
not visually compete with final output after completion.

## Truncation UX

When the final response metadata indicates `finish_reason=length`, the UI
should show a clear warning near the assistant message. The warning should say
that the answer may have been truncated by the output limit.

The following affordances are intentionally deferred and are not implemented by
this change:

- continue generation;
- increase output limit;
- shorten input or reduce context pressure.

The current implementation only renders the warning. Continue-generation,
settings-panel output-limit controls, and context budget guidance remain future
UX work.

## Output Token Limit

`max_output_tokens` should be included in the `/api/responses` request payload.
This first apply keeps a temporary manual-testing default of 4096 so long-output
UI checks do not stop at the older implicit limit. A later UI setting can
expose common values such as 256, 512, 1024, 2048, and 4096.

The request builder should avoid hiding the output limit in an adapter-only
default. The browser payload should be inspectable so operators can distinguish
context-window issues from output-limit truncation.

## Context Budget Warning

Context budget warning remains deferred. A complete budget warning requires a
reliable input token estimate and an effective runtime context limit. The UI
must not promise long-context capacity based only on model metadata.

## Privacy

Tests, logs, and reports must not include prompt text, generated content,
complete token arrays, KV/native payloads, credentials, private paths, endpoint
URLs, or real machine labels.

## Compatibility

Existing chat rendering should continue to work for streams that only emit
`response.output_text.delta`. Existing explicit `<think>...</think>` content
remains supported as a fallback. New metadata fields should be optional so older
servers or adapters do not break the UI.
