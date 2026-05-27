use super::*;
use std::{
    collections::BTreeMap,
    io,
    net::{Shutdown, SocketAddr, TcpStream, ToSocketAddrs},
    sync::{mpsc, Arc, Mutex},
    thread,
    time::Duration,
};

use serde::Serialize;
use serde_json::Value;
use skippy_runtime::{RuntimeKvPage, RuntimeKvPageDesc};

use crate::{
    binary_transport::pd_streaming_kv_source::{
        encode_token_payload, read_pd_stream_frame, validate_source_page_subframe_payload,
        write_pd_stream_frame, SourceControlEvent, SourceControlEventKind, SourceControlKind,
        SourceControlRequest, SourcePageFrame,
    },
    runtime_state::RuntimeState,
};

pub(crate) const PD_STREAMING_KV_PROTOCOL_VERSION: &str = "pd-kv-stream/1";
pub(crate) const PD_STREAMING_KV_CHECKSUM_ALGORITHM: &str = "sha256";
pub(crate) const PD_KV_STREAM_LIFECYCLE_PREFIX: &str = "pd.kv_stream.lifecycle";
const PD_STREAMING_KV_CLEANUP_WRITE_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PdKvStreamLifecycleDiagnostic {
    fields: BTreeMap<&'static str, Value>,
}

impl PdKvStreamLifecycleDiagnostic {
    pub(crate) fn new(side: &'static str, event: &'static str) -> Self {
        let mut fields = BTreeMap::new();
        fields.insert("event", json!(event));
        fields.insert("protocol", json!(PD_STREAMING_KV_PROTOCOL_VERSION));
        fields.insert("side", json!(side));
        Self { fields }
    }

    pub(crate) fn field(mut self, key: &'static str, value: impl Serialize) -> Self {
        self.fields
            .insert(key, serde_json::to_value(value).unwrap_or(Value::Null));
        self
    }

    pub(crate) fn optional_field(self, key: &'static str, value: Option<impl Serialize>) -> Self {
        match value {
            Some(value) => self.field(key, value),
            None => self,
        }
    }

    pub(crate) fn failure(self, phase: Option<&'static str>, reason: Option<&'static str>) -> Self {
        self.optional_field("failure_phase", phase)
            .optional_field("failure_reason", reason)
    }

    pub(crate) fn emit(self) {
        emit_pd_kv_stream_lifecycle(self);
    }
}

pub(crate) fn pd_kv_stream_lifecycle_line(diagnostic: &PdKvStreamLifecycleDiagnostic) -> String {
    let payload = serde_json::to_string(&diagnostic.fields)
        .unwrap_or_else(|_| "{\"event\":\"serialization_error\"}".to_string());
    format!("{PD_KV_STREAM_LIFECYCLE_PREFIX} {payload}")
}

pub(crate) fn emit_pd_kv_stream_lifecycle(diagnostic: PdKvStreamLifecycleDiagnostic) {
    eprintln!("{}", pd_kv_stream_lifecycle_line(&diagnostic));
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PdStreamingKvConfig {
    pub(super) control_addr: String,
    pub(super) page_addr: String,
    pub(super) max_in_flight_chunks: usize,
    pub(super) max_in_flight_bytes: u64,
    pub(super) max_frame_bytes: u64,
    pub(super) max_queue_depth: usize,
}

impl PdStreamingKvConfig {
    pub(super) fn capacity(&self) -> PdStreamingKvCapacity {
        PdStreamingKvCapacity {
            max_in_flight_chunks: self.max_in_flight_chunks,
            max_in_flight_bytes: self.max_in_flight_bytes,
            max_frame_bytes: self.max_frame_bytes,
            max_queue_depth: self.max_queue_depth,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PdStreamingKvCapacity {
    pub(crate) max_in_flight_chunks: usize,
    pub(crate) max_in_flight_bytes: u64,
    pub(crate) max_frame_bytes: u64,
    pub(crate) max_queue_depth: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PdServingHandoffMode {
    FullState,
    StreamingKv,
}

impl PdServingHandoffMode {
    pub(super) fn protocol_label(self) -> &'static str {
        match self {
            Self::FullState => "native-full-state/1",
            Self::StreamingKv => PD_STREAMING_KV_PROTOCOL_VERSION,
        }
    }
}

pub(super) fn pd_streaming_kv_config_from_args(
    args: &ServeOpenAiArgs,
    mode: PdServingMode,
) -> Result<Option<PdStreamingKvConfig>> {
    if !args.pd_streaming_kv_handoff {
        return Ok(None);
    }
    if mode != PdServingMode::Mvp {
        bail!("--pd-streaming-kv-handoff requires --pd-serving-mvp");
    }
    if args.pd_serving_mvp_allow_test_faults
        || args.pd_serving_mvp_test_fault != PdServingMvpTestFault::None
    {
        bail!("--pd-streaming-kv-handoff cannot be combined with PD serving MVP test faults");
    }

    let required = |value: &Option<String>, flag: &str| {
        value
            .as_ref()
            .filter(|value| !value.trim().is_empty())
            .cloned()
            .ok_or_else(|| anyhow!("--pd-streaming-kv-handoff requires {flag}"))
    };
    let config = PdStreamingKvConfig {
        control_addr: required(&args.pd_stream_control_addr, "--pd-stream-control-addr")?,
        page_addr: required(&args.pd_stream_page_addr, "--pd-stream-page-addr")?,
        max_in_flight_chunks: args.pd_stream_max_in_flight_chunks,
        max_in_flight_bytes: args.pd_stream_max_in_flight_bytes,
        max_frame_bytes: args.pd_stream_max_frame_bytes,
        max_queue_depth: args.pd_stream_max_queue_depth,
    };
    validate_pd_streaming_kv_capacity(config.capacity())?;
    if !args.pd_chunked_prefill {
        bail!("--pd-streaming-kv-handoff requires --pd-chunked-prefill");
    }
    Ok(Some(config))
}

pub(crate) fn validate_pd_streaming_kv_capacity(capacity: PdStreamingKvCapacity) -> Result<()> {
    if capacity.max_in_flight_chunks == 0 {
        bail!("--pd-stream-max-in-flight-chunks must be greater than zero");
    }
    if capacity.max_in_flight_bytes == 0 {
        bail!("--pd-stream-max-in-flight-bytes must be greater than zero");
    }
    if capacity.max_frame_bytes == 0 {
        bail!("--pd-stream-max-frame-bytes must be greater than zero");
    }
    if capacity.max_queue_depth == 0 {
        bail!("--pd-stream-max-queue-depth must be greater than zero");
    }
    if capacity.max_frame_bytes > capacity.max_in_flight_bytes {
        bail!("--pd-stream-max-frame-bytes cannot exceed --pd-stream-max-in-flight-bytes");
    }
    if capacity.max_in_flight_chunks > capacity.max_queue_depth {
        bail!("--pd-stream-max-in-flight-chunks cannot exceed --pd-stream-max-queue-depth");
    }
    Ok(())
}

fn pd_streaming_kv_unavailable_error(reason: &'static str) -> OpenAiError {
    OpenAiError::from_kind(
        StatusCode::SERVICE_UNAVAILABLE,
        OpenAiErrorKind::ServiceUnavailable,
        format!("PD streaming KV production lifecycle unavailable: {reason}"),
    )
}

#[cfg(test)]
pub(super) fn pd_streaming_kv_not_ready_error() -> OpenAiError {
    pd_streaming_kv_unavailable_error("source_integration_not_implemented")
}

pub(super) fn insert_pd_streaming_kv_status_attrs(
    attrs: &mut BTreeMap<String, Value>,
    config: Option<&PdStreamingKvConfig>,
) {
    let enabled = config.is_some();
    attrs.insert("pd.streaming_kv.enabled".to_string(), json!(enabled));
    attrs.insert(
        "pd.streaming_kv.protocol".to_string(),
        json!(PD_STREAMING_KV_PROTOCOL_VERSION),
    );
    attrs.insert(
        "pd.streaming_kv.lifecycle_state".to_string(),
        json!(if enabled {
            "skeleton_not_ready"
        } else {
            "disabled"
        }),
    );
    if let Some(config) = config {
        attrs.insert(
            "pd.streaming_kv.control_channel_configured".to_string(),
            json!(!config.control_addr.trim().is_empty()),
        );
        attrs.insert(
            "pd.streaming_kv.page_stream_configured".to_string(),
            json!(!config.page_addr.trim().is_empty()),
        );
        attrs.insert(
            "pd.streaming_kv.max_in_flight_chunks".to_string(),
            json!(config.max_in_flight_chunks),
        );
        attrs.insert(
            "pd.streaming_kv.max_in_flight_bytes".to_string(),
            json!(config.max_in_flight_bytes),
        );
        attrs.insert(
            "pd.streaming_kv.max_frame_bytes".to_string(),
            json!(config.max_frame_bytes),
        );
        attrs.insert(
            "pd.streaming_kv.max_queue_depth".to_string(),
            json!(config.max_queue_depth),
        );
    }
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub(crate) struct PdStreamingKvManifestIdentity {
    pub(crate) artifact_sha256: String,
    pub(crate) tokenizer_hash: String,
    pub(crate) chat_template_hash: String,
    pub(crate) dtype: String,
    pub(crate) layout: String,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub(crate) struct PdStreamingKvNativePageDesc {
    pub(crate) version: u32,
    pub(crate) layer_start: i32,
    pub(crate) layer_end: i32,
    pub(crate) token_start: u64,
    pub(crate) token_count: u64,
    pub(crate) layer_count: u32,
    pub(crate) k_type: u32,
    pub(crate) v_type: u32,
    pub(crate) k_row_bytes: u32,
    pub(crate) v_row_bytes: u32,
    pub(crate) v_element_bytes: u32,
    pub(crate) payload_bytes: u64,
    pub(crate) flags: u64,
}

impl From<&RuntimeKvPageDesc> for PdStreamingKvNativePageDesc {
    fn from(desc: &RuntimeKvPageDesc) -> Self {
        Self {
            version: desc.version,
            layer_start: desc.layer_start,
            layer_end: desc.layer_end,
            token_start: desc.token_start,
            token_count: desc.token_count,
            layer_count: desc.layer_count,
            k_type: desc.k_type,
            v_type: desc.v_type,
            k_row_bytes: desc.k_row_bytes,
            v_row_bytes: desc.v_row_bytes,
            v_element_bytes: desc.v_element_bytes,
            payload_bytes: desc.payload_bytes,
            flags: desc.flags,
        }
    }
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub(crate) struct PdStreamingKvSegmentManifest {
    pub(crate) protocol_version: String,
    pub(crate) chunk_index: usize,
    pub(crate) total_chunks: usize,
    pub(crate) token_start: usize,
    pub(crate) token_end: usize,
    pub(crate) cache_kind: String,
    pub(crate) segment_kind: String,
    pub(crate) layer_start: usize,
    pub(crate) layer_end: usize,
    pub(crate) dtype: String,
    pub(crate) layout: String,
    pub(crate) payload_bytes: u64,
    pub(crate) checksum_algorithm: String,
    pub(crate) checksum: String,
    pub(crate) artifact_sha256: String,
    pub(crate) tokenizer_hash: String,
    pub(crate) chat_template_hash: String,
    pub(crate) frame_kind: PdStreamingKvFrameKind,
    pub(crate) native_desc: Option<PdStreamingKvNativePageDesc>,
}

#[derive(Clone, Copy, Debug, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PdStreamingKvFrameKind {
    KvPage,
    FullState,
}

impl PdStreamingKvFrameKind {
    fn protocol_label(self) -> &'static str {
        match self {
            Self::KvPage => PD_STREAMING_KV_PROTOCOL_VERSION,
            Self::FullState => "native-full-state/1",
        }
    }

    fn from_protocol_label(label: &str) -> Result<Self, PdStreamingKvManifestError> {
        match label {
            PD_STREAMING_KV_PROTOCOL_VERSION => Ok(Self::KvPage),
            "native-full-state/1" => Ok(Self::FullState),
            _ => Err(PdStreamingKvManifestError {
                reason: "frame_kind",
            }),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PdStreamingKvManifestError {
    pub(crate) reason: &'static str,
}

pub(crate) fn validate_pd_streaming_kv_segments(
    segments: &[PdStreamingKvSegmentManifest],
    identity: &PdStreamingKvManifestIdentity,
) -> Result<(), PdStreamingKvManifestError> {
    let first = segments.first().ok_or(PdStreamingKvManifestError {
        reason: "missing_segment",
    })?;
    if first.total_chunks == 0 {
        return Err(PdStreamingKvManifestError {
            reason: "total_chunks",
        });
    }

    let mut expected_chunk = 0usize;
    let mut expected_token_start = 0usize;
    let mut chunk_has_segment = false;
    let mut saw_required_segment = false;
    let mut chunk_segment_kinds = std::collections::BTreeSet::new();
    for segment in segments {
        validate_pd_streaming_kv_segment_fields(segment, identity)?;
        if segment.total_chunks != first.total_chunks {
            return Err(PdStreamingKvManifestError {
                reason: "total_chunks",
            });
        }
        if segment.chunk_index > expected_chunk {
            if chunk_has_segment {
                return Err(PdStreamingKvManifestError {
                    reason: "missing_segment",
                });
            }
            return Err(PdStreamingKvManifestError {
                reason: "missing_chunk",
            });
        }
        if segment.chunk_index < expected_chunk {
            return Err(PdStreamingKvManifestError {
                reason: "out_of_order_chunk",
            });
        }
        if !chunk_has_segment {
            if segment.token_start > expected_token_start {
                return Err(PdStreamingKvManifestError {
                    reason: "position_gap",
                });
            }
            if segment.token_start < expected_token_start {
                return Err(PdStreamingKvManifestError {
                    reason: "position_overlap",
                });
            }
        } else if segment.token_start != expected_token_start {
            return Err(PdStreamingKvManifestError {
                reason: "segment_token_range",
            });
        }
        let expected_segment_kinds: &[&str] = if segment.cache_kind == "iswa" {
            &["base", "swa"]
        } else {
            &["regular"]
        };
        if !expected_segment_kinds.contains(&segment.segment_kind.as_str()) {
            return Err(PdStreamingKvManifestError {
                reason: "segment_kind",
            });
        }
        if !chunk_segment_kinds.insert(segment.segment_kind.as_str()) {
            return Err(PdStreamingKvManifestError {
                reason: "duplicate_segment",
            });
        }
        chunk_has_segment = true;
        saw_required_segment = expected_segment_kinds
            .iter()
            .all(|kind| chunk_segment_kinds.contains(*kind));
        if saw_required_segment {
            expected_token_start = segment.token_end;
            expected_chunk += 1;
            chunk_has_segment = false;
            chunk_segment_kinds.clear();
        }
    }

    if !saw_required_segment || expected_chunk != first.total_chunks {
        return Err(PdStreamingKvManifestError {
            reason: "missing_segment",
        });
    }
    Ok(())
}

pub(crate) fn pd_streaming_kv_manifest_from_runtime_page(
    page: &RuntimeKvPage,
    identity: &PdStreamingKvManifestIdentity,
    chunk_index: usize,
    total_chunks: usize,
) -> Result<PdStreamingKvSegmentManifest> {
    let token_start = usize::try_from(page.desc.token_start)?;
    let token_count = usize::try_from(page.desc.token_count)?;
    let token_end = token_start
        .checked_add(token_count)
        .ok_or_else(|| anyhow!("KV page token range overflows"))?;
    let layer_start = usize::try_from(page.desc.layer_start)?;
    let layer_end = usize::try_from(page.desc.layer_end)?;
    let payload_bytes = u64::try_from(page.payload.len())?;
    Ok(PdStreamingKvSegmentManifest {
        protocol_version: PD_STREAMING_KV_PROTOCOL_VERSION.to_string(),
        chunk_index,
        total_chunks,
        token_start,
        token_end,
        cache_kind: page.desc.cache_kind().as_label().to_string(),
        segment_kind: page.desc.segment_kind().as_label().to_string(),
        layer_start,
        layer_end,
        dtype: identity.dtype.clone(),
        layout: identity.layout.clone(),
        payload_bytes,
        checksum_algorithm: PD_STREAMING_KV_CHECKSUM_ALGORITHM.to_string(),
        checksum: pd_streaming_kv_payload_checksum(&page.payload),
        artifact_sha256: identity.artifact_sha256.clone(),
        tokenizer_hash: identity.tokenizer_hash.clone(),
        chat_template_hash: identity.chat_template_hash.clone(),
        frame_kind: PdStreamingKvFrameKind::KvPage,
        native_desc: Some((&page.desc).into()),
    })
}

pub(crate) fn pd_streaming_kv_payload_checksum(payload: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(payload);
    format!("{:x}", hasher.finalize())
}

fn validate_pd_streaming_kv_segment_fields(
    segment: &PdStreamingKvSegmentManifest,
    identity: &PdStreamingKvManifestIdentity,
) -> Result<(), PdStreamingKvManifestError> {
    let frame_kind =
        PdStreamingKvFrameKind::from_protocol_label(segment.frame_kind.protocol_label())?;
    if frame_kind != PdStreamingKvFrameKind::KvPage {
        return Err(PdStreamingKvManifestError {
            reason: "full_state_frame",
        });
    }
    if segment.protocol_version != PD_STREAMING_KV_PROTOCOL_VERSION {
        return Err(PdStreamingKvManifestError {
            reason: "protocol_version",
        });
    }
    if segment.token_end <= segment.token_start {
        return Err(PdStreamingKvManifestError {
            reason: "token_range",
        });
    }
    if segment.layer_end <= segment.layer_start {
        return Err(PdStreamingKvManifestError {
            reason: "layer_range",
        });
    }
    if segment.payload_bytes == 0 {
        return Err(PdStreamingKvManifestError {
            reason: "payload_bytes",
        });
    }
    if segment.checksum_algorithm != PD_STREAMING_KV_CHECKSUM_ALGORITHM
        || segment.checksum.len() != 64
        || !segment
            .checksum
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(PdStreamingKvManifestError { reason: "checksum" });
    }
    if segment.artifact_sha256 != identity.artifact_sha256
        || segment.tokenizer_hash != identity.tokenizer_hash
        || segment.chat_template_hash != identity.chat_template_hash
        || segment.dtype != identity.dtype
        || segment.layout != identity.layout
    {
        return Err(PdStreamingKvManifestError { reason: "identity" });
    }
    if segment.cache_kind != "regular" && segment.cache_kind != "iswa" {
        return Err(PdStreamingKvManifestError {
            reason: "cache_kind",
        });
    }
    let expected_segment_kinds: &[&str] = if segment.cache_kind == "iswa" {
        &["base", "swa"]
    } else {
        &["regular"]
    };
    if !expected_segment_kinds.contains(&segment.segment_kind.as_str()) {
        return Err(PdStreamingKvManifestError {
            reason: "segment_kind",
        });
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PdStreamingKvChunkRequest {
    pub(super) chunk_index: usize,
    pub(super) total_chunks: usize,
    pub(super) token_start: usize,
    pub(super) token_end: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PdStreamingKvReceivedSegment {
    pub(super) manifest: PdStreamingKvSegmentManifest,
    pub(super) payload: Vec<u8>,
}

#[derive(Clone, Debug)]
struct PdStreamingKvSegmentReassembler {
    request_id: u64,
    session_id: String,
    chunk: PdStreamingKvChunkRequest,
    segment_index: usize,
    segment_count: usize,
    identity: PdStreamingKvManifestIdentity,
    manifest: Option<PdStreamingKvSegmentManifest>,
    expected_subframe_index: usize,
    subframe_count: Option<usize>,
    payload: Vec<u8>,
}

enum RouterControlWatchMessage {
    Event(SourceControlEvent),
    Error(PdStreamingKvLifecycleError),
}

struct RouterControlErrorWatch {
    receiver: mpsc::Receiver<RouterControlWatchMessage>,
    cached: Option<RouterControlWatchMessage>,
}

impl RouterControlErrorWatch {
    fn start(
        control_stream: &TcpStream,
        page_stream: &TcpStream,
        max_frame_bytes: u64,
    ) -> Result<Self, PdStreamingKvLifecycleError> {
        let mut control = control_stream
            .try_clone()
            .map_err(|_| PdStreamingKvLifecycleError {
                phase: "control",
                reason: "control_watch",
            })?;
        let page_cancel = page_stream
            .try_clone()
            .map_err(|_| PdStreamingKvLifecycleError {
                phase: "page_stream",
                reason: "control_watch",
            })?;
        control.set_read_timeout(None).ok();
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let message = match read_pd_stream_frame::<_, SourceControlEvent>(
                &mut control,
                max_frame_bytes,
            ) {
                Ok((event, payload)) if payload.is_empty() => {
                    let cancel_page = event.kind == SourceControlEventKind::Error;
                    let message = RouterControlWatchMessage::Event(event);
                    let _ = sender.send(message);
                    if cancel_page {
                        let _ = page_cancel.shutdown(Shutdown::Both);
                    }
                    return;
                }
                Ok((_event, _payload)) => {
                    RouterControlWatchMessage::Error(PdStreamingKvLifecycleError {
                        phase: "control",
                        reason: "control_event_metadata",
                    })
                }
                Err(error) => RouterControlWatchMessage::Error(PdStreamingKvLifecycleError {
                    phase: "control",
                    reason: streaming_io_error_reason(
                        &error,
                        "control_read_timeout",
                        "control_read",
                    ),
                }),
            };
            let _ = sender.send(message);
            let _ = page_cancel.shutdown(Shutdown::Both);
        });
        Ok(Self {
            receiver,
            cached: None,
        })
    }

    fn try_recv(&mut self) -> Option<RouterControlWatchMessage> {
        if self.cached.is_some() {
            return self.cached.take();
        }
        match self.receiver.try_recv() {
            Ok(message) => Some(message),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(RouterControlWatchMessage::Error(
                PdStreamingKvLifecycleError {
                    phase: "control",
                    reason: "control_read",
                },
            )),
        }
    }

    fn recv_after_page_error(&mut self, grace: Duration) -> Option<RouterControlWatchMessage> {
        if self.cached.is_some() {
            return self.cached.take();
        }
        match self.receiver.recv_timeout(grace) {
            Ok(message) => Some(message),
            Err(mpsc::RecvTimeoutError::Timeout) => None,
            Err(mpsc::RecvTimeoutError::Disconnected) => Some(RouterControlWatchMessage::Error(
                PdStreamingKvLifecycleError {
                    phase: "control",
                    reason: "control_read",
                },
            )),
        }
    }

    fn wait_for_terminal_event(
        &mut self,
        timeout: Duration,
    ) -> Result<SourceControlEvent, PdStreamingKvLifecycleError> {
        if let Some(message) = self.cached.take() {
            return match message {
                RouterControlWatchMessage::Event(event) => Ok(event),
                RouterControlWatchMessage::Error(error) => Err(error),
            };
        }
        match self.receiver.recv_timeout(timeout) {
            Ok(RouterControlWatchMessage::Event(event)) => Ok(event),
            Ok(RouterControlWatchMessage::Error(error)) => Err(error),
            Err(mpsc::RecvTimeoutError::Timeout) => Err(PdStreamingKvLifecycleError {
                phase: "control",
                reason: "control_read_timeout",
            }),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(PdStreamingKvLifecycleError {
                phase: "control",
                reason: "control_read",
            }),
        }
    }
}

impl PdStreamingKvSegmentReassembler {
    fn new(
        request_id: u64,
        session_id: String,
        chunk: PdStreamingKvChunkRequest,
        segment_index: usize,
        segment_count: usize,
        identity: PdStreamingKvManifestIdentity,
    ) -> Self {
        Self {
            request_id,
            session_id,
            chunk,
            segment_index,
            segment_count,
            identity,
            manifest: None,
            expected_subframe_index: 0,
            subframe_count: None,
            payload: Vec::new(),
        }
    }

    fn push(
        &mut self,
        frame: SourcePageFrame,
        subframe_payload: Vec<u8>,
    ) -> Result<Option<PdStreamingKvReceivedSegment>, PdStreamingKvLifecycleError> {
        if frame.request_id != self.request_id
            || frame.session_id != self.session_id
            || frame.chunk_index != self.chunk.chunk_index
            || frame.segment_index != self.segment_index
            || frame.segment_count != self.segment_count
        {
            return Err(reassembly_error("page_frame_metadata"));
        }
        validate_source_page_subframe_payload(&frame, &self.identity, &subframe_payload)
            .map_err(|error| reassembly_error(subframe_validation_reason(&error)))?;
        if frame.subframe_index != self.expected_subframe_index {
            return Err(reassembly_error("subframe_order"));
        }
        if frame.byte_offset != self.payload.len() as u64 {
            return Err(reassembly_error("byte_offset"));
        }
        match (&self.manifest, self.subframe_count) {
            (None, None) => {
                self.subframe_count = Some(frame.subframe_count);
                self.manifest = Some(frame.manifest.clone());
            }
            (Some(manifest), Some(subframe_count)) => {
                if frame.subframe_count != subframe_count || frame.manifest != *manifest {
                    return Err(reassembly_error("segment_identity"));
                }
            }
            _ => return Err(reassembly_error("segment_state")),
        }
        self.payload.extend_from_slice(&subframe_payload);
        self.expected_subframe_index += 1;
        let subframe_count = self
            .subframe_count
            .ok_or(reassembly_error("segment_state"))?;
        if self.expected_subframe_index < subframe_count {
            return Ok(None);
        }
        let manifest = self
            .manifest
            .clone()
            .ok_or(reassembly_error("segment_state"))?;
        if self.payload.len() as u64 != manifest.payload_bytes {
            return Err(reassembly_error("logical_payload_bytes"));
        }
        if manifest.checksum != pd_streaming_kv_payload_checksum(&self.payload) {
            return Err(reassembly_error("logical_checksum"));
        }
        Ok(Some(PdStreamingKvReceivedSegment {
            manifest,
            payload: std::mem::take(&mut self.payload),
        }))
    }

    fn finish_incomplete(&self) -> Result<(), PdStreamingKvLifecycleError> {
        if self
            .subframe_count
            .is_some_and(|count| self.expected_subframe_index == count)
        {
            Ok(())
        } else {
            Err(reassembly_error("missing_subframe"))
        }
    }
}

fn reassembly_error(reason: &'static str) -> PdStreamingKvLifecycleError {
    PdStreamingKvLifecycleError {
        phase: "reassembly",
        reason,
    }
}

fn subframe_validation_reason(error: &anyhow::Error) -> &'static str {
    match error.to_string().as_str() {
        "subframe_checksum" => "subframe_checksum",
        "subframe_payload_bytes" => "subframe_payload_bytes",
        "byte_offset" => "byte_offset",
        "subframe_index" => "subframe_index",
        "identity" => "segment_identity",
        "full_state_frame" => "full_state_frame",
        "payload_bytes" => "logical_payload_bytes",
        "checksum" => "logical_checksum",
        "cache_kind" => "cache_kind",
        "segment_kind" => "segment_kind",
        _ => "subframe_validation",
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct PdStreamingKvLifecycleResult {
    pub(super) decode_start_position: usize,
    pub(super) decoded_tokens: usize,
    pub(super) segment_count: usize,
    pub(super) payload_bytes: u64,
    pub(super) import_ms: f64,
    pub(super) bootstrap_ms: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PdStreamingKvLifecycleError {
    pub(super) phase: &'static str,
    pub(super) reason: &'static str,
}

impl PdStreamingKvLifecycleError {
    fn openai_error(self) -> OpenAiError {
        pd_streaming_kv_unavailable_error(self.reason)
    }
}

fn pd_streaming_kv_lifecycle_error(error: PdStreamingKvLifecycleError) -> OpenAiError {
    PdKvStreamLifecycleDiagnostic::new("router", "router_request_error")
        .failure(Some(error.phase), Some(error.reason))
        .emit();
    error.openai_error()
}

pub(super) trait PdStreamingKvProductionBackend {
    fn export_chunk(
        &mut self,
        chunk: PdStreamingKvChunkRequest,
        token_ids: &[i32],
    ) -> Result<Vec<PdStreamingKvReceivedSegment>, PdStreamingKvLifecycleError>;

    fn import_segment(
        &mut self,
        segment: &PdStreamingKvSegmentManifest,
        payload: &[u8],
    ) -> Result<(), PdStreamingKvLifecycleError>;

    fn bootstrap(&mut self, imported_token_count: usize)
        -> Result<(), PdStreamingKvLifecycleError>;

    fn decode(
        &mut self,
        max_tokens: u32,
        on_token: &mut dyn FnMut(i32) -> OpenAiResult<PdRouterValidationTokenControl>,
    ) -> OpenAiResult<usize>;

    fn cleanup(&mut self);
}

pub(super) struct RouterPdStreamingKvBackend<'a> {
    config: &'a PdRouterValidationConfig,
    runtime: Arc<Mutex<RuntimeState>>,
    session_id: String,
    request_id: u64,
    prompt_token_ids: &'a [i32],
    sampling: &'a SamplingConfig,
    chat_sampling_metadata: Option<&'a str>,
    control_stream: Option<TcpStream>,
    page_stream: Option<TcpStream>,
    first_decoded_token: Option<i32>,
}

impl<'a> RouterPdStreamingKvBackend<'a> {
    pub(super) fn new(
        config: &'a PdRouterValidationConfig,
        runtime: Arc<Mutex<RuntimeState>>,
        session_id: String,
        request_id: u64,
        prompt_token_ids: &'a [i32],
        sampling: &'a SamplingConfig,
        chat_sampling_metadata: Option<&'a str>,
    ) -> Self {
        Self {
            config,
            runtime,
            session_id,
            request_id,
            prompt_token_ids,
            sampling,
            chat_sampling_metadata,
            control_stream: None,
            page_stream: None,
            first_decoded_token: None,
        }
    }

    fn streaming_config(&self) -> Result<&PdStreamingKvConfig, PdStreamingKvLifecycleError> {
        self.config
            .streaming_kv
            .as_ref()
            .ok_or(PdStreamingKvLifecycleError {
                phase: "config",
                reason: "streaming_kv_config",
            })
    }

    fn ensure_connected(&mut self) -> Result<(), PdStreamingKvLifecycleError> {
        if self.control_stream.is_some() && self.page_stream.is_some() {
            return Ok(());
        }
        self.diagnostic("router_connect_start").emit();
        let config = self.streaming_config()?;
        let timeout = streaming_kv_io_timeout(self.config);
        let control_addr = resolve_streaming_addr(&config.control_addr).map_err(|_| {
            PdStreamingKvLifecycleError {
                phase: "connect",
                reason: "control_addr",
            }
        })?;
        let page_addr =
            resolve_streaming_addr(&config.page_addr).map_err(|_| PdStreamingKvLifecycleError {
                phase: "connect",
                reason: "page_addr",
            })?;
        let control = TcpStream::connect_timeout(&control_addr, timeout).map_err(|_| {
            PdStreamingKvLifecycleError {
                phase: "connect",
                reason: "control_connect",
            }
        })?;
        prepare_router_stream(&control, timeout).map_err(|_| PdStreamingKvLifecycleError {
            phase: "connect",
            reason: "control_stream",
        })?;
        let page = TcpStream::connect_timeout(&page_addr, timeout).map_err(|_| {
            PdStreamingKvLifecycleError {
                phase: "connect",
                reason: "page_connect",
            }
        })?;
        prepare_router_stream(&page, timeout).map_err(|_| PdStreamingKvLifecycleError {
            phase: "connect",
            reason: "page_stream",
        })?;
        self.control_stream = Some(control);
        self.page_stream = Some(page);
        self.diagnostic("router_connect_end").emit();
        Ok(())
    }

    fn identity(&self) -> PdStreamingKvManifestIdentity {
        PdStreamingKvManifestIdentity {
            artifact_sha256: self.config.expected_artifact_sha256.clone(),
            tokenizer_hash: self.config.expected_tokenizer_hash.clone(),
            chat_template_hash: self.config.expected_chat_template_hash.clone(),
            dtype: pd_wire_dtype_label_for_streaming(self.config.wire_dtype).to_string(),
            layout: "llama.cpp-kv-page".to_string(),
        }
    }

    fn diagnostic(&self, event: &'static str) -> PdKvStreamLifecycleDiagnostic {
        PdKvStreamLifecycleDiagnostic::new("router", event).field("request_id", self.request_id)
    }

    fn chunk_diagnostic(
        &self,
        event: &'static str,
        chunk: PdStreamingKvChunkRequest,
    ) -> PdKvStreamLifecycleDiagnostic {
        self.diagnostic(event)
            .field("chunk_index", chunk.chunk_index)
            .field("total_chunks", chunk.total_chunks)
            .field("token_start", chunk.token_start)
            .field("token_end", chunk.token_end)
            .field(
                "token_count",
                chunk.token_end.saturating_sub(chunk.token_start),
            )
    }

    fn read_control_event(
        &mut self,
        chunk: PdStreamingKvChunkRequest,
        expected: SourceControlEventKind,
    ) -> Result<SourceControlEvent, PdStreamingKvLifecycleError> {
        let max_frame_bytes = self.streaming_config()?.max_frame_bytes;
        let control_stream = self
            .control_stream
            .as_mut()
            .ok_or(PdStreamingKvLifecycleError {
                phase: "control",
                reason: "not_connected",
            })?;
        let (event, payload) =
            read_pd_stream_frame::<_, SourceControlEvent>(control_stream, max_frame_bytes)
                .map_err(|error| PdStreamingKvLifecycleError {
                    phase: "control",
                    reason: streaming_io_error_reason(
                        &error,
                        "control_read_timeout",
                        "control_read",
                    ),
                })?;
        if !payload.is_empty() {
            return Err(PdStreamingKvLifecycleError {
                phase: "control",
                reason: "control_event_metadata",
            });
        }
        self.validate_control_event(chunk, &event, expected)?;
        self.chunk_diagnostic("router_control_event_received", chunk)
            .field("control_event", format!("{:?}", event.kind))
            .field("segment_count", event.page_segments)
            .field("page_bytes", event.page_bytes)
            .emit();
        Ok(event)
    }

    fn validate_control_event(
        &self,
        chunk: PdStreamingKvChunkRequest,
        event: &SourceControlEvent,
        expected: SourceControlEventKind,
    ) -> Result<(), PdStreamingKvLifecycleError> {
        if event.protocol_version != PD_STREAMING_KV_PROTOCOL_VERSION
            || event.request_id != self.request_id
            || event.session_id != self.session_id
            || event.chunk_index != chunk.chunk_index
            || event.token_start != chunk.token_start
            || event.token_end != chunk.token_end
        {
            return Err(PdStreamingKvLifecycleError {
                phase: "control",
                reason: "control_event_metadata",
            });
        }
        if event.kind == SourceControlEventKind::Error {
            self.chunk_diagnostic("router_control_error_received", chunk)
                .failure(
                    Some(sanitize_streaming_label(
                        event.failure_phase.as_deref().unwrap_or("source"),
                        "source",
                    )),
                    Some(sanitize_streaming_label(
                        event.failure_reason.as_deref().unwrap_or("source_error"),
                        "source_error",
                    )),
                )
                .emit();
            return Err(PdStreamingKvLifecycleError {
                phase: sanitize_streaming_label(
                    event.failure_phase.as_deref().unwrap_or("source"),
                    "source",
                ),
                reason: sanitize_streaming_label(
                    event.failure_reason.as_deref().unwrap_or("source_error"),
                    "source_error",
                ),
            });
        }
        if event.kind != expected {
            return Err(PdStreamingKvLifecycleError {
                phase: "control",
                reason: "control_event_order",
            });
        }
        Ok(())
    }

    fn handle_watched_control_message(
        &self,
        chunk: PdStreamingKvChunkRequest,
        message: RouterControlWatchMessage,
    ) -> Result<Option<SourceControlEvent>, PdStreamingKvLifecycleError> {
        match message {
            RouterControlWatchMessage::Event(event) => {
                if event.kind == SourceControlEventKind::Error {
                    self.validate_control_event(chunk, &event, SourceControlEventKind::Error)?;
                }
                Ok(Some(event))
            }
            RouterControlWatchMessage::Error(error) => {
                self.chunk_diagnostic("router_control_error_received", chunk)
                    .failure(Some(error.phase), Some(error.reason))
                    .emit();
                Err(error)
            }
        }
    }
}

impl PdStreamingKvProductionBackend for RouterPdStreamingKvBackend<'_> {
    fn export_chunk(
        &mut self,
        chunk: PdStreamingKvChunkRequest,
        token_ids: &[i32],
    ) -> Result<Vec<PdStreamingKvReceivedSegment>, PdStreamingKvLifecycleError> {
        self.ensure_connected()?;
        if token_ids.is_empty()
            || token_ids.len() != chunk.token_end.saturating_sub(chunk.token_start)
        {
            return Err(PdStreamingKvLifecycleError {
                phase: "chunk",
                reason: "token_payload",
            });
        }
        let identity = self.identity();
        let request = SourceControlRequest {
            protocol_version: PD_STREAMING_KV_PROTOCOL_VERSION.to_string(),
            kind: SourceControlKind::PrefillChunk,
            session_id: self.session_id.clone(),
            request_id: self.request_id,
            chunk_index: chunk.chunk_index,
            total_chunks: chunk.total_chunks,
            total_prompt_tokens: self.prompt_token_ids.len(),
            token_start: chunk.token_start,
            token_count: token_ids.len(),
            identity: identity.clone(),
        };
        let payload = encode_token_payload(token_ids);
        let max_frame_bytes = self.streaming_config()?.max_frame_bytes;
        self.chunk_diagnostic("router_chunk_request_send", chunk)
            .field("token_count", token_ids.len())
            .emit();
        let control_stream = self
            .control_stream
            .as_mut()
            .ok_or(PdStreamingKvLifecycleError {
                phase: "control",
                reason: "not_connected",
            })?;
        write_pd_stream_frame(control_stream, &request, &payload, max_frame_bytes).map_err(
            |error| PdStreamingKvLifecycleError {
                phase: "control",
                reason: streaming_io_error_reason(&error, "control_write_timeout", "control_write"),
            },
        )?;
        self.chunk_diagnostic("router_chunk_request_sent", chunk)
            .field("token_count", token_ids.len())
            .emit();
        self.read_control_event(chunk, SourceControlEventKind::PrefillStarted)?;
        self.read_control_event(chunk, SourceControlEventKind::PrefillCompleted)?;
        self.read_control_event(chunk, SourceControlEventKind::ExportStarted)?;
        let exported = self.read_control_event(chunk, SourceControlEventKind::ExportCompleted)?;
        if exported.page_segments == 0 || exported.page_bytes == 0 {
            return Err(PdStreamingKvLifecycleError {
                phase: "source",
                reason: "missing_segment",
            });
        }
        let mut control_watch = RouterControlErrorWatch::start(
            self.control_stream
                .as_ref()
                .ok_or(PdStreamingKvLifecycleError {
                    phase: "control",
                    reason: "not_connected",
                })?,
            self.page_stream
                .as_ref()
                .ok_or(PdStreamingKvLifecycleError {
                    phase: "page_stream",
                    reason: "not_connected",
                })?,
            max_frame_bytes,
        )?;
        let mut segments = Vec::with_capacity(exported.page_segments);
        for segment_index in 0..exported.page_segments {
            self.chunk_diagnostic("router_segment_reassembly_start", chunk)
                .field("segment_index", segment_index)
                .field("segment_count", exported.page_segments)
                .emit();
            let mut reassembler = PdStreamingKvSegmentReassembler::new(
                self.request_id,
                self.session_id.clone(),
                chunk,
                segment_index,
                exported.page_segments,
                identity.clone(),
            );
            loop {
                self.chunk_diagnostic("router_page_frame_receive_start", chunk)
                    .field("segment_index", segment_index)
                    .field("segment_count", exported.page_segments)
                    .field(
                        "expected_subframe_index",
                        reassembler.expected_subframe_index,
                    )
                    .emit();
                let page_stream = self
                    .page_stream
                    .as_mut()
                    .ok_or(PdStreamingKvLifecycleError {
                        phase: "page_stream",
                        reason: "not_connected",
                    })?;
                let (frame, payload) = match read_pd_stream_frame::<_, SourcePageFrame>(
                    page_stream,
                    max_frame_bytes,
                ) {
                    Ok(frame) => frame,
                    Err(error) => {
                        if let Some(message) =
                            control_watch.recv_after_page_error(Duration::from_millis(100))
                        {
                            if let Some(event) =
                                self.handle_watched_control_message(chunk, message)?
                            {
                                control_watch.cached =
                                    Some(RouterControlWatchMessage::Event(event));
                            }
                        }
                        return Err(PdStreamingKvLifecycleError {
                            phase: "page_stream",
                            reason: streaming_io_error_reason(
                                &error,
                                "page_read_timeout",
                                "page_read",
                            ),
                        });
                    }
                };
                if let Some(message) = control_watch.try_recv() {
                    if let Some(event) = self.handle_watched_control_message(chunk, message)? {
                        control_watch.cached = Some(RouterControlWatchMessage::Event(event));
                    }
                }
                self.chunk_diagnostic("router_subframe_received", chunk)
                    .field("segment_index", frame.segment_index)
                    .field("segment_count", frame.segment_count)
                    .field("subframe_index", frame.subframe_index)
                    .field("subframe_count", frame.subframe_count)
                    .field("byte_offset", frame.byte_offset)
                    .field("cache_kind", frame.manifest.cache_kind.as_str())
                    .field("segment_kind", frame.manifest.segment_kind.as_str())
                    .field("payload_bytes", payload.len())
                    .field("logical_payload_bytes", frame.manifest.payload_bytes)
                    .field("checksum_present", true)
                    .emit();
                match reassembler.push(frame, payload) {
                    Ok(Some(segment)) => {
                        self.chunk_diagnostic("router_segment_reassembly_end", chunk)
                            .field("segment_index", segment_index)
                            .field("segment_count", exported.page_segments)
                            .field("cache_kind", segment.manifest.cache_kind.as_str())
                            .field("segment_kind", segment.manifest.segment_kind.as_str())
                            .field("payload_bytes", segment.payload.len())
                            .field("checksum_valid", true)
                            .field("identity_valid", true)
                            .emit();
                        segments.push(segment);
                        break;
                    }
                    Ok(None) => {}
                    Err(error) => {
                        self.chunk_diagnostic("router_segment_reassembly_error", chunk)
                            .field("segment_index", segment_index)
                            .field("segment_count", exported.page_segments)
                            .failure(Some(error.phase), Some(error.reason))
                            .emit();
                        return Err(error);
                    }
                }
            }
            if let Err(error) = reassembler.finish_incomplete() {
                self.chunk_diagnostic("router_segment_reassembly_error", chunk)
                    .field("segment_index", segment_index)
                    .field("segment_count", exported.page_segments)
                    .failure(Some(error.phase), Some(error.reason))
                    .emit();
                return Err(error);
            }
        }
        let chunk_done =
            match control_watch.wait_for_terminal_event(streaming_kv_io_timeout(self.config)) {
                Ok(event) => event,
                Err(error) => {
                    self.chunk_diagnostic("router_control_error_received", chunk)
                        .failure(Some(error.phase), Some(error.reason))
                        .emit();
                    return Err(error);
                }
            };
        self.validate_control_event(chunk, &chunk_done, SourceControlEventKind::ChunkDone)?;
        self.chunk_diagnostic("router_control_event_received", chunk)
            .field("control_event", format!("{:?}", chunk_done.kind))
            .field("segment_count", chunk_done.page_segments)
            .field("page_bytes", chunk_done.page_bytes)
            .emit();
        Ok(segments)
    }

    fn import_segment(
        &mut self,
        segment: &PdStreamingKvSegmentManifest,
        payload: &[u8],
    ) -> Result<(), PdStreamingKvLifecycleError> {
        self.diagnostic("router_import_kv_page_start")
            .field("chunk_index", segment.chunk_index)
            .field("token_start", segment.token_start)
            .field("token_end", segment.token_end)
            .field("cache_kind", segment.cache_kind.as_str())
            .field("segment_kind", segment.segment_kind.as_str())
            .field("payload_bytes", payload.len())
            .emit();
        let desc =
            runtime_desc_from_manifest(segment).map_err(|_| PdStreamingKvLifecycleError {
                phase: "import",
                reason: "native_desc",
            })?;
        let mut runtime = self
            .runtime
            .lock()
            .map_err(|_| PdStreamingKvLifecycleError {
                phase: "import",
                reason: "runtime_lock",
            })?;
        runtime
            .import_kv_page(&self.session_id, &desc, payload)
            .map_err(|_| PdStreamingKvLifecycleError {
                phase: "import",
                reason: "import_failed",
            })?;
        self.diagnostic("router_import_kv_page_end")
            .field("chunk_index", segment.chunk_index)
            .field("token_start", segment.token_start)
            .field("token_end", segment.token_end)
            .field("cache_kind", segment.cache_kind.as_str())
            .field("segment_kind", segment.segment_kind.as_str())
            .field("payload_bytes", payload.len())
            .emit();
        Ok(())
    }

    fn bootstrap(
        &mut self,
        imported_token_count: usize,
    ) -> Result<(), PdStreamingKvLifecycleError> {
        self.diagnostic("router_trim_replay_bootstrap_start")
            .field("decode_start_position", imported_token_count)
            .field("logits_ready", false)
            .emit();
        if imported_token_count == 0 || imported_token_count > self.prompt_token_ids.len() {
            return Err(PdStreamingKvLifecycleError {
                phase: "bootstrap",
                reason: "token_count",
            });
        }
        let replay_token = self.prompt_token_ids[imported_token_count - 1];
        let trim_target = imported_token_count - 1;
        let mut runtime = self
            .runtime
            .lock()
            .map_err(|_| PdStreamingKvLifecycleError {
                phase: "bootstrap",
                reason: "runtime_lock",
            })?;
        if let Some(metadata) = self.chat_sampling_metadata {
            runtime
                .configure_chat_sampling(
                    &self.session_id,
                    metadata,
                    self.prompt_token_ids.len() as u64,
                    self.sampling.enabled.then_some(self.sampling),
                )
                .map_err(|_| PdStreamingKvLifecycleError {
                    phase: "bootstrap",
                    reason: "sampling_metadata",
                })?;
        }
        runtime
            .trim_session(&self.session_id, trim_target as u64)
            .map_err(|_| PdStreamingKvLifecycleError {
                phase: "bootstrap",
                reason: "trim_failed",
            })?;
        let first = runtime
            .decode_sampled(
                &self.session_id,
                replay_token,
                self.sampling.enabled.then_some(self.sampling),
            )
            .map_err(|_| PdStreamingKvLifecycleError {
                phase: "bootstrap",
                reason: "replay_failed",
            })?;
        self.first_decoded_token = Some(first);
        self.diagnostic("router_trim_replay_bootstrap_end")
            .field("decode_start_position", imported_token_count)
            .field("logits_ready", true)
            .emit();
        Ok(())
    }

    fn decode(
        &mut self,
        max_tokens: u32,
        on_token: &mut dyn FnMut(i32) -> OpenAiResult<PdRouterValidationTokenControl>,
    ) -> OpenAiResult<usize> {
        if max_tokens == 0 {
            return Ok(0);
        }
        self.diagnostic("router_decode_start")
            .field("requested_max_tokens", max_tokens)
            .emit();
        let mut current = self
            .first_decoded_token
            .take()
            .ok_or_else(|| pd_streaming_kv_unavailable_error("bootstrap_logits_not_ready"))?;
        let mut decoded = 0usize;
        loop {
            decoded += 1;
            let control = on_token(current)?;
            if control.control == TokenControl::Stop || decoded >= max_tokens as usize {
                break;
            }
            let mut runtime = self
                .runtime
                .lock()
                .map_err(|_| OpenAiError::backend("runtime lock poisoned"))?;
            current = runtime
                .decode_sampled(
                    &self.session_id,
                    current,
                    self.sampling.enabled.then_some(self.sampling),
                )
                .map_err(openai_backend_error)?;
        }
        Ok(decoded)
    }

    fn cleanup(&mut self) {
        self.diagnostic("router_cleanup").emit();
        if let Some(mut control_stream) = self.control_stream.take() {
            let _ = control_stream.set_write_timeout(Some(PD_STREAMING_KV_CLEANUP_WRITE_TIMEOUT));
            if let Some(config) = self.config.streaming_kv.as_ref() {
                let request = SourceControlRequest {
                    protocol_version: PD_STREAMING_KV_PROTOCOL_VERSION.to_string(),
                    kind: SourceControlKind::Stop,
                    session_id: self.session_id.clone(),
                    request_id: self.request_id,
                    chunk_index: 0,
                    total_chunks: 1,
                    total_prompt_tokens: self.prompt_token_ids.len(),
                    token_start: 0,
                    token_count: 0,
                    identity: self.identity(),
                };
                let _ = write_pd_stream_frame(
                    &mut control_stream,
                    &request,
                    &[],
                    config.max_frame_bytes,
                );
            }
            let _ = control_stream.shutdown(Shutdown::Both);
        }
        if let Some(page_stream) = self.page_stream.take() {
            let _ = page_stream.shutdown(Shutdown::Both);
        }
        if let Ok(mut runtime) = self.runtime.lock() {
            let _ = runtime.drop_session_timed(&self.session_id);
        }
    }
}

pub(super) fn run_pd_streaming_kv_production_lifecycle(
    config: &PdRouterValidationConfig,
    prompt_token_ids: &[i32],
    max_tokens: u32,
    backend: &mut dyn PdStreamingKvProductionBackend,
    mut on_token: impl FnMut(i32) -> OpenAiResult<PdRouterValidationTokenControl>,
) -> OpenAiResult<PdStreamingKvLifecycleResult> {
    let result = run_pd_streaming_kv_production_lifecycle_inner(
        config,
        prompt_token_ids,
        max_tokens,
        backend,
        &mut on_token,
    );
    backend.cleanup();
    result
}

fn run_pd_streaming_kv_production_lifecycle_inner(
    config: &PdRouterValidationConfig,
    prompt_token_ids: &[i32],
    max_tokens: u32,
    backend: &mut dyn PdStreamingKvProductionBackend,
    on_token: &mut dyn FnMut(i32) -> OpenAiResult<PdRouterValidationTokenControl>,
) -> OpenAiResult<PdStreamingKvLifecycleResult> {
    let chunked_config = config.chunked_prefill.ok_or_else(|| {
        pd_streaming_kv_unavailable_error("chunked_prefill_required_for_streaming_kv")
    })?;
    if prompt_token_ids.is_empty() {
        return Err(OpenAiError::invalid_request(
            "PD streaming KV requires at least one prompt token",
        ));
    }
    let plan =
        PdChunkedPrefillPlan::new(prompt_token_ids.len(), chunked_config).map_err(|error| {
            OpenAiError::backend(format!("PD streaming KV chunk planning failed: {error}"))
        })?;
    let identity = PdStreamingKvManifestIdentity {
        artifact_sha256: config.expected_artifact_sha256.clone(),
        tokenizer_hash: config.expected_tokenizer_hash.clone(),
        chat_template_hash: config.expected_chat_template_hash.clone(),
        dtype: pd_wire_dtype_label_for_streaming(config.wire_dtype).to_string(),
        layout: "llama.cpp-kv-page".to_string(),
    };

    let mut all_segments = Vec::new();
    let mut payload_bytes = 0_u64;
    let mut import_ms = 0.0_f64;
    for chunk in &plan.chunks {
        let request = PdStreamingKvChunkRequest {
            chunk_index: chunk.index,
            total_chunks: plan.chunks.len(),
            token_start: chunk.start_position,
            token_end: chunk.end_position,
        };
        let token_ids = &prompt_token_ids[chunk.start_position..chunk.end_position];
        let segments = backend
            .export_chunk(request, token_ids)
            .map_err(pd_streaming_kv_lifecycle_error)?;
        let segment_manifests = segments
            .iter()
            .map(|segment| segment.manifest.clone())
            .collect::<Vec<_>>();
        let chunk_payload_bytes = segments
            .iter()
            .map(|segment| segment.payload.len() as u64)
            .try_fold(0_u64, |sum, bytes| sum.checked_add(bytes))
            .ok_or_else(|| pd_streaming_kv_unavailable_error("payload_bytes"))?;
        if let Err(error) = validate_pd_streaming_kv_chunk_segments(&segments, &identity, request) {
            PdKvStreamLifecycleDiagnostic::new("router", "router_chunk_validation_fail")
                .field("chunk_index", request.chunk_index)
                .field("total_chunks", request.total_chunks)
                .field("token_start", request.token_start)
                .field("token_end", request.token_end)
                .field("segment_count", segments.len())
                .failure(Some("manifest"), Some(error.reason))
                .emit();
            return Err(pd_streaming_kv_unavailable_error(error.reason));
        }
        for segment in &segments {
            let import_timer = PhaseTimer::start();
            backend
                .import_segment(&segment.manifest, &segment.payload)
                .map_err(pd_streaming_kv_lifecycle_error)?;
            import_ms += import_timer.elapsed_ms();
        }
        payload_bytes = payload_bytes.saturating_add(chunk_payload_bytes);
        all_segments.extend(segment_manifests);
    }
    if let Err(error) = validate_pd_streaming_kv_segments(&all_segments, &identity) {
        PdKvStreamLifecycleDiagnostic::new("router", "router_final_contiguous_gate_fail")
            .field("segment_count", all_segments.len())
            .field("payload_bytes", payload_bytes)
            .failure(Some("final_gate"), Some(error.reason))
            .emit();
        return Err(pd_streaming_kv_unavailable_error(error.reason));
    }
    let decode_start_position = prompt_token_ids.len();
    PdKvStreamLifecycleDiagnostic::new("router", "router_final_contiguous_gate_pass")
        .field("segment_count", all_segments.len())
        .field("payload_bytes", payload_bytes)
        .field("decode_start_position", decode_start_position)
        .emit();
    let bootstrap_timer = PhaseTimer::start();
    PdKvStreamLifecycleDiagnostic::new("router", "router_trim_replay_bootstrap_start")
        .field("decode_start_position", decode_start_position)
        .field("logits_ready", false)
        .emit();
    backend
        .bootstrap(decode_start_position)
        .map_err(pd_streaming_kv_lifecycle_error)?;
    let bootstrap_ms = bootstrap_timer.elapsed_ms();
    PdKvStreamLifecycleDiagnostic::new("router", "router_trim_replay_bootstrap_end")
        .field("decode_start_position", decode_start_position)
        .field("logits_ready", true)
        .field("bootstrap_ms", bootstrap_ms)
        .emit();
    PdKvStreamLifecycleDiagnostic::new("router", "router_decode_start")
        .field("decode_start_position", decode_start_position)
        .field("requested_max_tokens", max_tokens)
        .emit();
    let decoded_tokens = backend.decode(max_tokens, on_token)?;
    Ok(PdStreamingKvLifecycleResult {
        decode_start_position,
        decoded_tokens,
        segment_count: all_segments.len(),
        payload_bytes,
        import_ms,
        bootstrap_ms,
    })
}

fn validate_pd_streaming_kv_chunk_segments(
    segments: &[PdStreamingKvReceivedSegment],
    identity: &PdStreamingKvManifestIdentity,
    chunk: PdStreamingKvChunkRequest,
) -> Result<(), PdStreamingKvManifestError> {
    let first =
        segments
            .first()
            .map(|segment| &segment.manifest)
            .ok_or(PdStreamingKvManifestError {
                reason: "missing_segment",
            })?;
    if first.chunk_index != chunk.chunk_index
        || first.total_chunks != chunk.total_chunks
        || first.token_start != chunk.token_start
        || first.token_end != chunk.token_end
    {
        return Err(PdStreamingKvManifestError {
            reason: "chunk_metadata",
        });
    }
    for segment in segments.iter().map(|segment| &segment.manifest) {
        if segment.chunk_index != first.chunk_index
            || segment.total_chunks != first.total_chunks
            || segment.token_start != first.token_start
            || segment.token_end != first.token_end
        {
            return Err(PdStreamingKvManifestError {
                reason: "segment_token_range",
            });
        }
    }
    let expected = if first.cache_kind == "iswa" {
        segments.len() == 2
            && segments
                .iter()
                .any(|segment| segment.manifest.segment_kind == "base")
            && segments
                .iter()
                .any(|segment| segment.manifest.segment_kind == "swa")
    } else {
        segments.len() == 1 && segments[0].manifest.segment_kind == "regular"
    };
    if !expected {
        return Err(PdStreamingKvManifestError {
            reason: "missing_segment",
        });
    }
    for segment in segments {
        validate_pd_streaming_kv_segment_fields(&segment.manifest, identity)?;
        if segment.manifest.payload_bytes == 0
            || segment.manifest.payload_bytes != segment.payload.len() as u64
            || segment.manifest.checksum != pd_streaming_kv_payload_checksum(&segment.payload)
        {
            return Err(PdStreamingKvManifestError {
                reason: "payload_bytes",
            });
        }
    }
    Ok(())
}

fn pd_wire_dtype_label_for_streaming(dtype: WireActivationDType) -> &'static str {
    match dtype {
        WireActivationDType::F32 => "f32",
        WireActivationDType::F16 => "f16",
        WireActivationDType::Q8 => "q8",
    }
}

fn resolve_streaming_addr(endpoint: &str) -> Result<SocketAddr> {
    let endpoint = endpoint.strip_prefix("tcp://").unwrap_or(endpoint);
    endpoint
        .to_socket_addrs()
        .with_context(|| format!("resolve PD streaming KV endpoint {endpoint}"))?
        .next()
        .ok_or_else(|| anyhow!("PD streaming KV endpoint did not resolve"))
}

fn streaming_kv_io_timeout(config: &PdRouterValidationConfig) -> Duration {
    Duration::from_secs(config.startup_timeout_secs.max(1))
}

fn prepare_router_stream(stream: &TcpStream, timeout: Duration) -> io::Result<()> {
    stream.set_nodelay(true).ok();
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    Ok(())
}

fn streaming_io_error_reason(
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

fn runtime_desc_from_manifest(segment: &PdStreamingKvSegmentManifest) -> Result<RuntimeKvPageDesc> {
    let desc = segment
        .native_desc
        .as_ref()
        .ok_or_else(|| anyhow!("missing native KV page descriptor"))?;
    if desc.layer_start != i32::try_from(segment.layer_start)?
        || desc.layer_end != i32::try_from(segment.layer_end)?
        || desc.token_start != u64::try_from(segment.token_start)?
        || desc.token_count != u64::try_from(segment.token_end.saturating_sub(segment.token_start))?
        || desc.payload_bytes != segment.payload_bytes
    {
        bail!("native KV page descriptor does not match manifest");
    }
    Ok(RuntimeKvPageDesc {
        version: desc.version,
        layer_start: desc.layer_start,
        layer_end: desc.layer_end,
        token_start: desc.token_start,
        token_count: desc.token_count,
        layer_count: desc.layer_count,
        k_type: desc.k_type,
        v_type: desc.v_type,
        k_row_bytes: desc.k_row_bytes,
        v_row_bytes: desc.v_row_bytes,
        v_element_bytes: desc.v_element_bytes,
        payload_bytes: desc.payload_bytes,
        flags: desc.flags,
    })
}

fn sanitize_streaming_label(value: &str, fallback: &'static str) -> &'static str {
    match value {
        "source" => "source",
        "control" => "control",
        "page_stream" => "page_stream",
        "manifest" => "manifest",
        "import" => "import",
        "bootstrap" => "bootstrap",
        "decode" => "decode",
        "request_validation" => "request_validation",
        "control_write" => "control_write",
        "runtime_lock" => "runtime_lock",
        "prefill_failed" => "prefill_failed",
        "prefill_position" => "prefill_position",
        "export_failed" => "export_failed",
        "missing_segment" => "missing_segment",
        "payload_bytes" => "payload_bytes",
        "capacity" => "capacity",
        "frame_too_large" => "frame_too_large",
        "source_frame_too_large" => "source_frame_too_large",
        "checksum" => "checksum",
        "manifest_validation" => "manifest_validation",
        "reassembly" => "reassembly",
        "subframe_validation" => "subframe_validation",
        "subframe_checksum" => "subframe_checksum",
        "subframe_payload_bytes" => "subframe_payload_bytes",
        "subframe_order" => "subframe_order",
        "subframe_index" => "subframe_index",
        "byte_offset" => "byte_offset",
        "logical_payload_bytes" => "logical_payload_bytes",
        "logical_checksum" => "logical_checksum",
        "segment_identity" => "segment_identity",
        "missing_subframe" => "missing_subframe",
        "page_write" => "page_write",
        "control_read" => "control_read",
        "control_read_timeout" => "control_read_timeout",
        "control_write_timeout" => "control_write_timeout",
        "page_read" => "page_read",
        "page_read_timeout" => "page_read_timeout",
        "page_write_timeout" => "page_write_timeout",
        "source_error" => "source_error",
        "source_stream_error" => "source_stream_error",
        _ => fallback,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binary_transport::pd_streaming_kv_source::SourcePageFrameKind;
    use std::net::TcpListener;

    const ARTIFACT: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const TOKENIZER: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const TEMPLATE: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

    fn s(value: &str) -> String {
        value.to_string()
    }

    fn identity() -> PdStreamingKvManifestIdentity {
        PdStreamingKvManifestIdentity {
            artifact_sha256: s(ARTIFACT),
            tokenizer_hash: s(TOKENIZER),
            chat_template_hash: s(TEMPLATE),
            dtype: s("f16"),
            layout: s("llama.cpp-kv-page"),
        }
    }

    fn segment(
        chunk_index: usize,
        token_start: usize,
        token_end: usize,
        segment_kind: &'static str,
    ) -> PdStreamingKvSegmentManifest {
        PdStreamingKvSegmentManifest {
            protocol_version: s(PD_STREAMING_KV_PROTOCOL_VERSION),
            chunk_index,
            total_chunks: 2,
            token_start,
            token_end,
            cache_kind: s("iswa"),
            segment_kind: s(segment_kind),
            layer_start: 0,
            layer_end: 60,
            dtype: s("f16"),
            layout: s("llama.cpp-kv-page"),
            payload_bytes: 1024,
            checksum_algorithm: s(PD_STREAMING_KV_CHECKSUM_ALGORITHM),
            checksum: pd_streaming_kv_payload_checksum(&vec![1_u8; 1024]),
            artifact_sha256: s(ARTIFACT),
            tokenizer_hash: s(TOKENIZER),
            chat_template_hash: s(TEMPLATE),
            frame_kind: PdStreamingKvFrameKind::KvPage,
            native_desc: None,
        }
    }

    fn valid_segments() -> Vec<PdStreamingKvSegmentManifest> {
        vec![
            segment(0, 0, 64, "base"),
            segment(0, 0, 64, "swa"),
            segment(1, 64, 128, "base"),
            segment(1, 64, 128, "swa"),
        ]
    }

    fn received_segment(segment: PdStreamingKvSegmentManifest) -> PdStreamingKvReceivedSegment {
        PdStreamingKvReceivedSegment {
            payload: vec![1_u8; segment.payload_bytes as usize],
            manifest: segment,
        }
    }

    fn manifest_for_payload(
        payload: &[u8],
        segment_kind: &'static str,
    ) -> PdStreamingKvSegmentManifest {
        let mut manifest = segment(0, 0, 64, segment_kind);
        manifest.payload_bytes = payload.len() as u64;
        manifest.checksum = pd_streaming_kv_payload_checksum(payload);
        manifest
    }

    fn subframe(
        manifest: PdStreamingKvSegmentManifest,
        subframe_index: usize,
        subframe_count: usize,
        byte_offset: u64,
        payload: &[u8],
    ) -> SourcePageFrame {
        SourcePageFrame {
            protocol_version: s(PD_STREAMING_KV_PROTOCOL_VERSION),
            kind: SourcePageFrameKind::PageSubframe,
            request_id: 7,
            session_id: s("session-a"),
            chunk_index: manifest.chunk_index,
            segment_index: 0,
            segment_count: 2,
            subframe_index,
            subframe_count,
            byte_offset,
            subframe_payload_bytes: payload.len() as u64,
            subframe_checksum_algorithm: s(PD_STREAMING_KV_CHECKSUM_ALGORITHM),
            subframe_checksum: pd_streaming_kv_payload_checksum(payload),
            manifest,
        }
    }

    fn control_event(
        kind: SourceControlEventKind,
        failure_phase: Option<&'static str>,
        failure_reason: Option<&'static str>,
    ) -> SourceControlEvent {
        SourceControlEvent {
            protocol_version: s(PD_STREAMING_KV_PROTOCOL_VERSION),
            kind,
            request_id: 7,
            session_id: s("session-a"),
            chunk_index: 0,
            token_start: 0,
            token_end: 64,
            page_segments: 2,
            page_bytes: 1024,
            failure_phase: failure_phase.map(s),
            failure_reason: failure_reason.map(s),
        }
    }

    fn tcp_pair() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let client = TcpStream::connect(addr).unwrap();
        let (server, _) = listener.accept().unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        server
            .set_write_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        (client, server)
    }

    fn reassembler() -> PdStreamingKvSegmentReassembler {
        PdStreamingKvSegmentReassembler::new(
            7,
            s("session-a"),
            PdStreamingKvChunkRequest {
                chunk_index: 0,
                total_chunks: 2,
                token_start: 0,
                token_end: 64,
            },
            0,
            2,
            identity(),
        )
    }

    fn config() -> PdRouterValidationConfig {
        PdRouterValidationConfig {
            mode: PdServingMode::Mvp,
            prefill_addr: s("127.0.0.1:19081"),
            decode_addr: s("127.0.0.1:19082"),
            wire_dtype: WireActivationDType::F16,
            startup_timeout_secs: 1,
            model_id: s("model"),
            expected_artifact_sha256: s(ARTIFACT),
            expected_tokenizer_hash: s(TOKENIZER),
            expected_chat_template_hash: s(TEMPLATE),
            source_node_id: s("pgx-prefill-mvp"),
            target_node_id: s("mac-decode-mvp"),
            fault_injection: PdRouterValidationFault::None,
            mvp_test_fault: PdServingMvpTestFault::None,
            admission: None,
            chunked_prefill: Some(PdChunkedPrefillConfig::new(64, 64).unwrap()),
            streaming_kv: Some(PdStreamingKvConfig {
                control_addr: s("127.0.0.1:19430"),
                page_addr: s("127.0.0.1:19431"),
                max_in_flight_chunks: 2,
                max_in_flight_bytes: 536_870_912,
                max_frame_bytes: 67_108_864,
                max_queue_depth: 4,
            }),
        }
    }

    #[derive(Default)]
    struct MockStreamingBackend {
        export_error_phase: Option<&'static str>,
        export_error: Option<&'static str>,
        corrupt_checksum: bool,
        full_state_frame: bool,
        missing_segment_chunk: Option<usize>,
        import_error: Option<&'static str>,
        bootstrap_error: Option<&'static str>,
        exports: Vec<PdStreamingKvChunkRequest>,
        imported: Vec<(usize, String)>,
        bootstrapped: Option<usize>,
        cleanup_called: bool,
    }

    impl MockStreamingBackend {
        fn segment_for(
            chunk: PdStreamingKvChunkRequest,
            segment_kind: &'static str,
        ) -> PdStreamingKvSegmentManifest {
            let mut segment = segment(
                chunk.chunk_index,
                chunk.token_start,
                chunk.token_end,
                segment_kind,
            );
            segment.total_chunks = chunk.total_chunks;
            segment
        }
    }

    impl PdStreamingKvProductionBackend for MockStreamingBackend {
        fn export_chunk(
            &mut self,
            chunk: PdStreamingKvChunkRequest,
            _token_ids: &[i32],
        ) -> Result<Vec<PdStreamingKvReceivedSegment>, PdStreamingKvLifecycleError> {
            self.exports.push(chunk);
            if let Some(reason) = self.export_error {
                return Err(PdStreamingKvLifecycleError {
                    phase: self.export_error_phase.unwrap_or("export"),
                    reason,
                });
            }
            if self.missing_segment_chunk == Some(chunk.chunk_index) {
                return Ok(vec![received_segment(Self::segment_for(chunk, "base"))]);
            }
            let mut segments = vec![
                Self::segment_for(chunk, "base"),
                Self::segment_for(chunk, "swa"),
            ];
            if self.corrupt_checksum {
                segments[0].checksum = s("bad-checksum");
            }
            if self.full_state_frame {
                segments[0].frame_kind = PdStreamingKvFrameKind::FullState;
            }
            Ok(segments.into_iter().map(received_segment).collect())
        }

        fn import_segment(
            &mut self,
            segment: &PdStreamingKvSegmentManifest,
            _payload: &[u8],
        ) -> Result<(), PdStreamingKvLifecycleError> {
            if let Some(reason) = self.import_error {
                return Err(PdStreamingKvLifecycleError {
                    phase: "import",
                    reason,
                });
            }
            self.imported
                .push((segment.chunk_index, segment.segment_kind.clone()));
            Ok(())
        }

        fn bootstrap(
            &mut self,
            imported_token_count: usize,
        ) -> Result<(), PdStreamingKvLifecycleError> {
            if let Some(reason) = self.bootstrap_error {
                return Err(PdStreamingKvLifecycleError {
                    phase: "bootstrap",
                    reason,
                });
            }
            self.bootstrapped = Some(imported_token_count);
            Ok(())
        }

        fn decode(
            &mut self,
            max_tokens: u32,
            on_token: &mut dyn FnMut(i32) -> OpenAiResult<PdRouterValidationTokenControl>,
        ) -> OpenAiResult<usize> {
            let mut decoded = 0usize;
            for token in 0..max_tokens {
                decoded += 1;
                let control = on_token(1000 + token as i32)?;
                if control.control == TokenControl::Stop {
                    break;
                }
            }
            Ok(decoded)
        }

        fn cleanup(&mut self) {
            self.cleanup_called = true;
        }
    }

    #[test]
    fn streaming_kv_manifest_accepts_iswa_base_swa_segments() {
        validate_pd_streaming_kv_segments(&valid_segments(), &identity()).unwrap();
    }

    #[test]
    fn streaming_kv_manifest_rejects_fail_closed_cases() {
        let cases = [
            ("checksum", {
                let mut segments = valid_segments();
                segments[0].checksum = s("not-a-checksum");
                segments
            }),
            ("identity", {
                let mut segments = valid_segments();
                segments[0].artifact_sha256 =
                    s("eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee");
                segments
            }),
            ("position_gap", {
                let mut segments = valid_segments();
                segments[2].token_start = 65;
                segments[3].token_start = 65;
                segments
            }),
            ("position_overlap", {
                let mut segments = valid_segments();
                segments[2].token_start = 63;
                segments[3].token_start = 63;
                segments
            }),
            ("missing_segment", {
                let mut segments = valid_segments();
                segments.remove(1);
                segments
            }),
            ("out_of_order_chunk", {
                let mut segments = valid_segments();
                segments[2].chunk_index = 0;
                segments[3].chunk_index = 0;
                segments
            }),
            ("full_state_frame", {
                let mut segments = valid_segments();
                segments[0].frame_kind = PdStreamingKvFrameKind::FullState;
                segments
            }),
        ];
        for (reason, segments) in cases {
            let error = validate_pd_streaming_kv_segments(&segments, &identity()).unwrap_err();
            assert_eq!(error.reason, reason);
        }
    }

    #[test]
    fn streaming_kv_capacity_rejects_invalid_limits() {
        let error = validate_pd_streaming_kv_capacity(PdStreamingKvCapacity {
            max_in_flight_chunks: 0,
            max_in_flight_bytes: 1,
            max_frame_bytes: 1,
            max_queue_depth: 1,
        })
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("--pd-stream-max-in-flight-chunks"),
            "{error:?}"
        );

        let error = validate_pd_streaming_kv_capacity(PdStreamingKvCapacity {
            max_in_flight_chunks: 2,
            max_in_flight_bytes: 1,
            max_frame_bytes: 2,
            max_queue_depth: 2,
        })
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("--pd-stream-max-frame-bytes cannot exceed"),
            "{error:?}"
        );
    }

    #[test]
    fn streaming_kv_lifecycle_diagnostic_line_is_bounded_and_sanitized() {
        let line = pd_kv_stream_lifecycle_line(
            &PdKvStreamLifecycleDiagnostic::new("router", "router_page_frame_received")
                .field("request_id", 7_u64)
                .field("chunk_index", 1_usize)
                .field("token_start", 64_usize)
                .field("token_end", 128_usize)
                .field("token_count", 64_usize)
                .field("segment_count", 2_usize)
                .field("payload_bytes", 4096_u64)
                .field("checksum_present", true)
                .field("checksum_valid", true)
                .field("identity_valid", true)
                .field("cache_kind", "iswa")
                .field("segment_kind", "base"),
        );

        assert!(line.starts_with(PD_KV_STREAM_LIFECYCLE_PREFIX));
        assert!(line.contains("router_page_frame_received"));
        assert!(line.contains("\"token_count\":64"));
        assert!(line.contains("\"payload_bytes\":4096"));
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
    fn streaming_kv_reassembler_accepts_valid_multi_subframe_segment() {
        let payload = b"abcdefghij".to_vec();
        let manifest = manifest_for_payload(&payload, "base");
        let mut reassembler = reassembler();

        assert!(reassembler
            .push(
                subframe(manifest.clone(), 0, 3, 0, &payload[..4]),
                payload[..4].to_vec()
            )
            .unwrap()
            .is_none());
        assert!(reassembler
            .push(
                subframe(manifest.clone(), 1, 3, 4, &payload[4..8]),
                payload[4..8].to_vec()
            )
            .unwrap()
            .is_none());
        let segment = reassembler
            .push(
                subframe(manifest, 2, 3, 8, &payload[8..]),
                payload[8..].to_vec(),
            )
            .unwrap()
            .expect("segment complete");

        assert_eq!(segment.payload, payload);
        assert_eq!(segment.manifest.segment_kind, "base");
    }

    #[test]
    fn streaming_kv_control_watch_reports_source_error_while_waiting_first_page_frame() {
        let (control_router, mut control_source) = tcp_pair();
        let (mut page_router, _page_source) = tcp_pair();
        let mut watch =
            RouterControlErrorWatch::start(&control_router, &page_router, 1024).unwrap();

        write_pd_stream_frame(
            &mut control_source,
            &control_event(
                SourceControlEventKind::Error,
                Some("source"),
                Some("source_frame_too_large"),
            ),
            &[],
            1024,
        )
        .unwrap();

        let page_error =
            read_pd_stream_frame::<_, SourcePageFrame>(&mut page_router, 1024).unwrap_err();
        let message = watch
            .recv_after_page_error(Duration::from_secs(1))
            .expect("control error should be observed before page timeout fallback");

        assert_ne!(
            streaming_io_error_reason(&page_error, "page_read_timeout", "page_read"),
            "page_read_timeout"
        );
        match message {
            RouterControlWatchMessage::Event(event) => {
                assert_eq!(event.kind, SourceControlEventKind::Error);
                assert_eq!(
                    event.failure_reason.as_deref(),
                    Some("source_frame_too_large")
                );
            }
            RouterControlWatchMessage::Error(error) => panic!("unexpected watch error: {error:?}"),
        }
    }

    #[test]
    fn streaming_kv_control_watch_reports_source_error_while_waiting_second_subframe() {
        let (control_router, mut control_source) = tcp_pair();
        let (mut page_router, mut page_source) = tcp_pair();
        let mut watch =
            RouterControlErrorWatch::start(&control_router, &page_router, 1024).unwrap();
        let payload = b"abcdefghij".to_vec();
        let manifest = manifest_for_payload(&payload, "base");
        let first = subframe(manifest.clone(), 0, 2, 0, &payload[..4]);

        write_pd_stream_frame(&mut page_source, &first, &payload[..4], 1024).unwrap();
        let (frame, subframe_payload) =
            read_pd_stream_frame::<_, SourcePageFrame>(&mut page_router, 1024).unwrap();
        let mut reassembler = reassembler();
        assert!(reassembler.push(frame, subframe_payload).unwrap().is_none());

        write_pd_stream_frame(
            &mut control_source,
            &control_event(
                SourceControlEventKind::Error,
                Some("source"),
                Some("source_stream_error"),
            ),
            &[],
            1024,
        )
        .unwrap();
        let _ = read_pd_stream_frame::<_, SourcePageFrame>(&mut page_router, 1024).unwrap_err();
        let message = watch
            .recv_after_page_error(Duration::from_secs(1))
            .expect("source error should interrupt second subframe wait");

        match message {
            RouterControlWatchMessage::Event(event) => {
                assert_eq!(event.kind, SourceControlEventKind::Error);
                assert_eq!(event.failure_reason.as_deref(), Some("source_stream_error"));
            }
            RouterControlWatchMessage::Error(error) => panic!("unexpected watch error: {error:?}"),
        }
    }

    #[test]
    fn streaming_kv_control_watch_leaves_page_success_path_unaffected() {
        let (control_router, mut control_source) = tcp_pair();
        let (mut page_router, mut page_source) = tcp_pair();
        let mut watch =
            RouterControlErrorWatch::start(&control_router, &page_router, 1024).unwrap();
        let payload = b"abcd".to_vec();
        let manifest = manifest_for_payload(&payload, "base");
        let frame = subframe(manifest, 0, 1, 0, &payload);

        write_pd_stream_frame(
            &mut control_source,
            &control_event(SourceControlEventKind::ChunkDone, None, None),
            &[],
            1024,
        )
        .unwrap();
        write_pd_stream_frame(&mut page_source, &frame, &payload, 1024).unwrap();

        let (received, received_payload) =
            read_pd_stream_frame::<_, SourcePageFrame>(&mut page_router, 1024).unwrap();
        assert_eq!(received.kind, SourcePageFrameKind::PageSubframe);
        assert_eq!(received_payload, payload);

        let event = watch
            .wait_for_terminal_event(Duration::from_secs(1))
            .expect("chunk_done should remain available");
        assert_eq!(event.kind, SourceControlEventKind::ChunkDone);
    }

    #[test]
    fn streaming_kv_control_watch_reports_control_eof_mid_page() {
        let (control_router, control_source) = tcp_pair();
        let (mut page_router, _page_source) = tcp_pair();
        let mut watch =
            RouterControlErrorWatch::start(&control_router, &page_router, 1024).unwrap();

        drop(control_source);
        let _ = read_pd_stream_frame::<_, SourcePageFrame>(&mut page_router, 1024).unwrap_err();
        let message = watch
            .recv_after_page_error(Duration::from_secs(1))
            .expect("control eof should interrupt page wait");

        match message {
            RouterControlWatchMessage::Error(error) => {
                assert_eq!(error.phase, "control");
                assert_eq!(error.reason, "control_read");
            }
            RouterControlWatchMessage::Event(event) => panic!("unexpected event: {event:?}"),
        }
    }

    #[test]
    fn streaming_kv_reassembler_fails_closed_for_missing_subframe() {
        let payload = b"abcdefghij".to_vec();
        let manifest = manifest_for_payload(&payload, "base");
        let mut reassembler = reassembler();

        assert!(reassembler
            .push(
                subframe(manifest, 0, 3, 0, &payload[..4]),
                payload[..4].to_vec()
            )
            .unwrap()
            .is_none());
        let error = reassembler.finish_incomplete().unwrap_err();
        assert_eq!(error.reason, "missing_subframe");
    }

    #[test]
    fn streaming_kv_reassembler_fails_closed_for_duplicate_or_out_of_order_subframe() {
        let payload = b"abcdefghij".to_vec();
        let manifest = manifest_for_payload(&payload, "base");
        let mut duplicate = reassembler();
        duplicate
            .push(
                subframe(manifest.clone(), 0, 3, 0, &payload[..4]),
                payload[..4].to_vec(),
            )
            .unwrap();
        let error = duplicate
            .push(
                subframe(manifest.clone(), 0, 3, 0, &payload[..4]),
                payload[..4].to_vec(),
            )
            .unwrap_err();
        assert_eq!(error.reason, "subframe_order");

        let mut out_of_order = reassembler();
        let error = out_of_order
            .push(
                subframe(manifest, 1, 3, 4, &payload[4..8]),
                payload[4..8].to_vec(),
            )
            .unwrap_err();
        assert_eq!(error.reason, "subframe_order");
    }

    #[test]
    fn streaming_kv_reassembler_fails_closed_for_offset_gap_or_overlap() {
        let payload = b"abcdefghij".to_vec();
        let manifest = manifest_for_payload(&payload, "base");
        let mut gap = reassembler();
        gap.push(
            subframe(manifest.clone(), 0, 3, 0, &payload[..4]),
            payload[..4].to_vec(),
        )
        .unwrap();
        let error = gap
            .push(
                subframe(manifest.clone(), 1, 3, 5, &payload[4..8]),
                payload[4..8].to_vec(),
            )
            .unwrap_err();
        assert_eq!(error.reason, "byte_offset");

        let mut overlap = reassembler();
        overlap
            .push(
                subframe(manifest.clone(), 0, 3, 0, &payload[..4]),
                payload[..4].to_vec(),
            )
            .unwrap();
        let error = overlap
            .push(
                subframe(manifest, 1, 3, 3, &payload[4..8]),
                payload[4..8].to_vec(),
            )
            .unwrap_err();
        assert_eq!(error.reason, "byte_offset");
    }

    #[test]
    fn streaming_kv_reassembler_fails_closed_for_subframe_checksum_mismatch() {
        let payload = b"abcdefghij".to_vec();
        let manifest = manifest_for_payload(&payload, "base");
        let mut frame = subframe(manifest, 0, 3, 0, &payload[..4]);
        frame.subframe_checksum = pd_streaming_kv_payload_checksum(b"xxxx");
        let error = reassembler()
            .push(frame, payload[..4].to_vec())
            .unwrap_err();
        assert_eq!(error.reason, "subframe_checksum");
    }

    #[test]
    fn streaming_kv_reassembler_fails_closed_for_logical_checksum_or_byte_mismatch() {
        let payload = b"abcdefghij".to_vec();
        let mut checksum_manifest = manifest_for_payload(&payload, "base");
        checksum_manifest.checksum =
            "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd".to_string();
        let mut checksum = reassembler();
        checksum
            .push(
                subframe(checksum_manifest.clone(), 0, 2, 0, &payload[..5]),
                payload[..5].to_vec(),
            )
            .unwrap();
        let error = checksum
            .push(
                subframe(checksum_manifest, 1, 2, 5, &payload[5..]),
                payload[5..].to_vec(),
            )
            .unwrap_err();
        assert_eq!(error.reason, "logical_checksum");

        let mut bytes_manifest = manifest_for_payload(&payload, "base");
        bytes_manifest.payload_bytes += 1;
        let mut bytes = reassembler();
        bytes
            .push(
                subframe(bytes_manifest.clone(), 0, 2, 0, &payload[..5]),
                payload[..5].to_vec(),
            )
            .unwrap();
        let error = bytes
            .push(
                subframe(bytes_manifest, 1, 2, 5, &payload[5..]),
                payload[5..].to_vec(),
            )
            .unwrap_err();
        assert_eq!(error.reason, "logical_payload_bytes");
    }

    #[test]
    fn streaming_kv_reassembler_fails_closed_for_identity_or_full_state_frame() {
        let payload = b"abcdefghij".to_vec();
        let mut identity_manifest = manifest_for_payload(&payload, "base");
        identity_manifest.artifact_sha256 =
            s("eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee");
        let error = reassembler()
            .push(
                subframe(identity_manifest, 0, 1, 0, &payload),
                payload.clone(),
            )
            .unwrap_err();
        assert_eq!(error.reason, "segment_identity");

        let mut full_state_manifest = manifest_for_payload(&payload, "base");
        full_state_manifest.frame_kind = PdStreamingKvFrameKind::FullState;
        let error = reassembler()
            .push(subframe(full_state_manifest, 0, 1, 0, &payload), payload)
            .unwrap_err();
        assert_eq!(error.reason, "full_state_frame");
    }

    #[test]
    fn streaming_kv_source_frame_too_large_control_error_is_bounded() {
        assert_eq!(
            sanitize_streaming_label("source_frame_too_large", "source_error"),
            "source_frame_too_large"
        );
    }

    #[test]
    fn streaming_kv_lifecycle_happy_path_reaches_decode_gate() {
        let config = config();
        let prompt_tokens = (0..128).collect::<Vec<_>>();
        let mut backend = MockStreamingBackend::default();
        let mut emitted = Vec::new();

        let result = run_pd_streaming_kv_production_lifecycle(
            &config,
            &prompt_tokens,
            2,
            &mut backend,
            |token| {
                emitted.push(token);
                Ok(PdRouterValidationTokenControl {
                    control: TokenControl::Continue,
                    emitted_content_delta: true,
                })
            },
        )
        .unwrap();

        assert_eq!(result.decode_start_position, 128);
        assert_eq!(result.decoded_tokens, 2);
        assert_eq!(emitted, vec![1000, 1001]);
        assert_eq!(backend.exports.len(), 2);
        assert_eq!(backend.imported.len(), 4);
        assert_eq!(backend.bootstrapped, Some(128));
        assert!(backend.cleanup_called);
    }

    #[test]
    fn streaming_kv_lifecycle_source_error_fails_before_content_and_cleans_up() {
        let config = config();
        let prompt_tokens = (0..128).collect::<Vec<_>>();
        let mut backend = MockStreamingBackend {
            export_error: Some("source_error"),
            ..Default::default()
        };
        let mut emitted = Vec::new();

        let error = run_pd_streaming_kv_production_lifecycle(
            &config,
            &prompt_tokens,
            2,
            &mut backend,
            |token| {
                emitted.push(token);
                Ok(PdRouterValidationTokenControl {
                    control: TokenControl::Continue,
                    emitted_content_delta: true,
                })
            },
        )
        .unwrap_err();

        assert_eq!(error.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert!(error.body().error.message.contains("source_error"));
        assert!(emitted.is_empty());
        assert!(backend.cleanup_called);
    }

    #[test]
    fn streaming_kv_lifecycle_page_stream_timeout_cleans_up_and_next_request_can_run() {
        let config = config();
        let prompt_tokens = (0..128).collect::<Vec<_>>();
        let mut timed_out_backend = MockStreamingBackend {
            export_error_phase: Some("page_stream"),
            export_error: Some("page_read_timeout"),
            ..Default::default()
        };
        let mut emitted = Vec::new();

        let error = run_pd_streaming_kv_production_lifecycle(
            &config,
            &prompt_tokens,
            2,
            &mut timed_out_backend,
            |token| {
                emitted.push(token);
                Ok(PdRouterValidationTokenControl {
                    control: TokenControl::Continue,
                    emitted_content_delta: true,
                })
            },
        )
        .unwrap_err();

        assert_eq!(error.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert!(error.body().error.message.contains("page_read_timeout"));
        assert!(emitted.is_empty());
        assert!(timed_out_backend.cleanup_called);

        let mut next_backend = MockStreamingBackend::default();
        let result = run_pd_streaming_kv_production_lifecycle(
            &config,
            &prompt_tokens,
            1,
            &mut next_backend,
            |_| {
                Ok(PdRouterValidationTokenControl {
                    control: TokenControl::Stop,
                    emitted_content_delta: true,
                })
            },
        )
        .unwrap();

        assert_eq!(result.decode_start_position, 128);
        assert_eq!(result.decoded_tokens, 1);
        assert!(next_backend.cleanup_called);
    }

    #[test]
    fn streaming_kv_lifecycle_source_control_error_releases_next_request() {
        let config = config();
        let prompt_tokens = (0..128).collect::<Vec<_>>();
        let mut source_error_backend = MockStreamingBackend {
            export_error_phase: Some("source"),
            export_error: Some("source_frame_too_large"),
            ..Default::default()
        };

        let error = run_pd_streaming_kv_production_lifecycle(
            &config,
            &prompt_tokens,
            2,
            &mut source_error_backend,
            |_| {
                Ok(PdRouterValidationTokenControl {
                    control: TokenControl::Continue,
                    emitted_content_delta: true,
                })
            },
        )
        .unwrap_err();

        assert_eq!(error.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert!(error
            .body()
            .error
            .message
            .contains("source_frame_too_large"));
        assert!(source_error_backend.cleanup_called);

        let mut next_backend = MockStreamingBackend::default();
        let result = run_pd_streaming_kv_production_lifecycle(
            &config,
            &prompt_tokens,
            1,
            &mut next_backend,
            |_| {
                Ok(PdRouterValidationTokenControl {
                    control: TokenControl::Stop,
                    emitted_content_delta: true,
                })
            },
        )
        .unwrap();

        assert_eq!(result.decode_start_position, 128);
        assert!(next_backend.cleanup_called);
    }

    #[test]
    fn streaming_kv_lifecycle_control_eof_cleans_up_before_content() {
        let config = config();
        let prompt_tokens = (0..128).collect::<Vec<_>>();
        let mut backend = MockStreamingBackend {
            export_error_phase: Some("control"),
            export_error: Some("control_read"),
            ..Default::default()
        };
        let mut emitted = Vec::new();

        let error = run_pd_streaming_kv_production_lifecycle(
            &config,
            &prompt_tokens,
            2,
            &mut backend,
            |token| {
                emitted.push(token);
                Ok(PdRouterValidationTokenControl {
                    control: TokenControl::Continue,
                    emitted_content_delta: true,
                })
            },
        )
        .unwrap_err();

        assert_eq!(error.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert!(error.body().error.message.contains("control_read"));
        assert!(emitted.is_empty());
        assert!(backend.cleanup_called);
    }

    #[test]
    fn streaming_kv_io_error_reason_labels_timeouts() {
        let timeout = anyhow::Error::new(io::Error::new(io::ErrorKind::TimedOut, "timeout"));
        let eof = anyhow::Error::new(io::Error::new(io::ErrorKind::UnexpectedEof, "eof"));

        assert_eq!(
            streaming_io_error_reason(&timeout, "page_read_timeout", "page_read"),
            "page_read_timeout"
        );
        assert_eq!(
            streaming_io_error_reason(&eof, "page_read_timeout", "page_read"),
            "page_read"
        );
    }

    #[test]
    fn streaming_kv_lifecycle_fail_closed_before_decode() {
        let cases = [
            MockStreamingBackend {
                corrupt_checksum: true,
                ..Default::default()
            },
            MockStreamingBackend {
                import_error: Some("import_failed"),
                ..Default::default()
            },
            MockStreamingBackend {
                missing_segment_chunk: Some(1),
                ..Default::default()
            },
            MockStreamingBackend {
                bootstrap_error: Some("bootstrap_failed"),
                ..Default::default()
            },
            MockStreamingBackend {
                full_state_frame: true,
                ..Default::default()
            },
        ];

        for mut backend in cases {
            let mut emitted = Vec::new();
            let error = run_pd_streaming_kv_production_lifecycle(
                &config(),
                &(0..128).collect::<Vec<_>>(),
                2,
                &mut backend,
                |token| {
                    emitted.push(token);
                    Ok(PdRouterValidationTokenControl {
                        control: TokenControl::Continue,
                        emitted_content_delta: true,
                    })
                },
            )
            .unwrap_err();

            assert_eq!(error.status(), StatusCode::SERVICE_UNAVAILABLE);
            assert!(emitted.is_empty(), "failure emitted content");
            assert!(backend.cleanup_called);
        }
    }

    #[test]
    fn streaming_kv_lifecycle_empty_prompt_rejects_without_decode() {
        let mut backend = MockStreamingBackend::default();
        let error =
            run_pd_streaming_kv_production_lifecycle(&config(), &[], 2, &mut backend, |_| {
                Ok(PdRouterValidationTokenControl {
                    control: TokenControl::Continue,
                    emitted_content_delta: true,
                })
            })
            .unwrap_err();

        assert_eq!(error.status(), StatusCode::BAD_REQUEST);
        assert!(backend.cleanup_called);
    }
}
