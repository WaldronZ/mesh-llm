# Design: PD Streaming KV Cleanup Hardening

## Approach

The production router keeps request cleanup centralized in
`run_pd_streaming_kv_production_lifecycle`, but blocking socket reads could
prevent that function from returning. The fix is to make control/page stream IO
bounded:

- set read/write timeouts on router control and page streams after connect;
- map timeout IO errors to sanitized reasons such as `page_read_timeout`;
- keep cleanup fail-safe by sending a best-effort `Stop` control frame with a
  short write timeout, then shutting down both sockets;
- keep source listener streams bounded so router disconnects and source write
  stalls fail closed instead of hanging the listener request forever.

Source listener behavior remains serial and request-scoped. EOF, bad frames,
router disconnects, and write errors clean up the current request and return to
the accept loop. Only bind failures, fatal listener errors, or process shutdown
stop the listener.

## Diagnostics

The existing stable prefix `pd.kv_stream.lifecycle` continues to be used.
Failure reasons are bounded labels only:

- `control_read_timeout`
- `control_write_timeout`
- `page_read_timeout`
- `page_write_timeout`
- existing fail-closed labels such as `manifest_validation`, `import_failed`,
  and `bootstrap_failed`

Diagnostics do not include prompt text, generated content, full token arrays,
KV/native payloads, private paths, real hostnames, endpoint URLs, or
credentials.

## Validation

Local tests cover cleanup after simulated page stream timeout, control EOF, and
existing import/bootstrap/manifest failures. They also verify a subsequent
mocked request can run after a timeout failure, which models generation slot
release once the lifecycle returns.

Foreground short cleanup smoke passed and showed the stale generation slot is
released after bounded cleanup. Cleanup-specific 4k/8k smokes, broader
failure-injection coverage, and long-soak validation remain future hardening.
