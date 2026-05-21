# Tasks: Gemma4 Reasoning Channel Parsing

## 1. Proposal And Spec

- [x] 1.1 Create OpenSpec proposal, design, tasks, and spec.
- [x] 1.2 Validate the proposed change with `openspec validate
      gemma4-reasoning-channel-parsing --strict`.

## 2. Backend Parser

- [x] 2.1 Locate the stage chat metadata path that writes the parser
      `reasoning_format`.
- [x] 2.2 Locate the Gemma4 parser generation path that decides whether the
      thought channel is parsed as reasoning or ordinary content.
- [x] 2.3 Enable reasoning parsing for Gemma4/template-supported thinking
      without changing non-reasoning templates.
- [x] 2.4 Add regression coverage showing Gemma4 channel markers become
      reasoning content instead of final content.

## 3. Responses Stream Adapter

- [x] 3.1 Map chat `choices.delta.reasoning_content` to
      `response.reasoning_text.delta`.
- [x] 3.2 Preserve `choices.delta.content` as `response.output_text.delta`.
- [x] 3.3 Add regression coverage for mixed reasoning and output deltas.

## 4. Manual Local Verification

- [x] 4.1 Verify short `/v1/chat/completions` streaming output does not expose
      raw Gemma4 channel markers in content.
- [x] 4.2 Verify short `/v1/responses` streaming output does not expose raw
      Gemma4 channel markers in output text and emits reasoning events when
      reasoning is produced.

Notes:

- Pre-fix direct requests reproduced raw marker leakage in
  `/v1/chat/completions` content and `/v1/responses` output text.
- The first metadata-only fix was insufficient because the Gemma4 parser had
  already been serialized with thought-channel rules tagged as content.
- Post-fix live requests used a rebuilt local router on `127.0.0.1:19337`.
  Short streaming chat returned both reasoning and content deltas with no raw
  channel markers in content. Short Responses streaming showed both
  `response.reasoning_text.delta` and `response.output_text.delta` with no raw
  channel markers in output text.
- Chat UI rendering details such as thinking collapse, final-answer layout,
  output-limit controls, and truncation UX remain out of scope for this change
  and belong to the separate `chat-reasoning-output-ux` change.

## 5. Validation

- [x] 5.1 Run `cargo fmt --all -- --check`.
- [x] 5.2 Run affected backend tests.
- [x] 5.3 Run affected backend cargo check.
- [x] 5.4 Run `openspec validate gemma4-reasoning-channel-parsing --strict`.
- [x] 5.5 Run `git diff --check`.
