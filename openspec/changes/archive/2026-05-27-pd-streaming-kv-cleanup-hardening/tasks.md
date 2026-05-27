# Tasks

## 1. Router IO Timeout And Cleanup

- [x] Set bounded read/write timeouts on production streaming KV control/page
  sockets.
- [x] Map timeout IO failures to sanitized `*_timeout` reasons.
- [x] Keep router cleanup best-effort and non-sticky by sending `Stop` with a
  short write timeout and closing both sockets.
- [x] Preserve no-full-state-fallback behavior in streaming mode.

## 2. Source Listener Cleanup

- [x] Set bounded IO timeouts on accepted source control/page streams.
- [x] Keep EOF, bad frame, router disconnect, and write failure scoped to the
  current request.
- [x] Continue accepting the next control/page stream pair after request
  cleanup.

## 3. Tests

- [x] Mocked page stream timeout fails closed, calls cleanup, and allows a
  subsequent request lifecycle.
- [x] Mocked control EOF/read failure fails closed before content and calls
  cleanup.
- [x] Existing manifest/import/bootstrap/full-state-frame failure cleanup tests
  remain covered.
- [x] Timeout label mapping tests cover bounded diagnostics.

## 4. Deferred Foreground Validation

- [x] Re-run short production serving smoke and confirm no stale in-flight
  request remains after a stalled or failed stream.
- [x] Keep 4k/8k cleanup-specific smokes, broader production failure injection,
  and long-soak validation deferred; later subframing validation re-used the
  hardened cleanup path for a 4k-class request.
