# PD KV Page Decode Bootstrap Report

## Result

`result`: `pass`

`recommendation`: `return_to_pd_kv_page_handoff_spike_closure`

## Scope

This report records the local implementation stage and the first small
foreground smoke attempt for `pd-kv-page-decode-bootstrap`. No 4k/8k prompt was
run, and no full-state handoff was used as a page-path pass.

## Local Implementation

- bootstrap strategy: `trim_replay_last_token`;
- imported page state is trimmed to `N - 1`;
- the final prompt token is replayed at position `N - 1`;
- logits are considered ready only after replay succeeds and session position
  returns to `N`;
- page-path decode then starts from the replay-produced first sampled token.

## Foreground Smoke Result

The two-chunk foreground smoke reached Mac-side page import before bootstrap.
After rebuilding the Mac Metal-linked correctness binary with direct ISWA trim
support, bootstrap completed the intended sequence:

- trim imported token count `N` to `N - 1`;
- replay the final prompt token at `N - 1`;
- produce current logits;
- restore decode start position to `N`;
- begin page-path decode.

An earlier run stopped later while creating the one-shot full-state baseline
restore session:

`no skippy execution lane is available`

Replay did run, `logits_ready=true`, and decode start position was proven as
`128`. Cross-runtime page decode correctness is still not proven because the
exact-match baseline comparison did not run.

After replacing that baseline with the local one-shot prefill/decode baseline,
the follow-up 128-token two-chunk foreground smoke completed the decode
bootstrap proof:

- page import completed;
- trim/replay produced current logits;
- `logits_ready=true`;
- `decode_start_position=128`;
- page-path decode exact-matched the local one-shot baseline.

The decode bootstrap blocker is cleared for the 128-token two-chunk scope.

## Scope Closure

This change does not implement `pd-streaming-kv-handoff`. It only proves the
decode bootstrap step needed after KV page import.

4k/8k, overlap/pipeline transfer, production scheduling, and throughput claims
are deferred to future changes.

## Privacy

Prompt text, generated content, complete token arrays, KV/native payload
contents, credentials, private paths, endpoint URLs, real machine labels, raw
pointers, and device addresses are excluded.
