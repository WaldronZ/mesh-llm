# Change: Gemma4 Reasoning Channel Parsing

## Why

Manual Chat testing showed raw Gemma4 channel markers in visible assistant
content. A direct `/v1/chat/completions` request to the local router already
emitted `<|channel>thought` and `<channel|>` through
`choices.delta.content`, with no `choices.delta.reasoning_content`.

This means the primary issue is not Chat UI styling. The Gemma4 chat template
uses channel markers for thought and final output, and the backend parser must
split those channels before OpenAI-compatible responses reach adapters or UI.

## What Changes

This change enables Gemma4 channel thought extraction in the llama.cpp Gemma4
parser generation path, keeps Skippy parser metadata aligned with that
behavior, and forwards parsed reasoning through the Responses stream adapter:

- Gemma4 `<|channel>thought ... <channel|>` output is parsed as
  `reasoning_content`;
- final answer text remains `content`;
- `/v1/responses` maps chat `reasoning_content` deltas to
  `response.reasoning_text.delta`;
- raw Gemma4 channel markers are not emitted as normal output text.

## Scope

Must:

- detect Gemma4 chat templates or template parser format and enable reasoning
  channel parsing for that format;
- ensure the serialized Gemma4 parser is generated with reasoning extraction
  enabled when the template supports thinking;
- ensure `/v1/chat/completions` streaming sends thought channel text through
  `choices.delta.reasoning_content`, not `choices.delta.content`;
- ensure final answer content does not include raw `<|channel>thought` or
  `<channel|>` markers;
- ensure `/v1/responses` maps chat reasoning deltas to
  `response.reasoning_text.delta`;
- preserve non-reasoning model behavior;
- add parser and adapter regression tests;
- avoid logging prompt text, generated content, complete token arrays,
  KV/native payloads, credentials, private paths, or real machine labels.

Won't:

- change PD runtime, PD protocol, chunked prefill, or large-state framing;
- change runtime handoff/framing behavior;
- redesign Chat UI visuals;
- implement thinking collapse, output-limit controls, truncation hints, or
  continue-generation UX; those belong to `chat-reasoning-output-ux`;
- add model-specific prompt hacks that only hide text after generation when the
  parser can split channels correctly;
- log generated content in diagnostics or reports.

## Impact

The intended behavior change is limited to OpenAI-compatible chat and
responses parsing. Models that do not emit recognized reasoning channels should
continue to stream ordinary output text as before.
