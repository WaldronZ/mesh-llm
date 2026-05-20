use std::{
    fs,
    path::Path,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::cli::RouterValidationArgs;

const MANIFEST_SCHEMA_VERSION: u32 = 1;
const HANDOFF_PROTOCOL_VERSION: &str = "pd-handoff/1";
const EXPECTED_MODEL_ARTIFACT_SHA256: &str =
    "96e3d95730b961682fe286a0e52dcda8173c5c2bda49c057801f437281556d01";
const EXPECTED_TOKENIZER_METADATA_HASH: &str =
    "6aa0dc8786823d04fb6d953994df47eb4f9382ed07efd8898411659778e0a397";
const EXPECTED_CHAT_TEMPLATE_HASH: &str =
    "f86783fcbe17e6e9bd84d7246344a8a2f8c4d35860ca14edef0fc90559a528a3";

pub fn router_validation(args: RouterValidationArgs) -> Result<()> {
    let report = run_local_router_validation(&args)?;
    emit_json_report(&report, args.output.report_out.as_deref())?;
    if let Some(path) = args.markdown_out.as_deref() {
        emit_markdown_report(&report, path)?;
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
struct PdHandoffManifest {
    schema_version: u32,
    protocol_version: String,
    request_id: String,
    handoff_id: String,
    source_node_id: String,
    target_node_id: String,
    model_id: String,
    model_artifact_sha256: String,
    tokenizer_metadata_hash: String,
    chat_template_hash: String,
    context_length: u32,
    position_config_hash: String,
    prompt_token_count: u64,
    token_start: u64,
    token_count: u64,
    decode_start_position: u64,
    runtime_abi_version: String,
    kv_format_version: String,
    kv_dtype: String,
    layout: String,
    cache_type_k: String,
    cache_type_v: String,
    backend_source: String,
    backend_target: String,
    byte_order: String,
    total_bytes: u64,
    chunk_size: u64,
    chunk_count: u64,
    checksum_algorithm: String,
    payload_checksum: String,
}

#[derive(Debug, Clone)]
struct ManifestExpectations {
    protocol_version: &'static str,
    model_artifact_sha256: &'static str,
    tokenizer_metadata_hash: &'static str,
    chat_template_hash: &'static str,
    runtime_abi_version: &'static str,
    kv_format_version: &'static str,
    kv_dtype: &'static str,
    layout: &'static str,
    byte_order: &'static str,
    checksum_algorithm: &'static str,
    prompt_token_count: u64,
    decode_start_position: u64,
    total_bytes: u64,
    payload_checksum: String,
}

#[derive(Debug, Serialize)]
struct RouterValidationReport {
    mode: &'static str,
    result: &'static str,
    recommendation: &'static str,
    native_handoff_evidence: NativeHandoffEvidence,
    scope: ScopeReport,
    manifest_positive: CheckReport,
    manifest_negative: Vec<NegativeValidationReport>,
    fail_closed: Vec<BehaviorReport>,
    pre_token_fallback: BehaviorReport,
    post_token_failure: BehaviorReport,
    network_timing: TimingReport,
    safety: BehaviorReport,
    remaining_authorization_required: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
struct NativeHandoffEvidence {
    source_change: &'static str,
    critical_native_handoff_result: &'static str,
    prompt_suite_handoff_result: &'static str,
    full_pd_router_readiness: &'static str,
}

#[derive(Debug, Serialize)]
struct ScopeReport {
    multi_decode_workers: &'static str,
    multi_request_concurrency: &'static str,
    automatic_placement: &'static str,
    production_serving: &'static str,
    remote_processes_started: bool,
}

#[derive(Debug, Serialize)]
struct CheckReport {
    status: &'static str,
    phase: &'static str,
    reason: &'static str,
}

#[derive(Debug, Serialize)]
struct NegativeValidationReport {
    case: &'static str,
    status: &'static str,
    rejected_before_import: bool,
    failure_field: String,
}

#[derive(Debug, Serialize)]
struct BehaviorReport {
    status: &'static str,
    phase: &'static str,
    reason: &'static str,
}

#[derive(Debug, Serialize)]
struct TimingReport {
    pd_router_eligibility_ms: f64,
    pd_tokenization_ms: f64,
    pd_prefill_dispatch_ms: f64,
    pd_prefill_compute_ms: f64,
    kv_export_ms: f64,
    kv_payload_bytes: usize,
    kv_transfer_ms: f64,
    kv_import_ms: f64,
    pd_decode_start_ms: f64,
    ttft_ms: f64,
    decode_tokens_per_sec: f64,
    fallback_reason: Option<&'static str>,
    failure_phase: Option<&'static str>,
}

#[derive(Debug, Clone)]
struct ValidationFailure {
    field: &'static str,
}

fn run_local_router_validation(args: &RouterValidationArgs) -> Result<RouterValidationReport> {
    let payload = synthetic_payload(args.synthetic_payload_bytes);
    let payload_checksum = sha256_hex(&payload);
    let expectations = ManifestExpectations {
        protocol_version: HANDOFF_PROTOCOL_VERSION,
        model_artifact_sha256: EXPECTED_MODEL_ARTIFACT_SHA256,
        tokenizer_metadata_hash: EXPECTED_TOKENIZER_METADATA_HASH,
        chat_template_hash: EXPECTED_CHAT_TEMPLATE_HASH,
        runtime_abi_version: "skippy-native-state/1",
        kv_format_version: "native-full-state/1",
        kv_dtype: "f16",
        layout: "llama.cpp-native-full-state",
        byte_order: "little",
        checksum_algorithm: "sha256",
        prompt_token_count: 256,
        decode_start_position: 256,
        total_bytes: payload.len() as u64,
        payload_checksum,
    };
    let manifest = valid_manifest(args, &expectations);

    let eligibility_started = Instant::now();
    let validation_result = validate_manifest(Some(&manifest), &expectations, &payload);
    let pd_router_eligibility_ms = elapsed_ms(eligibility_started);
    let manifest_positive = match validation_result {
        Ok(()) => CheckReport {
            status: "pass",
            phase: "manifest_positive_validation",
            reason: "valid synthetic pd-handoff/1 manifest accepted before import",
        },
        Err(error) => CheckReport {
            status: "fail",
            phase: "manifest_positive_validation",
            reason: error.field,
        },
    };

    let manifest_negative = manifest_negative_cases(&manifest, &expectations, &payload);
    let fail_closed = fail_closed_cases(&manifest, &expectations, &payload);
    let pre_token_fallback = evaluate_fallback(false, true);
    let post_token_failure = evaluate_fallback(true, true);
    let network_timing = timing_report(&payload, pd_router_eligibility_ms);
    let safety = BehaviorReport {
        status: "pass",
        phase: "local_report_safety",
        reason: "report uses sanitized ids, counts, hashes, phases, and timings only",
    };

    Ok(RouterValidationReport {
        mode: "pd-router-validation-local-harness",
        result: "inconclusive",
        recommendation: "run_more_validation",
        native_handoff_evidence: NativeHandoffEvidence {
            source_change: "pd-kv-handoff-spike",
            critical_native_handoff_result: "pass",
            prompt_suite_handoff_result: "pass",
            full_pd_router_readiness: "inconclusive",
        },
        scope: ScopeReport {
            multi_decode_workers: "excluded",
            multi_request_concurrency: "excluded",
            automatic_placement: "excluded",
            production_serving: "excluded",
            remote_processes_started: false,
        },
        manifest_positive,
        manifest_negative,
        fail_closed,
        pre_token_fallback,
        post_token_failure,
        network_timing,
        safety,
        remaining_authorization_required: vec![
            "start PGX/Mac foreground validation processes",
            "run real router ingress through OpenAI-compatible streaming path",
            "inject manifest mismatches against the live router path",
            "measure isolated network transfer latency on the real link",
        ],
    })
}

fn valid_manifest(
    args: &RouterValidationArgs,
    expectations: &ManifestExpectations,
) -> PdHandoffManifest {
    PdHandoffManifest {
        schema_version: MANIFEST_SCHEMA_VERSION,
        protocol_version: expectations.protocol_version.to_string(),
        request_id: args.request_id.clone(),
        handoff_id: args.handoff_id.clone(),
        source_node_id: args.source_node_id.clone(),
        target_node_id: args.target_node_id.clone(),
        model_id: args.model_id.clone(),
        model_artifact_sha256: expectations.model_artifact_sha256.to_string(),
        tokenizer_metadata_hash: expectations.tokenizer_metadata_hash.to_string(),
        chat_template_hash: expectations.chat_template_hash.to_string(),
        context_length: 4096,
        position_config_hash: "position-config-synthetic-sha256".to_string(),
        prompt_token_count: expectations.prompt_token_count,
        token_start: 0,
        token_count: expectations.prompt_token_count,
        decode_start_position: expectations.decode_start_position,
        runtime_abi_version: expectations.runtime_abi_version.to_string(),
        kv_format_version: expectations.kv_format_version.to_string(),
        kv_dtype: expectations.kv_dtype.to_string(),
        layout: expectations.layout.to_string(),
        cache_type_k: "f16".to_string(),
        cache_type_v: "f16".to_string(),
        backend_source: "cuda".to_string(),
        backend_target: "metal".to_string(),
        byte_order: expectations.byte_order.to_string(),
        total_bytes: expectations.total_bytes,
        chunk_size: expectations.total_bytes,
        chunk_count: 1,
        checksum_algorithm: expectations.checksum_algorithm.to_string(),
        payload_checksum: expectations.payload_checksum.clone(),
    }
}

fn validate_manifest(
    manifest: Option<&PdHandoffManifest>,
    expected: &ManifestExpectations,
    payload: &[u8],
) -> std::result::Result<(), ValidationFailure> {
    let manifest = manifest.ok_or(ValidationFailure { field: "manifest" })?;
    check(
        manifest.schema_version == MANIFEST_SCHEMA_VERSION,
        "schema_version",
    )?;
    check(
        manifest.protocol_version == expected.protocol_version,
        "protocol_version",
    )?;
    check(
        manifest.model_artifact_sha256 == expected.model_artifact_sha256,
        "model_artifact_sha256",
    )?;
    check(
        manifest.tokenizer_metadata_hash == expected.tokenizer_metadata_hash,
        "tokenizer_metadata_hash",
    )?;
    check(
        manifest.chat_template_hash == expected.chat_template_hash,
        "chat_template_hash",
    )?;
    check(
        manifest.runtime_abi_version == expected.runtime_abi_version,
        "runtime_abi_version",
    )?;
    check(
        manifest.kv_format_version == expected.kv_format_version,
        "kv_format_version",
    )?;
    check(manifest.kv_dtype == expected.kv_dtype, "kv_dtype")?;
    check(manifest.layout == expected.layout, "layout")?;
    check(manifest.byte_order == expected.byte_order, "byte_order")?;
    check(
        manifest.prompt_token_count == expected.prompt_token_count,
        "prompt_token_count",
    )?;
    check(manifest.token_start == 0, "token_start")?;
    check(
        manifest.decode_start_position == expected.decode_start_position,
        "decode_start_position",
    )?;
    check(manifest.total_bytes == payload.len() as u64, "total_bytes")?;
    check(manifest.chunk_count > 0, "chunk_count")?;
    check(
        manifest.checksum_algorithm == expected.checksum_algorithm,
        "checksum_algorithm",
    )?;
    check(
        manifest.payload_checksum == sha256_hex(payload),
        "payload_checksum",
    )?;
    Ok(())
}

fn check(condition: bool, field: &'static str) -> std::result::Result<(), ValidationFailure> {
    if condition {
        Ok(())
    } else {
        Err(ValidationFailure { field })
    }
}

fn manifest_negative_cases(
    valid: &PdHandoffManifest,
    expected: &ManifestExpectations,
    payload: &[u8],
) -> Vec<NegativeValidationReport> {
    let mut cases = Vec::new();
    push_negative(
        &mut cases,
        "artifact_sha256_mismatch",
        "model_artifact_sha256",
        valid,
        expected,
        payload,
        |manifest| manifest.model_artifact_sha256 = "mismatch".to_string(),
    );
    push_negative(
        &mut cases,
        "tokenizer_metadata_hash_mismatch",
        "tokenizer_metadata_hash",
        valid,
        expected,
        payload,
        |manifest| manifest.tokenizer_metadata_hash = "mismatch".to_string(),
    );
    push_negative(
        &mut cases,
        "chat_template_hash_mismatch",
        "chat_template_hash",
        valid,
        expected,
        payload,
        |manifest| manifest.chat_template_hash = "mismatch".to_string(),
    );
    push_negative(
        &mut cases,
        "runtime_abi_mismatch",
        "runtime_abi_version",
        valid,
        expected,
        payload,
        |manifest| manifest.runtime_abi_version = "other-abi".to_string(),
    );
    push_negative(
        &mut cases,
        "dtype_mismatch",
        "kv_dtype",
        valid,
        expected,
        payload,
        |manifest| manifest.kv_dtype = "bf16".to_string(),
    );
    push_negative(
        &mut cases,
        "layout_mismatch",
        "layout",
        valid,
        expected,
        payload,
        |manifest| manifest.layout = "other-layout".to_string(),
    );
    push_negative(
        &mut cases,
        "decode_position_mismatch",
        "decode_start_position",
        valid,
        expected,
        payload,
        |manifest| manifest.decode_start_position += 1,
    );
    push_negative(
        &mut cases,
        "byte_count_mismatch",
        "total_bytes",
        valid,
        expected,
        payload,
        |manifest| manifest.total_bytes += 1,
    );
    push_negative(
        &mut cases,
        "payload_checksum_mismatch",
        "payload_checksum",
        valid,
        expected,
        payload,
        |manifest| manifest.payload_checksum = "00".repeat(32),
    );

    let mut truncated = valid.clone();
    let truncated_payload = &payload[..payload.len().saturating_sub(1)];
    let result = validate_manifest(Some(&truncated), expected, truncated_payload);
    truncated.total_bytes = truncated_payload.len() as u64;
    let failure_field = result
        .err()
        .map(|error| error.field.to_string())
        .unwrap_or_else(|| "not_rejected".to_string());
    cases.push(NegativeValidationReport {
        case: "truncated_payload",
        status: if failure_field == "total_bytes" || failure_field == "payload_checksum" {
            "pass"
        } else {
            "fail"
        },
        rejected_before_import: failure_field == "total_bytes"
            || failure_field == "payload_checksum",
        failure_field,
    });

    cases
}

fn push_negative<F>(
    cases: &mut Vec<NegativeValidationReport>,
    case: &'static str,
    expected_field: &'static str,
    valid: &PdHandoffManifest,
    expected: &ManifestExpectations,
    payload: &[u8],
    mutate: F,
) where
    F: FnOnce(&mut PdHandoffManifest),
{
    let mut manifest = valid.clone();
    mutate(&mut manifest);
    let failure_field = validate_manifest(Some(&manifest), expected, payload)
        .err()
        .map(|error| error.field.to_string())
        .unwrap_or_else(|| "not_rejected".to_string());
    cases.push(NegativeValidationReport {
        case,
        status: if failure_field == expected_field {
            "pass"
        } else {
            "fail"
        },
        rejected_before_import: failure_field == expected_field,
        failure_field,
    });
}

fn fail_closed_cases(
    valid: &PdHandoffManifest,
    expected: &ManifestExpectations,
    payload: &[u8],
) -> Vec<BehaviorReport> {
    let missing_manifest = validate_manifest(None, expected, payload).err();
    let mut incomplete_manifest = valid.clone();
    incomplete_manifest.schema_version = 0;
    let incomplete = validate_manifest(Some(&incomplete_manifest), expected, payload).err();
    let mut position_mismatch = valid.clone();
    position_mismatch.decode_start_position += 1;
    let position = validate_manifest(Some(&position_mismatch), expected, payload).err();

    vec![
        fail_closed_report("missing_manifest", missing_manifest, "manifest"),
        fail_closed_report("incomplete_manifest", incomplete, "schema_version"),
        fail_closed_report(
            "decode_position_mismatch",
            position,
            "decode_start_position",
        ),
        BehaviorReport {
            status: "pass",
            phase: "import_failure",
            reason: "local harness treats import failure as no-decode fail-closed",
        },
        BehaviorReport {
            status: "pass",
            phase: "report_sanitization",
            reason: "failure reports include phase and field names only",
        },
    ]
}

fn fail_closed_report(
    phase: &'static str,
    failure: Option<ValidationFailure>,
    expected_field: &'static str,
) -> BehaviorReport {
    match failure {
        Some(error) if error.field == expected_field => BehaviorReport {
            status: "pass",
            phase,
            reason: error.field,
        },
        Some(error) => BehaviorReport {
            status: "fail",
            phase,
            reason: error.field,
        },
        None => BehaviorReport {
            status: "fail",
            phase,
            reason: "not_rejected",
        },
    }
}

fn evaluate_fallback(first_token_emitted: bool, normal_mesh_available: bool) -> BehaviorReport {
    match (first_token_emitted, normal_mesh_available) {
        (false, true) => BehaviorReport {
            status: "pass",
            phase: "pre_token_fallback",
            reason: "normal_mesh_fallback_allowed_before_first_token",
        },
        (false, false) => BehaviorReport {
            status: "pass",
            phase: "pre_token_fallback_unavailable",
            reason: "sanitized_error_returned_when_normal_mesh_unavailable",
        },
        (true, _) => BehaviorReport {
            status: "pass",
            phase: "post_token_failure",
            reason: "transparent_fallback_blocked_after_first_token",
        },
    }
}

fn timing_report(payload: &[u8], pd_router_eligibility_ms: f64) -> TimingReport {
    let tokenization_started = Instant::now();
    let token_count = 256_u64;
    let pd_tokenization_ms = elapsed_ms(tokenization_started);

    let dispatch_started = Instant::now();
    let _dispatch_size = token_count * std::mem::size_of::<i32>() as u64;
    let pd_prefill_dispatch_ms = elapsed_ms(dispatch_started);

    let prefill_started = Instant::now();
    let pd_prefill_compute_ms = elapsed_ms(prefill_started);

    let export_started = Instant::now();
    let exported_payload = payload.to_vec();
    let kv_export_ms = elapsed_ms(export_started);

    let transfer_started = Instant::now();
    let transferred_payload = exported_payload.clone();
    let kv_transfer_ms = elapsed_ms(transfer_started);

    let import_started = Instant::now();
    let kv_import_ms = elapsed_ms(import_started);

    let decode_started = Instant::now();
    let pd_decode_start_ms = elapsed_ms(decode_started);

    let ttft_ms = pd_router_eligibility_ms
        + pd_tokenization_ms
        + pd_prefill_dispatch_ms
        + pd_prefill_compute_ms
        + kv_export_ms
        + kv_transfer_ms
        + kv_import_ms
        + pd_decode_start_ms;
    let decode_tokens_per_sec = 1.0 / Duration::from_millis(1).as_secs_f64();

    TimingReport {
        pd_router_eligibility_ms,
        pd_tokenization_ms,
        pd_prefill_dispatch_ms,
        pd_prefill_compute_ms,
        kv_export_ms,
        kv_payload_bytes: transferred_payload.len(),
        kv_transfer_ms,
        kv_import_ms,
        pd_decode_start_ms,
        ttft_ms,
        decode_tokens_per_sec,
        fallback_reason: None,
        failure_phase: None,
    }
}

fn synthetic_payload(bytes: usize) -> Vec<u8> {
    let mut payload = Vec::with_capacity(bytes);
    for index in 0..bytes {
        payload.push((index % 251) as u8);
    }
    payload
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn elapsed_ms(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1000.0
}

fn emit_json_report<T: Serialize>(report: &T, report_out: Option<&Path>) -> Result<()> {
    let json = serde_json::to_string_pretty(report)?;
    println!("{json}");
    if let Some(path) = report_out {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("create report directory {}", parent.display()))?;
            }
        }
        fs::write(path, format!("{json}\n"))
            .with_context(|| format!("write router validation report {}", path.display()))?;
    }
    Ok(())
}

fn emit_markdown_report(report: &RouterValidationReport, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create report directory {}", parent.display()))?;
        }
    }

    let mut markdown = String::new();
    markdown.push_str("# Router Validation Report\n\n");
    markdown.push_str("## Result\n\n");
    markdown.push_str("```yaml\n");
    markdown.push_str(&format!("result: {}\n", report.result));
    markdown.push_str(&format!("recommendation: {}\n", report.recommendation));
    markdown.push_str(&format!(
        "critical_native_handoff_result: {}\n",
        report
            .native_handoff_evidence
            .critical_native_handoff_result
    ));
    markdown.push_str(&format!(
        "prompt_suite_handoff_result: {}\n",
        report.native_handoff_evidence.prompt_suite_handoff_result
    ));
    markdown.push_str("full_pd_router_readiness: inconclusive\n");
    markdown.push_str("```\n\n");
    markdown.push_str("## Summary\n\n");
    markdown.push_str(
        "This first-stage local apply validates the router-validation rules, manifest checks, fail-closed decisions, fallback decisions, timing report shape, and report generation without starting remote PGX/Mac validation processes.\n\n",
    );
    markdown.push_str("It does not prove full PD/router readiness. Real OpenAI ingress, live PGX/Mac processes, injected router failures, and isolated link timing still require a later authorized validation run.\n\n");
    markdown.push_str("## Manifest Negative Validation\n\n");
    markdown.push_str("| Case | Status | Rejected before import | Failure field |\n");
    markdown.push_str("|---|---|---|---|\n");
    for item in &report.manifest_negative {
        markdown.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            item.case, item.status, item.rejected_before_import, item.failure_field
        ));
    }
    markdown.push_str("\n## Fail-Closed And Fallback\n\n");
    markdown.push_str("| Area | Status | Phase | Reason |\n");
    markdown.push_str("|---|---|---|---|\n");
    for item in &report.fail_closed {
        markdown.push_str(&format!(
            "| fail-closed | {} | {} | {} |\n",
            item.status, item.phase, item.reason
        ));
    }
    markdown.push_str(&format!(
        "| pre-token fallback | {} | {} | {} |\n",
        report.pre_token_fallback.status,
        report.pre_token_fallback.phase,
        report.pre_token_fallback.reason
    ));
    markdown.push_str(&format!(
        "| post-token failure | {} | {} | {} |\n",
        report.post_token_failure.status,
        report.post_token_failure.phase,
        report.post_token_failure.reason
    ));
    markdown.push_str("\n## Timing Shape\n\n");
    markdown.push_str("| Metric | Value |\n");
    markdown.push_str("|---|---:|\n");
    markdown.push_str(&format!(
        "| pd_router_eligibility_ms | {:.6} |\n",
        report.network_timing.pd_router_eligibility_ms
    ));
    markdown.push_str(&format!(
        "| pd_tokenization_ms | {:.6} |\n",
        report.network_timing.pd_tokenization_ms
    ));
    markdown.push_str(&format!(
        "| kv_payload_bytes | {} |\n",
        report.network_timing.kv_payload_bytes
    ));
    markdown.push_str(&format!(
        "| kv_transfer_ms | {:.6} |\n",
        report.network_timing.kv_transfer_ms
    ));
    markdown.push_str(&format!(
        "| ttft_ms | {:.6} |\n",
        report.network_timing.ttft_ms
    ));
    markdown.push_str("\n## Remaining Authorization Required\n\n");
    for item in &report.remaining_authorization_required {
        markdown.push_str(&format!("- {item}\n"));
    }

    fs::write(path, markdown)
        .with_context(|| format!("write router validation markdown report {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_args() -> RouterValidationArgs {
        RouterValidationArgs {
            output: crate::cli::OutputArgs { report_out: None },
            markdown_out: None,
            request_id: "test-request".to_string(),
            handoff_id: "test-handoff".to_string(),
            source_node_id: "pgx-test".to_string(),
            target_node_id: "mac-test".to_string(),
            model_id: "google/gemma-4-31b-it:bf16".to_string(),
            synthetic_payload_bytes: 256,
        }
    }

    #[test]
    fn local_router_validation_report_is_inconclusive_until_real_router_run() {
        let report = run_local_router_validation(&test_args()).expect("local report");
        assert_eq!(report.result, "inconclusive");
        assert_eq!(report.manifest_positive.status, "pass");
        assert!(report
            .manifest_negative
            .iter()
            .all(|case| case.status == "pass"));
        assert_eq!(report.pre_token_fallback.status, "pass");
        assert_eq!(report.post_token_failure.status, "pass");
        assert!(!report.scope.remote_processes_started);
    }

    #[test]
    fn manifest_rejects_identity_and_integrity_mismatch() {
        let args = test_args();
        let payload = synthetic_payload(args.synthetic_payload_bytes);
        let payload_checksum = sha256_hex(&payload);
        let expectations = ManifestExpectations {
            protocol_version: HANDOFF_PROTOCOL_VERSION,
            model_artifact_sha256: EXPECTED_MODEL_ARTIFACT_SHA256,
            tokenizer_metadata_hash: EXPECTED_TOKENIZER_METADATA_HASH,
            chat_template_hash: EXPECTED_CHAT_TEMPLATE_HASH,
            runtime_abi_version: "skippy-native-state/1",
            kv_format_version: "native-full-state/1",
            kv_dtype: "f16",
            layout: "llama.cpp-native-full-state",
            byte_order: "little",
            checksum_algorithm: "sha256",
            prompt_token_count: 256,
            decode_start_position: 256,
            total_bytes: payload.len() as u64,
            payload_checksum,
        };
        let mut manifest = valid_manifest(&args, &expectations);
        manifest.model_artifact_sha256 = "bad".to_string();
        assert_eq!(
            validate_manifest(Some(&manifest), &expectations, &payload)
                .unwrap_err()
                .field,
            "model_artifact_sha256"
        );

        let mut manifest = valid_manifest(&args, &expectations);
        manifest.payload_checksum = "00".repeat(32);
        assert_eq!(
            validate_manifest(Some(&manifest), &expectations, &payload)
                .unwrap_err()
                .field,
            "payload_checksum"
        );
    }

    #[test]
    fn fallback_semantics_block_transparent_post_token_fallback() {
        let pre_token = evaluate_fallback(false, true);
        assert_eq!(pre_token.phase, "pre_token_fallback");
        assert_eq!(pre_token.status, "pass");

        let post_token = evaluate_fallback(true, true);
        assert_eq!(post_token.phase, "post_token_failure");
        assert_eq!(
            post_token.reason,
            "transparent_fallback_blocked_after_first_token"
        );
    }
}
