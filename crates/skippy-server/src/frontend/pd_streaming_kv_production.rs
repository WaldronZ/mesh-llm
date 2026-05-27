use super::*;
use std::{
    collections::BTreeMap,
    net::{SocketAddr, TcpStream, ToSocketAddrs},
    sync::{Arc, Mutex},
};

use serde::Serialize;
use serde_json::Value;
use skippy_runtime::{RuntimeKvPage, RuntimeKvPageDesc};

use crate::{
    binary_transport::pd_streaming_kv_source::{
        encode_token_payload, read_pd_stream_frame, validate_source_page_frame_payload,
        write_pd_stream_frame, SourceControlEvent, SourceControlEventKind, SourceControlKind,
        SourceControlRequest, SourcePageFrame,
    },
    runtime_state::RuntimeState,
};

pub(crate) const PD_STREAMING_KV_PROTOCOL_VERSION: &str = "pd-kv-stream/1";
pub(crate) const PD_STREAMING_KV_CHECKSUM_ALGORITHM: &str = "sha256";
pub(crate) const PD_KV_STREAM_LIFECYCLE_PREFIX: &str = "pd.kv_stream.lifecycle";

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
        let timeout = Duration::from_secs(self.config.startup_timeout_secs.max(1));
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
        control.set_nodelay(true).ok();
        let page = TcpStream::connect_timeout(&page_addr, timeout).map_err(|_| {
            PdStreamingKvLifecycleError {
                phase: "connect",
                reason: "page_connect",
            }
        })?;
        page.set_nodelay(true).ok();
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
                .map_err(|_| PdStreamingKvLifecycleError {
                    phase: "control",
                    reason: "control_read",
                })?;
        if !payload.is_empty()
            || event.protocol_version != PD_STREAMING_KV_PROTOCOL_VERSION
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
        self.chunk_diagnostic("router_control_event_received", chunk)
            .field("control_event", format!("{:?}", event.kind))
            .field("segment_count", event.page_segments)
            .field("page_bytes", event.page_bytes)
            .emit();
        Ok(event)
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
            |_| PdStreamingKvLifecycleError {
                phase: "control",
                reason: "control_write",
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
        let mut segments = Vec::with_capacity(exported.page_segments);
        for segment_index in 0..exported.page_segments {
            self.chunk_diagnostic("router_page_frame_receive_start", chunk)
                .field("segment_index", segment_index)
                .field("segment_count", exported.page_segments)
                .emit();
            let page_stream = self
                .page_stream
                .as_mut()
                .ok_or(PdStreamingKvLifecycleError {
                    phase: "page_stream",
                    reason: "not_connected",
                })?;
            let (frame, payload) =
                read_pd_stream_frame::<_, SourcePageFrame>(page_stream, max_frame_bytes).map_err(
                    |_| PdStreamingKvLifecycleError {
                        phase: "page_stream",
                        reason: "page_read",
                    },
                )?;
            if frame.request_id != self.request_id
                || frame.session_id != self.session_id
                || frame.chunk_index != chunk.chunk_index
                || frame.segment_index != segment_index
                || frame.segment_count != exported.page_segments
            {
                return Err(PdStreamingKvLifecycleError {
                    phase: "page_stream",
                    reason: "page_frame_metadata",
                });
            }
            validate_source_page_frame_payload(&frame, &identity, &payload).map_err(|_| {
                PdStreamingKvLifecycleError {
                    phase: "manifest",
                    reason: "manifest_validation",
                }
            })?;
            self.chunk_diagnostic("router_page_frame_received", chunk)
                .field("segment_index", segment_index)
                .field("segment_count", exported.page_segments)
                .field("cache_kind", frame.manifest.cache_kind.as_str())
                .field("segment_kind", frame.manifest.segment_kind.as_str())
                .field("payload_bytes", payload.len())
                .field("checksum_present", true)
                .field("checksum_valid", true)
                .field("identity_valid", true)
                .emit();
            segments.push(PdStreamingKvReceivedSegment {
                manifest: frame.manifest,
                payload,
            });
        }
        self.read_control_event(chunk, SourceControlEventKind::ChunkDone)?;
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
        }
        self.page_stream.take();
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
            .map_err(PdStreamingKvLifecycleError::openai_error)?;
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
                .map_err(PdStreamingKvLifecycleError::openai_error)?;
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
        .map_err(PdStreamingKvLifecycleError::openai_error)?;
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
        "checksum" => "checksum",
        "manifest_validation" => "manifest_validation",
        "page_write" => "page_write",
        "source_error" => "source_error",
        _ => fallback,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
                    phase: "export",
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
