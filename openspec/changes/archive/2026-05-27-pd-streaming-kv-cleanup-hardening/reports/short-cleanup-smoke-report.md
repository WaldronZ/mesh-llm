# Short Cleanup Smoke Report

## Result

`pass`

The short foreground cleanup smoke verified that a production streaming KV page
stream stall/failure no longer leaves the router permanently stuck behind the
single generation slot.

## Scope

- No 4k or 8k benchmark was run.
- No full-state fallback was used as a streaming KV pass.
- The smoke used the production `serve-openai --pd-streaming-kv-handoff` router
  and the production `serve-binary --pd-streaming-kv-source` source.
- Prompt text, generated content, full token arrays, KV/native payloads,
  private paths, endpoint URLs, hostnames, and credentials were not recorded.

## Procedure

1. Rebuilt the local `skippy-server` binary with cleanup hardening.
2. Stopped the stale Mac router process with SIGINT.
3. Restarted the Mac router in a detached local terminal session.
4. Confirmed a short production `/v1/responses` request returned HTTP 200,
   `[DONE]`, and non-empty output deltas.
5. Sent a bounded synthetic request that produced one page segment and then
   stalled waiting for the second segment.
6. Sent a second short request while the first was in flight and observed a
   temporary 429 with `retry-after: 1`.
7. Waited for bounded page read timeout cleanup.
8. Sent a third short request and observed HTTP 200, `[DONE]`, and non-empty
   output deltas.

## Evidence

- Stalled request token count: `156`.
- Stalled request elapsed: `61.547s`.
- During-stall request: HTTP `429`, `retry-after: 1`.
- Router lifecycle evidence:
  - `router_page_frame_receive_start` for segment 1;
  - `router_request_error` with `failure_phase=page_stream` and
    `failure_reason=page_read_timeout`;
  - `router_cleanup`;
  - subsequent request reached `router_final_contiguous_gate_pass`,
    `router_decode_start`, and `router_cleanup`.
- Source lifecycle evidence:
  - request error was scoped to the current request;
  - `request_cleanup`;
  - `listener_continue`;
  - subsequent request was accepted and completed.

## Conclusion

The stale generation slot was released after the bounded page stream timeout.
The next request was not permanently blocked by 429 and completed normally.
