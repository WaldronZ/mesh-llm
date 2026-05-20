use super::*;

const PD_HANDOFF_PROTOCOL_VERSION: &str = "pd-handoff/1";
const PD_KV_FORMAT_VERSION: &str = "native-full-state/1";
const PD_KV_LAYOUT: &str = "llama.cpp-native-full-state";
const PD_BYTE_ORDER: &str = "little";
const PD_CHECKSUM_ALGORITHM: &str = "sha256";

#[derive(Debug, Clone)]
struct PdHandoffManifest {
    protocol_version: &'static str,
    handoff_id: String,
    request_id: String,
    source_node_id: String,
    target_node_id: String,
    model_id: String,
    model_artifact_sha256: String,
    tokenizer_metadata_hash: String,
    chat_template_hash: String,
    runtime_abi_version: String,
    kv_format_version: &'static str,
    kv_dtype: &'static str,
    layout: &'static str,
    byte_order: &'static str,
    checksum_algorithm: &'static str,
    prompt_token_count: usize,
    decode_start_position: usize,
    total_bytes: u64,
    payload_checksum: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PdManifestError {
    field: &'static str,
}

#[derive(Default)]
struct PdTiming {
    router_overhead_ms: f64,
    prefill_dispatch_ms: f64,
    kv_export_ms: f64,
    kv_export_roundtrip_ms: f64,
    kv_transfer_ms: f64,
    kv_network_read_ms: f64,
    kv_network_write_ms: f64,
    kv_transfer_network_ms: f64,
    kv_transfer_isolated: bool,
    kv_import_ms: f64,
    decode_start_ms: f64,
    ttft_ms: f64,
    decode_tokens_per_sec: f64,
}

struct PdExportedState {
    bytes: Vec<u8>,
    roundtrip_ms: f64,
    network_read_ms: f64,
}

struct PdImportTiming {
    total_ms: f64,
    network_write_ms: f64,
}

impl StageOpenAiBackend {
    pub(super) fn generate_pd_router_validation_tokens(
        &self,
        request: PdRouterValidationGeneration<'_>,
        mut on_token: impl FnMut(i32) -> OpenAiResult<PdRouterValidationTokenControl>,
    ) -> OpenAiResult<GenerationCacheStats> {
        let started = PhaseTimer::start();
        if request.config.inject_pre_content_failure() {
            let reason = if request.config.mode == PdServingMode::Mvp {
                "pre_content_failure_injected"
            } else {
                "pre_token_failure_injected"
            };
            self.emit_pd_validation_fallback(request.ids, reason, started.elapsed_ms());
            return self.generate_local_tokens(
                LocalGeneration {
                    prompt_token_ids: request.prompt_token_ids,
                    max_tokens: request.max_tokens,
                    sampling: request.sampling,
                    chat_sampling_metadata: request.chat_sampling_metadata,
                    hook_request: None,
                    hook_runtime: None,
                    cancellation: request.cancellation,
                    ids: request.ids,
                },
                |token| on_token(token).map(|control| control.control),
            );
        }

        let mut timing = PdTiming::default();
        let prompt_token_count = request.prompt_token_ids.len();
        let prefill_token_count = prompt_token_count.saturating_sub(1);
        let source_session_id = request.ids.session_id.saturating_mul(2).max(1);
        let decode_session_id = source_session_id.saturating_add(1);
        let mut source_stream = self.connect_pd_validation_endpoint(
            request.config,
            &request.config.prefill_addr,
            request.config.startup_timeout_secs,
            request.ids,
            "pgx_prefill",
        )?;

        let result = (|| {
            if prefill_token_count > 0 {
                let prefill_timer = PhaseTimer::start();
                send_prefill_chunk(
                    &mut source_stream,
                    request.config.wire_dtype,
                    OpenAiPrefillChunk {
                        seq_id: 0,
                        pos_start: 0,
                        prefill_token_count,
                        tokens: &request.prompt_token_ids[..prefill_token_count],
                        request_id: request.ids.request_id,
                        session_id: source_session_id,
                    },
                )
                .map_err(openai_backend_error)?;
                timing.prefill_dispatch_ms = prefill_timer.elapsed_ms();
            }

            let exported = export_full_state_over_binary(
                &mut source_stream,
                request.config.wire_dtype,
                prefill_token_count,
                request.ids.request_id,
                source_session_id,
            )?;
            timing.kv_export_roundtrip_ms = exported.roundtrip_ms;
            timing.kv_network_read_ms = exported.network_read_ms;
            timing.kv_export_ms = (exported.roundtrip_ms - exported.network_read_ms).max(0.0);

            let mut manifest = build_pd_handoff_manifest(
                request.config,
                request.ids,
                prefill_token_count,
                &exported.bytes,
            );
            apply_pd_manifest_test_fault(request.config, &mut manifest);
            validate_pd_handoff_manifest(&manifest, request.config, &exported.bytes).map_err(
                |error| {
                    self.emit_pd_validation_failure(
                        request.ids,
                        "manifest_validation",
                        error.field,
                        None,
                        started.elapsed_ms(),
                    );
                    OpenAiError::backend(format!(
                        "PD handoff manifest validation failed: {}",
                        error.field
                    ))
                },
            )?;

            let mut decode_stream = self.connect_pd_validation_endpoint(
                request.config,
                &request.config.decode_addr,
                request.config.startup_timeout_secs,
                request.ids,
                "mac_decode",
            )?;
            let decode_result = (|| {
                let import_timing = import_full_state_over_binary(
                    &mut decode_stream,
                    request.config.wire_dtype,
                    &exported.bytes,
                    prefill_token_count,
                    request.ids.request_id,
                    decode_session_id,
                )?;
                timing.kv_network_write_ms = import_timing.network_write_ms;
                timing.kv_import_ms =
                    (import_timing.total_ms - import_timing.network_write_ms).max(0.0);
                timing.kv_transfer_network_ms =
                    timing.kv_network_read_ms + timing.kv_network_write_ms;
                timing.kv_transfer_ms = timing.kv_transfer_network_ms;
                timing.kv_transfer_isolated = true;

                if let Some(message) = generation_config_message(
                    request.config.wire_dtype,
                    request.ids.request_id,
                    decode_session_id,
                    prompt_token_count,
                    wire_sampling_config(request.sampling),
                    request.chat_sampling_metadata,
                )? {
                    write_stage_message(&mut decode_stream, &message, request.config.wire_dtype)
                        .map_err(openai_io_error)?;
                    let reply = recv_reply(&mut decode_stream).map_err(openai_io_error)?;
                    if reply.kind != WireReplyKind::Ack {
                        return Err(OpenAiError::backend(format!(
                            "expected PD generation config ACK, got {:?}",
                            reply.kind
                        )));
                    }
                }

                let decode_timer = PhaseTimer::start();
                let mut decoded_tokens = 0usize;
                let mut content_delta_count = 0usize;
                let mut current = *request
                    .prompt_token_ids
                    .last()
                    .expect("checked non-empty prompt");
                for decode_step in 0..request.max_tokens {
                    if request
                        .cancellation
                        .is_some_and(openai_frontend::CancellationToken::is_cancelled)
                    {
                        break;
                    }
                    let token_timer = PhaseTimer::start();
                    current = decode_one_pd_token(
                        &mut decode_stream,
                        request.config.wire_dtype,
                        current,
                        prefill_token_count + decode_step as usize,
                        decode_step as usize,
                        request.ids.request_id,
                        decode_session_id,
                        request.sampling,
                    )?;
                    if decoded_tokens == 0 {
                        timing.decode_start_ms = token_timer.elapsed_ms();
                        timing.ttft_ms = started.elapsed_ms();
                    }
                    decoded_tokens += 1;
                    let token_control = on_token(current)?;
                    if token_control.emitted_content_delta {
                        content_delta_count += 1;
                    }
                    if token_control.control == TokenControl::Stop {
                        break;
                    }
                    if decoded_tokens == 1
                        && request.config.fault_injection
                            == PdRouterValidationFault::PostTokenFailure
                    {
                        self.emit_pd_validation_failure(
                            request.ids,
                            "post_token_failure",
                            "transparent_fallback_blocked_after_first_token",
                            Some(content_delta_count),
                            started.elapsed_ms(),
                        );
                        return Err(OpenAiError::backend(
                            "PD validation post-token failure: transparent fallback blocked",
                        ));
                    }
                }
                let decode_ms = decode_timer.elapsed_ms();
                if decode_ms > 0.0 {
                    timing.decode_tokens_per_sec = decoded_tokens as f64 / (decode_ms / 1000.0);
                }
                Ok(())
            })();
            let decode_stop = stop_pd_binary_stream(
                &mut decode_stream,
                request.config.wire_dtype,
                request.ids.request_id,
                decode_session_id,
            );
            merge_pd_stop_result(decode_result, decode_stop)?;

            timing.router_overhead_ms = started.elapsed_ms();
            self.emit_pd_validation_summary(
                request.ids,
                &manifest,
                &timing,
                "pass",
                request.config,
                request.config.configured_fault_label(),
            );
            Ok(GenerationCacheStats::default())
        })();

        let source_stop = stop_pd_binary_stream(
            &mut source_stream,
            request.config.wire_dtype,
            request.ids.request_id,
            source_session_id,
        );
        merge_pd_stop_result(result, source_stop)
    }

    fn connect_pd_validation_endpoint(
        &self,
        config: &PdRouterValidationConfig,
        endpoint: &str,
        timeout_secs: u64,
        ids: &OpenAiGenerationIds,
        role: &'static str,
    ) -> OpenAiResult<TcpStream> {
        let timer = PhaseTimer::start();
        let stream =
            connect_endpoint_ready(endpoint, timeout_secs).map_err(openai_backend_error)?;
        let mut attrs = self.openai_attrs(ids);
        attrs.insert("pd.mode".to_string(), json!(config.mode.backend_label()));
        attrs.insert("pd.role".to_string(), json!(role));
        attrs.insert("pd.endpoint_configured".to_string(), json!(true));
        if config.mode == PdServingMode::Validation {
            attrs.insert("pd.validation.role".to_string(), json!(role));
            attrs.insert("pd.validation.endpoint_configured".to_string(), json!(true));
        }
        self.emit_openai_phase(config.mode.connect_event(), timer, attrs);
        Ok(stream)
    }

    fn emit_pd_validation_fallback(
        &self,
        ids: &OpenAiGenerationIds,
        reason: &'static str,
        elapsed_ms: f64,
    ) {
        let mut attrs = self.openai_attrs(ids);
        let mode = self.pd_serving_mode().unwrap_or(PdServingMode::Validation);
        attrs.insert("pd.mode".to_string(), json!(mode.backend_label()));
        attrs.insert("pd.validation_or_mvp.result".to_string(), json!("fallback"));
        attrs.insert(mode.result_attr().to_string(), json!("fallback"));
        attrs.insert(mode.fallback_attr().to_string(), json!(reason));
        attrs.insert("pd.pre_token".to_string(), json!(true));
        if mode == PdServingMode::Validation {
            attrs.insert("pd.validation.pre_token".to_string(), json!(true));
        }
        attrs.insert("llama_stage.elapsed_ms".to_string(), json!(elapsed_ms));
        self.telemetry.emit(mode.telemetry_event(), attrs);
    }

    pub(super) fn emit_pd_validation_failure(
        &self,
        ids: &OpenAiGenerationIds,
        phase: &'static str,
        reason: &'static str,
        content_delta_count: Option<usize>,
        elapsed_ms: f64,
    ) {
        let mut attrs = self.openai_attrs(ids);
        let mode = self.pd_serving_mode().unwrap_or(PdServingMode::Validation);
        attrs.insert("pd.mode".to_string(), json!(mode.backend_label()));
        attrs.insert("pd.validation_or_mvp.result".to_string(), json!("fail"));
        attrs.insert(mode.result_attr().to_string(), json!("fail"));
        attrs.insert(mode.failure_phase_attr().to_string(), json!(phase));
        attrs.insert(mode.failure_reason_attr().to_string(), json!(reason));
        if let Some(content_delta_count) = content_delta_count {
            attrs.insert(
                "pd.content_delta_count".to_string(),
                json!(content_delta_count),
            );
            if mode == PdServingMode::Validation {
                attrs.insert(
                    "pd.validation.content_delta_count".to_string(),
                    json!(content_delta_count),
                );
            }
        }
        attrs.insert("llama_stage.elapsed_ms".to_string(), json!(elapsed_ms));
        self.telemetry.emit(mode.telemetry_event(), attrs);
    }

    fn emit_pd_validation_summary(
        &self,
        ids: &OpenAiGenerationIds,
        manifest: &PdHandoffManifest,
        timing: &PdTiming,
        result: &'static str,
        config: &PdRouterValidationConfig,
        fault_injection: &'static str,
    ) {
        let mut attrs = self.openai_attrs(ids);
        attrs.insert("pd.mode".to_string(), json!(config.mode.backend_label()));
        attrs.insert("pd.validation_or_mvp.result".to_string(), json!(result));
        attrs.insert(config.mode.result_attr().to_string(), json!(result));
        attrs.insert(
            "pd.protocol_version".to_string(),
            json!(manifest.protocol_version),
        );
        attrs.insert("pd.handoff_id".to_string(), json!(manifest.handoff_id));
        attrs.insert(
            "pd.prefill_worker_role".to_string(),
            json!(config.prefill_worker_telemetry_label()),
        );
        attrs.insert(
            "pd.decode_worker_role".to_string(),
            json!(config.decode_worker_telemetry_label()),
        );
        attrs.insert(
            "pd.prompt_token_count".to_string(),
            json!(manifest.prompt_token_count),
        );
        attrs.insert(
            "pd.decode_start_position".to_string(),
            json!(manifest.decode_start_position),
        );
        if config.mode == PdServingMode::Validation {
            attrs.insert("pd.validation.result".to_string(), json!(result));
            attrs.insert(
                "pd.validation.protocol_version".to_string(),
                json!(manifest.protocol_version),
            );
            attrs.insert(
                "pd.validation.handoff_id".to_string(),
                json!(manifest.handoff_id),
            );
            attrs.insert(
                "pd.validation.prefill_worker_role".to_string(),
                json!(config.prefill_worker_telemetry_label()),
            );
            attrs.insert(
                "pd.validation.decode_worker_role".to_string(),
                json!(config.decode_worker_telemetry_label()),
            );
            attrs.insert(
                "pd.validation.fault_injection".to_string(),
                json!(fault_injection),
            );
            attrs.insert(
                "pd.validation.prompt_token_count".to_string(),
                json!(manifest.prompt_token_count),
            );
            attrs.insert(
                "pd.validation.decode_start_position".to_string(),
                json!(manifest.decode_start_position),
            );
        }
        insert_pd_timing_attrs(&mut attrs, manifest, timing);
        self.telemetry.emit(config.mode.telemetry_event(), attrs);
    }
}

fn merge_pd_stop_result<T>(
    result: OpenAiResult<T>,
    stop_result: std::io::Result<()>,
) -> OpenAiResult<T> {
    match (result, stop_result) {
        (Ok(value), Ok(())) => Ok(value),
        (Ok(_), Err(error)) => Err(openai_io_error(error)),
        (Err(error), _) => Err(error),
    }
}

fn insert_pd_timing_attrs(
    attrs: &mut BTreeMap<String, Value>,
    manifest: &PdHandoffManifest,
    timing: &PdTiming,
) {
    attrs.insert(
        "pd.kv_payload_bytes".to_string(),
        json!(manifest.total_bytes),
    );
    attrs.insert("pd.kv_export_ms".to_string(), json!(timing.kv_export_ms));
    attrs.insert(
        "pd.kv_export_roundtrip_ms".to_string(),
        json!(timing.kv_export_roundtrip_ms),
    );
    attrs.insert(
        "pd.kv_transfer_ms".to_string(),
        json!(timing.kv_transfer_ms),
    );
    attrs.insert(
        "pd.kv_network_read_ms".to_string(),
        json!(timing.kv_network_read_ms),
    );
    attrs.insert(
        "pd.kv_network_write_ms".to_string(),
        json!(timing.kv_network_write_ms),
    );
    attrs.insert(
        "pd.kv_transfer_network_ms".to_string(),
        json!(timing.kv_transfer_network_ms),
    );
    attrs.insert(
        "pd.kv_transfer_isolated".to_string(),
        json!(timing.kv_transfer_isolated),
    );
    attrs.insert("pd.kv_import_ms".to_string(), json!(timing.kv_import_ms));
    attrs.insert(
        "pd.router_overhead_ms".to_string(),
        json!(timing.router_overhead_ms),
    );
    attrs.insert(
        "pd.prefill_dispatch_ms".to_string(),
        json!(timing.prefill_dispatch_ms),
    );
    attrs.insert(
        "pd.decode_start_ms".to_string(),
        json!(timing.decode_start_ms),
    );
    attrs.insert("pd.ttft_ms".to_string(), json!(timing.ttft_ms));
    attrs.insert(
        "pd.decode_tokens_per_sec".to_string(),
        json!(timing.decode_tokens_per_sec),
    );
}

fn build_pd_handoff_manifest(
    config: &PdRouterValidationConfig,
    ids: &OpenAiGenerationIds,
    prompt_token_count: usize,
    payload: &[u8],
) -> PdHandoffManifest {
    PdHandoffManifest {
        protocol_version: PD_HANDOFF_PROTOCOL_VERSION,
        handoff_id: format!("pd-handoff-{}", ids.request_id_string()),
        request_id: ids.request_id_string(),
        source_node_id: config.source_node_id.clone(),
        target_node_id: config.target_node_id.clone(),
        model_id: config.model_id.clone(),
        model_artifact_sha256: config.expected_artifact_sha256.clone(),
        tokenizer_metadata_hash: config.expected_tokenizer_hash.clone(),
        chat_template_hash: config.expected_chat_template_hash.clone(),
        runtime_abi_version: format!("skippy-stage-state-{STAGE_STATE_VERSION}"),
        kv_format_version: PD_KV_FORMAT_VERSION,
        kv_dtype: pd_wire_dtype_label(config.wire_dtype),
        layout: PD_KV_LAYOUT,
        byte_order: PD_BYTE_ORDER,
        checksum_algorithm: PD_CHECKSUM_ALGORITHM,
        prompt_token_count,
        decode_start_position: prompt_token_count,
        total_bytes: payload.len() as u64,
        payload_checksum: sha256_hex(payload),
    }
}

fn apply_pd_manifest_test_fault(
    config: &PdRouterValidationConfig,
    manifest: &mut PdHandoffManifest,
) {
    if config.inject_manifest_mismatch() {
        manifest.payload_checksum = "fault-injected-manifest-mismatch".to_string();
    }
}

fn validate_pd_handoff_manifest(
    manifest: &PdHandoffManifest,
    config: &PdRouterValidationConfig,
    payload: &[u8],
) -> Result<(), PdManifestError> {
    let expected_runtime = format!("skippy-stage-state-{STAGE_STATE_VERSION}");
    let checks = [
        (
            manifest.protocol_version == PD_HANDOFF_PROTOCOL_VERSION,
            "protocol_version",
        ),
        (
            manifest.model_artifact_sha256.as_str() == config.expected_artifact_sha256.as_str(),
            "model_artifact_sha256",
        ),
        (
            manifest.tokenizer_metadata_hash.as_str() == config.expected_tokenizer_hash.as_str(),
            "tokenizer_metadata_hash",
        ),
        (
            manifest.chat_template_hash.as_str() == config.expected_chat_template_hash.as_str(),
            "chat_template_hash",
        ),
        (
            manifest.runtime_abi_version.as_str() == expected_runtime.as_str(),
            "runtime_abi_version",
        ),
        (
            manifest.kv_format_version == PD_KV_FORMAT_VERSION,
            "kv_format_version",
        ),
        (
            manifest.kv_dtype == pd_wire_dtype_label(config.wire_dtype),
            "kv_dtype",
        ),
        (manifest.layout == PD_KV_LAYOUT, "layout"),
        (manifest.byte_order == PD_BYTE_ORDER, "byte_order"),
        (
            manifest.checksum_algorithm == PD_CHECKSUM_ALGORITHM,
            "checksum_algorithm",
        ),
        (
            manifest.decode_start_position == manifest.prompt_token_count,
            "decode_start_position",
        ),
        (manifest.total_bytes == payload.len() as u64, "total_bytes"),
        (
            manifest.payload_checksum.as_str() == sha256_hex(payload).as_str(),
            "payload_checksum",
        ),
        (!manifest.request_id.is_empty(), "request_id"),
        (!manifest.handoff_id.is_empty(), "handoff_id"),
        (
            manifest.source_node_id.as_str() == config.source_node_id.as_str(),
            "source_node_id",
        ),
        (
            manifest.target_node_id.as_str() == config.target_node_id.as_str(),
            "target_node_id",
        ),
        (
            manifest.model_id.as_str() == config.model_id.as_str(),
            "model_id",
        ),
    ];
    for (ok, field) in checks {
        if !ok {
            return Err(PdManifestError { field });
        }
    }
    Ok(())
}

fn export_full_state_over_binary(
    stream: &mut TcpStream,
    wire_dtype: WireActivationDType,
    prompt_token_count: usize,
    request_id: u64,
    session_id: u64,
) -> OpenAiResult<PdExportedState> {
    let mut state = StageStateHeader::new(WireMessageKind::StateExport, wire_dtype);
    state.prompt_token_count = i32::try_from(prompt_token_count)
        .map_err(|_| OpenAiError::backend("prompt token count exceeds i32"))?;
    state.flags |= state_flags::FULL_STATE;
    let message = StageWireMessage {
        kind: WireMessageKind::StateExport,
        pos_start: 0,
        token_count: 0,
        state,
        request_id,
        session_id,
        sampling: None,
        chat_sampling_metadata: None,
        tokens: Vec::new(),
        positions: Vec::new(),
        activation: Vec::new(),
        raw_bytes: Vec::new(),
    };
    let export_timer = PhaseTimer::start();
    write_stage_message(&mut *stream, &message, wire_dtype).map_err(openai_io_error)?;
    let (reply, read_timing) =
        read_stage_message_timed(&mut *stream, 0).map_err(openai_io_error)?;
    let roundtrip_ms = export_timer.elapsed_ms();
    if reply.kind != WireMessageKind::StateImport {
        return Err(OpenAiError::backend(format!(
            "expected PD state export payload, got {:?}",
            reply.kind
        )));
    }
    if reply.raw_bytes.is_empty() {
        return Err(OpenAiError::backend(
            "PD state export returned empty payload",
        ));
    }
    Ok(PdExportedState {
        bytes: reply.raw_bytes,
        roundtrip_ms,
        network_read_ms: read_timing.raw_payload_ms,
    })
}

fn import_full_state_over_binary(
    stream: &mut TcpStream,
    wire_dtype: WireActivationDType,
    payload: &[u8],
    prompt_token_count: usize,
    request_id: u64,
    session_id: u64,
) -> OpenAiResult<PdImportTiming> {
    let mut state = StageStateHeader::new(WireMessageKind::StateImport, wire_dtype);
    state.prompt_token_count = i32::try_from(prompt_token_count)
        .map_err(|_| OpenAiError::backend("prompt token count exceeds i32"))?;
    state.flags |= state_flags::FULL_STATE;
    let message = StageWireMessage {
        kind: WireMessageKind::StateImport,
        pos_start: 0,
        token_count: i32::try_from(payload.len())
            .map_err(|_| OpenAiError::backend("PD state payload exceeds i32"))?,
        state,
        request_id,
        session_id,
        sampling: None,
        chat_sampling_metadata: None,
        tokens: Vec::new(),
        positions: Vec::new(),
        activation: Vec::new(),
        raw_bytes: payload.to_vec(),
    };
    let import_timer = PhaseTimer::start();
    let write_timing =
        write_stage_message_timed(&mut *stream, &message, wire_dtype).map_err(openai_io_error)?;
    let reply = recv_reply(&mut *stream).map_err(openai_io_error)?;
    if reply.kind != WireReplyKind::Ack {
        return Err(OpenAiError::backend(format!(
            "expected PD state import ACK, got {:?}",
            reply.kind
        )));
    }
    Ok(PdImportTiming {
        total_ms: import_timer.elapsed_ms(),
        network_write_ms: write_timing.raw_payload_ms,
    })
}

#[allow(clippy::too_many_arguments)]
fn decode_one_pd_token(
    stream: &mut TcpStream,
    wire_dtype: WireActivationDType,
    current: i32,
    pos_start: usize,
    decode_step: usize,
    request_id: u64,
    session_id: u64,
    sampling: &SamplingConfig,
) -> OpenAiResult<i32> {
    let mut state = StageStateHeader::new(WireMessageKind::DecodeEmbd, wire_dtype);
    state.prompt_token_count = i32::try_from(pos_start)
        .map_err(|_| OpenAiError::backend("decode position exceeds i32"))?;
    state.decode_step =
        i32::try_from(decode_step).map_err(|_| OpenAiError::backend("decode step exceeds i32"))?;
    state.current_token = current;
    state.source_stage_index = -1;
    let message = StageWireMessage {
        kind: WireMessageKind::DecodeEmbd,
        pos_start: i32::try_from(pos_start)
            .map_err(|_| OpenAiError::backend("decode position exceeds i32"))?,
        token_count: 1,
        state,
        request_id,
        session_id,
        sampling: wire_sampling_config(sampling),
        chat_sampling_metadata: None,
        tokens: vec![current],
        positions: Vec::new(),
        activation: Vec::new(),
        raw_bytes: Vec::new(),
    };
    write_stage_message(&mut *stream, &message, wire_dtype).map_err(openai_io_error)?;
    let reply = recv_reply(&mut *stream).map_err(openai_io_error)?;
    if reply.kind != WireReplyKind::PredictedToken {
        return Err(OpenAiError::backend(format!(
            "expected PD decode predicted-token reply, got {:?}",
            reply.kind
        )));
    }
    Ok(reply.predicted)
}

fn stop_pd_binary_stream(
    stream: &mut TcpStream,
    wire_dtype: WireActivationDType,
    request_id: u64,
    session_id: u64,
) -> std::io::Result<()> {
    write_stage_message(
        &mut *stream,
        &StageWireMessage::stop_with_identity(wire_dtype, request_id, session_id),
        wire_dtype,
    )?;
    let reply = recv_reply(&mut *stream)?;
    if reply.kind == WireReplyKind::Ack {
        Ok(())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("expected PD stop ACK, got {:?}", reply.kind),
        ))
    }
}

fn pd_wire_dtype_label(dtype: WireActivationDType) -> &'static str {
    match dtype {
        WireActivationDType::F32 => "f32",
        WireActivationDType::F16 => "f16",
        WireActivationDType::Q8 => "q8",
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> PdRouterValidationConfig {
        PdRouterValidationConfig {
            mode: PdServingMode::Validation,
            prefill_addr: "127.0.0.1:19081".to_string(),
            decode_addr: "127.0.0.1:19082".to_string(),
            wire_dtype: WireActivationDType::F16,
            startup_timeout_secs: 1,
            model_id: "google/gemma-4-31b-it:bf16".to_string(),
            expected_artifact_sha256:
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            expected_tokenizer_hash:
                "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
            expected_chat_template_hash:
                "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc".to_string(),
            source_node_id: "pgx-prefill-validation".to_string(),
            target_node_id: "mac-decode-validation".to_string(),
            fault_injection: PdRouterValidationFault::None,
            mvp_test_fault: PdServingMvpTestFault::None,
        }
    }

    #[test]
    fn pd_handoff_manifest_positive_validation_passes() {
        let config = config();
        let ids = OpenAiGenerationIds::new(OpenAiCacheHints::default());
        let payload = b"native state bytes";
        let manifest = build_pd_handoff_manifest(&config, &ids, 8, payload);

        validate_pd_handoff_manifest(&manifest, &config, payload).unwrap();
        assert_eq!(manifest.protocol_version, PD_HANDOFF_PROTOCOL_VERSION);
        assert_eq!(manifest.decode_start_position, 8);
        assert_eq!(manifest.total_bytes, payload.len() as u64);
    }

    #[test]
    fn pd_handoff_manifest_rejects_identity_and_integrity_mismatch() {
        let config = config();
        let ids = OpenAiGenerationIds::new(OpenAiCacheHints::default());
        let payload = b"native state bytes";
        let manifest = build_pd_handoff_manifest(&config, &ids, 8, payload);

        let mut bad_artifact = manifest.clone();
        bad_artifact.model_artifact_sha256 =
            "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd".to_string();
        assert_eq!(
            validate_pd_handoff_manifest(&bad_artifact, &config, payload)
                .unwrap_err()
                .field,
            "model_artifact_sha256"
        );

        let mut bad_checksum = manifest.clone();
        bad_checksum.payload_checksum = "bad".to_string();
        assert_eq!(
            validate_pd_handoff_manifest(&bad_checksum, &config, payload)
                .unwrap_err()
                .field,
            "payload_checksum"
        );

        let mut bad_position = manifest;
        bad_position.decode_start_position = 7;
        assert_eq!(
            validate_pd_handoff_manifest(&bad_position, &config, payload)
                .unwrap_err()
                .field,
            "decode_start_position"
        );
    }

    #[test]
    fn pd_fault_labels_are_sanitized() {
        assert_eq!(PdRouterValidationFault::None.as_label(), "none");
        assert_eq!(
            PdRouterValidationFault::ManifestMismatch.as_label(),
            "manifest-mismatch"
        );
        assert_eq!(
            PdRouterValidationFault::PreTokenFailure.as_label(),
            "pre-token-failure"
        );
        assert_eq!(
            PdRouterValidationFault::PostTokenFailure.as_label(),
            "post-token-failure"
        );
        assert_eq!(
            PdRouterValidationFault::PostContentTokenFailure.as_label(),
            "post-content-token-failure"
        );
    }

    #[test]
    fn pd_mvp_test_fault_labels_are_sanitized() {
        assert_eq!(PdServingMvpTestFault::None.as_label(), "none");
        assert_eq!(
            PdServingMvpTestFault::ManifestMismatch.as_label(),
            "manifest-mismatch"
        );
        assert_eq!(
            PdServingMvpTestFault::PreContentFailure.as_label(),
            "pre-content-failure"
        );
        assert_eq!(
            PdServingMvpTestFault::PostContentFailure.as_label(),
            "post-content-failure"
        );
    }

    #[test]
    fn pd_worker_telemetry_labels_do_not_expose_configured_node_ids() {
        let mut config = config();
        config.source_node_id = "private-prefill-host.example".to_string();
        config.target_node_id = "private-decode-host.example".to_string();

        assert_eq!(
            config.prefill_worker_telemetry_label(),
            "validation-prefill-worker"
        );
        assert_eq!(
            config.decode_worker_telemetry_label(),
            "validation-decode-worker"
        );

        config.mode = PdServingMode::Mvp;
        assert_eq!(
            config.prefill_worker_telemetry_label(),
            "mvp-prefill-worker"
        );
        assert_eq!(config.decode_worker_telemetry_label(), "mvp-decode-worker");
    }

    #[test]
    fn pd_mvp_manifest_mismatch_fault_corrupts_exported_manifest_before_import() {
        let mut config = config();
        config.mode = PdServingMode::Mvp;
        config.mvp_test_fault = PdServingMvpTestFault::ManifestMismatch;
        let ids = OpenAiGenerationIds::new(OpenAiCacheHints::default());
        let payload = b"native state bytes";
        let mut manifest = build_pd_handoff_manifest(&config, &ids, 8, payload);

        assert!(config.inject_manifest_mismatch());
        apply_pd_manifest_test_fault(&config, &mut manifest);

        let error = validate_pd_handoff_manifest(&manifest, &config, payload).unwrap_err();
        assert_eq!(error.field, "payload_checksum");
    }

    #[test]
    fn pd_mvp_pre_and_post_content_faults_are_separate() {
        let mut config = config();
        config.mode = PdServingMode::Mvp;
        config.mvp_test_fault = PdServingMvpTestFault::PreContentFailure;
        assert!(config.inject_pre_content_failure());
        assert!(!config.inject_post_content_failure());
        assert!(!config.inject_manifest_mismatch());

        config.mvp_test_fault = PdServingMvpTestFault::PostContentFailure;
        assert!(!config.inject_pre_content_failure());
        assert!(config.inject_post_content_failure());
        assert!(!config.inject_manifest_mismatch());
    }

    #[test]
    fn pd_timing_marks_network_transfer_isolated() {
        let mut timing = PdTiming {
            kv_export_roundtrip_ms: 120.0,
            kv_network_read_ms: 35.0,
            kv_network_write_ms: 20.0,
            ..PdTiming::default()
        };
        timing.kv_export_ms = (timing.kv_export_roundtrip_ms - timing.kv_network_read_ms).max(0.0);
        timing.kv_transfer_network_ms = timing.kv_network_read_ms + timing.kv_network_write_ms;
        timing.kv_transfer_ms = timing.kv_transfer_network_ms;
        timing.kv_transfer_isolated = true;

        assert_eq!(timing.kv_export_ms, 85.0);
        assert_eq!(timing.kv_transfer_network_ms, 55.0);
        assert_eq!(timing.kv_transfer_ms, 55.0);
        assert!(timing.kv_transfer_isolated);
    }

    #[test]
    fn pd_stop_result_preserves_original_error_and_reports_cleanup_error_on_success() {
        let success_stop_error =
            merge_pd_stop_result::<()>(Ok(()), Err(std::io::Error::other("stop ack failed")))
                .unwrap_err();
        assert!(
            success_stop_error.to_string().contains("stop ack failed"),
            "{success_stop_error:?}"
        );

        let original_error = merge_pd_stop_result::<()>(
            Err(OpenAiError::backend("decode failed before cleanup")),
            Err(std::io::Error::other("cleanup also failed")),
        )
        .unwrap_err();
        assert!(
            original_error
                .to_string()
                .contains("decode failed before cleanup"),
            "{original_error:?}"
        );
    }

    #[test]
    fn pd_timing_attrs_include_required_metrics_without_sensitive_payloads() {
        let config = config();
        let ids = OpenAiGenerationIds::new(OpenAiCacheHints::default());
        let payload = b"native state bytes that must not appear in telemetry";
        let manifest = build_pd_handoff_manifest(&config, &ids, 8, payload);
        let timing = PdTiming {
            router_overhead_ms: 10.0,
            prefill_dispatch_ms: 1.0,
            kv_export_ms: 2.0,
            kv_export_roundtrip_ms: 3.0,
            kv_transfer_ms: 4.0,
            kv_network_read_ms: 5.0,
            kv_network_write_ms: 6.0,
            kv_transfer_network_ms: 11.0,
            kv_transfer_isolated: true,
            kv_import_ms: 7.0,
            decode_start_ms: 8.0,
            ttft_ms: 9.0,
            decode_tokens_per_sec: 10.0,
        };
        let mut attrs = BTreeMap::new();
        insert_pd_timing_attrs(&mut attrs, &manifest, &timing);

        for key in [
            "pd.kv_payload_bytes",
            "pd.kv_export_ms",
            "pd.kv_export_roundtrip_ms",
            "pd.kv_transfer_ms",
            "pd.kv_network_read_ms",
            "pd.kv_network_write_ms",
            "pd.kv_transfer_network_ms",
            "pd.kv_transfer_isolated",
            "pd.kv_import_ms",
            "pd.router_overhead_ms",
            "pd.prefill_dispatch_ms",
            "pd.decode_start_ms",
            "pd.ttft_ms",
            "pd.decode_tokens_per_sec",
        ] {
            assert!(attrs.contains_key(key), "missing {key}");
        }

        let serialized = serde_json::to_string(&attrs).unwrap();
        assert!(!serialized.contains("native state bytes"));
        assert!(!serialized.contains("token array"));
        assert!(!serialized.contains("/Users/"));
    }
}
