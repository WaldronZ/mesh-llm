use std::{
    io::{self, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

use anyhow::{anyhow, bail, Context, Result};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::json;
use skippy_protocol::StageConfig;

use crate::{
    frontend::pd_streaming_kv_production::{
        emit_pd_kv_stream_lifecycle, pd_streaming_kv_manifest_from_runtime_page,
        pd_streaming_kv_payload_checksum, PdKvStreamLifecycleDiagnostic, PdStreamingKvFrameKind,
        PdStreamingKvManifestIdentity, PdStreamingKvSegmentManifest,
        PD_STREAMING_KV_PROTOCOL_VERSION,
    },
    runtime_state::RuntimeState,
    telemetry::{lifecycle_attrs, Telemetry},
};

const PD_STREAM_FRAME_MAX_HEADER_BYTES: usize = 64 * 1024;
const SOURCE_EVENT_ATTR: &str = "pd.streaming_kv.source.event";
const SOURCE_FAILURE_PHASE_ATTR: &str = "pd.streaming_kv.source.failure_phase";
const SOURCE_FAILURE_REASON_ATTR: &str = "pd.streaming_kv.source.failure_reason";
const SOURCE_STREAM_IO_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PdStreamingKvSourceOptions {
    pub(crate) control_bind_addr: SocketAddr,
    pub(crate) page_bind_addr: SocketAddr,
    pub(crate) max_in_flight_chunks: usize,
    pub(crate) max_in_flight_bytes: u64,
    pub(crate) max_frame_bytes: u64,
    pub(crate) max_queue_depth: usize,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SourceControlKind {
    PrefillChunk,
    Stop,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct SourceControlRequest {
    pub(crate) protocol_version: String,
    pub(crate) kind: SourceControlKind,
    pub(crate) session_id: String,
    pub(crate) request_id: u64,
    pub(crate) chunk_index: usize,
    pub(crate) total_chunks: usize,
    pub(crate) total_prompt_tokens: usize,
    pub(crate) token_start: usize,
    pub(crate) token_count: usize,
    pub(crate) identity: PdStreamingKvManifestIdentity,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SourceControlEventKind {
    PrefillStarted,
    PrefillCompleted,
    ExportStarted,
    ExportCompleted,
    ChunkDone,
    Stopped,
    Error,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct SourceControlEvent {
    pub(crate) protocol_version: String,
    pub(crate) kind: SourceControlEventKind,
    pub(crate) request_id: u64,
    pub(crate) session_id: String,
    pub(crate) chunk_index: usize,
    pub(crate) token_start: usize,
    pub(crate) token_end: usize,
    pub(crate) page_segments: usize,
    pub(crate) page_bytes: u64,
    pub(crate) failure_phase: Option<String>,
    pub(crate) failure_reason: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SourcePageFrameKind {
    Page,
    PageSubframe,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct SourcePageFrame {
    pub(crate) protocol_version: String,
    pub(crate) kind: SourcePageFrameKind,
    pub(crate) request_id: u64,
    pub(crate) session_id: String,
    pub(crate) chunk_index: usize,
    pub(crate) segment_index: usize,
    pub(crate) segment_count: usize,
    pub(crate) subframe_index: usize,
    pub(crate) subframe_count: usize,
    pub(crate) byte_offset: u64,
    pub(crate) subframe_payload_bytes: u64,
    pub(crate) subframe_checksum_algorithm: String,
    pub(crate) subframe_checksum: String,
    pub(crate) manifest: PdStreamingKvSegmentManifest,
}

pub(crate) fn spawn_pd_streaming_kv_source(
    options: PdStreamingKvSourceOptions,
    config: StageConfig,
    runtime: Arc<Mutex<RuntimeState>>,
    telemetry: Telemetry,
    shutdown: Arc<AtomicBool>,
) {
    thread::spawn(move || {
        if let Err(error) =
            run_pd_streaming_kv_source(options, &config, runtime, &telemetry, shutdown)
        {
            let mut attrs = lifecycle_attrs(&config);
            attrs.insert(
                "pd.streaming_kv.source.error".to_string(),
                json!(error.to_string()),
            );
            telemetry.emit("stage.pd_streaming_kv_source_error", attrs);
        }
    });
}

fn run_pd_streaming_kv_source(
    options: PdStreamingKvSourceOptions,
    config: &StageConfig,
    runtime: Arc<Mutex<RuntimeState>>,
    telemetry: &Telemetry,
    shutdown: Arc<AtomicBool>,
) -> Result<()> {
    let control_listener = TcpListener::bind(options.control_bind_addr)
        .context("bind PD streaming KV source control listener")?;
    let page_listener = TcpListener::bind(options.page_bind_addr)
        .context("bind PD streaming KV source page listener")?;
    control_listener.set_nonblocking(true)?;
    page_listener.set_nonblocking(true)?;
    let mut attrs = lifecycle_attrs(config);
    attrs.insert("pd.streaming_kv.source.enabled".to_string(), json!(true));
    attrs.insert(
        "pd.streaming_kv.protocol".to_string(),
        json!(PD_STREAMING_KV_PROTOCOL_VERSION),
    );
    attrs.insert(
        "pd.streaming_kv.max_frame_bytes".to_string(),
        json!(options.max_frame_bytes),
    );
    attrs.insert(
        "pd.streaming_kv.max_in_flight_chunks".to_string(),
        json!(options.max_in_flight_chunks),
    );
    attrs.insert(
        "pd.streaming_kv.max_in_flight_bytes".to_string(),
        json!(options.max_in_flight_bytes),
    );
    attrs.insert(
        "pd.streaming_kv.max_queue_depth".to_string(),
        json!(options.max_queue_depth),
    );
    telemetry.emit("stage.pd_streaming_kv_source_start", attrs);
    emit_source_lifecycle(telemetry, config, "source_listener_active", None, None);

    while !shutdown.load(Ordering::SeqCst) {
        let Some((mut control_stream, mut page_stream)) =
            accept_pd_stream_pair(&control_listener, &page_listener, &shutdown)?
        else {
            emit_source_lifecycle(telemetry, config, "listener_shutdown", None, None);
            return Ok(());
        };
        prepare_stream(&control_stream)?;
        prepare_stream(&page_stream)?;
        emit_source_lifecycle(telemetry, config, "source_request_start", None, None);

        match run_pd_streaming_kv_request_streams(
            &mut control_stream,
            &mut page_stream,
            &runtime,
            &options,
            telemetry,
            config,
            &shutdown,
        ) {
            SourceRequestOutcome::Continue => {
                emit_source_lifecycle(telemetry, config, "listener_continue", None, None);
            }
            SourceRequestOutcome::Shutdown => {
                emit_source_lifecycle(telemetry, config, "listener_shutdown", None, None);
                return Ok(());
            }
        }
    }
    emit_source_lifecycle(telemetry, config, "listener_shutdown", None, None);
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SourceFailure {
    reason: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum SourceControlRead<T> {
    Frame(T, Vec<u8>),
    Eof,
    BadFrame,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SourceRequestOutcome {
    Continue,
    Shutdown,
}

fn run_pd_streaming_kv_request_streams(
    control_stream: &mut TcpStream,
    page_stream: &mut TcpStream,
    runtime: &Arc<Mutex<RuntimeState>>,
    options: &PdStreamingKvSourceOptions,
    telemetry: &Telemetry,
    config: &StageConfig,
    shutdown: &AtomicBool,
) -> SourceRequestOutcome {
    let mut active_session_id: Option<String> = None;
    while !shutdown.load(Ordering::SeqCst) {
        let (request, payload) =
            match read_source_control_request(control_stream, options.max_frame_bytes) {
                SourceControlRead::Frame(request, payload) => (request, payload),
                SourceControlRead::Eof => {
                    if let Some(session_id) = active_session_id.take() {
                        cleanup_source_session(runtime, &session_id);
                        emit_source_lifecycle(telemetry, config, "request_cleanup", None, None);
                    }
                    emit_source_lifecycle(telemetry, config, "request_eof", None, None);
                    return SourceRequestOutcome::Continue;
                }
                SourceControlRead::BadFrame => {
                    if let Some(session_id) = active_session_id.take() {
                        cleanup_source_session(runtime, &session_id);
                        emit_source_lifecycle(telemetry, config, "request_cleanup", None, None);
                    }
                    emit_source_lifecycle(
                        telemetry,
                        config,
                        "request_error",
                        Some("control"),
                        Some("bad_frame"),
                    );
                    return SourceRequestOutcome::Continue;
                }
            };
        if !request.session_id.trim().is_empty() {
            active_session_id = Some(request.session_id.clone());
        }
        match request.kind {
            SourceControlKind::Stop => {
                cleanup_source_session(runtime, &request.session_id);
                emit_source_lifecycle(telemetry, config, "request_cleanup", None, None);
                let event =
                    control_event(&request, SourceControlEventKind::Stopped, 0, 0, None, None);
                let _ = write_control_event(control_stream, &event, options.max_frame_bytes);
                return SourceRequestOutcome::Continue;
            }
            SourceControlKind::PrefillChunk => {
                if let Err(error) = handle_prefill_chunk_request(
                    control_stream,
                    page_stream,
                    runtime,
                    &request,
                    &payload,
                    options,
                ) {
                    cleanup_source_session(runtime, &request.session_id);
                    active_session_id = None;
                    emit_source_lifecycle(
                        telemetry,
                        config,
                        "request_error",
                        Some("source"),
                        Some(error.reason),
                    );
                    emit_source_lifecycle(telemetry, config, "request_cleanup", None, None);
                    let event = control_event(
                        &request,
                        SourceControlEventKind::Error,
                        0,
                        0,
                        Some("source"),
                        Some(error.reason),
                    );
                    let _ = write_control_event(control_stream, &event, options.max_frame_bytes);
                }
            }
        }
    }
    if let Some(session_id) = active_session_id {
        cleanup_source_session(runtime, &session_id);
        emit_source_lifecycle(telemetry, config, "request_cleanup", None, None);
    }
    SourceRequestOutcome::Shutdown
}

fn handle_prefill_chunk_request(
    control_stream: &mut TcpStream,
    page_stream: &mut TcpStream,
    runtime: &Arc<Mutex<RuntimeState>>,
    request: &SourceControlRequest,
    payload: &[u8],
    options: &PdStreamingKvSourceOptions,
) -> std::result::Result<(), SourceFailure> {
    let token_ids =
        validate_prefill_chunk_request(request, payload).map_err(|_| SourceFailure {
            reason: "request_validation",
        })?;
    source_chunk_lifecycle("source_chunk_request_received", request)
        .field("token_count", token_ids.len())
        .emit();
    write_control_event(
        control_stream,
        &control_event(
            request,
            SourceControlEventKind::PrefillStarted,
            0,
            0,
            None,
            None,
        ),
        options.max_frame_bytes,
    )
    .map_err(|error| SourceFailure {
        reason: source_io_error_reason(&error, "control_write_timeout", "control_write"),
    })?;

    source_chunk_lifecycle("source_prefill_chunk_start", request)
        .field("token_count", token_ids.len())
        .emit();
    {
        let mut runtime = runtime.lock().map_err(|_| SourceFailure {
            reason: "runtime_lock",
        })?;
        let token_end = runtime
            .prefill_kv_stream_chunk(&request.session_id, request.token_start as u64, &token_ids)
            .map_err(|_| SourceFailure {
                reason: "prefill_failed",
            })?;
        let expected_end =
            request
                .token_start
                .checked_add(request.token_count)
                .ok_or(SourceFailure {
                    reason: "token_range",
                })? as u64;
        if token_end != expected_end {
            return Err(SourceFailure {
                reason: "prefill_position",
            });
        }
    }
    source_chunk_lifecycle("source_prefill_chunk_end", request)
        .field("token_count", token_ids.len())
        .emit();

    write_control_event(
        control_stream,
        &control_event(
            request,
            SourceControlEventKind::PrefillCompleted,
            0,
            0,
            None,
            None,
        ),
        options.max_frame_bytes,
    )
    .map_err(|error| SourceFailure {
        reason: source_io_error_reason(&error, "control_write_timeout", "control_write"),
    })?;
    write_control_event(
        control_stream,
        &control_event(
            request,
            SourceControlEventKind::ExportStarted,
            0,
            0,
            None,
            None,
        ),
        options.max_frame_bytes,
    )
    .map_err(|error| SourceFailure {
        reason: source_io_error_reason(&error, "control_write_timeout", "control_write"),
    })?;

    source_chunk_lifecycle("source_export_kv_page_segments_start", request).emit();
    let pages = {
        let mut runtime = runtime.lock().map_err(|_| SourceFailure {
            reason: "runtime_lock",
        })?;
        runtime
            .export_kv_page_segments(
                &request.session_id,
                request.token_start as u64,
                request.token_count as u64,
            )
            .map_err(|_| SourceFailure {
                reason: "export_failed",
            })?
    };
    if pages.is_empty() {
        return Err(SourceFailure {
            reason: "missing_segment",
        });
    }
    let page_bytes = pages
        .iter()
        .map(|page| page.payload.len() as u64)
        .try_fold(0_u64, |sum, bytes| sum.checked_add(bytes))
        .ok_or(SourceFailure {
            reason: "payload_bytes",
        })?;
    if page_bytes > options.max_in_flight_bytes {
        return Err(SourceFailure { reason: "capacity" });
    }
    source_chunk_lifecycle("source_export_kv_page_segments_end", request)
        .field("segment_count", pages.len())
        .field("page_bytes", page_bytes)
        .emit();
    write_control_event(
        control_stream,
        &control_event(
            request,
            SourceControlEventKind::ExportCompleted,
            pages.len(),
            page_bytes,
            None,
            None,
        ),
        options.max_frame_bytes,
    )
    .map_err(|error| SourceFailure {
        reason: source_io_error_reason(&error, "control_write_timeout", "control_write"),
    })?;

    let mut manifests = Vec::with_capacity(pages.len());
    for (segment_index, page) in pages.iter().enumerate() {
        let manifest = pd_streaming_kv_manifest_from_runtime_page(
            page,
            &request.identity,
            request.chunk_index,
            request.total_chunks,
        )
        .map_err(|_| SourceFailure { reason: "manifest" })?;
        manifests.push(manifest.clone());
        let max_subframe_bytes =
            max_subframe_payload_bytes(options.max_frame_bytes).map_err(|_| SourceFailure {
                reason: "source_frame_too_large",
            })?;
        let subframe_count = source_subframe_count(page.payload.len(), max_subframe_bytes)
            .map_err(|_| SourceFailure {
                reason: "source_frame_too_large",
            })?;
        for subframe_index in 0..subframe_count {
            let start = subframe_index
                .checked_mul(max_subframe_bytes)
                .ok_or(SourceFailure {
                    reason: "source_frame_too_large",
                })?;
            let end = start
                .saturating_add(max_subframe_bytes)
                .min(page.payload.len());
            let subframe_payload = &page.payload[start..end];
            let frame = source_page_subframe(
                request,
                &manifest,
                segment_index,
                pages.len(),
                subframe_index,
                subframe_count,
                start as u64,
                subframe_payload,
            );
            validate_source_page_subframe_payload(&frame, &request.identity, subframe_payload)
                .map_err(|_| SourceFailure {
                    reason: "manifest_validation",
                })?;
            source_chunk_lifecycle("source_subframe_write_start", request)
                .field("segment_index", segment_index)
                .field("segment_count", pages.len())
                .field("subframe_index", subframe_index)
                .field("subframe_count", subframe_count)
                .field("byte_offset", start)
                .field("cache_kind", frame.manifest.cache_kind.as_str())
                .field("segment_kind", frame.manifest.segment_kind.as_str())
                .field("payload_bytes", subframe_payload.len())
                .field("logical_payload_bytes", page.payload.len())
                .field("checksum_present", true)
                .field("checksum_valid", true)
                .emit();
            write_pd_stream_frame(
                page_stream,
                &frame,
                subframe_payload,
                options.max_frame_bytes,
            )
            .map_err(|error| SourceFailure {
                reason: source_page_write_error_reason(&error),
            })?;
            source_chunk_lifecycle("source_subframe_write_end", request)
                .field("segment_index", segment_index)
                .field("segment_count", pages.len())
                .field("subframe_index", subframe_index)
                .field("subframe_count", subframe_count)
                .field("byte_offset", start)
                .field("cache_kind", frame.manifest.cache_kind.as_str())
                .field("segment_kind", frame.manifest.segment_kind.as_str())
                .field("payload_bytes", subframe_payload.len())
                .field("logical_payload_bytes", page.payload.len())
                .field("checksum_valid", true)
                .emit();
        }
    }
    validate_source_chunk_manifests(&manifests, request).map_err(|_| SourceFailure {
        reason: "manifest_validation",
    })?;
    write_control_event(
        control_stream,
        &control_event(
            request,
            SourceControlEventKind::ChunkDone,
            pages.len(),
            page_bytes,
            None,
            None,
        ),
        options.max_frame_bytes,
    )
    .map_err(|error| SourceFailure {
        reason: source_io_error_reason(&error, "control_write_timeout", "control_write"),
    })?;
    source_chunk_lifecycle("source_chunk_done", request)
        .field("segment_count", pages.len())
        .field("page_bytes", page_bytes)
        .emit();
    Ok(())
}

fn validate_prefill_chunk_request(
    request: &SourceControlRequest,
    payload: &[u8],
) -> Result<Vec<i32>> {
    if request.protocol_version != PD_STREAMING_KV_PROTOCOL_VERSION {
        bail!("protocol_version");
    }
    if request.session_id.trim().is_empty() {
        bail!("session_id");
    }
    if request.total_chunks == 0 || request.chunk_index >= request.total_chunks {
        bail!("chunk_index");
    }
    if request.token_count == 0 {
        bail!("token_count");
    }
    let token_end = request
        .token_start
        .checked_add(request.token_count)
        .ok_or_else(|| anyhow!("token_range"))?;
    if token_end > request.total_prompt_tokens {
        bail!("token_range");
    }
    decode_token_payload(payload, request.token_count)
}

fn validate_source_page_frame(
    frame: &SourcePageFrame,
    identity: &PdStreamingKvManifestIdentity,
) -> Result<()> {
    if frame.protocol_version != PD_STREAMING_KV_PROTOCOL_VERSION {
        bail!("protocol_version");
    }
    if frame.manifest.frame_kind != PdStreamingKvFrameKind::KvPage {
        bail!("full_state_frame");
    }
    if frame.manifest.protocol_version != PD_STREAMING_KV_PROTOCOL_VERSION {
        bail!("protocol_version");
    }
    if frame.manifest.artifact_sha256 != identity.artifact_sha256
        || frame.manifest.tokenizer_hash != identity.tokenizer_hash
        || frame.manifest.chat_template_hash != identity.chat_template_hash
        || frame.manifest.dtype != identity.dtype
        || frame.manifest.layout != identity.layout
    {
        bail!("identity");
    }
    if frame.manifest.payload_bytes == 0 {
        bail!("payload_bytes");
    }
    if frame.manifest.checksum_algorithm != "sha256"
        || frame.manifest.checksum.len() != 64
        || !frame
            .manifest
            .checksum
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        bail!("checksum");
    }
    if frame.manifest.cache_kind == "iswa" {
        if frame.manifest.segment_kind != "base" && frame.manifest.segment_kind != "swa" {
            bail!("segment_kind");
        }
    } else if frame.manifest.cache_kind == "regular" {
        if frame.manifest.segment_kind != "regular" {
            bail!("segment_kind");
        }
    } else {
        bail!("cache_kind");
    }
    Ok(())
}

fn max_subframe_payload_bytes(max_frame_bytes: u64) -> Result<usize> {
    if max_frame_bytes == 0 {
        bail!("max_frame_bytes");
    }
    Ok(usize::try_from(max_frame_bytes).unwrap_or(usize::MAX))
}

fn source_subframe_count(payload_len: usize, max_subframe_bytes: usize) -> Result<usize> {
    if payload_len == 0 || max_subframe_bytes == 0 {
        bail!("subframe_count");
    }
    Ok((payload_len - 1) / max_subframe_bytes + 1)
}

fn source_page_subframe(
    request: &SourceControlRequest,
    manifest: &PdStreamingKvSegmentManifest,
    segment_index: usize,
    segment_count: usize,
    subframe_index: usize,
    subframe_count: usize,
    byte_offset: u64,
    payload: &[u8],
) -> SourcePageFrame {
    SourcePageFrame {
        protocol_version: PD_STREAMING_KV_PROTOCOL_VERSION.to_string(),
        kind: SourcePageFrameKind::PageSubframe,
        request_id: request.request_id,
        session_id: request.session_id.clone(),
        chunk_index: request.chunk_index,
        segment_index,
        segment_count,
        subframe_index,
        subframe_count,
        byte_offset,
        subframe_payload_bytes: payload.len() as u64,
        subframe_checksum_algorithm: "sha256".to_string(),
        subframe_checksum: pd_streaming_kv_payload_checksum(payload),
        manifest: manifest.clone(),
    }
}

fn single_page_frame_from_manifest(
    request: &SourceControlRequest,
    manifest: PdStreamingKvSegmentManifest,
    segment_index: usize,
    segment_count: usize,
) -> SourcePageFrame {
    SourcePageFrame {
        protocol_version: PD_STREAMING_KV_PROTOCOL_VERSION.to_string(),
        kind: SourcePageFrameKind::Page,
        request_id: request.request_id,
        session_id: request.session_id.clone(),
        chunk_index: request.chunk_index,
        segment_index,
        segment_count,
        subframe_index: 0,
        subframe_count: 1,
        byte_offset: 0,
        subframe_payload_bytes: manifest.payload_bytes,
        subframe_checksum_algorithm: "sha256".to_string(),
        subframe_checksum: manifest.checksum.clone(),
        manifest,
    }
}

pub(crate) fn validate_source_page_subframe_payload(
    frame: &SourcePageFrame,
    identity: &PdStreamingKvManifestIdentity,
    payload: &[u8],
) -> Result<()> {
    validate_source_page_frame(frame, identity)?;
    if frame.subframe_count == 0 || frame.subframe_index >= frame.subframe_count {
        bail!("subframe_index");
    }
    if frame.subframe_payload_bytes == 0 || frame.subframe_payload_bytes != payload.len() as u64 {
        bail!("subframe_payload_bytes");
    }
    if frame.subframe_checksum_algorithm != "sha256"
        || frame.subframe_checksum.len() != 64
        || !frame
            .subframe_checksum
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        bail!("subframe_checksum");
    }
    if frame.subframe_checksum != pd_streaming_kv_payload_checksum(payload) {
        bail!("subframe_checksum");
    }
    let subframe_end = frame
        .byte_offset
        .checked_add(frame.subframe_payload_bytes)
        .ok_or_else(|| anyhow!("byte_offset"))?;
    if frame.byte_offset >= frame.manifest.payload_bytes
        || subframe_end > frame.manifest.payload_bytes
    {
        bail!("byte_offset");
    }
    if frame.kind == SourcePageFrameKind::Page
        && (frame.subframe_index != 0
            || frame.subframe_count != 1
            || frame.byte_offset != 0
            || frame.subframe_payload_bytes != frame.manifest.payload_bytes)
    {
        bail!("subframe_metadata");
    }
    Ok(())
}

#[cfg(test)]
fn validate_source_page_frame_payload(
    frame: &SourcePageFrame,
    identity: &PdStreamingKvManifestIdentity,
    payload: &[u8],
) -> Result<()> {
    validate_source_page_subframe_payload(frame, identity, payload)?;
    if frame.subframe_index != 0
        || frame.subframe_count != 1
        || frame.byte_offset != 0
        || frame.subframe_payload_bytes != frame.manifest.payload_bytes
    {
        bail!("payload_bytes");
    }
    if frame.manifest.payload_bytes != payload.len() as u64 {
        bail!("payload_bytes");
    }
    if frame.manifest.checksum != pd_streaming_kv_payload_checksum(payload) {
        bail!("checksum");
    }
    Ok(())
}

fn validate_source_chunk_manifests(
    manifests: &[PdStreamingKvSegmentManifest],
    request: &SourceControlRequest,
) -> Result<()> {
    if manifests.is_empty() {
        bail!("missing_segment");
    }
    let mut saw_base = false;
    let mut saw_swa = false;
    let mut saw_regular = false;
    let expected_end = request
        .token_start
        .checked_add(request.token_count)
        .ok_or_else(|| anyhow!("token_range"))?;
    let cache_kind = manifests[0].cache_kind.as_str();
    for manifest in manifests {
        if manifest.chunk_index != request.chunk_index
            || manifest.total_chunks != request.total_chunks
            || manifest.token_start != request.token_start
            || manifest.token_end != expected_end
        {
            bail!("chunk_range");
        }
        let frame = single_page_frame_from_manifest(request, manifest.clone(), 0, manifests.len());
        validate_source_page_frame(&frame, &request.identity)?;
        if manifest.cache_kind != cache_kind {
            bail!("cache_kind");
        }
        match manifest.segment_kind.as_str() {
            "regular" => {
                if saw_regular {
                    bail!("duplicate_segment");
                }
                saw_regular = true;
            }
            "base" => {
                if saw_base {
                    bail!("duplicate_segment");
                }
                saw_base = true;
            }
            "swa" => {
                if saw_swa {
                    bail!("duplicate_segment");
                }
                saw_swa = true;
            }
            _ => bail!("segment_kind"),
        }
    }
    match cache_kind {
        "regular" if saw_regular && !saw_base && !saw_swa => Ok(()),
        "iswa" if saw_base && saw_swa && !saw_regular => Ok(()),
        _ => bail!("missing_segment"),
    }
}

fn control_event(
    request: &SourceControlRequest,
    kind: SourceControlEventKind,
    page_segments: usize,
    page_bytes: u64,
    failure_phase: Option<&'static str>,
    failure_reason: Option<&'static str>,
) -> SourceControlEvent {
    let token_end = request
        .token_start
        .checked_add(request.token_count)
        .unwrap_or(request.token_start);
    SourceControlEvent {
        protocol_version: PD_STREAMING_KV_PROTOCOL_VERSION.to_string(),
        kind,
        request_id: request.request_id,
        session_id: request.session_id.clone(),
        chunk_index: request.chunk_index,
        token_start: request.token_start,
        token_end,
        page_segments,
        page_bytes,
        failure_phase: failure_phase.map(str::to_string),
        failure_reason: failure_reason.map(str::to_string),
    }
}

fn write_control_event(
    stream: &mut TcpStream,
    event: &SourceControlEvent,
    max_frame_bytes: u64,
) -> Result<()> {
    write_pd_stream_frame(stream, event, &[], max_frame_bytes)
}

fn cleanup_source_session(runtime: &Arc<Mutex<RuntimeState>>, session_id: &str) {
    if session_id.trim().is_empty() {
        return;
    }
    if let Ok(mut runtime) = runtime.lock() {
        let _ = runtime.drop_session_timed(session_id);
    }
}

fn emit_source_lifecycle(
    telemetry: &Telemetry,
    config: &StageConfig,
    event: &'static str,
    failure_phase: Option<&'static str>,
    failure_reason: Option<&'static str>,
) {
    let mut attrs = lifecycle_attrs(config);
    attrs.insert(SOURCE_EVENT_ATTR.to_string(), json!(event));
    attrs.insert(
        "pd.streaming_kv.protocol".to_string(),
        json!(PD_STREAMING_KV_PROTOCOL_VERSION),
    );
    if let Some(phase) = failure_phase {
        attrs.insert(SOURCE_FAILURE_PHASE_ATTR.to_string(), json!(phase));
    }
    if let Some(reason) = failure_reason {
        attrs.insert(SOURCE_FAILURE_REASON_ATTR.to_string(), json!(reason));
    }
    telemetry.emit("stage.pd_streaming_kv_source_lifecycle", attrs);
    emit_pd_kv_stream_lifecycle(
        PdKvStreamLifecycleDiagnostic::new("source", event).failure(failure_phase, failure_reason),
    );
}

fn source_chunk_lifecycle(
    event: &'static str,
    request: &SourceControlRequest,
) -> PdKvStreamLifecycleDiagnostic {
    let token_end = request
        .token_start
        .checked_add(request.token_count)
        .unwrap_or(request.token_start);
    PdKvStreamLifecycleDiagnostic::new("source", event)
        .field("request_id", request.request_id)
        .field("chunk_index", request.chunk_index)
        .field("total_chunks", request.total_chunks)
        .field("token_start", request.token_start)
        .field("token_end", token_end)
        .field("token_count", request.token_count)
}

fn accept_pd_stream_pair(
    control_listener: &TcpListener,
    page_listener: &TcpListener,
    shutdown: &AtomicBool,
) -> Result<Option<(TcpStream, TcpStream)>> {
    let Some(control_stream) = accept_pd_stream(control_listener, shutdown, "control channel")?
    else {
        return Ok(None);
    };
    let Some(page_stream) = accept_pd_stream(page_listener, shutdown, "page stream")? else {
        return Ok(None);
    };
    Ok(Some((control_stream, page_stream)))
}

fn accept_pd_stream(
    listener: &TcpListener,
    shutdown: &AtomicBool,
    label: &'static str,
) -> Result<Option<TcpStream>> {
    while !shutdown.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => return Ok(Some(stream)),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(50));
            }
            Err(error) => return Err(error).context(format!("accept PD streaming KV {label}")),
        }
    }
    Ok(None)
}

fn prepare_stream(stream: &TcpStream) -> Result<()> {
    stream.set_nonblocking(false)?;
    stream.set_nodelay(true).ok();
    stream.set_read_timeout(Some(SOURCE_STREAM_IO_TIMEOUT))?;
    stream.set_write_timeout(Some(SOURCE_STREAM_IO_TIMEOUT))?;
    Ok(())
}

fn source_io_error_reason(
    error: &anyhow::Error,
    timeout_reason: &'static str,
    fallback: &'static str,
) -> &'static str {
    if error.chain().any(|cause| {
        cause.downcast_ref::<io::Error>().is_some_and(|io_error| {
            matches!(
                io_error.kind(),
                io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
            )
        })
    }) {
        timeout_reason
    } else {
        fallback
    }
}

fn source_page_write_error_reason(error: &anyhow::Error) -> &'static str {
    if error.to_string().contains("frame_too_large") {
        "source_frame_too_large"
    } else {
        source_io_error_reason(error, "page_write_timeout", "page_write")
    }
}

fn read_source_control_request<R>(
    reader: &mut R,
    max_frame_bytes: u64,
) -> SourceControlRead<SourceControlRequest>
where
    R: Read,
{
    match read_pd_stream_frame::<_, SourceControlRequest>(reader, max_frame_bytes) {
        Ok((request, payload)) => SourceControlRead::Frame(request, payload),
        Err(error)
            if error
                .downcast_ref::<io::Error>()
                .is_some_and(|io_error| io_error.kind() == io::ErrorKind::UnexpectedEof) =>
        {
            SourceControlRead::Eof
        }
        Err(_) => SourceControlRead::BadFrame,
    }
}

pub(crate) fn encode_token_payload(tokens: &[i32]) -> Vec<u8> {
    let mut payload = Vec::with_capacity(tokens.len() * std::mem::size_of::<i32>());
    for token in tokens {
        payload.extend_from_slice(&token.to_le_bytes());
    }
    payload
}

fn decode_token_payload(payload: &[u8], expected_tokens: usize) -> Result<Vec<i32>> {
    if expected_tokens == 0 || payload.is_empty() {
        bail!("token_payload_required");
    }
    let expected_bytes = expected_tokens
        .checked_mul(std::mem::size_of::<i32>())
        .ok_or_else(|| anyhow!("token_payload_size"))?;
    if payload.len() != expected_bytes {
        bail!("token_payload_size");
    }
    Ok(payload
        .chunks_exact(std::mem::size_of::<i32>())
        .map(|chunk| i32::from_le_bytes(chunk.try_into().expect("exact chunk length")))
        .collect())
}

pub(crate) fn write_pd_stream_frame<W, T>(
    writer: &mut W,
    header: &T,
    payload: &[u8],
    max_frame_bytes: u64,
) -> Result<()>
where
    W: Write,
    T: Serialize,
{
    if payload.len() as u64 > max_frame_bytes {
        bail!("frame_too_large");
    }
    let header = serde_json::to_vec(header)?;
    if header.len() > PD_STREAM_FRAME_MAX_HEADER_BYTES {
        bail!("frame_header_too_large");
    }
    writer.write_all(&(header.len() as u32).to_le_bytes())?;
    writer.write_all(&(payload.len() as u64).to_le_bytes())?;
    writer.write_all(&header)?;
    writer.write_all(payload)?;
    writer.flush()?;
    Ok(())
}

pub(crate) fn read_pd_stream_frame<R, T>(
    reader: &mut R,
    max_frame_bytes: u64,
) -> Result<(T, Vec<u8>)>
where
    R: Read,
    T: DeserializeOwned,
{
    let mut header_len = [0_u8; 4];
    reader.read_exact(&mut header_len)?;
    let header_len = u32::from_le_bytes(header_len) as usize;
    if header_len == 0 || header_len > PD_STREAM_FRAME_MAX_HEADER_BYTES {
        bail!("frame_header_size");
    }
    let mut payload_len = [0_u8; 8];
    reader.read_exact(&mut payload_len)?;
    let payload_len = u64::from_le_bytes(payload_len);
    if payload_len > max_frame_bytes {
        bail!("frame_too_large");
    }
    let mut header = vec![0_u8; header_len];
    reader.read_exact(&mut header)?;
    let mut payload = vec![0_u8; payload_len as usize];
    reader.read_exact(&mut payload)?;
    Ok((serde_json::from_slice(&header)?, payload))
}

#[cfg(test)]
mod tests {
    use std::{
        io::Cursor,
        sync::{
            atomic::{AtomicBool, Ordering},
            mpsc, Arc,
        },
        thread,
        time::Duration,
    };

    use super::*;
    use crate::frontend::pd_streaming_kv_production::{
        pd_kv_stream_lifecycle_line, PD_KV_STREAM_LIFECYCLE_PREFIX,
    };

    const ARTIFACT: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const TOKENIZER: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const TEMPLATE: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

    fn identity() -> PdStreamingKvManifestIdentity {
        PdStreamingKvManifestIdentity {
            artifact_sha256: ARTIFACT.to_string(),
            tokenizer_hash: TOKENIZER.to_string(),
            chat_template_hash: TEMPLATE.to_string(),
            dtype: "f16".to_string(),
            layout: "llama.cpp-kv-page".to_string(),
        }
    }

    fn request() -> SourceControlRequest {
        SourceControlRequest {
            protocol_version: PD_STREAMING_KV_PROTOCOL_VERSION.to_string(),
            kind: SourceControlKind::PrefillChunk,
            session_id: "session-a".to_string(),
            request_id: 7,
            chunk_index: 0,
            total_chunks: 1,
            total_prompt_tokens: 4,
            token_start: 0,
            token_count: 4,
            identity: identity(),
        }
    }

    fn manifest() -> PdStreamingKvSegmentManifest {
        PdStreamingKvSegmentManifest {
            protocol_version: PD_STREAMING_KV_PROTOCOL_VERSION.to_string(),
            chunk_index: 0,
            total_chunks: 1,
            token_start: 0,
            token_end: 4,
            cache_kind: "regular".to_string(),
            segment_kind: "regular".to_string(),
            layer_start: 0,
            layer_end: 60,
            dtype: "f16".to_string(),
            layout: "llama.cpp-kv-page".to_string(),
            payload_bytes: 4,
            checksum_algorithm: "sha256".to_string(),
            checksum: pd_streaming_kv_payload_checksum(&[1, 2, 3, 4]),
            artifact_sha256: ARTIFACT.to_string(),
            tokenizer_hash: TOKENIZER.to_string(),
            chat_template_hash: TEMPLATE.to_string(),
            frame_kind: PdStreamingKvFrameKind::KvPage,
            native_desc: None,
        }
    }

    fn page_frame(manifest: PdStreamingKvSegmentManifest) -> SourcePageFrame {
        single_page_frame_from_manifest(&request(), manifest, 0, 1)
    }

    fn manifest_for_payload(payload: &[u8]) -> PdStreamingKvSegmentManifest {
        let mut manifest = manifest();
        manifest.payload_bytes = payload.len() as u64;
        manifest.checksum = pd_streaming_kv_payload_checksum(payload);
        manifest
    }

    #[test]
    fn source_wire_round_trips_chunk_request_without_json_tokens() {
        let tokens = [11, 22, 33, 44];
        let payload = encode_token_payload(&tokens);
        let mut buffer = Vec::new();
        write_pd_stream_frame(&mut buffer, &request(), &payload, 1024).unwrap();
        let serialized = String::from_utf8_lossy(&buffer);
        assert!(!serialized.contains("11,"));
        assert!(!serialized.contains("token_ids"));

        let (decoded, decoded_payload): (SourceControlRequest, Vec<u8>) =
            read_pd_stream_frame(&mut Cursor::new(buffer), 1024).unwrap();
        assert_eq!(decoded, request());
        assert_eq!(
            validate_prefill_chunk_request(&decoded, &decoded_payload).unwrap(),
            tokens
        );
    }

    #[test]
    fn source_wire_rejects_wrong_protocol_version() {
        let mut request = request();
        request.protocol_version = "pd-kv-stream/0".to_string();
        let payload = encode_token_payload(&[1, 2, 3, 4]);
        let error = validate_prefill_chunk_request(&request, &payload).unwrap_err();
        assert_eq!(error.to_string(), "protocol_version");
    }

    #[test]
    fn source_wire_requires_token_payload_for_prefill_chunk() {
        let error = validate_prefill_chunk_request(&request(), &[]).unwrap_err();
        assert_eq!(error.to_string(), "token_payload_required");

        let error = validate_prefill_chunk_request(&request(), &[1, 2]).unwrap_err();
        assert_eq!(error.to_string(), "token_payload_size");
    }

    #[test]
    fn source_control_read_classifies_eof_and_bad_frame_as_non_fatal() {
        assert!(matches!(
            read_source_control_request(&mut Cursor::new(Vec::<u8>::new()), 1024),
            SourceControlRead::Eof
        ));

        let mut bad_frame = Vec::new();
        bad_frame.extend_from_slice(&0_u32.to_le_bytes());
        assert!(matches!(
            read_source_control_request(&mut Cursor::new(bad_frame), 1024),
            SourceControlRead::BadFrame
        ));
    }

    #[test]
    fn source_io_error_reason_labels_timeouts() {
        let timeout = anyhow::Error::new(io::Error::new(io::ErrorKind::TimedOut, "timeout"));
        let broken_pipe = anyhow::Error::new(io::Error::new(io::ErrorKind::BrokenPipe, "closed"));

        assert_eq!(
            source_io_error_reason(&timeout, "page_write_timeout", "page_write"),
            "page_write_timeout"
        );
        assert_eq!(
            source_io_error_reason(&broken_pipe, "page_write_timeout", "page_write"),
            "page_write"
        );
    }

    #[test]
    fn source_lifecycle_diagnostic_line_is_bounded_and_sanitized() {
        let line = pd_kv_stream_lifecycle_line(
            &source_chunk_lifecycle("source_chunk_request_received", &request())
                .field("segment_count", 2_usize)
                .field("page_bytes", 4096_u64)
                .field("checksum_present", true)
                .field("checksum_valid", true),
        );

        assert!(line.starts_with(PD_KV_STREAM_LIFECYCLE_PREFIX));
        assert!(line.contains("source_chunk_request_received"));
        assert!(line.contains("\"token_count\":4"));
        assert!(line.contains("\"page_bytes\":4096"));
        for forbidden in [
            "token_ids",
            "full_token_array",
            "prompt_text",
            "generated_content",
            "kv_native_payload",
            "credential",
            "endpoint_url",
            "private_path",
            "/Users/",
            "http://",
        ] {
            assert!(!line.contains(forbidden), "{forbidden} leaked in {line}");
        }
    }

    #[test]
    fn source_accept_pair_can_continue_after_eof_pair_is_dropped() {
        let control_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let page_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        control_listener.set_nonblocking(true).unwrap();
        page_listener.set_nonblocking(true).unwrap();
        let control_addr = control_listener.local_addr().unwrap();
        let page_addr = page_listener.local_addr().unwrap();
        let shutdown = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        let worker_shutdown = shutdown.clone();
        let worker_control = control_listener.try_clone().unwrap();
        let worker_page = page_listener.try_clone().unwrap();
        let worker = thread::spawn(move || {
            for _ in 0..2 {
                let Some((control, page)) =
                    accept_pd_stream_pair(&worker_control, &worker_page, &worker_shutdown).unwrap()
                else {
                    break;
                };
                drop(control);
                drop(page);
                tx.send(()).unwrap();
            }
        });

        for _ in 0..2 {
            let control = TcpStream::connect(control_addr).unwrap();
            let page = TcpStream::connect(page_addr).unwrap();
            drop(control);
            drop(page);
            rx.recv_timeout(Duration::from_secs(2)).unwrap();
        }
        shutdown.store(true, Ordering::SeqCst);
        worker.join().unwrap();
    }

    #[test]
    fn source_page_frame_rejects_full_state_manifest() {
        let mut manifest = manifest();
        manifest.frame_kind = PdStreamingKvFrameKind::FullState;
        let frame = page_frame(manifest);
        let error = validate_source_page_frame(&frame, &identity()).unwrap_err();
        assert_eq!(error.to_string(), "full_state_frame");
    }

    #[test]
    fn source_page_frame_rejects_identity_mismatch() {
        let frame = page_frame(manifest());
        let mut identity = identity();
        identity.artifact_sha256 =
            "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee".to_string();
        let error = validate_source_page_frame(&frame, &identity).unwrap_err();
        assert_eq!(error.to_string(), "identity");
    }

    #[test]
    fn source_page_frame_rejects_checksum_mismatch() {
        let mut manifest = manifest();
        manifest.checksum = "not-a-checksum".to_string();
        let frame = page_frame(manifest);
        let error = validate_source_page_frame(&frame, &identity()).unwrap_err();
        assert_eq!(error.to_string(), "checksum");
    }

    #[test]
    fn source_page_frame_rejects_payload_checksum_mismatch() {
        let frame = page_frame(manifest());
        let error =
            validate_source_page_frame_payload(&frame, &identity(), &[9, 9, 9, 9]).unwrap_err();
        assert_eq!(error.to_string(), "subframe_checksum");
    }

    #[test]
    fn source_subframes_large_logical_segment_into_bounded_frames() {
        let payload = (0_u8..10).collect::<Vec<_>>();
        let manifest = manifest_for_payload(&payload);
        let max_subframe_bytes = max_subframe_payload_bytes(4).unwrap();
        let subframe_count = source_subframe_count(payload.len(), max_subframe_bytes).unwrap();
        assert_eq!(subframe_count, 3);

        let mut offsets = Vec::new();
        for subframe_index in 0..subframe_count {
            let start = subframe_index * max_subframe_bytes;
            let end = start.saturating_add(max_subframe_bytes).min(payload.len());
            let subframe_payload = &payload[start..end];
            let frame = source_page_subframe(
                &request(),
                &manifest,
                0,
                1,
                subframe_index,
                subframe_count,
                start as u64,
                subframe_payload,
            );
            validate_source_page_subframe_payload(&frame, &identity(), subframe_payload).unwrap();
            assert!(subframe_payload.len() <= 4);
            assert_eq!(frame.manifest.payload_bytes, payload.len() as u64);
            assert_eq!(
                frame.manifest.checksum,
                pd_streaming_kv_payload_checksum(&payload)
            );
            offsets.push(frame.byte_offset);
        }
        assert_eq!(offsets, vec![0, 4, 8]);
    }

    #[test]
    fn source_subframe_rejects_payload_checksum_mismatch() {
        let payload = [1, 2, 3, 4, 5, 6];
        let manifest = manifest_for_payload(&payload);
        let mut frame = source_page_subframe(&request(), &manifest, 0, 1, 0, 2, 0, &payload[..4]);
        frame.subframe_checksum = pd_streaming_kv_payload_checksum(&[9, 9, 9, 9]);
        let error =
            validate_source_page_subframe_payload(&frame, &identity(), &payload[..4]).unwrap_err();
        assert_eq!(error.to_string(), "subframe_checksum");
    }

    #[test]
    fn source_chunk_manifest_rejects_duplicate_segment() {
        let request = request();
        let manifests = vec![manifest(), manifest()];
        let error = validate_source_chunk_manifests(&manifests, &request).unwrap_err();
        assert_eq!(error.to_string(), "duplicate_segment");
    }

    #[test]
    fn source_chunk_manifest_rejects_out_of_order_chunk() {
        let request = request();
        let mut manifest = manifest();
        manifest.chunk_index = 1;
        let error = validate_source_chunk_manifests(&[manifest], &request).unwrap_err();
        assert_eq!(error.to_string(), "chunk_range");
    }

    #[test]
    fn source_error_event_is_sanitized() {
        let event = control_event(
            &request(),
            SourceControlEventKind::Error,
            0,
            0,
            Some("source"),
            Some("export_failed"),
        );
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("export_failed"));
        assert!(!json.contains("payload"));
        assert!(!json.contains("token_ids"));
        assert!(!json.contains("/Users/"));
    }

    #[test]
    fn source_frame_size_limit_is_enforced() {
        let mut buffer = Vec::new();
        let error = write_pd_stream_frame(&mut buffer, &request(), &[0; 8], 4).unwrap_err();
        assert_eq!(error.to_string(), "frame_too_large");
    }
}
