# Design: Gemma4 Reasoning Channel Parsing

## Root Cause

Gemma4 GGUF chat templates can emit thought using channel markers:

- `<|channel>thought`
- `<channel|>`

The local llama.cpp Gemma4 parser decides whether the thought channel becomes
reasoning while it builds the serialized PEG parser. That happens before
mesh-llm stores the parser metadata used later by Skippy.

The first attempted fix only normalized Skippy metadata from
`reasoning_format=none` to `deepseek`. That was too late for the live path:
the serialized Gemma4 parser had already been built with
`extract_reasoning=false`, so the parser rule still tagged the thought channel
as ordinary content. OpenAI streaming then sent the raw channel text as
`choices.delta.content`; the Responses adapter inherited that as
`response.output_text.delta`.

## Backend Parser Strategy

The llama.cpp Gemma4 parser generation should treat Gemma4 template thinking
support as a reason to extract the thought channel. The lowest-risk path is
template-driven:

- when Gemma4 chat params are initialized, use `inputs.enable_thinking` as
  sufficient signal to extract the `<|channel>thought` region;
- keep a non-`none` parser metadata value for Gemma4 thinking templates so the
  downstream Skippy parser also knows reasoning extraction is enabled;
- use `deepseek` as the existing llama.cpp-compatible non-`none`
  `common_reasoning_format` value because this pinned llama.cpp exposes
  `none`, `auto`, `deepseek`, and `deepseek-legacy`, not a Gemma-specific enum;
- keep metadata as `none` for templates that do not support thinking.

This keeps the behavior tied to the parser/template capability rather than to
the public model id.

The durable llama-side change is carried in the patch queue, not only in the
prepared `.deps/llama.cpp` checkout.

## Responses Adapter Strategy

The Responses streaming route already consumes Chat Completions chunks. It
must treat chat deltas independently:

- `choices.delta.reasoning_content` becomes
  `response.reasoning_text.delta`;
- `choices.delta.content` remains `response.output_text.delta`;
- completion usage and finish metadata continue to be emitted through the
  existing completed response event.

## Compatibility

Small non-reasoning deltas remain unchanged. Existing clients that only consume
`response.output_text.delta` still receive final answer text. Clients that
support reasoning events can display reasoning separately.

## Privacy

Tests should use synthetic marker strings only. Diagnostics must record marker
presence and event types, not full generated text.
