# pd-streaming-kv-cleanup-hardening Specification

## Purpose
TBD - created by archiving change pd-streaming-kv-cleanup-hardening. Update Purpose after archive.
## Requirements
### Requirement: Router shall fail closed and clean up stalled streaming KV IO

The production `pd-kv-stream/1` router SHALL bound control/page stream IO so a
stalled source or page stream cannot permanently hold the generation lane.

#### Scenario: Page stream stalls mid-request

- **GIVEN** production streaming KV handoff is enabled
- **AND** the router has started a request-scoped page stream read
- **WHEN** the page stream stalls past the configured IO timeout
- **THEN** the router SHALL fail the request closed
- **AND** it SHALL emit a sanitized timeout reason
- **AND** it SHALL run request cleanup
- **AND** it SHALL release the generation slot for a later request.

#### Scenario: Control stream closes or stalls

- **GIVEN** a production streaming KV request is active
- **WHEN** the control stream returns EOF, a bad frame, or a timeout
- **THEN** the router SHALL fail closed before decode
- **AND** it SHALL run request cleanup
- **AND** it SHALL NOT report a full-state fallback as a streaming KV pass.

### Requirement: Source listener shall survive request-scoped stream failures

The production streaming KV source listener SHALL treat a single request EOF,
bad frame, router disconnect, or page write failure as request-scoped.

#### Scenario: Router disconnects during streaming

- **GIVEN** the PGX source listener is active
- **WHEN** the router disconnects or stops during a request
- **THEN** the source SHALL clean up the current request session
- **AND** it SHALL continue accepting the next control/page stream pair unless
  the process is shutting down or the listener itself fails fatally.

### Requirement: Cleanup diagnostics shall remain sanitized

Cleanup diagnostics SHALL use bounded labels and counts only.

#### Scenario: Timeout cleanup is reported

- **WHEN** a timeout or stream failure triggers request cleanup
- **THEN** diagnostics SHALL include the lifecycle phase and sanitized reason
- **AND** diagnostics SHALL NOT include prompt text, generated content, full
  token arrays, KV/native payloads, private paths, endpoint URLs, hostnames, or
  credentials.

