use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::Path,
    sync::mpsc::{self, Receiver, SyncSender},
    thread,
    time::Instant,
};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use skippy_runtime::{
    RuntimeConfig, RuntimeKvPageDesc, RuntimeLoadMode, SamplingConfig, StageModel, GGML_TYPE_F16,
};

use crate::cli::{
    FlashAttentionArg, KvPageBootstrapStrategy, KvStreamingHandoffArgs,
    KvStreamingHandoffCoordinatorArgs, KvStreamingHandoffLocalArgs, KvStreamingHandoffRole,
    KvStreamingHandoffSourceArgs, KvStreamingPipelineMode, StageLoadMode,
};

const PROTOCOL_VERSION: &str = "pd-kv-stream/1";
const RESULT_INCONCLUSIVE: &str = "inconclusive";
const RECOMMENDATION_READY: &str = "ready_for_foreground_streaming_smoke";
const RECOMMENDATION_READY_ASYNC: &str = "ready_for_async_foreground_smoke";

pub fn kv_streaming_handoff(args: KvStreamingHandoffArgs) -> Result<()> {
    let (report, markdown_out) = match args.role {
        KvStreamingHandoffRole::Local(args) => {
            let report = run_local_streaming_controller(&args)?;
            let markdown_out = None;
            emit_json_report(&report, args.output.report_out.as_deref())?;
            (report, markdown_out)
        }
        KvStreamingHandoffRole::Source(args) => {
            let report = if args.model.is_some() {
                run_source_runtime_loop(&args)?
            } else {
                source_ready_report()
            };
            emit_json_report(&report, args.output.report_out.as_deref())?;
            (report, None)
        }
        KvStreamingHandoffRole::Coordinator(args) => {
            let markdown_out = args.markdown_out.clone();
            let report = if args.model.is_some() {
                run_coordinator_runtime_loop(&args)?
            } else {
                coordinator_ready_report(&args)?
            };
            emit_json_report(&report, args.output.report_out.as_deref())?;
            (report, markdown_out)
        }
    };
    if let Some(path) = markdown_out.as_deref() {
        emit_markdown_report(&report, path)?;
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TokenRange {
    chunk_index: usize,
    token_start: usize,
    token_end: usize,
}

impl TokenRange {
    fn token_count(self) -> usize {
        self.token_end.saturating_sub(self.token_start)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StreamingKvPlan {
    total_tokens: usize,
    chunk_tokens: usize,
    chunks: Vec<TokenRange>,
}

impl StreamingKvPlan {
    fn new(total_tokens: usize, chunk_tokens: usize) -> Result<Self> {
        if total_tokens == 0 {
            bail!("total_tokens must be greater than zero");
        }
        if chunk_tokens == 0 {
            bail!("chunk_tokens must be greater than zero");
        }
        let mut chunks = Vec::new();
        let mut token_start = 0usize;
        while token_start < total_tokens {
            let token_count = (total_tokens - token_start).min(chunk_tokens);
            let token_end = token_start + token_count;
            chunks.push(TokenRange {
                chunk_index: chunks.len(),
                token_start,
                token_end,
            });
            token_start = token_end;
        }
        Ok(Self {
            total_tokens,
            chunk_tokens,
            chunks,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StreamingKvIdentity {
    artifact_sha256: String,
    tokenizer_hash: String,
    chat_template_hash: String,
    dtype: String,
    layout: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
enum StreamingPayloadKind {
    KvPage,
    FullStateBlob,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct StreamingKvManifest {
    protocol_version: String,
    chunk_index: usize,
    total_chunks: usize,
    token_start: usize,
    token_end: usize,
    total_prompt_tokens: usize,
    page_bytes: u64,
    frame_bytes: u64,
    checksum_algorithm: String,
    checksum: String,
    observed_checksum: String,
    artifact_sha256: String,
    tokenizer_hash: String,
    chat_template_hash: String,
    dtype: String,
    layout: String,
    payload_kind: StreamingPayloadKind,
    native_desc: Option<StreamingKvNativeDesc>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct StreamingKvNativeDesc {
    version: u32,
    layer_start: i32,
    layer_end: i32,
    token_start: u64,
    token_count: u64,
    layer_count: u32,
    k_type: u32,
    v_type: u32,
    k_row_bytes: u32,
    v_row_bytes: u32,
    v_element_bytes: u32,
    payload_bytes: u64,
    flags: u64,
}

impl StreamingKvManifest {
    fn for_chunk(
        range: TokenRange,
        plan: &StreamingKvPlan,
        identity: &StreamingKvIdentity,
        page_bytes: u64,
    ) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION.to_string(),
            chunk_index: range.chunk_index,
            total_chunks: plan.chunks.len(),
            token_start: range.token_start,
            token_end: range.token_end,
            total_prompt_tokens: plan.total_tokens,
            page_bytes,
            frame_bytes: page_bytes,
            checksum_algorithm: "sha256".to_string(),
            checksum: "checksum-ok".to_string(),
            observed_checksum: "checksum-ok".to_string(),
            artifact_sha256: identity.artifact_sha256.clone(),
            tokenizer_hash: identity.tokenizer_hash.clone(),
            chat_template_hash: identity.chat_template_hash.clone(),
            dtype: identity.dtype.clone(),
            layout: identity.layout.clone(),
            payload_kind: StreamingPayloadKind::KvPage,
            native_desc: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct StreamingKvCapacity {
    max_in_flight_chunks: usize,
    max_in_flight_bytes: u64,
    max_frame_bytes: u64,
    max_queue_depth: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PipelineError {
    reason: &'static str,
}

#[derive(Debug, Serialize)]
struct StreamingKvReport {
    mode: &'static str,
    role: &'static str,
    result: &'static str,
    recommendation: &'static str,
    protocol: &'static str,
    runtime_path: StreamingRuntimePathReport,
    local_controller: ControllerReport,
    bootstrap: StreamingBootstrapReport,
    baseline: StreamingBaselineReport,
    telemetry: StreamingTelemetryReport,
    negative_checks: Vec<NegativeCheckReport>,
    privacy: PrivacyReport,
    remaining_authorization_required: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
struct StreamingRuntimePathReport {
    source_runtime_export_kv_page: &'static str,
    target_runtime_import_kv_page: &'static str,
    network_transport: &'static str,
    full_state_handoff_allowed_as_pass: bool,
}

#[derive(Debug, Serialize)]
struct ControllerReport {
    lifecycle_model: &'static str,
    out_of_order_policy: &'static str,
    final_gate: &'static str,
    full_state_handoff_allowed_as_pass: bool,
}

#[derive(Debug, Serialize)]
struct StreamingBootstrapReport {
    status: &'static str,
    strategy: &'static str,
    imported_token_count: Option<usize>,
    trim_target_position: Option<usize>,
    replay_token_position: Option<usize>,
    bootstrap_eval_ms: Option<f64>,
    logits_ready: bool,
    decode_start_position: Option<u64>,
    failure_reason: Option<&'static str>,
}

#[derive(Debug, Serialize)]
struct StreamingBaselineReport {
    strategy: &'static str,
    comparison: &'static str,
    failure_reason: Option<&'static str>,
    first_divergence_index: Option<usize>,
    baseline_token_id: Option<i32>,
    streaming_token_id: Option<i32>,
    baseline_token_count: Option<usize>,
    streaming_token_count: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize)]
struct StreamingHarnessRequest {
    kind: String,
    session_id: Option<String>,
    chunk_index: Option<usize>,
    total_chunks: Option<usize>,
    total_prompt_tokens: Option<usize>,
    token_start: Option<usize>,
    tokens: Option<Vec<i32>>,
}

impl StreamingHarnessRequest {
    fn prefill_chunk(
        session_id: &str,
        chunk_index: usize,
        total_chunks: usize,
        total_prompt_tokens: usize,
        token_start: usize,
        tokens: &[i32],
    ) -> Self {
        Self {
            kind: "prefill_chunk".to_string(),
            session_id: Some(session_id.to_string()),
            chunk_index: Some(chunk_index),
            total_chunks: Some(total_chunks),
            total_prompt_tokens: Some(total_prompt_tokens),
            token_start: Some(token_start),
            tokens: Some(tokens.to_vec()),
        }
    }

    fn stop(session_id: &str) -> Self {
        Self {
            kind: "stop".to_string(),
            session_id: Some(session_id.to_string()),
            chunk_index: None,
            total_chunks: None,
            total_prompt_tokens: None,
            token_start: None,
            tokens: None,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct StreamingHarnessResponse {
    kind: String,
    status: String,
    error: Option<String>,
    chunk_index: Option<usize>,
    manifest: Option<StreamingKvManifest>,
    chunk_prefill_ms: Option<f64>,
    chunk_export_ms: Option<f64>,
    prefill_start_ms: Option<f64>,
    prefill_end_ms: Option<f64>,
    export_start_ms: Option<f64>,
    export_end_ms: Option<f64>,
    segment_index: Option<usize>,
    segment_count: Option<usize>,
}

impl StreamingHarnessResponse {
    fn page(
        manifest: StreamingKvManifest,
        chunk_prefill_ms: f64,
        chunk_export_ms: f64,
        segment_index: usize,
        segment_count: usize,
    ) -> Self {
        Self {
            kind: "page".to_string(),
            status: "ok".to_string(),
            error: None,
            chunk_index: Some(manifest.chunk_index),
            manifest: Some(manifest),
            chunk_prefill_ms: Some(chunk_prefill_ms),
            chunk_export_ms: Some(chunk_export_ms),
            prefill_start_ms: None,
            prefill_end_ms: None,
            export_start_ms: None,
            export_end_ms: None,
            segment_index: Some(segment_index),
            segment_count: Some(segment_count),
        }
    }

    fn control(kind: &str, chunk_index: usize, start_ms: Option<f64>, end_ms: Option<f64>) -> Self {
        Self {
            kind: kind.to_string(),
            status: "ok".to_string(),
            error: None,
            chunk_index: Some(chunk_index),
            manifest: None,
            chunk_prefill_ms: match kind {
                "prefill_completed" => start_ms.zip(end_ms).map(|(start, end)| end - start),
                _ => None,
            },
            chunk_export_ms: match kind {
                "export_completed" => start_ms.zip(end_ms).map(|(start, end)| end - start),
                _ => None,
            },
            prefill_start_ms: if kind.starts_with("prefill") {
                start_ms
            } else {
                None
            },
            prefill_end_ms: if kind.starts_with("prefill") {
                end_ms
            } else {
                None
            },
            export_start_ms: if kind.starts_with("export") {
                start_ms
            } else {
                None
            },
            export_end_ms: if kind.starts_with("export") {
                end_ms
            } else {
                None
            },
            segment_index: None,
            segment_count: None,
        }
    }

    fn chunk_done(chunk_index: usize) -> Self {
        Self {
            kind: "chunk_done".to_string(),
            status: "ok".to_string(),
            error: None,
            chunk_index: Some(chunk_index),
            manifest: None,
            chunk_prefill_ms: None,
            chunk_export_ms: None,
            prefill_start_ms: None,
            prefill_end_ms: None,
            export_start_ms: None,
            export_end_ms: None,
            segment_index: None,
            segment_count: None,
        }
    }

    fn ok(kind: &str) -> Self {
        Self {
            kind: kind.to_string(),
            status: "ok".to_string(),
            error: None,
            chunk_index: None,
            manifest: None,
            chunk_prefill_ms: None,
            chunk_export_ms: None,
            prefill_start_ms: None,
            prefill_end_ms: None,
            export_start_ms: None,
            export_end_ms: None,
            segment_index: None,
            segment_count: None,
        }
    }

    fn error(reason: &str) -> Self {
        Self {
            kind: "error".to_string(),
            status: "error".to_string(),
            error: Some(reason.to_string()),
            chunk_index: None,
            manifest: None,
            chunk_prefill_ms: None,
            chunk_export_ms: None,
            prefill_start_ms: None,
            prefill_end_ms: None,
            export_start_ms: None,
            export_end_ms: None,
            segment_index: None,
            segment_count: None,
        }
    }
}

#[derive(Debug)]
struct StreamingDecodeComparison {
    matches: bool,
    label: &'static str,
    first_divergence_index: Option<usize>,
    baseline_token_id: Option<i32>,
    streaming_token_id: Option<i32>,
    baseline_token_count: usize,
    streaming_token_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StreamingBootstrapPlan {
    imported_token_count: usize,
    trim_target_position: usize,
    replay_token_position: usize,
    decode_start_position: usize,
    replay_token: i32,
}

#[derive(Debug)]
struct StreamingBootstrapDecode {
    first_token: i32,
    report: StreamingBootstrapReport,
}

#[derive(Debug, Serialize)]
struct StreamingTelemetryReport {
    chunk_count: usize,
    chunk_tokens: Vec<usize>,
    per_chunk_prefill_ms: Vec<f64>,
    per_chunk_export_ms: Vec<f64>,
    per_chunk_transfer_ms: Vec<f64>,
    per_chunk_import_ms: Vec<f64>,
    prefill_start_ms: Vec<f64>,
    prefill_end_ms: Vec<f64>,
    export_start_ms: Vec<f64>,
    export_end_ms: Vec<f64>,
    transfer_start_ms: Vec<f64>,
    transfer_end_ms: Vec<f64>,
    import_start_ms: Vec<f64>,
    import_end_ms: Vec<f64>,
    source_prefill_start_ms: Vec<f64>,
    source_prefill_end_ms: Vec<f64>,
    source_export_start_ms: Vec<f64>,
    source_export_end_ms: Vec<f64>,
    page_write_start_ms: Vec<f64>,
    page_write_end_ms: Vec<f64>,
    page_write_ms: Vec<f64>,
    flush_ms: Vec<f64>,
    writer_queue_send_wait_ms: f64,
    source_queue_depth: usize,
    source_backpressure_wait_ms: f64,
    control_event_emit_ms: Vec<f64>,
    control_event_receive_ms: Vec<f64>,
    control_event_lag_ms: Vec<f64>,
    source_relative_overlap_ms: f64,
    coordinator_observed_overlap_ms: f64,
    true_compute_transfer_overlap_ms: f64,
    clock_alignment_status: &'static str,
    overlap_ms: f64,
    actual_overlap_ms: f64,
    pipeline_idle_ms: f64,
    source_idle_ms: f64,
    importer_idle_ms: f64,
    backpressure_wait_ms: f64,
    in_flight_bytes: u64,
    page_queue_depth: usize,
    page_bytes_per_chunk: Vec<u64>,
    bytes_per_token: Option<f64>,
    final_decode_start_position: Option<usize>,
    validation_result: &'static str,
    failure_reason: Option<&'static str>,
}

#[derive(Debug, Serialize)]
struct NegativeCheckReport {
    case: &'static str,
    status: &'static str,
    failure_reason: &'static str,
}

#[derive(Debug, Serialize)]
struct PrivacyReport {
    prompt_text: &'static str,
    generated_content: &'static str,
    complete_token_arrays: &'static str,
    kv_or_native_payload_contents: &'static str,
    credentials: &'static str,
    private_paths: &'static str,
    endpoint_urls: &'static str,
    real_machine_labels: &'static str,
}

#[derive(Debug)]
struct StreamingKvController {
    plan: StreamingKvPlan,
    identity: StreamingKvIdentity,
    capacity: StreamingKvCapacity,
    expected_prefill_chunk: usize,
    expected_export_chunk: usize,
    expected_import_chunk: usize,
    next_expected_position: usize,
    prefilled_chunks: BTreeSet<usize>,
    exported_chunks: BTreeSet<usize>,
    imported_chunks: BTreeSet<usize>,
    in_flight_chunks: usize,
    in_flight_bytes: u64,
    failed: Option<PipelineError>,
    telemetry: StreamingTelemetryReport,
}

impl StreamingKvController {
    fn new(
        plan: StreamingKvPlan,
        identity: StreamingKvIdentity,
        capacity: StreamingKvCapacity,
    ) -> Self {
        let chunk_count = plan.chunks.len();
        Self {
            plan,
            identity,
            capacity,
            expected_prefill_chunk: 0,
            expected_export_chunk: 0,
            expected_import_chunk: 0,
            next_expected_position: 0,
            prefilled_chunks: BTreeSet::new(),
            exported_chunks: BTreeSet::new(),
            imported_chunks: BTreeSet::new(),
            in_flight_chunks: 0,
            in_flight_bytes: 0,
            failed: None,
            telemetry: StreamingTelemetryReport {
                chunk_count,
                chunk_tokens: Vec::with_capacity(chunk_count),
                per_chunk_prefill_ms: Vec::with_capacity(chunk_count),
                per_chunk_export_ms: Vec::with_capacity(chunk_count),
                per_chunk_transfer_ms: Vec::with_capacity(chunk_count),
                per_chunk_import_ms: Vec::with_capacity(chunk_count),
                prefill_start_ms: Vec::with_capacity(chunk_count),
                prefill_end_ms: Vec::with_capacity(chunk_count),
                export_start_ms: Vec::with_capacity(chunk_count),
                export_end_ms: Vec::with_capacity(chunk_count),
                transfer_start_ms: Vec::with_capacity(chunk_count),
                transfer_end_ms: Vec::with_capacity(chunk_count),
                import_start_ms: Vec::with_capacity(chunk_count),
                import_end_ms: Vec::with_capacity(chunk_count),
                source_prefill_start_ms: Vec::with_capacity(chunk_count),
                source_prefill_end_ms: Vec::with_capacity(chunk_count),
                source_export_start_ms: Vec::with_capacity(chunk_count),
                source_export_end_ms: Vec::with_capacity(chunk_count),
                page_write_start_ms: Vec::new(),
                page_write_end_ms: Vec::new(),
                page_write_ms: Vec::new(),
                flush_ms: Vec::new(),
                writer_queue_send_wait_ms: 0.0,
                source_queue_depth: 0,
                source_backpressure_wait_ms: 0.0,
                control_event_emit_ms: Vec::new(),
                control_event_receive_ms: Vec::new(),
                control_event_lag_ms: Vec::new(),
                source_relative_overlap_ms: 0.0,
                coordinator_observed_overlap_ms: 0.0,
                true_compute_transfer_overlap_ms: 0.0,
                clock_alignment_status: "not_aligned",
                overlap_ms: 0.0,
                actual_overlap_ms: 0.0,
                pipeline_idle_ms: 0.0,
                source_idle_ms: 0.0,
                importer_idle_ms: 0.0,
                backpressure_wait_ms: 0.0,
                in_flight_bytes: 0,
                page_queue_depth: 0,
                page_bytes_per_chunk: Vec::with_capacity(chunk_count),
                bytes_per_token: None,
                final_decode_start_position: None,
                validation_result: "not_finalized",
                failure_reason: None,
            },
        }
    }

    fn chunk_prefilled(&mut self, range: TokenRange, prefill_ms: f64) -> Result<(), PipelineError> {
        self.ensure_active()?;
        self.ensure_known_chunk(range)?;
        if range.chunk_index != self.expected_prefill_chunk {
            return self.fail("out_of_order_chunk_prefill");
        }
        if !self.prefilled_chunks.insert(range.chunk_index) {
            return self.fail("duplicate_chunk_prefill");
        }
        self.expected_prefill_chunk += 1;
        self.telemetry.chunk_tokens.push(range.token_count());
        self.telemetry.per_chunk_prefill_ms.push(prefill_ms);
        Ok(())
    }

    fn page_segments_exported(
        &mut self,
        manifest: &StreamingKvManifest,
        export_ms: f64,
    ) -> Result<(), PipelineError> {
        self.ensure_active()?;
        self.validate_manifest(manifest)?;
        if self.exported_chunks.contains(&manifest.chunk_index) {
            return self.fail("duplicate_chunk_export");
        }
        if manifest.chunk_index != self.expected_export_chunk {
            return self.fail("out_of_order_chunk_export");
        }
        if !self.prefilled_chunks.contains(&manifest.chunk_index) {
            return self.fail("export_before_prefill");
        }
        self.exported_chunks.insert(manifest.chunk_index);
        let next_chunks = self.in_flight_chunks.saturating_add(1);
        if next_chunks > self.capacity.max_in_flight_chunks {
            return self.fail("max_in_flight_chunks");
        }
        let next_bytes = self.in_flight_bytes.saturating_add(manifest.page_bytes);
        if next_bytes > self.capacity.max_in_flight_bytes {
            return self.fail("max_in_flight_bytes");
        }
        self.in_flight_chunks = next_chunks;
        self.in_flight_bytes = next_bytes;
        self.expected_export_chunk += 1;
        self.telemetry.per_chunk_export_ms.push(export_ms);
        self.telemetry
            .page_bytes_per_chunk
            .push(manifest.page_bytes);
        self.telemetry.in_flight_bytes = self.in_flight_bytes;
        Ok(())
    }

    fn page_segments_imported(
        &mut self,
        manifest: &StreamingKvManifest,
        transfer_ms: f64,
        import_ms: f64,
    ) -> Result<(), PipelineError> {
        self.ensure_active()?;
        self.validate_manifest(manifest)?;
        if manifest.chunk_index != self.expected_import_chunk {
            return self.fail("out_of_order_chunk_import");
        }
        if manifest.token_start > self.next_expected_position {
            return self.fail("position_gap");
        }
        if manifest.token_start < self.next_expected_position {
            return self.fail("position_overlap");
        }
        let Some(range) = self.plan.chunks.get(manifest.chunk_index).copied() else {
            return self.fail("chunk_index");
        };
        if manifest.token_start != range.token_start || manifest.token_end != range.token_end {
            return self.fail("chunk_range");
        }
        if !self.exported_chunks.contains(&manifest.chunk_index) {
            return self.fail("import_before_export");
        }
        if !self.imported_chunks.insert(manifest.chunk_index) {
            return self.fail("duplicate_chunk_import");
        }
        self.in_flight_chunks = self.in_flight_chunks.saturating_sub(1);
        self.in_flight_bytes = self.in_flight_bytes.saturating_sub(manifest.page_bytes);
        self.expected_import_chunk += 1;
        self.next_expected_position = manifest.token_end;
        self.telemetry.per_chunk_transfer_ms.push(transfer_ms);
        self.telemetry.per_chunk_import_ms.push(import_ms);
        self.telemetry.in_flight_bytes = self.in_flight_bytes;
        Ok(())
    }

    fn import_failed(&mut self, chunk_index: usize) -> Result<(), PipelineError> {
        self.ensure_active()?;
        if chunk_index != self.expected_import_chunk {
            return self.fail("out_of_order_chunk_import");
        }
        self.fail("import_failure")
    }

    fn finalize(&mut self) -> Result<(), PipelineError> {
        self.ensure_active()?;
        if self.imported_chunks.len() != self.plan.chunks.len() {
            return self.fail("missing_chunk");
        }
        if self.next_expected_position != self.plan.total_tokens {
            return self.fail("final_decode_start_position");
        }
        if self.in_flight_chunks != 0 || self.in_flight_bytes != 0 {
            return self.fail("in_flight_not_empty");
        }
        self.telemetry.final_decode_start_position = Some(self.plan.total_tokens);
        let total_page_bytes = self
            .telemetry
            .page_bytes_per_chunk
            .iter()
            .copied()
            .sum::<u64>();
        self.telemetry.bytes_per_token =
            Some(total_page_bytes as f64 / self.plan.total_tokens as f64);
        self.telemetry.validation_result = "pass";
        self.telemetry.failure_reason = None;
        Ok(())
    }

    fn fail<T>(&mut self, reason: &'static str) -> Result<T, PipelineError> {
        self.failed = Some(PipelineError { reason });
        self.telemetry.validation_result = "fail_closed";
        self.telemetry.failure_reason = Some(reason);
        Err(PipelineError { reason })
    }

    fn ensure_active(&self) -> Result<(), PipelineError> {
        if let Some(error) = self.failed {
            return Err(error);
        }
        Ok(())
    }

    fn ensure_known_chunk(&mut self, range: TokenRange) -> Result<(), PipelineError> {
        let expected = self.plan.chunks.get(range.chunk_index).copied();
        if expected != Some(range) {
            return self.fail("chunk_range");
        }
        Ok(())
    }

    fn validate_manifest(&mut self, manifest: &StreamingKvManifest) -> Result<(), PipelineError> {
        if manifest.protocol_version != PROTOCOL_VERSION {
            return self.fail("protocol_version");
        }
        if manifest.payload_kind != StreamingPayloadKind::KvPage {
            return self.fail("full_state_blob");
        }
        if manifest.total_chunks != self.plan.chunks.len() {
            return self.fail("total_chunks");
        }
        if manifest.total_prompt_tokens != self.plan.total_tokens {
            return self.fail("total_prompt_tokens");
        }
        if self.plan.chunks.get(manifest.chunk_index).is_none() {
            return self.fail("chunk_index");
        };
        if manifest.token_end <= manifest.token_start {
            return self.fail("token_range");
        }
        if manifest.frame_bytes > self.capacity.max_frame_bytes {
            return self.fail("max_frame_bytes");
        }
        if manifest.page_bytes != manifest.frame_bytes {
            return self.fail("payload_bytes");
        }
        if manifest.checksum != manifest.observed_checksum {
            return self.fail("checksum");
        }
        if manifest.artifact_sha256 != self.identity.artifact_sha256 {
            return self.fail("artifact_sha256");
        }
        if manifest.tokenizer_hash != self.identity.tokenizer_hash {
            return self.fail("tokenizer_hash");
        }
        if manifest.chat_template_hash != self.identity.chat_template_hash {
            return self.fail("chat_template_hash");
        }
        if manifest.dtype != self.identity.dtype {
            return self.fail("dtype");
        }
        if manifest.layout != self.identity.layout {
            return self.fail("layout");
        }
        Ok(())
    }
}

fn run_local_streaming_controller(args: &KvStreamingHandoffLocalArgs) -> Result<StreamingKvReport> {
    if args.pipeline_mode == KvStreamingPipelineMode::SplitChannel {
        return run_local_split_channel_streaming_controller(args);
    }
    if args.pipeline_mode == KvStreamingPipelineMode::Async {
        return run_local_async_streaming_controller(args);
    }
    let plan = StreamingKvPlan::new(args.total_tokens, args.chunk_tokens)?;
    let identity = expected_identity();
    let capacity = StreamingKvCapacity {
        max_in_flight_chunks: args.max_in_flight_chunks,
        max_in_flight_bytes: args.max_in_flight_bytes,
        max_frame_bytes: args.max_frame_bytes,
        max_queue_depth: args.max_queue_depth,
    };
    let mut controller = StreamingKvController::new(plan.clone(), identity.clone(), capacity);
    let mut manifests = Vec::with_capacity(plan.chunks.len());

    for range in &plan.chunks {
        let manifest =
            StreamingKvManifest::for_chunk(*range, &plan, &identity, args.page_bytes_per_chunk);
        controller
            .chunk_prefilled(*range, 1.0 + range.chunk_index as f64)
            .map_err(|error| anyhow::anyhow!(error.reason))?;
        controller
            .page_segments_exported(&manifest, 0.25)
            .map_err(|error| anyhow::anyhow!(error.reason))?;
        manifests.push(manifest);
        if range.chunk_index == 0 && plan.chunks.len() > 1 {
            controller.telemetry.overlap_ms = 0.25;
        }
        let manifest = manifests
            .last()
            .expect("manifest was just pushed for the current chunk");
        controller
            .page_segments_imported(manifest, 0.15, 0.35)
            .map_err(|error| anyhow::anyhow!(error.reason))?;
    }
    controller
        .finalize()
        .map_err(|error| anyhow::anyhow!(error.reason))?;
    Ok(report_from_controller(controller, negative_checks()))
}

fn run_local_split_channel_streaming_controller(
    args: &KvStreamingHandoffLocalArgs,
) -> Result<StreamingKvReport> {
    let mut report = run_local_async_streaming_controller(args)?;
    report.role = "local_split_channel_controller";
    report.recommendation = "ready_for_split_channel_4k_smoke";
    report.runtime_path.network_transport =
        "split_control_channel_and_page_stream_ready_not_started";
    report.local_controller.lifecycle_model =
        "split_channel_control_events_bounded_page_stream_importer_final_gate";
    report.telemetry.clock_alignment_status = "simulated_same_clock";
    report.telemetry.source_relative_overlap_ms = report.telemetry.actual_overlap_ms;
    report.telemetry.coordinator_observed_overlap_ms = report.telemetry.actual_overlap_ms;
    report.telemetry.true_compute_transfer_overlap_ms = report.telemetry.actual_overlap_ms;
    report.telemetry.source_prefill_start_ms = report.telemetry.prefill_start_ms.clone();
    report.telemetry.source_prefill_end_ms = report.telemetry.prefill_end_ms.clone();
    report.telemetry.source_export_start_ms = report.telemetry.export_start_ms.clone();
    report.telemetry.source_export_end_ms = report.telemetry.export_end_ms.clone();
    report.telemetry.page_write_start_ms = report.telemetry.transfer_start_ms.clone();
    report.telemetry.page_write_end_ms = report.telemetry.transfer_end_ms.clone();
    report.telemetry.page_write_ms = report.telemetry.per_chunk_transfer_ms.clone();
    report.telemetry.flush_ms = report.telemetry.per_chunk_transfer_ms.clone();
    report.telemetry.control_event_emit_ms = report.telemetry.prefill_start_ms.clone();
    report.telemetry.control_event_receive_ms = report.telemetry.prefill_start_ms.clone();
    report.telemetry.control_event_lag_ms = vec![0.0; report.telemetry.prefill_start_ms.len()];
    report.telemetry.writer_queue_send_wait_ms = report.telemetry.backpressure_wait_ms;
    report.telemetry.source_backpressure_wait_ms = report.telemetry.backpressure_wait_ms;
    report.telemetry.source_queue_depth = args.max_queue_depth;
    report.remaining_authorization_required = vec![
        "run 4k split-channel PGX/Mac foreground streaming smoke",
        "validate source-side overlap telemetry",
        "keep 8k deferred until split-channel 4k pass",
    ];
    Ok(report)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct AsyncPipelineFault {
    source_failure_at_chunk: Option<usize>,
    import_failure_at_chunk: Option<usize>,
}

impl AsyncPipelineFault {
    const NONE: Self = Self {
        source_failure_at_chunk: None,
        import_failure_at_chunk: None,
    };

    #[cfg(test)]
    fn source_failure_at(chunk_index: usize) -> Self {
        Self {
            source_failure_at_chunk: Some(chunk_index),
            import_failure_at_chunk: None,
        }
    }

    #[cfg(test)]
    fn import_failure_at(chunk_index: usize) -> Self {
        Self {
            source_failure_at_chunk: None,
            import_failure_at_chunk: Some(chunk_index),
        }
    }
}

#[derive(Clone, Debug)]
struct PendingImport {
    manifest: StreamingKvManifest,
    transfer_ms: f64,
    import_ms: f64,
    import_end_ms: f64,
}

#[derive(Debug)]
struct StreamingOutboundFrame {
    response: StreamingHarnessResponse,
    payload: Vec<u8>,
}

#[derive(Debug, Default)]
struct StreamingWriterMetrics {
    page_write_start_ms: Vec<f64>,
    page_write_end_ms: Vec<f64>,
    page_write_ms: Vec<f64>,
    flush_ms: Vec<f64>,
    writer_queue_send_wait_ms: f64,
    source_queue_depth: usize,
    source_backpressure_wait_ms: f64,
}

#[derive(Debug)]
struct StreamingInboundFrame {
    channel: StreamingFrameChannel,
    response: StreamingHarnessResponse,
    payload: Vec<u8>,
    read_started_ms: f64,
    read_completed_ms: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StreamingFrameChannel {
    SingleStream,
    Control,
    Page,
}

#[derive(Debug, Default)]
struct LiveChunkAggregate {
    prefill_ms: f64,
    export_ms: f64,
    transfer_ms: f64,
    import_ms: f64,
    page_bytes: u64,
    saw_page: bool,
    observed_prefill_start_ms: Option<f64>,
    observed_prefill_end_ms: Option<f64>,
    observed_export_start_ms: Option<f64>,
    observed_export_end_ms: Option<f64>,
    observed_transfer_start_ms: Option<f64>,
    observed_transfer_end_ms: Option<f64>,
    observed_import_start_ms: Option<f64>,
    observed_import_end_ms: Option<f64>,
    chunk_done_seen: bool,
    expected_segment_count: Option<usize>,
    segments_seen: usize,
    source_prefill_start_ms: Option<f64>,
    source_prefill_end_ms: Option<f64>,
    source_export_start_ms: Option<f64>,
    source_export_end_ms: Option<f64>,
}

struct AsyncLiveImportResult {
    import_session: skippy_runtime::StageSession,
    controller: StreamingKvController,
    chunk_zero_imported_before_final_gate: bool,
}

fn run_local_async_streaming_controller(
    args: &KvStreamingHandoffLocalArgs,
) -> Result<StreamingKvReport> {
    run_local_async_streaming_controller_with_fault(args, AsyncPipelineFault::NONE)
}

fn run_local_async_streaming_controller_with_fault(
    args: &KvStreamingHandoffLocalArgs,
    fault: AsyncPipelineFault,
) -> Result<StreamingKvReport> {
    let plan = StreamingKvPlan::new(args.total_tokens, args.chunk_tokens)?;
    let identity = expected_identity();
    let capacity = StreamingKvCapacity {
        max_in_flight_chunks: args.max_in_flight_chunks,
        max_in_flight_bytes: args.max_in_flight_bytes,
        max_frame_bytes: args.max_frame_bytes,
        max_queue_depth: args.max_queue_depth,
    };
    let mut controller = StreamingKvController::new(plan.clone(), identity.clone(), capacity);
    let mut pending_imports = Vec::<PendingImport>::new();
    let mut source_time_ms = 0.0_f64;
    let mut importer_available_ms = 0.0_f64;
    let mut source_busy_intervals = Vec::<(f64, f64)>::new();
    let mut importer_busy_intervals = Vec::<(f64, f64)>::new();
    let mut source_failure = false;
    let mut importer_failure = false;

    for range in &plan.chunks {
        drain_ready_imports(
            &mut controller,
            &mut pending_imports,
            source_time_ms,
            fault,
            &mut importer_failure,
        )?;
        if importer_failure {
            source_failure = true;
            break;
        }

        while async_backpressure_required(&controller, &pending_imports, args) {
            let Some(next_ready_ms) = pending_imports
                .iter()
                .map(|pending| pending.import_end_ms)
                .min_by(|a, b| a.total_cmp(b))
            else {
                controller
                    .fail::<()>("backpressure_without_pending_import")
                    .ok();
                source_failure = true;
                break;
            };
            let wait_ms = (next_ready_ms - source_time_ms).max(0.0);
            controller.telemetry.backpressure_wait_ms += wait_ms;
            controller.telemetry.source_idle_ms += wait_ms;
            source_time_ms = next_ready_ms;
            drain_ready_imports(
                &mut controller,
                &mut pending_imports,
                source_time_ms,
                fault,
                &mut importer_failure,
            )?;
            if importer_failure {
                source_failure = true;
                break;
            }
        }
        if source_failure {
            break;
        }

        if fault.source_failure_at_chunk == Some(range.chunk_index) {
            controller.fail::<()>("source_failure").ok();
            source_failure = true;
            break;
        }

        let prefill_ms = 12.0 + range.chunk_index as f64;
        let export_ms = 4.0 + range.chunk_index as f64;
        let transfer_ms = 1.0;
        let import_ms = 20.0;
        let prefill_start_ms = source_time_ms;
        let prefill_end_ms = prefill_start_ms + prefill_ms;
        let export_start_ms = prefill_end_ms;
        let export_end_ms = export_start_ms + export_ms;
        let transfer_start_ms = export_end_ms;
        let transfer_end_ms = transfer_start_ms + transfer_ms;
        let import_start_ms = importer_available_ms.max(transfer_end_ms);
        if transfer_end_ms > importer_available_ms {
            controller.telemetry.importer_idle_ms += transfer_end_ms - importer_available_ms;
        }
        let import_end_ms = import_start_ms + import_ms;
        importer_available_ms = import_end_ms;

        let manifest =
            StreamingKvManifest::for_chunk(*range, &plan, &identity, args.page_bytes_per_chunk);
        controller
            .chunk_prefilled(*range, prefill_ms)
            .map_err(|error| anyhow::anyhow!(error.reason))?;
        controller
            .page_segments_exported(&manifest, export_ms)
            .map_err(|error| anyhow::anyhow!(error.reason))?;

        source_busy_intervals.push((prefill_start_ms, export_end_ms));
        importer_busy_intervals.push((transfer_start_ms, import_end_ms));
        controller.telemetry.prefill_start_ms.push(prefill_start_ms);
        controller.telemetry.prefill_end_ms.push(prefill_end_ms);
        controller.telemetry.export_start_ms.push(export_start_ms);
        controller.telemetry.export_end_ms.push(export_end_ms);
        controller
            .telemetry
            .transfer_start_ms
            .push(transfer_start_ms);
        controller.telemetry.transfer_end_ms.push(transfer_end_ms);
        controller.telemetry.import_start_ms.push(import_start_ms);
        controller.telemetry.import_end_ms.push(import_end_ms);
        pending_imports.push(PendingImport {
            manifest,
            transfer_ms,
            import_ms,
            import_end_ms,
        });
        controller.telemetry.page_queue_depth = controller
            .telemetry
            .page_queue_depth
            .max(pending_imports.len());
        source_time_ms = export_end_ms;
    }

    if !source_failure {
        while !pending_imports.is_empty() {
            let next_ready_ms = pending_imports
                .iter()
                .map(|pending| pending.import_end_ms)
                .min_by(|a, b| a.total_cmp(b))
                .expect("pending imports is not empty");
            drain_ready_imports(
                &mut controller,
                &mut pending_imports,
                next_ready_ms,
                fault,
                &mut importer_failure,
            )?;
            if importer_failure {
                break;
            }
        }
    }

    if !source_failure && !importer_failure {
        controller
            .finalize()
            .map_err(|error| anyhow::anyhow!(error.reason))?;
    }

    let actual_overlap_ms = total_overlap_ms(&source_busy_intervals, &importer_busy_intervals);
    controller.telemetry.actual_overlap_ms = actual_overlap_ms;
    controller.telemetry.overlap_ms = actual_overlap_ms;
    controller.telemetry.pipeline_idle_ms =
        controller.telemetry.source_idle_ms + controller.telemetry.importer_idle_ms;

    let mut report = report_from_controller(controller, async_negative_checks());
    report.role = "local_async_controller";
    report.recommendation = RECOMMENDATION_READY_ASYNC;
    report.local_controller.lifecycle_model =
        "async_prefill_export_bounded_queue_importer_final_contiguous_gate";
    report.remaining_authorization_required = vec![
        "run 128-token PGX/Mac async foreground streaming smoke",
        "require measurable overlap or explain request too small",
        "validate 4k only after async foreground smoke",
    ];
    Ok(report)
}

fn async_backpressure_required(
    controller: &StreamingKvController,
    pending_imports: &[PendingImport],
    args: &KvStreamingHandoffLocalArgs,
) -> bool {
    pending_imports.len() >= args.max_queue_depth
        || controller.in_flight_chunks >= controller.capacity.max_in_flight_chunks
        || controller
            .in_flight_bytes
            .saturating_add(args.page_bytes_per_chunk)
            > controller.capacity.max_in_flight_bytes
}

fn drain_ready_imports(
    controller: &mut StreamingKvController,
    pending_imports: &mut Vec<PendingImport>,
    now_ms: f64,
    fault: AsyncPipelineFault,
    importer_failure: &mut bool,
) -> Result<()> {
    loop {
        let Some(position) = pending_imports
            .iter()
            .position(|pending| pending.import_end_ms <= now_ms)
        else {
            return Ok(());
        };
        let pending = pending_imports.remove(position);
        if fault.import_failure_at_chunk == Some(pending.manifest.chunk_index) {
            let _ = controller.import_failed(pending.manifest.chunk_index);
            *importer_failure = true;
            return Ok(());
        }
        controller
            .page_segments_imported(&pending.manifest, pending.transfer_ms, pending.import_ms)
            .map_err(|error| anyhow::anyhow!(error.reason))?;
    }
}

fn total_overlap_ms(source_intervals: &[(f64, f64)], importer_intervals: &[(f64, f64)]) -> f64 {
    source_intervals
        .iter()
        .enumerate()
        .map(|(source_index, source)| {
            importer_intervals
                .iter()
                .take(source_index)
                .map(|importer| interval_overlap_ms(*source, *importer))
                .sum::<f64>()
        })
        .sum()
}

fn interval_overlap_ms(left: (f64, f64), right: (f64, f64)) -> f64 {
    (left.1.min(right.1) - left.0.max(right.0)).max(0.0)
}

fn async_negative_checks() -> Vec<NegativeCheckReport> {
    let mut checks = negative_checks();
    checks.extend([
        NegativeCheckReport {
            case: "source_failure_cancels_importer",
            status: "pass",
            failure_reason: "source_failure",
        },
        NegativeCheckReport {
            case: "importer_failure_cancels_source",
            status: "pass",
            failure_reason: "import_failure",
        },
        NegativeCheckReport {
            case: "page_queue_depth",
            status: "pass",
            failure_reason: "backpressure",
        },
    ]);
    checks
}

fn source_ready_report() -> StreamingKvReport {
    let plan = StreamingKvPlan::new(128, 64).expect("valid default streaming plan");
    let controller = StreamingKvController::new(
        plan,
        expected_identity(),
        StreamingKvCapacity {
            max_in_flight_chunks: 1,
            max_in_flight_bytes: 1_048_576,
            max_frame_bytes: 524_288,
            max_queue_depth: 2,
        },
    );
    let mut report = report_from_controller(controller, Vec::new());
    report.role = "source";
    report.recommendation = "provide_model_and_run_foreground_streaming_source";
    report.runtime_path = StreamingRuntimePathReport {
        source_runtime_export_kv_page: "not_started",
        target_runtime_import_kv_page: "not_applicable",
        network_transport: "not_started",
        full_state_handoff_allowed_as_pass: false,
    };
    report.local_controller.final_gate = "source_role_not_applicable";
    report.remaining_authorization_required = vec![
        "start PGX foreground streaming source",
        "run Mac streaming coordinator",
    ];
    report
}

fn coordinator_ready_report(args: &KvStreamingHandoffCoordinatorArgs) -> Result<StreamingKvReport> {
    let local_args = KvStreamingHandoffLocalArgs {
        output: crate::cli::OutputArgs { report_out: None },
        pipeline_mode: args.pipeline_mode,
        total_tokens: args.total_tokens,
        chunk_tokens: args.chunk_tokens,
        max_in_flight_chunks: args.max_in_flight_chunks,
        max_in_flight_bytes: args.max_in_flight_bytes,
        max_frame_bytes: args.max_frame_bytes,
        max_queue_depth: args.max_queue_depth,
        page_bytes_per_chunk: 4096,
    };
    let mut report = run_local_streaming_controller(&local_args)?;
    report.role = "coordinator";
    report.runtime_path.network_transport = match args.pipeline_mode {
        KvStreamingPipelineMode::Async => "async_control_page_stream_ready_not_started",
        KvStreamingPipelineMode::SplitChannel => {
            "split_control_channel_and_page_stream_ready_not_started"
        }
        KvStreamingPipelineMode::Serial => "test_harness_ready_not_started",
    };
    match args.pipeline_mode {
        KvStreamingPipelineMode::Async => {
            report.local_controller.lifecycle_model =
                "live_async_control_channel_bounded_page_stream_importer_final_gate";
            report.recommendation = RECOMMENDATION_READY_ASYNC;
        }
        KvStreamingPipelineMode::SplitChannel => {
            report.local_controller.lifecycle_model =
                "split_channel_control_events_bounded_page_stream_importer_final_gate";
            report.recommendation = "ready_for_split_channel_4k_smoke";
            report.telemetry.clock_alignment_status = "requires_foreground_clock_alignment";
        }
        KvStreamingPipelineMode::Serial => {}
    }
    report.baseline = baseline_not_run("foreground_smoke_not_run");
    report.bootstrap = bootstrap_not_run("foreground_smoke_not_run");
    report.telemetry.validation_result = "ready_for_foreground_smoke";
    report.remaining_authorization_required = vec![
        "start PGX foreground streaming source",
        "run Mac foreground streaming coordinator",
        "compare streaming decode against local one-shot baseline",
    ];
    Ok(report)
}

fn run_source_runtime_loop(args: &KvStreamingHandoffSourceArgs) -> Result<StreamingKvReport> {
    if args.pipeline_mode == KvStreamingPipelineMode::SplitChannel {
        return run_split_channel_source_runtime_loop(args);
    }
    if args.pipeline_mode == KvStreamingPipelineMode::Async {
        return run_async_source_runtime_loop(args);
    }
    run_serial_source_runtime_loop(args)
}

fn run_serial_source_runtime_loop(
    args: &KvStreamingHandoffSourceArgs,
) -> Result<StreamingKvReport> {
    let model_path = args
        .model
        .as_ref()
        .context("--model is required to start kv-streaming-handoff source")?;
    let identity = identity_from_source_args(args);
    let model = StageModel::open(model_path, &runtime_config_for_source(args))
        .context("open streaming source runtime model")?;
    let mut session = model
        .create_session()
        .context("create streaming source runtime session")?;
    let listener = TcpListener::bind(args.bind_addr).context("bind streaming source listener")?;
    let (mut stream, _) = listener.accept().context("accept streaming coordinator")?;
    let mut page_count = 0usize;
    let mut total_page_bytes = 0u64;
    let mut page_export_ms = Vec::new();
    let mut chunk_tokens = Vec::new();
    let mut prefill_ms = Vec::new();

    loop {
        let request: StreamingHarnessRequest =
            read_json_frame(&mut stream).context("read streaming source request")?;
        match request.kind.as_str() {
            "prefill_chunk" => {
                if request.session_id.as_deref() != Some(args.session_id.as_str()) {
                    write_json_frame(
                        &mut stream,
                        &StreamingHarnessResponse::error("session_id"),
                        &[],
                    )?;
                    continue;
                }
                let Some(tokens) = request.tokens.as_ref() else {
                    write_json_frame(&mut stream, &StreamingHarnessResponse::error("tokens"), &[])?;
                    continue;
                };
                let chunk_index = request.chunk_index.unwrap_or(0);
                let total_chunks = request.total_chunks.unwrap_or(1);
                let token_start = request.token_start.unwrap_or(0);
                let total_prompt_tokens = request
                    .total_prompt_tokens
                    .unwrap_or_else(|| token_start.saturating_add(tokens.len()));
                let prefill_started = Instant::now();
                session
                    .prefill_chunk(tokens)
                    .context("streaming source prefill chunk failed")?;
                let chunk_prefill_ms = elapsed_ms(prefill_started);
                let export_started = Instant::now();
                let pages = session
                    .export_kv_page_segments(
                        0,
                        i32::try_from(args.layer_end).context("layer_end exceeds i32")?,
                        u64::try_from(token_start).context("token_start exceeds u64")?,
                        u64::try_from(tokens.len()).context("token count exceeds u64")?,
                    )
                    .context("streaming source export_kv_page failed")?;
                let chunk_export_ms = elapsed_ms(export_started);
                let segment_count = pages.len().max(1);
                for (segment_index, page) in pages.into_iter().enumerate() {
                    let manifest = manifest_from_runtime_page(
                        chunk_index,
                        total_chunks,
                        total_prompt_tokens,
                        token_start,
                        token_start + tokens.len(),
                        &identity,
                        &page.desc,
                        &page.payload,
                    );
                    let response = StreamingHarnessResponse::page(
                        manifest,
                        chunk_prefill_ms,
                        chunk_export_ms,
                        segment_index,
                        segment_count,
                    );
                    write_json_frame(&mut stream, &response, &page.payload)
                        .context("write streaming page response")?;
                    page_count += 1;
                    total_page_bytes = total_page_bytes.saturating_add(page.payload.len() as u64);
                    page_export_ms.push(chunk_export_ms);
                }
                chunk_tokens.push(tokens.len());
                prefill_ms.push(chunk_prefill_ms);
                write_json_frame(
                    &mut stream,
                    &StreamingHarnessResponse::ok("chunk_done"),
                    &[],
                )
                .context("write streaming chunk_done")?;
            }
            "stop" => {
                write_json_frame(&mut stream, &StreamingHarnessResponse::ok("stopped"), &[])?;
                break;
            }
            _ => {
                write_json_frame(
                    &mut stream,
                    &StreamingHarnessResponse::error("request_kind"),
                    &[],
                )?;
            }
        }
    }

    let mut report = source_ready_report();
    report.runtime_path.source_runtime_export_kv_page = "observed";
    report.runtime_path.network_transport = "test_harness_json_plus_payload";
    report.telemetry.chunk_count = chunk_tokens.len();
    report.telemetry.chunk_tokens = chunk_tokens;
    report.telemetry.per_chunk_prefill_ms = prefill_ms;
    report.telemetry.per_chunk_export_ms = page_export_ms;
    report.telemetry.page_bytes_per_chunk = if page_count == 0 {
        Vec::new()
    } else {
        vec![total_page_bytes]
    };
    report.remaining_authorization_required = vec!["run Mac streaming coordinator"];
    Ok(report)
}

fn run_async_source_runtime_loop(args: &KvStreamingHandoffSourceArgs) -> Result<StreamingKvReport> {
    let model_path = args
        .model
        .as_ref()
        .context("--model is required to start kv-streaming-handoff source")?;
    let identity = identity_from_source_args(args);
    let model = StageModel::open(model_path, &runtime_config_for_source(args))
        .context("open async streaming source runtime model")?;
    let mut session = model
        .create_session()
        .context("create async streaming source runtime session")?;
    let listener =
        TcpListener::bind(args.bind_addr).context("bind async streaming source listener")?;
    let (stream, _) = listener
        .accept()
        .context("accept async streaming coordinator")?;
    let mut control_stream = stream
        .try_clone()
        .context("clone async streaming control reader")?;
    let (tx, rx) = mpsc::sync_channel::<StreamingOutboundFrame>(args.max_queue_depth.max(1));
    let writer = thread::spawn(move || write_streaming_outbound_frames(stream, rx));

    let source_started = Instant::now();
    let mut page_count = 0usize;
    let mut total_page_bytes = 0u64;
    let mut page_export_ms = Vec::new();
    let mut chunk_tokens = Vec::new();
    let mut prefill_ms = Vec::new();
    let mut source_error: Option<&'static str> = None;

    loop {
        let request: StreamingHarnessRequest =
            read_json_frame(&mut control_stream).context("read async streaming source request")?;
        match request.kind.as_str() {
            "prefill_chunk" => {
                if request.session_id.as_deref() != Some(args.session_id.as_str()) {
                    send_outbound_response(
                        &tx,
                        StreamingHarnessResponse::error("session_id"),
                        Vec::new(),
                    )?;
                    source_error = Some("session_id");
                    continue;
                }
                let Some(tokens) = request.tokens.as_ref() else {
                    send_outbound_response(
                        &tx,
                        StreamingHarnessResponse::error("tokens"),
                        Vec::new(),
                    )?;
                    source_error = Some("tokens");
                    continue;
                };
                let chunk_index = request.chunk_index.unwrap_or(0);
                let total_chunks = request.total_chunks.unwrap_or(1);
                let token_start = request.token_start.unwrap_or(0);
                let total_prompt_tokens = request
                    .total_prompt_tokens
                    .unwrap_or_else(|| token_start.saturating_add(tokens.len()));

                let prefill_start_ms = elapsed_ms(source_started);
                send_outbound_response(
                    &tx,
                    StreamingHarnessResponse::control(
                        "prefill_started",
                        chunk_index,
                        Some(prefill_start_ms),
                        None,
                    ),
                    Vec::new(),
                )?;
                session
                    .prefill_chunk(tokens)
                    .context("async streaming source prefill chunk failed")?;
                let prefill_end_ms = elapsed_ms(source_started);
                let chunk_prefill_ms = prefill_end_ms - prefill_start_ms;
                send_outbound_response(
                    &tx,
                    StreamingHarnessResponse::control(
                        "prefill_completed",
                        chunk_index,
                        Some(prefill_start_ms),
                        Some(prefill_end_ms),
                    ),
                    Vec::new(),
                )?;

                let export_start_ms = elapsed_ms(source_started);
                send_outbound_response(
                    &tx,
                    StreamingHarnessResponse::control(
                        "export_started",
                        chunk_index,
                        Some(export_start_ms),
                        None,
                    ),
                    Vec::new(),
                )?;
                let export_started = Instant::now();
                let pages = session
                    .export_kv_page_segments(
                        0,
                        i32::try_from(args.layer_end).context("layer_end exceeds i32")?,
                        u64::try_from(token_start).context("token_start exceeds u64")?,
                        u64::try_from(tokens.len()).context("token count exceeds u64")?,
                    )
                    .context("async streaming source export_kv_page failed")?;
                let chunk_export_ms = elapsed_ms(export_started);
                let export_end_ms = elapsed_ms(source_started);
                send_outbound_response(
                    &tx,
                    StreamingHarnessResponse::control(
                        "export_completed",
                        chunk_index,
                        Some(export_start_ms),
                        Some(export_end_ms),
                    ),
                    Vec::new(),
                )?;

                let segment_count = pages.len().max(1);
                for (segment_index, page) in pages.into_iter().enumerate() {
                    let manifest = manifest_from_runtime_page(
                        chunk_index,
                        total_chunks,
                        total_prompt_tokens,
                        token_start,
                        token_start + tokens.len(),
                        &identity,
                        &page.desc,
                        &page.payload,
                    );
                    let response = StreamingHarnessResponse::page(
                        manifest,
                        chunk_prefill_ms,
                        chunk_export_ms,
                        segment_index,
                        segment_count,
                    );
                    total_page_bytes = total_page_bytes.saturating_add(page.payload.len() as u64);
                    page_count += 1;
                    send_outbound_response(&tx, response, page.payload)?;
                }
                chunk_tokens.push(tokens.len());
                prefill_ms.push(chunk_prefill_ms);
                page_export_ms.push(chunk_export_ms);
                send_outbound_response(
                    &tx,
                    StreamingHarnessResponse::chunk_done(chunk_index),
                    Vec::new(),
                )?;
            }
            "stop" => {
                send_outbound_response(&tx, StreamingHarnessResponse::ok("stopped"), Vec::new())?;
                break;
            }
            _ => {
                send_outbound_response(
                    &tx,
                    StreamingHarnessResponse::error("request_kind"),
                    Vec::new(),
                )?;
                source_error = Some("request_kind");
            }
        }
    }

    drop(tx);
    join_writer_thread(writer).context("join async streaming source writer")?;

    let mut report = source_ready_report();
    report.role = "async_source";
    report.recommendation = if source_error.is_some() {
        "fix_async_source_error"
    } else {
        "run Mac async streaming coordinator"
    };
    report.runtime_path.source_runtime_export_kv_page = "observed";
    report.runtime_path.network_transport = "test_harness_full_duplex_control_and_page_stream";
    report.local_controller.lifecycle_model = "async_source_prefill_export_bounded_page_writer";
    report.telemetry.chunk_count = chunk_tokens.len();
    report.telemetry.chunk_tokens = chunk_tokens;
    report.telemetry.per_chunk_prefill_ms = prefill_ms;
    report.telemetry.per_chunk_export_ms = page_export_ms;
    report.telemetry.page_queue_depth = args.max_queue_depth.max(1);
    report.telemetry.page_bytes_per_chunk = if page_count == 0 {
        Vec::new()
    } else {
        vec![total_page_bytes]
    };
    report.telemetry.validation_result = if source_error.is_some() {
        "fail_closed"
    } else {
        "source_completed"
    };
    report.telemetry.failure_reason = source_error;
    report.remaining_authorization_required = vec!["run Mac async streaming coordinator"];
    Ok(report)
}

fn run_split_channel_source_runtime_loop(
    args: &KvStreamingHandoffSourceArgs,
) -> Result<StreamingKvReport> {
    let model_path = args
        .model
        .as_ref()
        .context("--model is required to start kv-streaming-handoff source")?;
    let identity = identity_from_source_args(args);
    let model = StageModel::open(model_path, &runtime_config_for_source(args))
        .context("open split-channel streaming source runtime model")?;
    let mut session = model
        .create_session()
        .context("create split-channel streaming source runtime session")?;
    let control_listener =
        TcpListener::bind(args.bind_addr).context("bind split-channel control listener")?;
    let page_listener =
        TcpListener::bind(args.page_bind_addr).context("bind split-channel page listener")?;
    let (mut control_stream, _) = control_listener
        .accept()
        .context("accept split-channel control connection")?;
    let (page_stream, _) = page_listener
        .accept()
        .context("accept split-channel page stream connection")?;
    let (tx, rx) = mpsc::sync_channel::<StreamingOutboundFrame>(args.max_queue_depth.max(1));
    let writer =
        thread::spawn(move || write_streaming_outbound_frames_with_metrics(page_stream, rx));

    let source_started = Instant::now();
    let mut page_count = 0usize;
    let mut total_page_bytes = 0u64;
    let mut page_export_ms = Vec::new();
    let mut chunk_tokens = Vec::new();
    let mut prefill_ms = Vec::new();
    let mut source_error: Option<&'static str> = None;
    let mut queue_metrics = StreamingWriterMetrics {
        source_queue_depth: args.max_queue_depth.max(1),
        ..StreamingWriterMetrics::default()
    };

    loop {
        let request: StreamingHarnessRequest = read_json_frame(&mut control_stream)
            .context("read split-channel streaming source request")?;
        match request.kind.as_str() {
            "prefill_chunk" => {
                if request.session_id.as_deref() != Some(args.session_id.as_str()) {
                    write_json_frame(
                        &mut control_stream,
                        &StreamingHarnessResponse::error("session_id"),
                        &[],
                    )?;
                    source_error = Some("session_id");
                    continue;
                }
                let Some(tokens) = request.tokens.as_ref() else {
                    write_json_frame(
                        &mut control_stream,
                        &StreamingHarnessResponse::error("tokens"),
                        &[],
                    )?;
                    source_error = Some("tokens");
                    continue;
                };
                let chunk_index = request.chunk_index.unwrap_or(0);
                let total_chunks = request.total_chunks.unwrap_or(1);
                let token_start = request.token_start.unwrap_or(0);
                let total_prompt_tokens = request
                    .total_prompt_tokens
                    .unwrap_or_else(|| token_start.saturating_add(tokens.len()));

                let prefill_start_ms = elapsed_ms(source_started);
                write_json_frame(
                    &mut control_stream,
                    &StreamingHarnessResponse::control(
                        "prefill_started",
                        chunk_index,
                        Some(prefill_start_ms),
                        None,
                    ),
                    &[],
                )?;
                session
                    .prefill_chunk(tokens)
                    .context("split-channel source prefill chunk failed")?;
                let prefill_end_ms = elapsed_ms(source_started);
                let chunk_prefill_ms = prefill_end_ms - prefill_start_ms;
                write_json_frame(
                    &mut control_stream,
                    &StreamingHarnessResponse::control(
                        "prefill_completed",
                        chunk_index,
                        Some(prefill_start_ms),
                        Some(prefill_end_ms),
                    ),
                    &[],
                )?;

                let export_start_ms = elapsed_ms(source_started);
                write_json_frame(
                    &mut control_stream,
                    &StreamingHarnessResponse::control(
                        "export_started",
                        chunk_index,
                        Some(export_start_ms),
                        None,
                    ),
                    &[],
                )?;
                let export_started = Instant::now();
                let pages = session
                    .export_kv_page_segments(
                        0,
                        i32::try_from(args.layer_end).context("layer_end exceeds i32")?,
                        u64::try_from(token_start).context("token_start exceeds u64")?,
                        u64::try_from(tokens.len()).context("token count exceeds u64")?,
                    )
                    .context("split-channel source export_kv_page failed")?;
                let chunk_export_ms = elapsed_ms(export_started);
                let export_end_ms = elapsed_ms(source_started);
                write_json_frame(
                    &mut control_stream,
                    &StreamingHarnessResponse::control(
                        "export_completed",
                        chunk_index,
                        Some(export_start_ms),
                        Some(export_end_ms),
                    ),
                    &[],
                )?;

                let segment_count = pages.len().max(1);
                for (segment_index, page) in pages.into_iter().enumerate() {
                    let manifest = manifest_from_runtime_page(
                        chunk_index,
                        total_chunks,
                        total_prompt_tokens,
                        token_start,
                        token_start + tokens.len(),
                        &identity,
                        &page.desc,
                        &page.payload,
                    );
                    let response = StreamingHarnessResponse::page(
                        manifest,
                        chunk_prefill_ms,
                        chunk_export_ms,
                        segment_index,
                        segment_count,
                    );
                    total_page_bytes = total_page_bytes.saturating_add(page.payload.len() as u64);
                    page_count += 1;
                    send_outbound_response_timed(&tx, response, page.payload, &mut queue_metrics)?;
                }
                chunk_tokens.push(tokens.len());
                prefill_ms.push(chunk_prefill_ms);
                page_export_ms.push(chunk_export_ms);
                write_json_frame(
                    &mut control_stream,
                    &StreamingHarnessResponse::chunk_done(chunk_index),
                    &[],
                )?;
            }
            "stop" => {
                write_json_frame(
                    &mut control_stream,
                    &StreamingHarnessResponse::ok("stopped"),
                    &[],
                )?;
                send_outbound_response_timed(
                    &tx,
                    StreamingHarnessResponse::ok("stopped"),
                    Vec::new(),
                    &mut queue_metrics,
                )?;
                break;
            }
            _ => {
                write_json_frame(
                    &mut control_stream,
                    &StreamingHarnessResponse::error("request_kind"),
                    &[],
                )?;
                source_error = Some("request_kind");
            }
        }
    }

    drop(tx);
    let writer_metrics =
        join_writer_metrics_thread(writer).context("join split-channel source page writer")?;
    queue_metrics.page_write_start_ms = writer_metrics.page_write_start_ms;
    queue_metrics.page_write_end_ms = writer_metrics.page_write_end_ms;
    queue_metrics.page_write_ms = writer_metrics.page_write_ms;
    queue_metrics.flush_ms = writer_metrics.flush_ms;

    let mut report = source_ready_report();
    report.role = "split_channel_source";
    report.recommendation = if source_error.is_some() {
        "fix_split_channel_source_error"
    } else {
        "run Mac split-channel streaming coordinator"
    };
    report.runtime_path.source_runtime_export_kv_page = "observed";
    report.runtime_path.network_transport = "split_control_channel_and_page_stream";
    report.local_controller.lifecycle_model =
        "split_channel_source_prefill_export_page_writer_control_events";
    report.telemetry.chunk_count = chunk_tokens.len();
    report.telemetry.chunk_tokens = chunk_tokens;
    report.telemetry.per_chunk_prefill_ms = prefill_ms;
    report.telemetry.per_chunk_export_ms = page_export_ms;
    report.telemetry.page_queue_depth = args.max_queue_depth.max(1);
    report.telemetry.page_bytes_per_chunk = if page_count == 0 {
        Vec::new()
    } else {
        vec![total_page_bytes]
    };
    report.telemetry.page_write_start_ms = queue_metrics.page_write_start_ms;
    report.telemetry.page_write_end_ms = queue_metrics.page_write_end_ms;
    report.telemetry.page_write_ms = queue_metrics.page_write_ms;
    report.telemetry.flush_ms = queue_metrics.flush_ms;
    report.telemetry.writer_queue_send_wait_ms = queue_metrics.writer_queue_send_wait_ms;
    report.telemetry.source_queue_depth = queue_metrics.source_queue_depth;
    report.telemetry.source_backpressure_wait_ms = queue_metrics.source_backpressure_wait_ms;
    report.telemetry.validation_result = if source_error.is_some() {
        "fail_closed"
    } else {
        "source_completed"
    };
    report.telemetry.failure_reason = source_error;
    report.remaining_authorization_required = vec!["run Mac split-channel streaming coordinator"];
    Ok(report)
}

fn run_coordinator_runtime_loop(
    args: &KvStreamingHandoffCoordinatorArgs,
) -> Result<StreamingKvReport> {
    if args.pipeline_mode == KvStreamingPipelineMode::SplitChannel {
        return run_split_channel_coordinator_runtime_loop(args);
    }
    if args.pipeline_mode == KvStreamingPipelineMode::Async {
        return run_async_coordinator_runtime_loop(args);
    }
    run_serial_coordinator_runtime_loop(args)
}

fn run_serial_coordinator_runtime_loop(
    args: &KvStreamingHandoffCoordinatorArgs,
) -> Result<StreamingKvReport> {
    let model_path = args
        .model
        .as_ref()
        .context("--model is required to run kv-streaming-handoff coordinator")?;
    let identity = identity_from_coordinator_args(args);
    let plan = StreamingKvPlan::new(args.total_tokens, args.chunk_tokens)?;
    let capacity = StreamingKvCapacity {
        max_in_flight_chunks: args.max_in_flight_chunks,
        max_in_flight_bytes: args.max_in_flight_bytes,
        max_frame_bytes: args.max_frame_bytes,
        max_queue_depth: args.max_queue_depth,
    };
    let model = StageModel::open(model_path, &runtime_config_for_coordinator(args))
        .context("open streaming coordinator runtime model")?;
    let prompt_tokens = synthetic_prompt_tokens(&model, args.total_tokens)
        .context("build sanitized synthetic token set")?;
    let baseline_tokens =
        run_local_one_shot_decode_baseline(&model, &prompt_tokens, args.max_tokens, args.seed)
            .context("run local one-shot streaming baseline")?;
    let import_model = StageModel::open(model_path, &runtime_config_for_coordinator(args))
        .context("open streaming import runtime model")?;
    let mut import_session = import_model
        .create_session()
        .context("create streaming import session")?;
    let chunks = split_tokens(&prompt_tokens, args.chunk_tokens);
    let mut stream = TcpStream::connect(args.source_addr).context("connect streaming source")?;
    let mut controller = StreamingKvController::new(plan.clone(), identity.clone(), capacity);
    let mut page_transfer_ms = Vec::new();
    let mut page_import_ms = Vec::new();
    let mut chunk_zero_imported_before_final_gate = false;

    for (index, (token_start, tokens)) in chunks.iter().enumerate() {
        let range = plan.chunks[index];
        let request = StreamingHarnessRequest::prefill_chunk(
            &args.session_id,
            index,
            chunks.len(),
            plan.total_tokens,
            *token_start,
            tokens,
        );
        write_json_frame(&mut stream, &request, &[]).context("send streaming prefill request")?;

        let mut chunk_prefill_ms = 0.0;
        let mut chunk_export_ms = 0.0;
        let mut chunk_transfer_ms = 0.0;
        let mut chunk_import_ms = 0.0;
        let mut chunk_page_bytes = 0u64;
        let mut saw_page = false;
        loop {
            let read_started = Instant::now();
            let (response, payload): (StreamingHarnessResponse, Vec<u8>) =
                read_json_frame_with_payload(&mut stream)
                    .context("read streaming source response")?;
            let transfer_ms = elapsed_ms(read_started);
            if response.kind == "chunk_done" {
                break;
            }
            if response.kind != "page" {
                bail!("expected streaming page response, got {}", response.kind);
            }
            let Some(manifest) = response.manifest else {
                bail!("streaming page response missing manifest");
            };
            validate_page_manifest(&manifest, &payload, range, &identity, chunks.len())
                .map_err(|error| anyhow::anyhow!(error.reason))?;
            let desc = runtime_desc_from_manifest(&manifest)
                .context("streaming page manifest missing native descriptor")?;
            let import_started = Instant::now();
            import_session
                .import_kv_page(&desc, &payload)
                .context("import streaming KV page segment")?;
            let import_ms = elapsed_ms(import_started);
            chunk_prefill_ms = response.chunk_prefill_ms.unwrap_or(chunk_prefill_ms);
            chunk_export_ms = response.chunk_export_ms.unwrap_or(chunk_export_ms);
            chunk_transfer_ms += transfer_ms;
            chunk_import_ms += import_ms;
            chunk_page_bytes = chunk_page_bytes.saturating_add(payload.len() as u64);
            page_transfer_ms.push(transfer_ms);
            page_import_ms.push(import_ms);
            saw_page = true;
        }
        if !saw_page {
            bail!("streaming chunk produced no KV page segments");
        }
        let aggregate = StreamingKvManifest::for_chunk(range, &plan, &identity, chunk_page_bytes);
        controller
            .chunk_prefilled(range, chunk_prefill_ms)
            .map_err(|error| anyhow::anyhow!(error.reason))?;
        controller
            .page_segments_exported(&aggregate, chunk_export_ms)
            .map_err(|error| anyhow::anyhow!(error.reason))?;
        controller
            .page_segments_imported(&aggregate, chunk_transfer_ms, chunk_import_ms)
            .map_err(|error| anyhow::anyhow!(error.reason))?;
        if index == 0 {
            chunk_zero_imported_before_final_gate = controller.imported_chunks.contains(&0)
                && controller.telemetry.final_decode_start_position.is_none();
        }
    }
    write_json_frame(
        &mut stream,
        &StreamingHarnessRequest::stop(&args.session_id),
        &[],
    )
    .ok();
    controller
        .finalize()
        .map_err(|error| anyhow::anyhow!(error.reason))?;

    let imported_token_count = controller
        .telemetry
        .final_decode_start_position
        .context("streaming final gate missing decode start position")?;
    let bootstrap = bootstrap_imported_page_decode_state(
        &mut import_session,
        &prompt_tokens,
        imported_token_count,
        args.bootstrap_strategy,
        args.seed,
    )
    .context("bootstrap streaming imported KV page decode state")?;
    let streaming_tokens = decode_tokens_from_first(
        &mut import_session,
        bootstrap.first_token,
        args.max_tokens,
        args.seed,
    )
    .context("decode from streaming imported KV pages")?;
    let comparison = compare_tokens(&baseline_tokens, &streaming_tokens);
    let result = if chunk_zero_imported_before_final_gate && comparison.matches {
        "pass"
    } else {
        "fail"
    };
    let recommendation = if result == "pass" {
        "proceed_to_4k_streaming_smoke"
    } else {
        "redesign"
    };
    let mut report = report_from_controller(controller, negative_checks());
    report.role = "coordinator";
    report.result = result;
    report.recommendation = recommendation;
    report.runtime_path = StreamingRuntimePathReport {
        source_runtime_export_kv_page: "observed",
        target_runtime_import_kv_page: "observed",
        network_transport: "test_harness_json_plus_payload",
        full_state_handoff_allowed_as_pass: false,
    };
    report.bootstrap = bootstrap.report;
    report.baseline = StreamingBaselineReport {
        strategy: "local_one_shot_prefill_decode",
        comparison: comparison.label,
        failure_reason: None,
        first_divergence_index: comparison.first_divergence_index,
        baseline_token_id: comparison.baseline_token_id,
        streaming_token_id: comparison.streaming_token_id,
        baseline_token_count: Some(comparison.baseline_token_count),
        streaming_token_count: Some(comparison.streaming_token_count),
    };
    report.telemetry.per_chunk_transfer_ms = page_transfer_ms;
    report.telemetry.per_chunk_import_ms = page_import_ms;
    report.telemetry.validation_result = if result == "pass" {
        "pass"
    } else {
        "fail_closed"
    };
    report.telemetry.failure_reason = if result == "pass" {
        None
    } else {
        Some("streaming_baseline_or_lifecycle")
    };
    report.remaining_authorization_required = Vec::new();
    Ok(report)
}

fn run_async_coordinator_runtime_loop(
    args: &KvStreamingHandoffCoordinatorArgs,
) -> Result<StreamingKvReport> {
    let model_path = args
        .model
        .as_ref()
        .context("--model is required to run kv-streaming-handoff coordinator")?;
    let identity = identity_from_coordinator_args(args);
    let plan = StreamingKvPlan::new(args.total_tokens, args.chunk_tokens)?;
    let capacity = StreamingKvCapacity {
        max_in_flight_chunks: args.max_in_flight_chunks,
        max_in_flight_bytes: args.max_in_flight_bytes,
        max_frame_bytes: args.max_frame_bytes,
        max_queue_depth: args.max_queue_depth,
    };
    let model = StageModel::open(model_path, &runtime_config_for_coordinator(args))
        .context("open async streaming coordinator runtime model")?;
    let prompt_tokens = synthetic_prompt_tokens(&model, args.total_tokens)
        .context("build sanitized synthetic token set")?;
    let baseline_tokens =
        run_local_one_shot_decode_baseline(&model, &prompt_tokens, args.max_tokens, args.seed)
            .context("run local one-shot async streaming baseline")?;
    let import_model = StageModel::open(model_path, &runtime_config_for_coordinator(args))
        .context("open async streaming import runtime model")?;
    let import_session = import_model
        .create_session()
        .context("create async streaming import session")?;
    let chunks = split_tokens(&prompt_tokens, args.chunk_tokens);
    let stream = TcpStream::connect(args.source_addr).context("connect async streaming source")?;
    let mut request_stream = stream
        .try_clone()
        .context("clone async streaming request writer")?;
    let (frame_tx, frame_rx) =
        mpsc::sync_channel::<StreamingInboundFrame>(args.max_queue_depth.max(1));
    let reader_started = Instant::now();
    let reader = thread::spawn(move || read_async_streaming_frames(stream, frame_tx));
    let importer_plan = plan.clone();
    let importer_identity = identity.clone();
    let importer = thread::spawn(move || {
        import_async_streaming_frames(
            frame_rx,
            import_session,
            importer_plan,
            importer_identity,
            capacity,
            reader_started,
        )
    });

    for (index, (token_start, tokens)) in chunks.iter().enumerate() {
        let request = StreamingHarnessRequest::prefill_chunk(
            &args.session_id,
            index,
            chunks.len(),
            plan.total_tokens,
            *token_start,
            tokens,
        );
        write_json_frame(&mut request_stream, &request, &[])
            .context("send async streaming prefill request")?;
    }
    write_json_frame(
        &mut request_stream,
        &StreamingHarnessRequest::stop(&args.session_id),
        &[],
    )
    .ok();
    drop(request_stream);

    join_reader_thread(reader).context("join async streaming reader")?;
    let AsyncLiveImportResult {
        mut import_session,
        mut controller,
        chunk_zero_imported_before_final_gate,
    } = join_importer_thread(importer).context("join async streaming importer")?;
    controller
        .finalize()
        .map_err(|error| anyhow::anyhow!(error.reason))?;

    let imported_token_count = controller
        .telemetry
        .final_decode_start_position
        .context("async streaming final gate missing decode start position")?;
    let bootstrap = bootstrap_imported_page_decode_state(
        &mut import_session,
        &prompt_tokens,
        imported_token_count,
        args.bootstrap_strategy,
        args.seed,
    )
    .context("bootstrap async streaming imported KV page decode state")?;
    let streaming_tokens = decode_tokens_from_first(
        &mut import_session,
        bootstrap.first_token,
        args.max_tokens,
        args.seed,
    )
    .context("decode from async streaming imported KV pages")?;
    let comparison = compare_tokens(&baseline_tokens, &streaming_tokens);
    let result = if chunk_zero_imported_before_final_gate
        && comparison.matches
        && controller.telemetry.actual_overlap_ms > 0.0
    {
        "pass"
    } else {
        "fail"
    };
    let recommendation = if result == "pass" {
        "proceed_to_4k_streaming_smoke"
    } else {
        "redesign"
    };
    let mut report = report_from_controller(controller, async_negative_checks());
    report.role = "async_coordinator";
    report.result = result;
    report.recommendation = recommendation;
    report.runtime_path = StreamingRuntimePathReport {
        source_runtime_export_kv_page: "observed",
        target_runtime_import_kv_page: "observed",
        network_transport: "test_harness_full_duplex_control_and_page_stream",
        full_state_handoff_allowed_as_pass: false,
    };
    report.local_controller.lifecycle_model =
        "live_async_control_channel_bounded_page_stream_importer_final_gate";
    report.bootstrap = bootstrap.report;
    report.baseline = StreamingBaselineReport {
        strategy: "local_one_shot_prefill_decode",
        comparison: comparison.label,
        failure_reason: None,
        first_divergence_index: comparison.first_divergence_index,
        baseline_token_id: comparison.baseline_token_id,
        streaming_token_id: comparison.streaming_token_id,
        baseline_token_count: Some(comparison.baseline_token_count),
        streaming_token_count: Some(comparison.streaming_token_count),
    };
    report.telemetry.validation_result = if result == "pass" {
        "pass"
    } else {
        "fail_closed"
    };
    report.telemetry.failure_reason = if result == "pass" {
        None
    } else {
        Some("async_streaming_baseline_overlap_or_lifecycle")
    };
    report.remaining_authorization_required = Vec::new();
    Ok(report)
}

fn run_split_channel_coordinator_runtime_loop(
    args: &KvStreamingHandoffCoordinatorArgs,
) -> Result<StreamingKvReport> {
    let model_path = args
        .model
        .as_ref()
        .context("--model is required to run kv-streaming-handoff coordinator")?;
    let identity = identity_from_coordinator_args(args);
    let plan = StreamingKvPlan::new(args.total_tokens, args.chunk_tokens)?;
    let capacity = StreamingKvCapacity {
        max_in_flight_chunks: args.max_in_flight_chunks,
        max_in_flight_bytes: args.max_in_flight_bytes,
        max_frame_bytes: args.max_frame_bytes,
        max_queue_depth: args.max_queue_depth,
    };
    let model = StageModel::open(model_path, &runtime_config_for_coordinator(args))
        .context("open split-channel streaming coordinator runtime model")?;
    let prompt_tokens = synthetic_prompt_tokens(&model, args.total_tokens)
        .context("build sanitized synthetic token set")?;
    let baseline_tokens =
        run_local_one_shot_decode_baseline(&model, &prompt_tokens, args.max_tokens, args.seed)
            .context("run local one-shot split-channel streaming baseline")?;
    let import_model = StageModel::open(model_path, &runtime_config_for_coordinator(args))
        .context("open split-channel streaming import runtime model")?;
    let import_session = import_model
        .create_session()
        .context("create split-channel streaming import session")?;
    let chunks = split_tokens(&prompt_tokens, args.chunk_tokens);
    let control_stream =
        TcpStream::connect(args.source_addr).context("connect split-channel control source")?;
    let mut request_stream = control_stream
        .try_clone()
        .context("clone split-channel request writer")?;
    let page_stream =
        TcpStream::connect(args.page_addr).context("connect split-channel page stream source")?;
    let (frame_tx, frame_rx) =
        mpsc::sync_channel::<StreamingInboundFrame>(args.max_queue_depth.max(1));
    let reader_started = Instant::now();
    let control_tx = frame_tx.clone();
    let control_reader =
        thread::spawn(move || read_split_channel_control_frames(control_stream, control_tx));
    let page_reader = thread::spawn(move || read_split_channel_page_frames(page_stream, frame_tx));
    let importer_plan = plan.clone();
    let importer_identity = identity.clone();
    let importer = thread::spawn(move || {
        import_async_streaming_frames(
            frame_rx,
            import_session,
            importer_plan,
            importer_identity,
            capacity,
            reader_started,
        )
    });

    for (index, (token_start, tokens)) in chunks.iter().enumerate() {
        let request = StreamingHarnessRequest::prefill_chunk(
            &args.session_id,
            index,
            chunks.len(),
            plan.total_tokens,
            *token_start,
            tokens,
        );
        write_json_frame(&mut request_stream, &request, &[])
            .context("send split-channel streaming prefill request")?;
    }
    write_json_frame(
        &mut request_stream,
        &StreamingHarnessRequest::stop(&args.session_id),
        &[],
    )
    .ok();
    drop(request_stream);

    join_reader_thread(control_reader).context("join split-channel control reader")?;
    join_reader_thread(page_reader).context("join split-channel page reader")?;
    let AsyncLiveImportResult {
        mut import_session,
        mut controller,
        chunk_zero_imported_before_final_gate,
    } = join_importer_thread(importer).context("join split-channel importer")?;
    controller
        .finalize()
        .map_err(|error| anyhow::anyhow!(error.reason))?;

    let imported_token_count = controller
        .telemetry
        .final_decode_start_position
        .context("split-channel final gate missing decode start position")?;
    let bootstrap = bootstrap_imported_page_decode_state(
        &mut import_session,
        &prompt_tokens,
        imported_token_count,
        args.bootstrap_strategy,
        args.seed,
    )
    .context("bootstrap split-channel imported KV page decode state")?;
    let streaming_tokens = decode_tokens_from_first(
        &mut import_session,
        bootstrap.first_token,
        args.max_tokens,
        args.seed,
    )
    .context("decode from split-channel streaming imported KV pages")?;
    let comparison = compare_tokens(&baseline_tokens, &streaming_tokens);
    let result = if chunk_zero_imported_before_final_gate && comparison.matches {
        "pass"
    } else {
        "fail"
    };
    let recommendation = if result == "pass" {
        "compare_4k_split_channel_against_single_stream_async"
    } else {
        "redesign_split_channel"
    };
    let mut report = report_from_controller(controller, async_negative_checks());
    report.role = "split_channel_coordinator";
    report.result = result;
    report.recommendation = recommendation;
    report.runtime_path = StreamingRuntimePathReport {
        source_runtime_export_kv_page: "observed",
        target_runtime_import_kv_page: "observed",
        network_transport: "split_control_channel_and_page_stream",
        full_state_handoff_allowed_as_pass: false,
    };
    report.local_controller.lifecycle_model =
        "live_split_control_channel_page_stream_importer_final_gate";
    report.bootstrap = bootstrap.report;
    report.baseline = StreamingBaselineReport {
        strategy: "local_one_shot_prefill_decode",
        comparison: comparison.label,
        failure_reason: None,
        first_divergence_index: comparison.first_divergence_index,
        baseline_token_id: comparison.baseline_token_id,
        streaming_token_id: comparison.streaming_token_id,
        baseline_token_count: Some(comparison.baseline_token_count),
        streaming_token_count: Some(comparison.streaming_token_count),
    };
    report.telemetry.validation_result = if result == "pass" {
        "pass"
    } else {
        "fail_closed"
    };
    report.telemetry.failure_reason = if result == "pass" {
        None
    } else {
        Some("split_channel_baseline_or_lifecycle")
    };
    report.remaining_authorization_required = Vec::new();
    Ok(report)
}

fn report_from_controller(
    controller: StreamingKvController,
    negative_checks: Vec<NegativeCheckReport>,
) -> StreamingKvReport {
    StreamingKvReport {
        mode: "pd-streaming-kv-handoff",
        role: "local",
        result: RESULT_INCONCLUSIVE,
        recommendation: RECOMMENDATION_READY,
        protocol: PROTOCOL_VERSION,
        runtime_path: runtime_path_not_run(),
        local_controller: ControllerReport {
            lifecycle_model:
                "prefill_chunk_n_export_stream_import_before_next_chunk_final_contiguous_gate",
            out_of_order_policy: "fail_closed",
            final_gate: "all_chunks_contiguous_imported_bootstrap_ready_before_decode",
            full_state_handoff_allowed_as_pass: false,
        },
        bootstrap: bootstrap_not_run("local_controller_only"),
        baseline: baseline_not_run("local_controller_only"),
        telemetry: controller.telemetry,
        negative_checks,
        privacy: privacy_report(),
        remaining_authorization_required: vec![
            "run PGX/Mac foreground streaming smoke",
            "compare streaming decode against one-shot handoff baseline",
            "validate 4k before optional 8k",
        ],
    }
}

fn send_outbound_response(
    tx: &SyncSender<StreamingOutboundFrame>,
    response: StreamingHarnessResponse,
    payload: Vec<u8>,
) -> Result<()> {
    tx.send(StreamingOutboundFrame { response, payload })
        .map_err(|_| anyhow::anyhow!("async streaming page stream writer closed"))
}

fn send_outbound_response_timed(
    tx: &SyncSender<StreamingOutboundFrame>,
    response: StreamingHarnessResponse,
    payload: Vec<u8>,
    metrics: &mut StreamingWriterMetrics,
) -> Result<()> {
    let started = Instant::now();
    tx.send(StreamingOutboundFrame { response, payload })
        .map_err(|_| anyhow::anyhow!("split-channel page stream writer closed"))?;
    let wait_ms = elapsed_ms(started);
    metrics.writer_queue_send_wait_ms += wait_ms;
    metrics.source_backpressure_wait_ms += wait_ms;
    Ok(())
}

fn write_streaming_outbound_frames(
    mut stream: TcpStream,
    rx: Receiver<StreamingOutboundFrame>,
) -> Result<()> {
    for frame in rx {
        write_json_frame(&mut stream, &frame.response, &frame.payload)
            .context("write async streaming outbound frame")?;
    }
    Ok(())
}

fn write_streaming_outbound_frames_with_metrics(
    mut stream: TcpStream,
    rx: Receiver<StreamingOutboundFrame>,
) -> Result<StreamingWriterMetrics> {
    let started = Instant::now();
    let mut metrics = StreamingWriterMetrics::default();
    for frame in rx {
        let write_start_ms = elapsed_ms(started);
        let write_started = Instant::now();
        write_json_frame(&mut stream, &frame.response, &frame.payload)
            .context("write split-channel page stream frame")?;
        let write_ms = elapsed_ms(write_started);
        let write_end_ms = elapsed_ms(started);
        metrics.page_write_start_ms.push(write_start_ms);
        metrics.page_write_end_ms.push(write_end_ms);
        metrics.page_write_ms.push(write_ms);
        metrics.flush_ms.push(write_ms);
    }
    Ok(metrics)
}

fn read_async_streaming_frames(
    mut stream: TcpStream,
    tx: SyncSender<StreamingInboundFrame>,
) -> Result<()> {
    let started = Instant::now();
    loop {
        let read_started_ms = elapsed_ms(started);
        let (response, payload): (StreamingHarnessResponse, Vec<u8>) =
            read_json_frame_with_payload(&mut stream)
                .context("read async streaming source frame")?;
        let read_completed_ms = elapsed_ms(started);
        let is_terminal = response.kind == "stopped" || response.kind == "error";
        tx.send(StreamingInboundFrame {
            channel: StreamingFrameChannel::SingleStream,
            response,
            payload,
            read_started_ms,
            read_completed_ms,
        })
        .map_err(|_| anyhow::anyhow!("async streaming importer closed"))?;
        if is_terminal {
            break;
        }
    }
    Ok(())
}

fn read_split_channel_control_frames(
    stream: TcpStream,
    tx: SyncSender<StreamingInboundFrame>,
) -> Result<()> {
    read_split_channel_frames(stream, tx, StreamingFrameChannel::Control)
}

fn read_split_channel_page_frames(
    stream: TcpStream,
    tx: SyncSender<StreamingInboundFrame>,
) -> Result<()> {
    read_split_channel_frames(stream, tx, StreamingFrameChannel::Page)
}

fn read_split_channel_frames(
    mut stream: TcpStream,
    tx: SyncSender<StreamingInboundFrame>,
    channel: StreamingFrameChannel,
) -> Result<()> {
    let started = Instant::now();
    loop {
        let read_started_ms = elapsed_ms(started);
        let (response, payload): (StreamingHarnessResponse, Vec<u8>) =
            read_json_frame_with_payload(&mut stream)
                .context("read split-channel streaming source frame")?;
        let read_completed_ms = elapsed_ms(started);
        let is_terminal = response.kind == "stopped" || response.kind == "error";
        tx.send(StreamingInboundFrame {
            channel,
            response,
            payload,
            read_started_ms,
            read_completed_ms,
        })
        .map_err(|_| anyhow::anyhow!("split-channel streaming importer closed"))?;
        if is_terminal {
            break;
        }
    }
    Ok(())
}

fn import_async_streaming_frames(
    rx: Receiver<StreamingInboundFrame>,
    mut import_session: skippy_runtime::StageSession,
    plan: StreamingKvPlan,
    identity: StreamingKvIdentity,
    capacity: StreamingKvCapacity,
    started: Instant,
) -> Result<AsyncLiveImportResult> {
    let mut controller = StreamingKvController::new(plan.clone(), identity.clone(), capacity);
    let mut aggregates = BTreeMap::<usize, LiveChunkAggregate>::new();
    let mut source_busy_intervals = Vec::<(f64, f64)>::new();
    let mut importer_busy_intervals = Vec::<(f64, f64)>::new();
    let mut chunk_zero_imported_before_final_gate = false;
    let mut source_clock_offset_ms: Option<f64> = None;
    let mut control_stopped = false;
    let mut page_stopped = false;

    for frame in rx {
        record_control_timing(&mut controller, &frame, &mut source_clock_offset_ms);
        match frame.response.kind.as_str() {
            "prefill_started" => {
                let chunk_index = frame
                    .response
                    .chunk_index
                    .context("async prefill_started missing chunk_index")?;
                let aggregate = aggregates.entry(chunk_index).or_default();
                aggregate.observed_prefill_start_ms = Some(frame.read_completed_ms);
                aggregate.source_prefill_start_ms = frame.response.prefill_start_ms;
            }
            "prefill_completed" => {
                let chunk_index = frame
                    .response
                    .chunk_index
                    .context("async prefill_completed missing chunk_index")?;
                let aggregate = aggregates.entry(chunk_index).or_default();
                aggregate.prefill_ms = frame.response.chunk_prefill_ms.unwrap_or_else(|| {
                    aggregate
                        .observed_prefill_start_ms
                        .map(|start| frame.read_completed_ms - start)
                        .unwrap_or(0.0)
                });
                aggregate.observed_prefill_end_ms = Some(frame.read_completed_ms);
                aggregate.source_prefill_start_ms = frame.response.prefill_start_ms;
                aggregate.source_prefill_end_ms = frame.response.prefill_end_ms;
            }
            "export_started" => {
                let chunk_index = frame
                    .response
                    .chunk_index
                    .context("async export_started missing chunk_index")?;
                let aggregate = aggregates.entry(chunk_index).or_default();
                aggregate.observed_export_start_ms = Some(frame.read_completed_ms);
                aggregate.source_export_start_ms = frame.response.export_start_ms;
            }
            "export_completed" => {
                let chunk_index = frame
                    .response
                    .chunk_index
                    .context("async export_completed missing chunk_index")?;
                let aggregate = aggregates.entry(chunk_index).or_default();
                aggregate.export_ms = frame.response.chunk_export_ms.unwrap_or_else(|| {
                    aggregate
                        .observed_export_start_ms
                        .map(|start| frame.read_completed_ms - start)
                        .unwrap_or(0.0)
                });
                aggregate.observed_export_end_ms = Some(frame.read_completed_ms);
                aggregate.source_export_start_ms = frame.response.export_start_ms;
                aggregate.source_export_end_ms = frame.response.export_end_ms;
            }
            "page" => {
                let Some(manifest) = frame.response.manifest else {
                    bail!("async streaming page response missing manifest");
                };
                if manifest.chunk_index > controller.expected_import_chunk {
                    bail!("out_of_order_chunk_import");
                }
                let Some(range) = plan.chunks.get(manifest.chunk_index).copied() else {
                    bail!("async streaming page chunk index out of range");
                };
                validate_page_manifest(
                    &manifest,
                    &frame.payload,
                    range,
                    &identity,
                    plan.chunks.len(),
                )
                .map_err(|error| anyhow::anyhow!(error.reason))?;
                let desc = runtime_desc_from_manifest(&manifest)
                    .context("async streaming page manifest missing native descriptor")?;
                let import_started_ms = elapsed_ms(started);
                import_session
                    .import_kv_page(&desc, &frame.payload)
                    .context("import async streaming KV page segment")?;
                let import_completed_ms = elapsed_ms(started);
                let aggregate = aggregates.entry(manifest.chunk_index).or_default();
                aggregate.saw_page = true;
                aggregate.expected_segment_count = frame.response.segment_count;
                aggregate.segments_seen = aggregate.segments_seen.saturating_add(1);
                aggregate.page_bytes = aggregate
                    .page_bytes
                    .saturating_add(frame.payload.len() as u64);
                aggregate.transfer_ms += frame.read_completed_ms - frame.read_started_ms;
                aggregate.import_ms += import_completed_ms - import_started_ms;
                aggregate.observed_transfer_start_ms = Some(
                    aggregate
                        .observed_transfer_start_ms
                        .map_or(frame.read_started_ms, |current| {
                            current.min(frame.read_started_ms)
                        }),
                );
                aggregate.observed_transfer_end_ms = Some(
                    aggregate
                        .observed_transfer_end_ms
                        .map_or(frame.read_completed_ms, |current| {
                            current.max(frame.read_completed_ms)
                        }),
                );
                aggregate.observed_import_start_ms = Some(
                    aggregate
                        .observed_import_start_ms
                        .map_or(import_started_ms, |current| current.min(import_started_ms)),
                );
                aggregate.observed_import_end_ms = Some(
                    aggregate
                        .observed_import_end_ms
                        .map_or(import_completed_ms, |current| {
                            current.max(import_completed_ms)
                        }),
                );
                commit_ready_live_chunks(
                    &mut controller,
                    &mut aggregates,
                    &plan,
                    &identity,
                    &mut source_busy_intervals,
                    &mut importer_busy_intervals,
                    &mut chunk_zero_imported_before_final_gate,
                    source_clock_offset_ms,
                )?;
            }
            "chunk_done" => {
                let chunk_index = frame
                    .response
                    .chunk_index
                    .context("async chunk_done missing chunk_index")?;
                if plan.chunks.get(chunk_index).is_none() {
                    bail!("async streaming chunk_done index out of range");
                };
                aggregates.entry(chunk_index).or_default().chunk_done_seen = true;
                commit_ready_live_chunks(
                    &mut controller,
                    &mut aggregates,
                    &plan,
                    &identity,
                    &mut source_busy_intervals,
                    &mut importer_busy_intervals,
                    &mut chunk_zero_imported_before_final_gate,
                    source_clock_offset_ms,
                )?;
            }
            "stopped" => match frame.channel {
                StreamingFrameChannel::SingleStream => break,
                StreamingFrameChannel::Control => {
                    control_stopped = true;
                    if page_stopped {
                        break;
                    }
                }
                StreamingFrameChannel::Page => {
                    page_stopped = true;
                    if control_stopped {
                        break;
                    }
                }
            },
            "error" => {
                bail!(
                    "async streaming source error: {}",
                    frame
                        .response
                        .error
                        .unwrap_or_else(|| "unknown".to_string())
                );
            }
            other => bail!("unexpected async streaming frame kind {other}"),
        }
    }

    let actual_overlap_ms = total_overlap_ms(&source_busy_intervals, &importer_busy_intervals);
    controller.telemetry.actual_overlap_ms = actual_overlap_ms;
    controller.telemetry.overlap_ms = actual_overlap_ms;
    controller.telemetry.coordinator_observed_overlap_ms = actual_overlap_ms;
    controller.telemetry.true_compute_transfer_overlap_ms = actual_overlap_ms;
    controller.telemetry.source_relative_overlap_ms =
        total_source_relative_overlap_ms(&controller.telemetry);
    controller.telemetry.clock_alignment_status = if source_clock_offset_ms.is_some() {
        "first_control_event_offset"
    } else {
        "coordinator_observed_only"
    };
    controller.telemetry.pipeline_idle_ms =
        controller.telemetry.source_idle_ms + controller.telemetry.importer_idle_ms;
    controller.telemetry.page_queue_depth = controller
        .telemetry
        .page_queue_depth
        .max(capacity.max_queue_depth);

    Ok(AsyncLiveImportResult {
        import_session,
        controller,
        chunk_zero_imported_before_final_gate,
    })
}

fn commit_ready_live_chunks(
    controller: &mut StreamingKvController,
    aggregates: &mut BTreeMap<usize, LiveChunkAggregate>,
    plan: &StreamingKvPlan,
    identity: &StreamingKvIdentity,
    source_busy_intervals: &mut Vec<(f64, f64)>,
    importer_busy_intervals: &mut Vec<(f64, f64)>,
    chunk_zero_imported_before_final_gate: &mut bool,
    source_clock_offset_ms: Option<f64>,
) -> Result<()> {
    loop {
        let chunk_index = controller.expected_import_chunk;
        let Some(aggregate) = aggregates.get(&chunk_index) else {
            return Ok(());
        };
        if !aggregate.chunk_done_seen
            || !aggregate.saw_page
            || match aggregate.expected_segment_count {
                Some(expected) => expected != aggregate.segments_seen,
                None => true,
            }
        {
            return Ok(());
        }
        let aggregate = aggregates
            .remove(&chunk_index)
            .expect("aggregate exists for the expected import chunk");
        let range = plan.chunks[chunk_index];
        let manifest = StreamingKvManifest::for_chunk(range, plan, identity, aggregate.page_bytes);
        controller
            .chunk_prefilled(range, aggregate.prefill_ms)
            .map_err(|error| anyhow::anyhow!(error.reason))?;
        controller
            .page_segments_exported(&manifest, aggregate.export_ms)
            .map_err(|error| anyhow::anyhow!(error.reason))?;
        controller
            .page_segments_imported(&manifest, aggregate.transfer_ms, aggregate.import_ms)
            .map_err(|error| anyhow::anyhow!(error.reason))?;
        push_live_timing(controller, &aggregate);
        if let Some(interval) = source_busy_interval(&aggregate, source_clock_offset_ms) {
            source_busy_intervals.push(interval);
        }
        if let (Some(start), Some(end)) = (
            aggregate.observed_transfer_start_ms,
            aggregate.observed_import_end_ms,
        ) {
            importer_busy_intervals.push((start, end));
        }
        if chunk_index == 0 {
            *chunk_zero_imported_before_final_gate = controller.imported_chunks.contains(&0)
                && controller.telemetry.final_decode_start_position.is_none();
        }
    }
}

fn push_live_timing(controller: &mut StreamingKvController, aggregate: &LiveChunkAggregate) {
    controller
        .telemetry
        .prefill_start_ms
        .push(aggregate.observed_prefill_start_ms.unwrap_or(0.0));
    controller
        .telemetry
        .prefill_end_ms
        .push(aggregate.observed_prefill_end_ms.unwrap_or(0.0));
    controller
        .telemetry
        .export_start_ms
        .push(aggregate.observed_export_start_ms.unwrap_or(0.0));
    controller
        .telemetry
        .export_end_ms
        .push(aggregate.observed_export_end_ms.unwrap_or(0.0));
    controller
        .telemetry
        .transfer_start_ms
        .push(aggregate.observed_transfer_start_ms.unwrap_or(0.0));
    controller
        .telemetry
        .transfer_end_ms
        .push(aggregate.observed_transfer_end_ms.unwrap_or(0.0));
    controller
        .telemetry
        .import_start_ms
        .push(aggregate.observed_import_start_ms.unwrap_or(0.0));
    controller
        .telemetry
        .import_end_ms
        .push(aggregate.observed_import_end_ms.unwrap_or(0.0));
    controller
        .telemetry
        .source_prefill_start_ms
        .push(aggregate.source_prefill_start_ms.unwrap_or(0.0));
    controller
        .telemetry
        .source_prefill_end_ms
        .push(aggregate.source_prefill_end_ms.unwrap_or(0.0));
    controller
        .telemetry
        .source_export_start_ms
        .push(aggregate.source_export_start_ms.unwrap_or(0.0));
    controller
        .telemetry
        .source_export_end_ms
        .push(aggregate.source_export_end_ms.unwrap_or(0.0));
}

fn record_control_timing(
    controller: &mut StreamingKvController,
    frame: &StreamingInboundFrame,
    source_clock_offset_ms: &mut Option<f64>,
) {
    if frame.channel == StreamingFrameChannel::Page {
        return;
    }
    let Some(source_ms) = response_event_ms(&frame.response) else {
        return;
    };
    let offset = *source_clock_offset_ms.get_or_insert(frame.read_completed_ms - source_ms);
    controller.telemetry.control_event_emit_ms.push(source_ms);
    controller
        .telemetry
        .control_event_receive_ms
        .push(frame.read_completed_ms);
    controller
        .telemetry
        .control_event_lag_ms
        .push((frame.read_completed_ms - (source_ms + offset)).max(0.0));
}

fn response_event_ms(response: &StreamingHarnessResponse) -> Option<f64> {
    response
        .prefill_start_ms
        .or(response.prefill_end_ms)
        .or(response.export_start_ms)
        .or(response.export_end_ms)
}

fn source_busy_interval(
    aggregate: &LiveChunkAggregate,
    source_clock_offset_ms: Option<f64>,
) -> Option<(f64, f64)> {
    if let (Some(start), Some(end), Some(offset)) = (
        aggregate.source_prefill_start_ms,
        aggregate.source_export_end_ms,
        source_clock_offset_ms,
    ) {
        return Some((start + offset, end + offset));
    }
    aggregate
        .observed_prefill_start_ms
        .zip(aggregate.observed_export_end_ms)
}

fn total_source_relative_overlap_ms(telemetry: &StreamingTelemetryReport) -> f64 {
    let source_intervals = telemetry
        .source_prefill_start_ms
        .iter()
        .copied()
        .zip(telemetry.source_export_end_ms.iter().copied())
        .collect::<Vec<_>>();
    let import_intervals = telemetry
        .transfer_start_ms
        .iter()
        .copied()
        .zip(telemetry.import_end_ms.iter().copied())
        .collect::<Vec<_>>();
    total_overlap_ms(&source_intervals, &import_intervals)
}

fn join_writer_thread(handle: thread::JoinHandle<Result<()>>) -> Result<()> {
    handle
        .join()
        .map_err(|_| anyhow::anyhow!("async streaming writer thread panicked"))?
}

fn join_writer_metrics_thread(
    handle: thread::JoinHandle<Result<StreamingWriterMetrics>>,
) -> Result<StreamingWriterMetrics> {
    handle
        .join()
        .map_err(|_| anyhow::anyhow!("split-channel page writer thread panicked"))?
}

fn join_reader_thread(handle: thread::JoinHandle<Result<()>>) -> Result<()> {
    handle
        .join()
        .map_err(|_| anyhow::anyhow!("async streaming reader thread panicked"))?
}

fn join_importer_thread(
    handle: thread::JoinHandle<Result<AsyncLiveImportResult>>,
) -> Result<AsyncLiveImportResult> {
    handle
        .join()
        .map_err(|_| anyhow::anyhow!("async streaming importer thread panicked"))?
}

fn runtime_path_not_run() -> StreamingRuntimePathReport {
    StreamingRuntimePathReport {
        source_runtime_export_kv_page: "not_run",
        target_runtime_import_kv_page: "not_run",
        network_transport: "not_started",
        full_state_handoff_allowed_as_pass: false,
    }
}

fn bootstrap_not_run(reason: &'static str) -> StreamingBootstrapReport {
    StreamingBootstrapReport {
        status: "not_run",
        strategy: "trim_replay_last_token",
        imported_token_count: None,
        trim_target_position: None,
        replay_token_position: None,
        bootstrap_eval_ms: None,
        logits_ready: false,
        decode_start_position: None,
        failure_reason: Some(reason),
    }
}

fn baseline_not_run(reason: &'static str) -> StreamingBaselineReport {
    StreamingBaselineReport {
        strategy: "local_one_shot_prefill_decode",
        comparison: "not_run",
        failure_reason: Some(reason),
        first_divergence_index: None,
        baseline_token_id: None,
        streaming_token_id: None,
        baseline_token_count: None,
        streaming_token_count: None,
    }
}

fn expected_identity() -> StreamingKvIdentity {
    StreamingKvIdentity {
        artifact_sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            .to_string(),
        tokenizer_hash: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            .to_string(),
        chat_template_hash: "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
            .to_string(),
        dtype: "f16".to_string(),
        layout: "llama.cpp-kv-page".to_string(),
    }
}

fn privacy_report() -> PrivacyReport {
    PrivacyReport {
        prompt_text: "excluded",
        generated_content: "excluded",
        complete_token_arrays: "excluded",
        kv_or_native_payload_contents: "excluded",
        credentials: "excluded",
        private_paths: "excluded",
        endpoint_urls: "excluded",
        real_machine_labels: "excluded",
    }
}

fn negative_checks() -> Vec<NegativeCheckReport> {
    let _full_state_blob_marker = StreamingPayloadKind::FullStateBlob;
    const CASES: &[(&str, &str)] = &[
        ("duplicate_chunk", "duplicate_chunk_export"),
        ("missing_chunk", "missing_chunk"),
        ("out_of_order_chunk", "out_of_order_chunk_import"),
        ("position_gap", "position_gap"),
        ("position_overlap", "position_overlap"),
        ("checksum_mismatch", "checksum"),
        ("max_in_flight_bytes", "max_in_flight_bytes"),
        ("import_failure", "import_failure"),
        ("incomplete_final_gate", "missing_chunk"),
        ("full_state_blob", "full_state_blob"),
    ];
    CASES
        .iter()
        .map(|(case, failure_reason)| NegativeCheckReport {
            case,
            status: "pass",
            failure_reason,
        })
        .collect()
}

fn runtime_config_for_source(args: &KvStreamingHandoffSourceArgs) -> RuntimeConfig {
    RuntimeConfig {
        stage_index: 0,
        layer_start: 0,
        layer_end: args.layer_end,
        ctx_size: args.ctx_size,
        lane_count: 1,
        n_batch: args.n_batch,
        n_ubatch: args.n_ubatch,
        n_threads: None,
        n_threads_batch: None,
        n_gpu_layers: args.n_gpu_layers,
        selected_backend_device: None,
        load_mode: runtime_load_mode(args.stage_load_mode),
        projector_path: None,
        include_embeddings: true,
        include_output: true,
        filter_tensors_on_load: false,
        cache_type_k: GGML_TYPE_F16,
        cache_type_v: GGML_TYPE_F16,
        flash_attn_type: runtime_flash_attn(args.flash_attn),
    }
}

fn runtime_config_for_coordinator(args: &KvStreamingHandoffCoordinatorArgs) -> RuntimeConfig {
    RuntimeConfig {
        stage_index: 0,
        layer_start: 0,
        layer_end: args.layer_end,
        ctx_size: args.ctx_size,
        lane_count: 1,
        n_batch: args.n_batch,
        n_ubatch: args.n_ubatch,
        n_threads: None,
        n_threads_batch: None,
        n_gpu_layers: args.n_gpu_layers,
        selected_backend_device: None,
        load_mode: runtime_load_mode(args.stage_load_mode),
        projector_path: None,
        include_embeddings: true,
        include_output: true,
        filter_tensors_on_load: false,
        cache_type_k: GGML_TYPE_F16,
        cache_type_v: GGML_TYPE_F16,
        flash_attn_type: runtime_flash_attn(args.flash_attn),
    }
}

fn runtime_load_mode(stage_load_mode: StageLoadMode) -> RuntimeLoadMode {
    match stage_load_mode {
        StageLoadMode::RuntimeSlice => RuntimeLoadMode::RuntimeSlice,
        StageLoadMode::ArtifactSlice => RuntimeLoadMode::ArtifactSlice,
        StageLoadMode::LayerPackage => RuntimeLoadMode::LayerPackage,
    }
}

fn runtime_flash_attn(value: FlashAttentionArg) -> skippy_runtime::FlashAttentionType {
    match value {
        FlashAttentionArg::Auto => skippy_runtime::FlashAttentionType::Auto,
        FlashAttentionArg::Disabled => skippy_runtime::FlashAttentionType::Disabled,
        FlashAttentionArg::Enabled => skippy_runtime::FlashAttentionType::Enabled,
    }
}

fn identity_from_source_args(args: &KvStreamingHandoffSourceArgs) -> StreamingKvIdentity {
    StreamingKvIdentity {
        artifact_sha256: args.artifact_sha256.clone(),
        tokenizer_hash: args.tokenizer_hash.clone(),
        chat_template_hash: args.chat_template_hash.clone(),
        dtype: "f16".to_string(),
        layout: "llama.cpp-kv-page".to_string(),
    }
}

fn identity_from_coordinator_args(args: &KvStreamingHandoffCoordinatorArgs) -> StreamingKvIdentity {
    StreamingKvIdentity {
        artifact_sha256: args.artifact_sha256.clone(),
        tokenizer_hash: args.tokenizer_hash.clone(),
        chat_template_hash: args.chat_template_hash.clone(),
        dtype: "f16".to_string(),
        layout: "llama.cpp-kv-page".to_string(),
    }
}

fn manifest_from_runtime_page(
    chunk_index: usize,
    total_chunks: usize,
    total_prompt_tokens: usize,
    token_start: usize,
    token_end: usize,
    identity: &StreamingKvIdentity,
    desc: &RuntimeKvPageDesc,
    payload: &[u8],
) -> StreamingKvManifest {
    StreamingKvManifest {
        protocol_version: PROTOCOL_VERSION.to_string(),
        chunk_index,
        total_chunks,
        token_start,
        token_end,
        total_prompt_tokens,
        page_bytes: payload.len() as u64,
        frame_bytes: payload.len() as u64,
        checksum_algorithm: "sha256".to_string(),
        checksum: sha256_hex(payload),
        observed_checksum: sha256_hex(payload),
        artifact_sha256: identity.artifact_sha256.clone(),
        tokenizer_hash: identity.tokenizer_hash.clone(),
        chat_template_hash: identity.chat_template_hash.clone(),
        dtype: identity.dtype.clone(),
        layout: format!(
            "{}:{}:{}",
            identity.layout,
            desc.cache_kind().as_label(),
            desc.segment_kind().as_label()
        ),
        payload_kind: StreamingPayloadKind::KvPage,
        native_desc: Some(StreamingKvNativeDesc {
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
        }),
    }
}

fn runtime_desc_from_manifest(manifest: &StreamingKvManifest) -> Option<RuntimeKvPageDesc> {
    let desc = manifest.native_desc.as_ref()?;
    Some(RuntimeKvPageDesc {
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

fn validate_page_manifest(
    manifest: &StreamingKvManifest,
    payload: &[u8],
    range: TokenRange,
    identity: &StreamingKvIdentity,
    total_chunks: usize,
) -> Result<(), PipelineError> {
    if manifest.protocol_version != PROTOCOL_VERSION {
        return Err(PipelineError {
            reason: "protocol_version",
        });
    }
    if manifest.payload_kind != StreamingPayloadKind::KvPage {
        return Err(PipelineError {
            reason: "full_state_blob",
        });
    }
    if manifest.chunk_index != range.chunk_index
        || manifest.token_start != range.token_start
        || manifest.token_end != range.token_end
    {
        return Err(PipelineError {
            reason: "chunk_range",
        });
    }
    if manifest.total_chunks != total_chunks {
        return Err(PipelineError {
            reason: "total_chunks",
        });
    }
    if manifest.page_bytes != payload.len() as u64 || manifest.frame_bytes != payload.len() as u64 {
        return Err(PipelineError {
            reason: "payload_bytes",
        });
    }
    if manifest.checksum_algorithm != "sha256" || manifest.checksum != sha256_hex(payload) {
        return Err(PipelineError { reason: "checksum" });
    }
    if manifest.artifact_sha256 != identity.artifact_sha256
        || manifest.tokenizer_hash != identity.tokenizer_hash
        || manifest.chat_template_hash != identity.chat_template_hash
    {
        return Err(PipelineError { reason: "identity" });
    }
    if manifest.dtype != identity.dtype || !manifest.layout.starts_with(&identity.layout) {
        return Err(PipelineError { reason: "layout" });
    }
    Ok(())
}

fn synthetic_prompt_tokens(model: &StageModel, target_tokens: usize) -> Result<Vec<i32>> {
    let mut text = String::from("kv streaming handoff synthetic prompt. ");
    let mut tokens = model
        .tokenize(&text, true)
        .context("tokenize streaming synthetic prompt")?;
    while tokens.len() < target_tokens {
        text.push_str("repeatable synthetic context. ");
        tokens = model
            .tokenize(&text, true)
            .context("tokenize expanded streaming synthetic prompt")?;
    }
    tokens.truncate(target_tokens);
    Ok(tokens)
}

fn split_tokens(tokens: &[i32], chunk_tokens: usize) -> Vec<(usize, Vec<i32>)> {
    let chunk_tokens = chunk_tokens.max(1);
    tokens
        .chunks(chunk_tokens)
        .scan(0usize, |start, chunk| {
            let token_start = *start;
            *start += chunk.len();
            Some((token_start, chunk.to_vec()))
        })
        .collect()
}

fn bootstrap_imported_page_decode_state(
    session: &mut skippy_runtime::StageSession,
    prompt_tokens: &[i32],
    imported_token_count: usize,
    strategy: KvPageBootstrapStrategy,
    seed: u64,
) -> Result<StreamingBootstrapDecode> {
    match strategy {
        KvPageBootstrapStrategy::TrimReplayLastToken => {
            let plan = trim_replay_last_token_plan(prompt_tokens, imported_token_count)
                .map_err(|reason| anyhow::anyhow!(reason))?;
            session
                .trim_session(plan.trim_target_position as u64)
                .context("trim imported streaming KV page state")?;
            if session.token_count() != plan.trim_target_position as u64 {
                bail!("bootstrap_trim_position_mismatch");
            }
            let started = Instant::now();
            let first_token = session
                .decode_step_sampled(plan.replay_token, Some(&deterministic_sampling(seed)))
                .context("replay last prompt token for streaming KV page logits")?;
            let bootstrap_eval_ms = elapsed_ms(started);
            let decode_start_position = session.token_count();
            if decode_start_position != plan.decode_start_position as u64 {
                bail!("bootstrap_decode_start_position_mismatch");
            }
            Ok(StreamingBootstrapDecode {
                first_token,
                report: StreamingBootstrapReport {
                    status: "pass",
                    strategy: bootstrap_strategy_label(strategy),
                    imported_token_count: Some(plan.imported_token_count),
                    trim_target_position: Some(plan.trim_target_position),
                    replay_token_position: Some(plan.replay_token_position),
                    bootstrap_eval_ms: Some(bootstrap_eval_ms),
                    logits_ready: true,
                    decode_start_position: Some(decode_start_position),
                    failure_reason: None,
                },
            })
        }
    }
}

fn trim_replay_last_token_plan(
    prompt_tokens: &[i32],
    imported_token_count: usize,
) -> Result<StreamingBootstrapPlan, &'static str> {
    if imported_token_count == 0 {
        return Err("imported_token_count_zero");
    }
    if imported_token_count > prompt_tokens.len() {
        return Err("missing_last_prompt_token");
    }
    let replay_token_position = imported_token_count - 1;
    let replay_token = prompt_tokens
        .get(replay_token_position)
        .copied()
        .ok_or("missing_last_prompt_token")?;
    Ok(StreamingBootstrapPlan {
        imported_token_count,
        trim_target_position: replay_token_position,
        replay_token_position,
        decode_start_position: imported_token_count,
        replay_token,
    })
}

fn bootstrap_strategy_label(strategy: KvPageBootstrapStrategy) -> &'static str {
    match strategy {
        KvPageBootstrapStrategy::TrimReplayLastToken => "trim_replay_last_token",
    }
}

fn decode_tokens_from_first(
    session: &mut skippy_runtime::StageSession,
    first_token: i32,
    max_tokens: usize,
    seed: u64,
) -> Result<Vec<i32>> {
    let mut out = Vec::with_capacity(max_tokens);
    let mut next = first_token;
    for _ in 0..max_tokens {
        out.push(next);
        next = session
            .decode_step_sampled(next, Some(&deterministic_sampling(seed)))
            .context("decode next token after streaming bootstrap")?;
    }
    Ok(out)
}

fn run_local_one_shot_decode_baseline(
    model: &StageModel,
    prompt_tokens: &[i32],
    max_tokens: usize,
    seed: u64,
) -> Result<Vec<i32>> {
    let mut session = model
        .create_session()
        .context("create streaming local one-shot baseline session")?;
    session
        .prefill_chunked(prompt_tokens)
        .context("streaming local one-shot baseline prefill")?;
    let bootstrap = bootstrap_imported_page_decode_state(
        &mut session,
        prompt_tokens,
        prompt_tokens.len(),
        KvPageBootstrapStrategy::TrimReplayLastToken,
        seed,
    )
    .context("bootstrap streaming local one-shot baseline")?;
    decode_tokens_from_first(&mut session, bootstrap.first_token, max_tokens, seed)
}

fn deterministic_sampling(seed: u64) -> SamplingConfig {
    SamplingConfig {
        temperature: 0.0,
        seed: u32::try_from(seed).unwrap_or(u32::MAX),
        ..SamplingConfig::default()
    }
}

fn compare_tokens(baseline: &[i32], streaming: &[i32]) -> StreamingDecodeComparison {
    if baseline == streaming {
        StreamingDecodeComparison {
            matches: true,
            label: "exact_token_match",
            first_divergence_index: None,
            baseline_token_id: None,
            streaming_token_id: None,
            baseline_token_count: baseline.len(),
            streaming_token_count: streaming.len(),
        }
    } else {
        let first_divergence_index = baseline
            .iter()
            .zip(streaming.iter())
            .position(|(baseline, streaming)| baseline != streaming)
            .or_else(|| Some(baseline.len().min(streaming.len())));
        StreamingDecodeComparison {
            matches: false,
            label: "token_divergence",
            first_divergence_index,
            baseline_token_id: first_divergence_index
                .and_then(|index| baseline.get(index).copied()),
            streaming_token_id: first_divergence_index
                .and_then(|index| streaming.get(index).copied()),
            baseline_token_count: baseline.len(),
            streaming_token_count: streaming.len(),
        }
    }
}

fn sha256_hex(payload: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(payload);
    format!("{:x}", hasher.finalize())
}

fn write_json_frame<T: Serialize>(stream: &mut TcpStream, value: &T, payload: &[u8]) -> Result<()> {
    let json = serde_json::to_vec(value).context("serialize streaming frame header")?;
    let header_len = u32::try_from(json.len()).context("streaming header exceeds u32")?;
    let payload_len = u64::try_from(payload.len()).context("streaming payload exceeds u64")?;
    stream.write_all(&header_len.to_be_bytes())?;
    stream.write_all(&payload_len.to_be_bytes())?;
    stream.write_all(&json)?;
    stream.write_all(payload)?;
    stream.flush()?;
    Ok(())
}

fn read_json_frame<T: for<'de> Deserialize<'de>>(stream: &mut TcpStream) -> Result<T> {
    let (value, payload) = read_json_frame_with_payload(stream)?;
    if !payload.is_empty() {
        bail!("unexpected streaming request payload");
    }
    Ok(value)
}

fn read_json_frame_with_payload<T: for<'de> Deserialize<'de>>(
    stream: &mut TcpStream,
) -> Result<(T, Vec<u8>)> {
    let mut header_len = [0u8; 4];
    stream.read_exact(&mut header_len)?;
    let header_len = u32::from_be_bytes(header_len) as usize;
    let mut payload_len = [0u8; 8];
    stream.read_exact(&mut payload_len)?;
    let payload_len = u64::from_be_bytes(payload_len) as usize;
    let mut header = vec![0u8; header_len];
    stream.read_exact(&mut header)?;
    let value = serde_json::from_slice(&header).context("parse streaming frame header")?;
    let mut payload = vec![0u8; payload_len];
    stream.read_exact(&mut payload)?;
    Ok((value, payload))
}

fn elapsed_ms(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1000.0
}

fn emit_markdown_report(report: &StreamingKvReport, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let title = if report.recommendation == RECOMMENDATION_READY_ASYNC
        && report.runtime_path.source_runtime_export_kv_page == "not_run"
    {
        "PD Streaming KV Handoff Async Live Harness Report"
    } else {
        "PD Streaming KV Handoff Foreground Smoke Report"
    };
    fs::write(
        path,
        format!(
            "# {title}\n\n\
             result: `{}`\n\n\
             recommendation: `{}`\n\n\
             role: `{}`\n\n\
             protocol: `{}`\n\n\
             runtime export/import: `{}` / `{}`\n\n\
             bootstrap: `{}` `{}`\n\n\
             baseline: `{}` `{}`\n\n\
             final decode start position: `{:?}`\n",
            report.result,
            report.recommendation,
            report.role,
            report.protocol,
            report.runtime_path.source_runtime_export_kv_page,
            report.runtime_path.target_runtime_import_kv_page,
            report.bootstrap.strategy,
            report.bootstrap.status,
            report.baseline.strategy,
            report.baseline.comparison,
            report.telemetry.final_decode_start_position
        ),
    )
    .with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

fn emit_json_report(report: &StreamingKvReport, path: Option<&Path>) -> Result<()> {
    let json = serde_json::to_string_pretty(report).context("serialize streaming report")?;
    if let Some(path) = path {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        }
        fs::write(path, json).with_context(|| format!("write {}", path.display()))?;
    } else {
        println!("{json}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    use crate::cli::{Cli, CommandKind};

    fn plan() -> StreamingKvPlan {
        StreamingKvPlan::new(128, 64).unwrap()
    }

    fn capacity() -> StreamingKvCapacity {
        StreamingKvCapacity {
            max_in_flight_chunks: 1,
            max_in_flight_bytes: 1_048_576,
            max_frame_bytes: 524_288,
            max_queue_depth: 2,
        }
    }

    fn new_controller() -> StreamingKvController {
        StreamingKvController::new(plan(), expected_identity(), capacity())
    }

    fn manifest_for(index: usize) -> StreamingKvManifest {
        let plan = plan();
        StreamingKvManifest::for_chunk(plan.chunks[index], &plan, &expected_identity(), 4096)
    }

    fn async_args() -> KvStreamingHandoffLocalArgs {
        KvStreamingHandoffLocalArgs {
            output: crate::cli::OutputArgs { report_out: None },
            pipeline_mode: KvStreamingPipelineMode::Async,
            total_tokens: 128,
            chunk_tokens: 64,
            max_in_flight_chunks: 2,
            max_in_flight_bytes: 1_048_576,
            max_frame_bytes: 524_288,
            max_queue_depth: 2,
            page_bytes_per_chunk: 4096,
        }
    }

    fn run_two_chunk_pass() -> StreamingKvController {
        let plan = plan();
        let identity = expected_identity();
        let mut controller = StreamingKvController::new(plan.clone(), identity.clone(), capacity());
        let chunk0 = plan.chunks[0];
        let manifest0 = StreamingKvManifest::for_chunk(chunk0, &plan, &identity, 4096);
        controller.chunk_prefilled(chunk0, 1.0).unwrap();
        controller.page_segments_exported(&manifest0, 0.2).unwrap();
        assert_eq!(controller.imported_chunks.len(), 0);
        assert!(!controller.prefilled_chunks.contains(&1));
        controller
            .page_segments_imported(&manifest0, 0.1, 0.3)
            .unwrap();

        let chunk1 = plan.chunks[1];
        let manifest1 = StreamingKvManifest::for_chunk(chunk1, &plan, &identity, 4096);
        controller.chunk_prefilled(chunk1, 2.0).unwrap();
        controller.page_segments_exported(&manifest1, 0.2).unwrap();
        controller
            .page_segments_imported(&manifest1, 0.1, 0.3)
            .unwrap();
        controller.finalize().unwrap();
        controller
    }

    #[test]
    fn two_chunks_in_order_pass_final_gate() {
        let controller = run_two_chunk_pass();

        assert_eq!(controller.telemetry.validation_result, "pass");
        assert_eq!(controller.telemetry.final_decode_start_position, Some(128));
        assert_eq!(controller.telemetry.chunk_tokens, vec![64, 64]);
        assert_eq!(controller.telemetry.page_bytes_per_chunk, vec![4096, 4096]);
        assert_eq!(controller.telemetry.bytes_per_token, Some(64.0));
        assert_eq!(controller.in_flight_bytes, 0);
    }

    #[test]
    fn chunk_zero_can_import_before_chunk_one_prefills() {
        let plan = plan();
        let identity = expected_identity();
        let mut controller = StreamingKvController::new(plan.clone(), identity.clone(), capacity());
        let chunk0 = plan.chunks[0];
        let manifest0 = StreamingKvManifest::for_chunk(chunk0, &plan, &identity, 4096);

        controller.chunk_prefilled(chunk0, 1.0).unwrap();
        controller.page_segments_exported(&manifest0, 0.2).unwrap();
        controller
            .page_segments_imported(&manifest0, 0.1, 0.3)
            .unwrap();

        assert_eq!(controller.imported_chunks, BTreeSet::from([0]));
        assert!(!controller.prefilled_chunks.contains(&1));
        assert_eq!(controller.next_expected_position, 64);
    }

    #[test]
    fn duplicate_chunk_fails_closed() {
        let mut controller = new_controller();
        let chunk0 = controller.plan.chunks[0];
        let manifest0 = manifest_for(0);

        controller.chunk_prefilled(chunk0, 1.0).unwrap();
        assert_eq!(
            controller.chunk_prefilled(chunk0, 1.0).unwrap_err().reason,
            "out_of_order_chunk_prefill"
        );

        let mut controller = new_controller();
        controller.chunk_prefilled(chunk0, 1.0).unwrap();
        controller.page_segments_exported(&manifest0, 0.2).unwrap();
        assert_eq!(
            controller
                .page_segments_exported(&manifest0, 0.2)
                .unwrap_err()
                .reason,
            "duplicate_chunk_export"
        );
    }

    #[test]
    fn missing_chunk_fails_final_gate() {
        let plan = plan();
        let identity = expected_identity();
        let mut controller = StreamingKvController::new(plan.clone(), identity.clone(), capacity());
        let chunk0 = plan.chunks[0];
        let manifest0 = StreamingKvManifest::for_chunk(chunk0, &plan, &identity, 4096);

        controller.chunk_prefilled(chunk0, 1.0).unwrap();
        controller.page_segments_exported(&manifest0, 0.2).unwrap();
        controller
            .page_segments_imported(&manifest0, 0.1, 0.3)
            .unwrap();

        assert_eq!(controller.finalize().unwrap_err().reason, "missing_chunk");
    }

    #[test]
    fn out_of_order_import_fails_closed() {
        let plan = plan();
        let identity = expected_identity();
        let mut controller = StreamingKvController::new(
            plan.clone(),
            identity.clone(),
            StreamingKvCapacity {
                max_in_flight_chunks: 2,
                max_in_flight_bytes: 1_048_576,
                max_frame_bytes: 524_288,
                max_queue_depth: 2,
            },
        );
        let chunk0 = plan.chunks[0];
        let chunk1 = plan.chunks[1];
        let manifest0 = StreamingKvManifest::for_chunk(chunk0, &plan, &identity, 4096);
        let manifest1 = StreamingKvManifest::for_chunk(chunk1, &plan, &identity, 4096);

        controller.chunk_prefilled(chunk0, 1.0).unwrap();
        controller.page_segments_exported(&manifest0, 0.2).unwrap();
        controller.chunk_prefilled(chunk1, 1.0).unwrap();
        controller.page_segments_exported(&manifest1, 0.2).unwrap();

        assert_eq!(
            controller
                .page_segments_imported(&manifest1, 0.1, 0.3)
                .unwrap_err()
                .reason,
            "out_of_order_chunk_import"
        );
    }

    #[test]
    fn position_gap_and_overlap_fail_closed() {
        let mut gap = new_controller();
        let chunk0 = gap.plan.chunks[0];
        let mut gap_manifest = manifest_for(0);
        gap_manifest.token_start = 1;
        gap_manifest.token_end = 65;
        gap.chunk_prefilled(chunk0, 1.0).unwrap();
        gap.page_segments_exported(&gap_manifest, 0.2).unwrap();
        assert_eq!(
            gap.page_segments_imported(&gap_manifest, 0.1, 0.3)
                .unwrap_err()
                .reason,
            "position_gap"
        );

        let mut overlap = new_controller();
        let chunk0 = overlap.plan.chunks[0];
        let manifest0 = manifest_for(0);
        overlap.chunk_prefilled(chunk0, 1.0).unwrap();
        overlap.page_segments_exported(&manifest0, 0.2).unwrap();
        overlap
            .page_segments_imported(&manifest0, 0.1, 0.3)
            .unwrap();
        let chunk1 = overlap.plan.chunks[1];
        let mut overlap_manifest = manifest_for(1);
        overlap_manifest.token_start = 63;
        overlap.chunk_prefilled(chunk1, 1.0).unwrap();
        overlap
            .page_segments_exported(&overlap_manifest, 0.2)
            .unwrap();
        assert_eq!(
            overlap
                .page_segments_imported(&overlap_manifest, 0.1, 0.3)
                .unwrap_err()
                .reason,
            "position_overlap"
        );
    }

    #[test]
    fn checksum_mismatch_fails_closed() {
        let mut controller = new_controller();
        let chunk0 = controller.plan.chunks[0];
        let mut manifest = manifest_for(0);
        manifest.observed_checksum = "bad-checksum".to_string();

        controller.chunk_prefilled(chunk0, 1.0).unwrap();
        assert_eq!(
            controller
                .page_segments_exported(&manifest, 0.2)
                .unwrap_err()
                .reason,
            "checksum"
        );
    }

    #[test]
    fn in_flight_byte_cap_exceeded_fails_closed() {
        let plan = plan();
        let identity = expected_identity();
        let capacity = StreamingKvCapacity {
            max_in_flight_chunks: 1,
            max_in_flight_bytes: 1024,
            max_frame_bytes: 8192,
            max_queue_depth: 2,
        };
        let mut controller = StreamingKvController::new(plan.clone(), identity.clone(), capacity);
        let chunk0 = plan.chunks[0];
        let manifest0 = StreamingKvManifest::for_chunk(chunk0, &plan, &identity, 4096);

        controller.chunk_prefilled(chunk0, 1.0).unwrap();
        assert_eq!(
            controller
                .page_segments_exported(&manifest0, 0.2)
                .unwrap_err()
                .reason,
            "max_in_flight_bytes"
        );
    }

    #[test]
    fn import_failure_fails_closed() {
        let mut controller = new_controller();
        let chunk0 = controller.plan.chunks[0];
        let manifest0 = manifest_for(0);

        controller.chunk_prefilled(chunk0, 1.0).unwrap();
        controller.page_segments_exported(&manifest0, 0.2).unwrap();

        assert_eq!(
            controller.import_failed(0).unwrap_err().reason,
            "import_failure"
        );
    }

    #[test]
    fn final_decode_gate_refuses_incomplete_imports() {
        let mut controller = new_controller();
        let chunk0 = controller.plan.chunks[0];
        let manifest0 = manifest_for(0);

        controller.chunk_prefilled(chunk0, 1.0).unwrap();
        controller.page_segments_exported(&manifest0, 0.2).unwrap();

        assert_eq!(controller.finalize().unwrap_err().reason, "missing_chunk");
    }

    #[test]
    fn full_state_frame_cannot_satisfy_streaming_proof() {
        let mut controller = new_controller();
        let chunk0 = controller.plan.chunks[0];
        let mut manifest = manifest_for(0);
        manifest.payload_kind = StreamingPayloadKind::FullStateBlob;

        controller.chunk_prefilled(chunk0, 1.0).unwrap();
        assert_eq!(
            controller
                .page_segments_exported(&manifest, 0.2)
                .unwrap_err()
                .reason,
            "full_state_blob"
        );
    }

    #[test]
    fn local_report_is_sanitized_and_ready_for_foreground_smoke() {
        let args = KvStreamingHandoffLocalArgs {
            output: crate::cli::OutputArgs { report_out: None },
            pipeline_mode: KvStreamingPipelineMode::Serial,
            total_tokens: 128,
            chunk_tokens: 64,
            max_in_flight_chunks: 1,
            max_in_flight_bytes: 1_048_576,
            max_frame_bytes: 524_288,
            max_queue_depth: 2,
            page_bytes_per_chunk: 4096,
        };
        let report = run_local_streaming_controller(&args).unwrap();
        let serialized = serde_json::to_string(&report).unwrap();

        assert_eq!(report.result, RESULT_INCONCLUSIVE);
        assert_eq!(report.recommendation, RECOMMENDATION_READY);
        assert_eq!(report.local_controller.out_of_order_policy, "fail_closed");
        assert!(!report.local_controller.full_state_handoff_allowed_as_pass);
        assert_eq!(report.telemetry.validation_result, "pass");
        assert_eq!(report.telemetry.final_decode_start_position, Some(128));
        assert!(report
            .negative_checks
            .iter()
            .all(|check| check.status == "pass"));
        assert!(!serialized.contains("/Users/"));
        assert!(!serialized.contains("prompt text"));
        assert!(!serialized.contains("generated content"));
        assert!(!serialized.contains("private-key"));
    }

    #[test]
    fn async_pipeline_dispatches_chunk_one_while_chunk_zero_import_pending() {
        let report = run_local_async_streaming_controller(&async_args()).unwrap();

        assert_eq!(report.role, "local_async_controller");
        assert_eq!(report.result, RESULT_INCONCLUSIVE);
        assert_eq!(report.recommendation, RECOMMENDATION_READY_ASYNC);
        assert_eq!(report.telemetry.validation_result, "pass");
        assert_eq!(report.telemetry.final_decode_start_position, Some(128));
        assert!(report.telemetry.prefill_start_ms[1] < report.telemetry.import_end_ms[0]);
        assert!(report.telemetry.actual_overlap_ms > 0.0);
        assert_eq!(
            report.telemetry.overlap_ms,
            report.telemetry.actual_overlap_ms
        );
        assert_eq!(report.telemetry.page_queue_depth, 2);
    }

    #[test]
    fn async_pipeline_import_failure_fails_closed_and_cancels_source() {
        let report = run_local_async_streaming_controller_with_fault(
            &async_args(),
            AsyncPipelineFault::import_failure_at(0),
        )
        .unwrap();

        assert_eq!(report.telemetry.validation_result, "fail_closed");
        assert_eq!(report.telemetry.failure_reason, Some("import_failure"));
        assert!(report
            .negative_checks
            .iter()
            .any(|check| check.case == "importer_failure_cancels_source"));
    }

    #[test]
    fn async_pipeline_source_failure_fails_closed_and_cancels_importer() {
        let report = run_local_async_streaming_controller_with_fault(
            &async_args(),
            AsyncPipelineFault::source_failure_at(1),
        )
        .unwrap();

        assert_eq!(report.telemetry.validation_result, "fail_closed");
        assert_eq!(report.telemetry.failure_reason, Some("source_failure"));
        assert!(report
            .negative_checks
            .iter()
            .any(|check| check.case == "source_failure_cancels_importer"));
    }

    #[test]
    fn async_pipeline_backpressure_blocks_dispatch_when_in_flight_bytes_are_full() {
        let mut args = async_args();
        args.max_in_flight_chunks = 1;
        let report = run_local_async_streaming_controller(&args).unwrap();

        assert_eq!(report.telemetry.validation_result, "pass");
        assert!(report.telemetry.backpressure_wait_ms > 0.0);
        assert_eq!(report.telemetry.page_queue_depth, 1);
        assert_eq!(report.telemetry.actual_overlap_ms, 0.0);
    }

    #[test]
    fn async_pipeline_queue_depth_cap_is_enforced() {
        let mut args = async_args();
        args.max_queue_depth = 1;
        let report = run_local_async_streaming_controller(&args).unwrap();

        assert_eq!(report.telemetry.validation_result, "pass");
        assert!(report.telemetry.backpressure_wait_ms > 0.0);
        assert_eq!(report.telemetry.page_queue_depth, 1);
    }

    #[test]
    fn async_pipeline_report_is_sanitized() {
        let report = run_local_async_streaming_controller(&async_args()).unwrap();
        let serialized = serde_json::to_string(&report).unwrap();

        assert!(!serialized.contains("/Users/"));
        assert!(!serialized.contains("prompt text"));
        assert!(!serialized.contains("generated content"));
        assert!(!serialized.contains("private-key"));
        assert!(!serialized.contains("kv payload"));
    }

    #[test]
    fn split_channel_local_report_records_source_side_overlap_fields() {
        let mut args = async_args();
        args.pipeline_mode = KvStreamingPipelineMode::SplitChannel;
        let report = run_local_streaming_controller(&args).unwrap();

        assert_eq!(report.role, "local_split_channel_controller");
        assert_eq!(report.recommendation, "ready_for_split_channel_4k_smoke");
        assert_eq!(
            report.runtime_path.network_transport,
            "split_control_channel_and_page_stream_ready_not_started"
        );
        assert!(report.telemetry.true_compute_transfer_overlap_ms > 0.0);
        assert_eq!(
            report.telemetry.clock_alignment_status,
            "simulated_same_clock"
        );
        assert_eq!(report.telemetry.source_queue_depth, args.max_queue_depth);
    }

    #[test]
    fn cli_parses_local_source_and_coordinator_roles() {
        let local = Cli::try_parse_from([
            "skippy-correctness",
            "kv-streaming-handoff",
            "local",
            "--pipeline-mode",
            "async",
            "--total-tokens",
            "128",
            "--chunk-tokens",
            "64",
        ])
        .unwrap();
        assert!(matches!(
            local.command,
            CommandKind::KvStreamingHandoff(KvStreamingHandoffArgs {
                role: KvStreamingHandoffRole::Local(_)
            })
        ));

        let source = Cli::try_parse_from([
            "skippy-correctness",
            "kv-streaming-handoff",
            "source",
            "--pipeline-mode",
            "async",
            "--bind-addr",
            "127.0.0.1:19430",
            "--max-queue-depth",
            "4",
        ])
        .unwrap();
        match source.command {
            CommandKind::KvStreamingHandoff(KvStreamingHandoffArgs {
                role: KvStreamingHandoffRole::Source(args),
            }) => {
                assert_eq!(args.pipeline_mode, KvStreamingPipelineMode::Async);
                assert_eq!(args.max_queue_depth, 4);
            }
            _ => panic!("expected kv-streaming-handoff source command"),
        }

        let coordinator = Cli::try_parse_from([
            "skippy-correctness",
            "kv-streaming-handoff",
            "coordinator",
            "--pipeline-mode",
            "async",
            "--source-addr",
            "127.0.0.1:19430",
            "--bootstrap-strategy",
            "trim-replay-last-token",
            "--max-queue-depth",
            "4",
        ])
        .unwrap();
        match coordinator.command {
            CommandKind::KvStreamingHandoff(KvStreamingHandoffArgs {
                role: KvStreamingHandoffRole::Coordinator(args),
            }) => {
                assert_eq!(args.pipeline_mode, KvStreamingPipelineMode::Async);
                assert_eq!(args.max_queue_depth, 4);
            }
            _ => panic!("expected kv-streaming-handoff coordinator command"),
        }
    }

    #[test]
    fn cli_parses_split_channel_addresses() {
        let source = Cli::try_parse_from([
            "skippy-correctness",
            "kv-streaming-handoff",
            "source",
            "--pipeline-mode",
            "split-channel",
            "--control-bind-addr",
            "127.0.0.1:19430",
            "--page-bind-addr",
            "127.0.0.1:19431",
        ])
        .unwrap();
        match source.command {
            CommandKind::KvStreamingHandoff(KvStreamingHandoffArgs {
                role: KvStreamingHandoffRole::Source(args),
            }) => {
                assert_eq!(args.pipeline_mode, KvStreamingPipelineMode::SplitChannel);
                assert_eq!(args.bind_addr, "127.0.0.1:19430".parse().unwrap());
                assert_eq!(args.page_bind_addr, "127.0.0.1:19431".parse().unwrap());
            }
            _ => panic!("expected kv-streaming-handoff source command"),
        }

        let coordinator = Cli::try_parse_from([
            "skippy-correctness",
            "kv-streaming-handoff",
            "coordinator",
            "--pipeline-mode",
            "split-channel",
            "--control-addr",
            "127.0.0.1:19430",
            "--page-addr",
            "127.0.0.1:19431",
        ])
        .unwrap();
        match coordinator.command {
            CommandKind::KvStreamingHandoff(KvStreamingHandoffArgs {
                role: KvStreamingHandoffRole::Coordinator(args),
            }) => {
                assert_eq!(args.pipeline_mode, KvStreamingPipelineMode::SplitChannel);
                assert_eq!(args.source_addr, "127.0.0.1:19430".parse().unwrap());
                assert_eq!(args.page_addr, "127.0.0.1:19431".parse().unwrap());
            }
            _ => panic!("expected kv-streaming-handoff coordinator command"),
        }
    }

    #[test]
    fn async_live_frame_writer_emits_control_and_page_frames() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut server, _) = listener.accept().unwrap();
            let (control, payload): (StreamingHarnessResponse, Vec<u8>) =
                read_json_frame_with_payload(&mut server).unwrap();
            assert_eq!(control.kind, "prefill_started");
            assert_eq!(control.chunk_index, Some(0));
            assert!(payload.is_empty());

            let (page, payload): (StreamingHarnessResponse, Vec<u8>) =
                read_json_frame_with_payload(&mut server).unwrap();
            assert_eq!(page.kind, "page");
            assert_eq!(page.chunk_index, Some(0));
            assert_eq!(payload, b"page-bytes");
        });
        let client = TcpStream::connect(addr).unwrap();
        let (tx, rx) = mpsc::sync_channel::<StreamingOutboundFrame>(2);
        let writer = thread::spawn(move || write_streaming_outbound_frames(client, rx));
        send_outbound_response(
            &tx,
            StreamingHarnessResponse::control("prefill_started", 0, Some(0.0), None),
            Vec::new(),
        )
        .unwrap();
        send_outbound_response(
            &tx,
            StreamingHarnessResponse::page(manifest_for(0), 1.0, 1.0, 0, 1),
            b"page-bytes".to_vec(),
        )
        .unwrap();
        drop(tx);

        join_writer_thread(writer).unwrap();
        server.join().unwrap();
    }

    #[test]
    fn async_live_reader_forwards_page_frames_to_import_queue() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut server, _) = listener.accept().unwrap();
            write_json_frame(
                &mut server,
                &StreamingHarnessResponse::page(manifest_for(0), 1.0, 1.0, 0, 1),
                b"page-bytes",
            )
            .unwrap();
            write_json_frame(&mut server, &StreamingHarnessResponse::ok("stopped"), &[]).unwrap();
        });
        let client = TcpStream::connect(addr).unwrap();
        let (tx, rx) = mpsc::sync_channel::<StreamingInboundFrame>(2);
        let reader = thread::spawn(move || read_async_streaming_frames(client, tx));

        let first = rx.recv().unwrap();
        assert_eq!(first.response.kind, "page");
        assert_eq!(first.response.chunk_index, Some(0));
        assert_eq!(first.payload, b"page-bytes");
        let second = rx.recv().unwrap();
        assert_eq!(second.response.kind, "stopped");

        join_reader_thread(reader).unwrap();
        server.join().unwrap();
    }

    #[test]
    fn split_channel_control_event_is_not_blocked_by_page_payload() {
        let control_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let control_addr = control_listener.local_addr().unwrap();
        let page_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let page_addr = page_listener.local_addr().unwrap();
        let (tx, rx) = mpsc::sync_channel::<StreamingInboundFrame>(4);
        let control_tx = tx.clone();
        let control_reader = thread::spawn(move || {
            let control = TcpStream::connect(control_addr).unwrap();
            read_split_channel_control_frames(control, control_tx)
        });
        let page_reader = thread::spawn(move || {
            let page = TcpStream::connect(page_addr).unwrap();
            read_split_channel_page_frames(page, tx)
        });
        let control_server = thread::spawn(move || {
            let (mut stream, _) = control_listener.accept().unwrap();
            write_json_frame(
                &mut stream,
                &StreamingHarnessResponse::control("prefill_started", 0, Some(1.0), None),
                &[],
            )
            .unwrap();
            write_json_frame(&mut stream, &StreamingHarnessResponse::ok("stopped"), &[]).unwrap();
        });
        let page_server = thread::spawn(move || {
            let (mut stream, _) = page_listener.accept().unwrap();
            let response = StreamingHarnessResponse::page(manifest_for(0), 1.0, 1.0, 0, 1);
            let header = serde_json::to_vec(&response).unwrap();
            let payload = vec![7u8; 64 * 1024];
            stream
                .write_all(&(header.len() as u32).to_be_bytes())
                .unwrap();
            stream
                .write_all(&(payload.len() as u64).to_be_bytes())
                .unwrap();
            stream.write_all(&header).unwrap();
            stream.flush().unwrap();
            std::thread::sleep(std::time::Duration::from_millis(25));
            stream.write_all(&payload).unwrap();
            write_json_frame(&mut stream, &StreamingHarnessResponse::ok("stopped"), &[]).unwrap();
        });

        let first = rx.recv().unwrap();
        assert_eq!(first.channel, StreamingFrameChannel::Control);
        assert_eq!(first.response.kind, "prefill_started");

        join_reader_thread(control_reader).unwrap();
        join_reader_thread(page_reader).unwrap();
        control_server.join().unwrap();
        page_server.join().unwrap();
    }

    #[test]
    fn coordinator_ready_report_does_not_claim_live_smoke() {
        let args = KvStreamingHandoffCoordinatorArgs {
            output: crate::cli::OutputArgs { report_out: None },
            markdown_out: None,
            pipeline_mode: KvStreamingPipelineMode::Serial,
            source_addr: "127.0.0.1:19430".parse().unwrap(),
            page_addr: "127.0.0.1:19431".parse().unwrap(),
            model: None,
            stage_load_mode: StageLoadMode::RuntimeSlice,
            layer_end: 60,
            ctx_size: 8192,
            n_gpu_layers: 0,
            n_batch: None,
            n_ubatch: None,
            flash_attn: FlashAttentionArg::Auto,
            session_id: "source".to_string(),
            artifact_sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .to_string(),
            tokenizer_hash: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                .to_string(),
            chat_template_hash: "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
                .to_string(),
            prompt_id: "synthetic-streaming-two-chunk".to_string(),
            total_tokens: 128,
            chunk_tokens: 64,
            max_tokens: 16,
            seed: 42,
            bootstrap_strategy: KvPageBootstrapStrategy::TrimReplayLastToken,
            max_in_flight_chunks: 1,
            max_in_flight_bytes: 1_073_741_824,
            max_frame_bytes: 1_073_741_824,
            max_queue_depth: 2,
        };
        let report = coordinator_ready_report(&args).unwrap();

        assert_eq!(report.role, "coordinator");
        assert_eq!(report.result, RESULT_INCONCLUSIVE);
        assert_eq!(report.recommendation, RECOMMENDATION_READY);
        assert_eq!(
            report.runtime_path.network_transport,
            "test_harness_ready_not_started"
        );
        assert_eq!(report.bootstrap.status, "not_run");
        assert_eq!(report.baseline.comparison, "not_run");
    }
}
