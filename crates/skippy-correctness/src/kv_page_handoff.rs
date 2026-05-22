mod manifest;
#[cfg(test)]
mod tests;

use std::{
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::Path,
    time::Instant,
};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use skippy_runtime::{RuntimeConfig, RuntimeLoadMode, SamplingConfig, StageModel, GGML_TYPE_F16};

use crate::cli::{
    FlashAttentionArg, KvPageBootstrapStrategy, KvPageHandoffArgs, KvPageHandoffCoordinatorArgs,
    KvPageHandoffRole, KvPageHandoffSourceArgs, StageLoadMode,
};
use manifest::*;

const PROTOCOL_VERSION: &str = "pd-kv-page/1";
const CHECKSUM_ALGORITHM: &str = "sha256";
const SOURCE_CAPABILITY: &str = "kv-page-export-runtime/1";
const TARGET_CAPABILITY: &str = "kv-page-import-runtime/1";
const RECOMMENDATION_READY: &str = "ready_for_foreground_smoke";
const RECOMMENDATION_SOURCE_UNSUPPORTED: &str = "provide_model_and_run_foreground_source";

pub fn kv_page_handoff(args: KvPageHandoffArgs) -> Result<()> {
    let (report, markdown_out) = match args.role {
        KvPageHandoffRole::Source(args) => {
            let report = if args.model.is_some() {
                run_source_runtime_loop(&args)?
            } else {
                source_skeleton_report(
                    args.bind_addr.to_string(),
                    args.output.report_out.as_deref(),
                )
            };
            emit_json_report(&report, args.output.report_out.as_deref())?;
            (report, None)
        }
        KvPageHandoffRole::Coordinator(args) => {
            let markdown_out = args.markdown_out.clone();
            let report = if args.model.is_some() {
                run_coordinator_runtime_loop(&args)?
            } else {
                run_local_coordinator_harness(&args)
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

#[derive(Debug, Serialize)]
struct KvPageHandoffReport {
    mode: &'static str,
    role: &'static str,
    result: &'static str,
    recommendation: &'static str,
    runtime_path: RuntimePathReport,
    local_manifest_validation: CheckReport,
    negative_checks: Vec<NegativeCheckReport>,
    bootstrap: BootstrapReport,
    telemetry_shape: TelemetryShapeReport,
    baseline: BaselineReport,
    privacy: PrivacyReport,
    remaining_authorization_required: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
struct RuntimePathReport {
    source_runtime_export_kv_page: &'static str,
    target_runtime_import_kv_page: &'static str,
    network_transport: &'static str,
    full_state_handoff_allowed_as_pass: bool,
}

#[derive(Debug, Serialize)]
struct CheckReport {
    status: &'static str,
    phase: &'static str,
    reason: &'static str,
}

#[derive(Debug, Serialize)]
struct NegativeCheckReport {
    case: &'static str,
    status: &'static str,
    failure_reason: &'static str,
}

#[derive(Debug, Serialize)]
struct BootstrapReport {
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
struct TelemetryShapeReport {
    page_count: usize,
    total_page_bytes: u64,
    page_export_ms: Vec<f64>,
    page_transfer_ms: Vec<f64>,
    page_import_ms: Vec<f64>,
    decode_ttft_after_import_ms: Option<f64>,
}

#[derive(Debug, Serialize)]
struct BaselineReport {
    deterministic_settings: DeterministicSettingsReport,
    strategy: &'static str,
    one_shot_full_state_baseline: &'static str,
    page_handoff_decode: &'static str,
    comparison: &'static str,
    failure_reason: Option<&'static str>,
    first_divergence_index: Option<usize>,
    baseline_token_id: Option<i32>,
    page_token_id: Option<i32>,
    baseline_token_count: Option<usize>,
    page_token_count: Option<usize>,
}

#[derive(Debug, Serialize)]
struct DeterministicSettingsReport {
    temperature: f32,
    seed: u64,
    max_tokens: usize,
    prompt_id: String,
    prompt_token_count: usize,
    chunk_tokens: usize,
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

#[derive(Debug, Deserialize, Serialize)]
struct HarnessRequest {
    kind: String,
    session_id: Option<String>,
    chunk_index: Option<usize>,
    total_pages: Option<usize>,
    token_start: Option<usize>,
    tokens: Option<Vec<i32>>,
}

impl HarnessRequest {
    fn prefill_chunk(
        session_id: &str,
        chunk_index: usize,
        total_pages: usize,
        token_start: usize,
        tokens: &[i32],
    ) -> Self {
        Self {
            kind: "prefill_chunk".to_string(),
            session_id: Some(session_id.to_string()),
            chunk_index: Some(chunk_index),
            total_pages: Some(total_pages),
            token_start: Some(token_start),
            tokens: Some(tokens.to_vec()),
        }
    }

    fn stop(session_id: &str) -> Self {
        Self {
            kind: "stop".to_string(),
            session_id: Some(session_id.to_string()),
            chunk_index: None,
            total_pages: None,
            token_start: None,
            tokens: None,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct HarnessResponse {
    kind: String,
    status: String,
    error: Option<String>,
    manifest: Option<KvPageManifest>,
    page_export_ms: Option<f64>,
}

impl HarnessResponse {
    fn page(manifest: KvPageManifest, page_export_ms: f64) -> Self {
        Self {
            kind: "page".to_string(),
            status: "ok".to_string(),
            error: None,
            manifest: Some(manifest),
            page_export_ms: Some(page_export_ms),
        }
    }

    fn ok(kind: &str) -> Self {
        Self {
            kind: kind.to_string(),
            status: "ok".to_string(),
            error: None,
            manifest: None,
            page_export_ms: None,
        }
    }

    fn error(reason: &str) -> Self {
        Self {
            kind: "error".to_string(),
            status: "error".to_string(),
            error: Some(reason.to_string()),
            manifest: None,
            page_export_ms: None,
        }
    }
}

#[derive(Debug)]
struct DecodeComparison {
    matches: bool,
    label: &'static str,
    first_divergence_index: Option<usize>,
    baseline_token_id: Option<i32>,
    page_token_id: Option<i32>,
    baseline_token_count: usize,
    page_token_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BootstrapPlan {
    imported_token_count: usize,
    trim_target_position: usize,
    replay_token_position: usize,
    decode_start_position: usize,
    replay_token: i32,
}

#[derive(Debug)]
struct BootstrapDecode {
    first_token: i32,
    report: BootstrapReport,
}

fn source_skeleton_report(bind_addr: String, report_out: Option<&Path>) -> KvPageHandoffReport {
    let _ = (bind_addr, report_out);
    KvPageHandoffReport {
        mode: "pd-kv-page-handoff-spike",
        role: "source",
        result: "inconclusive",
        recommendation: RECOMMENDATION_SOURCE_UNSUPPORTED,
        runtime_path: RuntimePathReport {
            source_runtime_export_kv_page: "skeleton_not_started",
            target_runtime_import_kv_page: "not_applicable",
            network_transport: "not_started",
            full_state_handoff_allowed_as_pass: false,
        },
        local_manifest_validation: CheckReport {
            status: "not_run",
            phase: "source_runtime_loop",
            reason:
                "source runtime loop requires --model and a separately authorized foreground run",
        },
        negative_checks: Vec::new(),
        bootstrap: bootstrap_not_run("source_role"),
        telemetry_shape: TelemetryShapeReport {
            page_count: 0,
            total_page_bytes: 0,
            page_export_ms: Vec::new(),
            page_transfer_ms: Vec::new(),
            page_import_ms: Vec::new(),
            decode_ttft_after_import_ms: None,
        },
        baseline: BaselineReport {
            deterministic_settings: DeterministicSettingsReport {
                temperature: 0.0,
                seed: 42,
                max_tokens: 0,
                prompt_id: "not_started".to_string(),
                prompt_token_count: 0,
                chunk_tokens: 0,
            },
            strategy: "not_run",
            one_shot_full_state_baseline: "not_run",
            page_handoff_decode: "not_run",
            comparison: "not_run",
            failure_reason: None,
            first_divergence_index: None,
            baseline_token_id: None,
            page_token_id: None,
            baseline_token_count: None,
            page_token_count: None,
        },
        privacy: privacy_report(),
        remaining_authorization_required: vec![
            "start PGX foreground source process",
            "run Mac coordinator foreground smoke",
        ],
    }
}

fn run_source_runtime_loop(args: &KvPageHandoffSourceArgs) -> Result<KvPageHandoffReport> {
    let model_path = args
        .model
        .as_ref()
        .context("--model is required to start kv-page-handoff source runtime loop")?;
    let identity = identity_from_source_args(args);
    let model = StageModel::open(model_path, &runtime_config_for_source(args))
        .context("open kv-page source runtime model")?;
    let mut session = model
        .create_session()
        .context("create kv-page source runtime session")?;
    let listener = TcpListener::bind(args.bind_addr).context("bind kv-page source listener")?;
    let (mut stream, _) = listener.accept().context("accept kv-page coordinator")?;
    let mut exported_pages = 0usize;
    let mut prefilled_tokens = 0usize;
    let mut total_page_bytes = 0u64;
    let mut page_export_ms = Vec::new();

    loop {
        let request: HarnessRequest =
            read_json_frame(&mut stream).context("read kv-page source request")?;
        match request.kind.as_str() {
            "prefill_chunk" => {
                if request.session_id.as_deref() != Some(args.session_id.as_str()) {
                    write_json_frame(&mut stream, &HarnessResponse::error("session_id"), &[])?;
                    continue;
                }
                let Some(tokens) = request.tokens.as_ref() else {
                    write_json_frame(&mut stream, &HarnessResponse::error("tokens"), &[])?;
                    continue;
                };
                let token_start = request.token_start.unwrap_or(0);
                let started = Instant::now();
                session
                    .prefill_chunk(tokens)
                    .context("source prefill chunk failed")?;
                prefilled_tokens = prefilled_tokens.saturating_add(tokens.len());
                let pages = session
                    .export_kv_page_segments(
                        0,
                        i32::try_from(args.layer_end).context("layer_end exceeds i32")?,
                        u64::try_from(token_start).context("token_start exceeds u64")?,
                        u64::try_from(tokens.len()).context("token count exceeds u64")?,
                    )
                    .context("source export_kv_page failed")?;
                let export_ms = elapsed_ms(started);
                let chunk_index = request.chunk_index.unwrap_or(exported_pages);
                let total_chunks = request.total_pages.unwrap_or(args.chunk_count);
                let segment_count = pages.len().max(1);
                let total_pages = total_chunks.saturating_mul(segment_count);
                for (segment_index, page) in pages.into_iter().enumerate() {
                    let page_index = chunk_index
                        .saturating_mul(segment_count)
                        .saturating_add(segment_index);
                    let manifest = manifest_from_runtime_page(
                        page_index,
                        total_pages,
                        token_start,
                        token_start + tokens.len(),
                        &identity,
                        &page.desc,
                        &page.payload,
                    );
                    let response = HarnessResponse::page(manifest, export_ms);
                    write_json_frame(&mut stream, &response, &page.payload)
                        .context("write kv-page source response")?;
                    exported_pages += 1;
                    total_page_bytes = total_page_bytes.saturating_add(page.payload.len() as u64);
                    page_export_ms.push(export_ms);
                }
                write_json_frame(&mut stream, &HarnessResponse::ok("chunk_done"), &[])
                    .context("write kv-page source chunk_done")?;
            }
            "stop" => {
                write_json_frame(&mut stream, &HarnessResponse::ok("stopped"), &[])?;
                break;
            }
            _ => {
                write_json_frame(&mut stream, &HarnessResponse::error("request_kind"), &[])?;
            }
        }
    }

    Ok(KvPageHandoffReport {
        mode: "pd-kv-page-handoff-spike",
        role: "source",
        result: "inconclusive",
        recommendation: RECOMMENDATION_READY,
        runtime_path: RuntimePathReport {
            source_runtime_export_kv_page: "implemented_not_foreground_verified",
            target_runtime_import_kv_page: "not_applicable",
            network_transport: "test_harness_json_plus_payload",
            full_state_handoff_allowed_as_pass: false,
        },
        local_manifest_validation: CheckReport {
            status: "pass",
            phase: "source_runtime_loop",
            reason: "source runtime loop can prefill chunks and export KV pages when foreground smoke is authorized",
        },
        negative_checks: Vec::new(),
        bootstrap: bootstrap_not_run("source_role"),
        telemetry_shape: TelemetryShapeReport {
            page_count: exported_pages,
            total_page_bytes,
            page_export_ms,
            page_transfer_ms: Vec::new(),
            page_import_ms: Vec::new(),
            decode_ttft_after_import_ms: None,
        },
        baseline: BaselineReport {
            deterministic_settings: DeterministicSettingsReport {
                temperature: 0.0,
                seed: 42,
                max_tokens: 0,
                prompt_id: "source_runtime".to_string(),
                prompt_token_count: prefilled_tokens,
                chunk_tokens: args.chunk_tokens,
            },
            strategy: "not_run",
            one_shot_full_state_baseline: "not_run",
            page_handoff_decode: "not_run",
            comparison: "not_run",
            failure_reason: None,
            first_divergence_index: None,
            baseline_token_id: None,
            page_token_id: None,
            baseline_token_count: None,
            page_token_count: None,
        },
        privacy: privacy_report(),
        remaining_authorization_required: vec![
            "run Mac coordinator foreground smoke",
            "compare page decode with one-shot full-state baseline",
        ],
    })
}

fn run_coordinator_runtime_loop(
    args: &KvPageHandoffCoordinatorArgs,
) -> Result<KvPageHandoffReport> {
    let model_path = args
        .model
        .as_ref()
        .context("--model is required to run kv-page-handoff coordinator runtime loop")?;
    let _started = Instant::now();
    let identity = identity_from_coordinator_args(args);
    let model = StageModel::open(model_path, &runtime_config_for_coordinator(args))
        .context("open kv-page coordinator runtime model")?;
    let prompt_tokens = synthetic_prompt_tokens(&model, args.total_tokens)
        .context("build sanitized synthetic token set")?;
    let chunks = split_tokens(&prompt_tokens, args.chunk_tokens);
    let mut stream = TcpStream::connect(args.source_addr).context("connect kv-page source")?;
    let mut records = Vec::with_capacity(chunks.len());
    let mut page_export_ms = Vec::with_capacity(chunks.len());
    let mut page_transfer_ms = Vec::with_capacity(chunks.len());

    for (index, (token_start, tokens)) in chunks.iter().enumerate() {
        let request = HarnessRequest::prefill_chunk(
            &args.session_id,
            index,
            chunks.len(),
            *token_start,
            tokens,
        );
        let transfer_started = Instant::now();
        write_json_frame(&mut stream, &request, &[]).context("send kv-page prefill request")?;
        let transfer_ms = elapsed_ms(transfer_started);
        loop {
            let (response, payload): (HarnessResponse, Vec<u8>) =
                read_json_frame_with_payload(&mut stream)
                    .context("read kv-page source response")?;
            if response.kind == "chunk_done" {
                break;
            }
            let Some(manifest) = response.manifest else {
                bail!("source response did not include KV page manifest");
            };
            if response.kind != "page" {
                bail!("expected KV page response, got {}", response.kind);
            }
            page_export_ms.push(response.page_export_ms.unwrap_or(0.0));
            page_transfer_ms.push(transfer_ms);
            records.push(KvPageRecord {
                manifest,
                payload,
                payload_kind: KvPagePayloadKind::KvPage,
            });
        }
    }
    write_json_frame(&mut stream, &HarnessRequest::stop(&args.session_id), &[]).ok();

    let validation = validate_kv_page_handoff(&records, &identity)
        .map_err(|error| anyhow::anyhow!(error.reason))?;

    let baseline_tokens = match run_local_one_shot_decode_baseline(
        &model,
        &prompt_tokens,
        args.max_tokens,
        args.seed,
    ) {
        Ok(tokens) => tokens,
        Err(error) => {
            return Ok(baseline_unavailable_report(
                args,
                validation,
                page_export_ms,
                page_transfer_ms,
                baseline_failure_reason(&error),
            ));
        }
    };

    let import_model = StageModel::open(model_path, &runtime_config_for_coordinator(args))
        .context("open kv-page import runtime model")?;
    let mut import_session = import_model
        .create_session()
        .context("create kv-page import session")?;
    let mut page_import_ms = Vec::with_capacity(records.len());
    for record in &records {
        let desc = runtime_desc_from_manifest(&record.manifest)
            .context("manifest missing native KV page descriptor")?;
        let import_started = Instant::now();
        import_session
            .import_kv_page(&desc, &record.payload)
            .context("import native KV page")?;
        page_import_ms.push(elapsed_ms(import_started));
    }
    let bootstrap = bootstrap_imported_page_decode_state(
        &mut import_session,
        &prompt_tokens,
        validation.final_decode_start_position,
        args.bootstrap_strategy,
        args.seed,
    )
    .context("bootstrap imported KV page decode state")?;
    let decode_started = Instant::now();
    let page_tokens = decode_tokens_from_first(
        &mut import_session,
        bootstrap.first_token,
        args.max_tokens,
        args.seed,
    )
    .context("decode from bootstrapped imported KV pages")?;
    let decode_ttft_after_import_ms = elapsed_ms(decode_started);

    let comparison = compare_tokens(&baseline_tokens, &page_tokens);
    let result = if comparison.matches { "pass" } else { "fail" };
    let recommendation = if comparison.matches {
        "proceed_to_streaming_handoff"
    } else {
        "redesign"
    };

    Ok(KvPageHandoffReport {
        mode: "pd-kv-page-handoff-spike",
        role: "coordinator",
        result,
        recommendation,
        runtime_path: RuntimePathReport {
            source_runtime_export_kv_page: "observed",
            target_runtime_import_kv_page: "observed",
            network_transport: "test_harness_json_plus_payload",
            full_state_handoff_allowed_as_pass: false,
        },
        local_manifest_validation: CheckReport {
            status: "pass",
            phase: "foreground_runtime_manifest_validation",
            reason: if comparison.matches {
                "native page import decode matched one-shot baseline"
            } else {
                "native page import decode diverged from one-shot baseline"
            },
        },
        negative_checks: negative_checks(&records, &identity),
        bootstrap: bootstrap.report,
        telemetry_shape: TelemetryShapeReport {
            page_count: validation.page_count,
            total_page_bytes: validation.total_page_bytes,
            page_export_ms,
            page_transfer_ms,
            page_import_ms,
            decode_ttft_after_import_ms: Some(decode_ttft_after_import_ms),
        },
        baseline: BaselineReport {
            deterministic_settings: DeterministicSettingsReport {
                temperature: 0.0,
                seed: args.seed,
                max_tokens: args.max_tokens,
                prompt_id: sanitized_prompt_id(&args.prompt_id),
                prompt_token_count: prompt_tokens.len(),
                chunk_tokens: args.chunk_tokens,
            },
            strategy: "local_one_shot_prefill_decode",
            one_shot_full_state_baseline: "not_used",
            page_handoff_decode: "observed",
            comparison: comparison.label,
            failure_reason: None,
            first_divergence_index: comparison.first_divergence_index,
            baseline_token_id: comparison.baseline_token_id,
            page_token_id: comparison.page_token_id,
            baseline_token_count: Some(comparison.baseline_token_count),
            page_token_count: Some(comparison.page_token_count),
        },
        privacy: privacy_report(),
        remaining_authorization_required: Vec::new(),
    })
}

fn run_local_coordinator_harness(args: &KvPageHandoffCoordinatorArgs) -> KvPageHandoffReport {
    let started = Instant::now();
    let identity = expected_identity();
    let records = synthetic_records(args.total_tokens, args.chunk_tokens, &identity);
    let validation = validate_kv_page_handoff(&records, &identity);
    let local_manifest_validation = match validation.as_ref() {
        Ok(_) => CheckReport {
            status: "pass",
            phase: "local_manifest_validation",
            reason: "synthetic two-page manifest accepted without using full-state handoff",
        },
        Err(error) => CheckReport {
            status: "fail",
            phase: "local_manifest_validation",
            reason: error.reason,
        },
    };
    let negative_checks = negative_checks(&records, &identity);
    let telemetry_shape = match validation {
        Ok(validation) => TelemetryShapeReport {
            page_count: validation.page_count,
            total_page_bytes: validation.total_page_bytes,
            page_export_ms: vec![0.0; validation.page_count],
            page_transfer_ms: vec![0.0; validation.page_count],
            page_import_ms: vec![0.0; validation.page_count],
            decode_ttft_after_import_ms: None,
        },
        Err(_) => TelemetryShapeReport {
            page_count: 0,
            total_page_bytes: 0,
            page_export_ms: Vec::new(),
            page_transfer_ms: Vec::new(),
            page_import_ms: Vec::new(),
            decode_ttft_after_import_ms: None,
        },
    };
    let _elapsed_ms = elapsed_ms(started);

    KvPageHandoffReport {
        mode: "pd-kv-page-handoff-spike",
        role: "coordinator",
        result: "inconclusive",
        recommendation: RECOMMENDATION_READY,
        runtime_path: RuntimePathReport {
            source_runtime_export_kv_page: "not_run",
            target_runtime_import_kv_page: "not_run",
            network_transport: "skeleton_not_started",
            full_state_handoff_allowed_as_pass: false,
        },
        local_manifest_validation,
        negative_checks,
        bootstrap: bootstrap_not_run("local_manifest_only"),
        telemetry_shape,
        baseline: BaselineReport {
            deterministic_settings: DeterministicSettingsReport {
                temperature: 0.0,
                seed: args.seed,
                max_tokens: args.max_tokens,
                prompt_id: sanitized_prompt_id(&args.prompt_id),
                prompt_token_count: args.total_tokens,
                chunk_tokens: args.chunk_tokens,
            },
            strategy: "not_run",
            one_shot_full_state_baseline: "not_run",
            page_handoff_decode: "not_run",
            comparison: "not_run",
            failure_reason: None,
            first_divergence_index: None,
            baseline_token_id: None,
            page_token_id: None,
            baseline_token_count: None,
            page_token_count: None,
        },
        privacy: privacy_report(),
        remaining_authorization_required: vec![
            "implement network transport or binary-control source role",
            "start PGX foreground source process",
            "import native pages into Mac runtime",
            "compare page decode with one-shot full-state baseline",
        ],
    }
}

fn runtime_config_for_source(args: &KvPageHandoffSourceArgs) -> RuntimeConfig {
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

fn runtime_config_for_coordinator(args: &KvPageHandoffCoordinatorArgs) -> RuntimeConfig {
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

fn identity_from_source_args(args: &KvPageHandoffSourceArgs) -> KvPageIdentity {
    KvPageIdentity {
        artifact_sha256: args.artifact_sha256.clone(),
        tokenizer_hash: args.tokenizer_hash.clone(),
        chat_template_hash: args.chat_template_hash.clone(),
        dtype: "f16".to_string(),
        layout: "llama.cpp-kv-page".to_string(),
        source_capability: SOURCE_CAPABILITY.to_string(),
        target_capability: TARGET_CAPABILITY.to_string(),
    }
}

fn identity_from_coordinator_args(args: &KvPageHandoffCoordinatorArgs) -> KvPageIdentity {
    KvPageIdentity {
        artifact_sha256: args.artifact_sha256.clone(),
        tokenizer_hash: args.tokenizer_hash.clone(),
        chat_template_hash: args.chat_template_hash.clone(),
        dtype: "f16".to_string(),
        layout: "llama.cpp-kv-page".to_string(),
        source_capability: SOURCE_CAPABILITY.to_string(),
        target_capability: TARGET_CAPABILITY.to_string(),
    }
}

fn synthetic_prompt_tokens(model: &StageModel, target_tokens: usize) -> Result<Vec<i32>> {
    let mut text = String::from("kv page handoff synthetic prompt. ");
    let mut tokens = model
        .tokenize(&text, true)
        .context("tokenize synthetic prompt")?;
    while tokens.len() < target_tokens {
        text.push_str("repeatable synthetic context. ");
        tokens = model
            .tokenize(&text, true)
            .context("tokenize expanded synthetic prompt")?;
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
            .context("decode next token after bootstrap")?;
    }
    Ok(out)
}

fn bootstrap_imported_page_decode_state(
    session: &mut skippy_runtime::StageSession,
    prompt_tokens: &[i32],
    imported_token_count: usize,
    strategy: KvPageBootstrapStrategy,
    seed: u64,
) -> Result<BootstrapDecode> {
    match strategy {
        KvPageBootstrapStrategy::TrimReplayLastToken => {
            let plan = trim_replay_last_token_plan(prompt_tokens, imported_token_count)
                .map_err(|reason| anyhow::anyhow!(reason))?;
            session
                .trim_session(plan.trim_target_position as u64)
                .context("trim imported KV page state before replaying last prompt token")?;
            if session.token_count() != plan.trim_target_position as u64 {
                bail!("bootstrap_trim_position_mismatch");
            }
            let started = Instant::now();
            let first_token = session
                .decode_step_sampled(plan.replay_token, Some(&deterministic_sampling(seed)))
                .context("replay last prompt token for imported KV page logits")?;
            let bootstrap_eval_ms = elapsed_ms(started);
            let decode_start_position = session.token_count();
            if decode_start_position != plan.decode_start_position as u64 {
                bail!("bootstrap_decode_start_position_mismatch");
            }
            Ok(BootstrapDecode {
                first_token,
                report: BootstrapReport {
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
) -> Result<BootstrapPlan, &'static str> {
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
    Ok(BootstrapPlan {
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

fn bootstrap_not_run(reason: &'static str) -> BootstrapReport {
    BootstrapReport {
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

fn baseline_unavailable_report(
    args: &KvPageHandoffCoordinatorArgs,
    validation: KvPageValidation,
    page_export_ms: Vec<f64>,
    page_transfer_ms: Vec<f64>,
    failure_reason: &'static str,
) -> KvPageHandoffReport {
    KvPageHandoffReport {
        mode: "pd-kv-page-handoff-spike",
        role: "coordinator",
        result: "inconclusive",
        recommendation: "fix_baseline_harness_before_streaming",
        runtime_path: RuntimePathReport {
            source_runtime_export_kv_page: "observed",
            target_runtime_import_kv_page: "not_run_baseline_unavailable",
            network_transport: "test_harness_json_plus_payload",
            full_state_handoff_allowed_as_pass: false,
        },
        local_manifest_validation: CheckReport {
            status: "pass",
            phase: "foreground_runtime_manifest_validation",
            reason: "native page manifests validated before baseline became unavailable",
        },
        negative_checks: Vec::new(),
        bootstrap: bootstrap_not_run("baseline_unavailable"),
        telemetry_shape: TelemetryShapeReport {
            page_count: validation.page_count,
            total_page_bytes: validation.total_page_bytes,
            page_export_ms,
            page_transfer_ms,
            page_import_ms: Vec::new(),
            decode_ttft_after_import_ms: None,
        },
        baseline: BaselineReport {
            deterministic_settings: DeterministicSettingsReport {
                temperature: 0.0,
                seed: args.seed,
                max_tokens: args.max_tokens,
                prompt_id: sanitized_prompt_id(&args.prompt_id),
                prompt_token_count: args.total_tokens,
                chunk_tokens: args.chunk_tokens,
            },
            strategy: "local_one_shot_prefill_decode",
            one_shot_full_state_baseline: "not_used",
            page_handoff_decode: "not_run",
            comparison: "baseline_unavailable",
            failure_reason: Some(failure_reason),
            first_divergence_index: None,
            baseline_token_id: None,
            page_token_id: None,
            baseline_token_count: None,
            page_token_count: None,
        },
        privacy: privacy_report(),
        remaining_authorization_required: vec!["rerun 128-token two-chunk foreground smoke"],
    }
}

fn baseline_failure_reason(error: &anyhow::Error) -> &'static str {
    let message = error.to_string();
    if message.contains("no skippy execution lane") {
        "no_skippy_execution_lane"
    } else if message.contains("sample current token") {
        "sample_current_failed"
    } else if message.contains("prefill") {
        "prefill_failed"
    } else if message.contains("decode") {
        "decode_failed"
    } else {
        "local_one_shot_baseline_unavailable"
    }
}

fn run_local_one_shot_decode_baseline(
    model: &StageModel,
    prompt_tokens: &[i32],
    max_tokens: usize,
    seed: u64,
) -> Result<Vec<i32>> {
    let mut session = model
        .create_session()
        .context("create local one-shot baseline session")?;
    session
        .prefill_chunked(prompt_tokens)
        .context("local one-shot baseline prefill")?;
    let bootstrap = bootstrap_imported_page_decode_state(
        &mut session,
        prompt_tokens,
        prompt_tokens.len(),
        KvPageBootstrapStrategy::TrimReplayLastToken,
        seed,
    )
    .context("bootstrap local one-shot baseline decode state")?;
    decode_tokens_from_first(&mut session, bootstrap.first_token, max_tokens, seed)
}

fn deterministic_sampling(seed: u64) -> SamplingConfig {
    SamplingConfig {
        temperature: 0.0,
        seed: u32::try_from(seed).unwrap_or(u32::MAX),
        ..SamplingConfig::default()
    }
}

fn compare_tokens(baseline: &[i32], page: &[i32]) -> DecodeComparison {
    if baseline == page {
        DecodeComparison {
            matches: true,
            label: "exact_token_match",
            first_divergence_index: None,
            baseline_token_id: None,
            page_token_id: None,
            baseline_token_count: baseline.len(),
            page_token_count: page.len(),
        }
    } else {
        let first_divergence_index = baseline
            .iter()
            .zip(page.iter())
            .position(|(baseline, page)| baseline != page)
            .or_else(|| Some(baseline.len().min(page.len())));
        let baseline_token_id =
            first_divergence_index.and_then(|index| baseline.get(index).copied());
        let page_token_id = first_divergence_index.and_then(|index| page.get(index).copied());
        DecodeComparison {
            matches: false,
            label: "token_divergence",
            first_divergence_index,
            baseline_token_id,
            page_token_id,
            baseline_token_count: baseline.len(),
            page_token_count: page.len(),
        }
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

fn sanitized_prompt_id(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_')
        .take(64)
        .collect::<String>()
}

fn emit_json_report(report: &KvPageHandoffReport, path: Option<&Path>) -> Result<()> {
    let json = serde_json::to_string_pretty(report)?;
    if let Some(path) = path {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, format!("{json}\n"))?;
    } else {
        println!("{json}");
    }
    Ok(())
}

fn emit_markdown_report(report: &KvPageHandoffReport, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        path,
        format!(
            "# PD KV Page Handoff Foreground Smoke Report\n\n\
             result: `{}`\n\n\
             recommendation: `{}`\n\n\
             role: `{}`\n\n\
             local manifest validation: `{}`\n\n\
             runtime export/import: `{}` / `{}`\n\n\
             bootstrap: `{}` `{}`\n\n\
             baseline strategy: `{}`\n\n\
             baseline comparison: `{}`\n",
            report.result,
            report.recommendation,
            report.role,
            report.local_manifest_validation.status,
            report.runtime_path.source_runtime_export_kv_page,
            report.runtime_path.target_runtime_import_kv_page,
            report.bootstrap.strategy,
            report.bootstrap.status,
            report.baseline.strategy,
            report.baseline.comparison
        ),
    )?;
    Ok(())
}

fn write_json_frame<T: Serialize>(stream: &mut TcpStream, value: &T, payload: &[u8]) -> Result<()> {
    let json = serde_json::to_vec(value).context("serialize kv-page frame header")?;
    let header_len = u32::try_from(json.len()).context("kv-page header exceeds u32")?;
    let payload_len = u64::try_from(payload.len()).context("kv-page payload exceeds u64")?;
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
        bail!("unexpected kv-page request payload");
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
    let value = serde_json::from_slice(&header).context("parse kv-page frame header")?;
    let mut payload = vec![0u8; payload_len];
    stream.read_exact(&mut payload)?;
    Ok((value, payload))
}

fn elapsed_ms(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1000.0
}
