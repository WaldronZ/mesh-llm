# Tasks: Chat Reasoning Output UX

## 1. Stream And Request Shape

- [x] 1.1 Confirm `/api/responses` request payload includes
      `max_output_tokens` with a default of at least 256.
- [x] 1.2 Preserve `response.reasoning_text.delta` as a separate thinking
      stream when present.
- [x] 1.3 Preserve `response.output_text.delta` as final answer content.
- [x] 1.4 Carry optional finish reason metadata from completed responses when
      available.

## 2. Thinking Rendering

- [x] 2.1 Keep `<think>...</think>` fallback parsing for completed and
      streaming assistant text.
- [x] 2.2 Ensure untagged assistant text remains final-answer content, not
      thinking.
- [x] 2.2a Ensure isolated `</think>` markers do not retroactively turn
      preceding untagged text into thinking.
- [x] 2.3 Show active thinking while streaming.
- [x] 2.4 Collapse completed thinking by default and allow user expansion.
- [x] 2.5 Keep final answer content primary and visible.

## 3. Truncation UX

- [x] 3.1 Surface `finish_reason=length` as an answer-truncated warning.
- [x] 3.2 Show the warning without logging generated content.
- [x] 3.3 Deferred / second phase: continue generation, generation settings,
      and context-budget guidance remain future UX work.

## 4. Output Limit UX Preparation

- [x] 4.1 Keep output limit explicit in the request builder.
- [x] 4.2 Use 4096 as a temporary manual-testing default until a settings UI
      lands. Deferred / second phase: user-selectable max-output choices
      256/512/1024/2048/4096.
- [x] 4.3 Avoid changing PD runtime, protocol, or server behavior.

## 5. Tests

- [x] 5.1 Test reasoning delta renders as thinking.
- [x] 5.2 Test output delta renders as final answer.
- [x] 5.3 Test `<think>...</think>` fallback parsing.
- [x] 5.4 Test untagged streaming text remains final answer content.
- [x] 5.5 Test completed thinking defaults to collapsed.
- [x] 5.6 Test `max_output_tokens` appears in request payload.
- [x] 5.7 Test length finish reason maps to truncation metadata or warning.

## 6. Validation

- [x] 6.1 Run targeted UI tests for the touched Chat files.
- [x] 6.2 Run UI typecheck or the repo-supported UI validation if the
      implementation touches typed API surfaces.
- [x] 6.3 Run `openspec validate chat-reasoning-output-ux --strict`.

Notes:

- The first apply keeps `max_output_tokens` explicit and uses 4096 as the
  manual testing default so long-output UI checks do not stop at 1024 tokens.
- A full generation settings panel and continue-generation workflow are
  intentionally deferred to a second phase. The long-term UX should let users
  choose common values such as 256/512/1024/2048/4096 instead of relying on a
  fixed manual-testing default.
- Backend Gemma4 reasoning channel parsing is already fixed. This UI change
  preserves first-class reasoning events and only keeps explicit
  `<think>...</think>` parsing as compatibility fallback; it does not hide raw
  channel marker leaks as a backend workaround.
- The UI now carries and renders `finish_reason` when it is present.
