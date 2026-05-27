# Design: PD Streaming KV Page Subframing

## Overview

The current production page stream treats a logical KV page segment as one
binary frame. For Gemma4 ISWA, the `swa` segment is much larger than the
`base` segment and can exceed practical frame limits even for medium prompts.
The proposed design keeps the logical KV page manifest, checksum, identity,
and import semantics intact, but slices the payload into bounded subframes on
the wire.

The router reassembles subframes into the original logical segment payload and
only then calls `import_kv_page`. This avoids changing the native runtime ABI
in this change.

## Wire Shape

`pd-kv-stream/1` should add a versioned page subframe shape for page stream
frames. A subframe header should include bounded metadata:

- protocol version;
- request id and request-scoped session id;
- chunk index and total chunk count;
- chunk token start/end or token count;
- segment index and segment count;
- cache kind and segment kind reference, such as `iswa/base` or `iswa/swa`;
- subframe index;
- subframe count, or an explicit final marker if count is not known up front;
- byte offset within the logical segment payload;
- subframe payload byte count;
- logical segment payload byte count;
- logical segment checksum algorithm and checksum;
- per-subframe checksum algorithm and checksum;
- dtype/layout and artifact/tokenizer/chat-template identity reference.

The source should choose subframe sizes from configured capacity, with a target
default in the `16 MiB` to `64 MiB` range. The default frame cap must not rely
on a 1 GiB single-frame workaround.

## Source Behavior

After each prefill chunk, the source still calls `export_kv_page_segments` and
obtains logical segment payloads. For each logical segment:

1. compute or reuse the logical segment manifest and checksum;
2. split the payload into bounded contiguous subframes;
3. emit `source_subframe_write_start` / `source_subframe_write_end` diagnostics;
4. write subframes to the page stream in order;
5. emit the existing chunk completion control event only after all expected
   subframes for all logical segments in the chunk have been written.

If a payload cannot be split safely, or any subframe write fails, the source
must emit a bounded error and clean up the request. It must not send a partial
chunk as a pass.

## Router Behavior

The router should reassemble subframes by `(request_id, chunk_index,
segment_index)`.

The first implementation should keep an in-order policy for subframes and
chunks. If a subframe arrives out of order, duplicated, with a byte offset gap,
with overlapping bytes, with a mismatched identity, or with a checksum mismatch,
the router fails closed. Future work can add bounded buffering, but this change
should prefer a simple fail-closed policy.

When a logical segment is complete:

1. verify subframe count or final marker;
2. verify the concatenated byte length equals manifest payload bytes;
3. verify the logical segment checksum;
4. verify cache kind, segment kind, dtype/layout, identity, and token range;
5. emit `router_segment_reassembled`;
6. call `import_kv_page` once for the logical segment;
7. emit `router_import_kv_page_start` / `router_import_kv_page_end`.

The final contiguous gate remains unchanged: decode cannot begin until all
chunks and all expected logical segments have been imported.

## Control Error Visibility

The previous large-frame failure was reported to the router as
`page_read_timeout` even though the source emitted `frame_too_large`. The router
should avoid this ambiguity. While reading page subframes, it should continue
to observe control errors promptly, either by using a concurrent control reader
or another bounded mechanism that cannot be blocked behind page payload reads.

Diagnostics should distinguish:

- `source_frame_too_large`;
- `source_subframe_write_start`;
- `source_subframe_write_end`;
- `router_subframe_received`;
- `router_segment_reassembled`;
- `router_import_kv_page_start`;
- `router_import_kv_page_end`;
- `page_stream_timeout`;
- `control_error_received`.

## Capacity

The capacity model should distinguish:

- max frame bytes: maximum bytes in one subframe frame;
- max logical segment bytes: optional bounded guard for one reassembled
  segment;
- max in-flight bytes: total buffered/reassembly bytes for one request or
  active chunk;
- max subframes per segment: optional guard to catch malformed streams.

The 1 GiB cap workaround should not be required after this change. The
production runbook should prefer a smaller `max_frame_bytes` such as `16 MiB`
or `64 MiB`, then validate with short and medium prompt smokes before any 4k
smoke.

## Validation Plan

Validation should proceed in this order:

1. local parser/reassembler tests;
2. production source/router mock lifecycle tests;
3. short production serving smoke;
4. 495-token regression smoke covering a large ISWA `swa` segment;
5. optional 4k production serving smoke after the regression path passes.

Reports should compare the subframing path against the temporary 1 GiB cap
workaround, but must not claim production performance readiness until
production timing and overlap telemetry are implemented and validated.

## Privacy

Diagnostics and reports must use bounded labels, counts, byte sizes, checksums,
and boolean validation results only. They must not include prompt text,
generated content, complete token arrays, KV/native payload contents, private
paths, real hostnames, endpoint URLs, or credentials.
