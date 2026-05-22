use clap::Parser as _;

use super::*;
use crate::cli::{Cli, CommandKind, OutputArgs};

fn coordinator_args() -> KvPageHandoffCoordinatorArgs {
    KvPageHandoffCoordinatorArgs {
        output: OutputArgs { report_out: None },
        markdown_out: None,
        source_addr: "127.0.0.1:19430".parse().unwrap(),
        model: None,
        stage_load_mode: StageLoadMode::RuntimeSlice,
        layer_end: 30,
        ctx_size: 512,
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
        prompt_id: "synthetic-two-chunk".to_string(),
        total_tokens: 128,
        chunk_tokens: 64,
        max_tokens: 16,
        seed: 42,
        bootstrap_strategy: KvPageBootstrapStrategy::TrimReplayLastToken,
    }
}

#[test]
fn cli_parses_source_and_coordinator_roles() {
    let cli = Cli::parse_from([
        "skippy-correctness",
        "kv-page-handoff",
        "source",
        "--bind-addr",
        "127.0.0.1:19430",
    ]);
    let CommandKind::KvPageHandoff(args) = cli.command else {
        panic!("expected kv-page-handoff command");
    };
    assert!(matches!(args.role, KvPageHandoffRole::Source(_)));

    let cli = Cli::parse_from([
        "skippy-correctness",
        "kv-page-handoff",
        "coordinator",
        "--source-addr",
        "127.0.0.1:19430",
        "--total-tokens",
        "128",
        "--chunk-tokens",
        "64",
        "--bootstrap-strategy",
        "trim-replay-last-token",
    ]);
    let CommandKind::KvPageHandoff(args) = cli.command else {
        panic!("expected kv-page-handoff command");
    };
    assert!(matches!(args.role, KvPageHandoffRole::Coordinator(_)));
}

#[test]
fn coordinator_local_report_is_ready_but_inconclusive() {
    let args = coordinator_args();
    let report = run_local_coordinator_harness(&args);

    assert_eq!(report.result, "inconclusive");
    assert_eq!(report.recommendation, RECOMMENDATION_READY);
    assert_eq!(report.local_manifest_validation.status, "pass");
    assert_eq!(report.telemetry_shape.page_count, 2);
    assert!(!report.runtime_path.full_state_handoff_allowed_as_pass);
    assert_eq!(report.baseline.comparison, "not_run");
    assert_eq!(report.bootstrap.status, "not_run");
    assert_eq!(report.bootstrap.strategy, "trim_replay_last_token");
}

#[test]
fn local_manifest_validation_rejects_negative_cases() {
    let identity = expected_identity();
    let records = synthetic_records(128, 64, &identity);
    let validation = validate_kv_page_handoff(&records, &identity).unwrap();
    assert_eq!(validation.final_decode_start_position, 128);
    assert_eq!(validation.page_count, 2);
    assert_eq!(
        validation.total_page_bytes,
        records
            .iter()
            .map(|record| record.payload.len() as u64)
            .sum::<u64>()
    );

    let checks = negative_checks(&records, &identity);
    assert_eq!(checks.len(), 14);
    for check in checks {
        assert_eq!(check.status, "pass", "{check:?}");
        assert_ne!(check.failure_reason, "unexpected_pass");
    }
}

#[test]
fn iswa_manifest_requires_base_and_swa_segments_per_token_range() {
    let identity = expected_identity();
    let records = synthetic_iswa_records(128, 64, &identity);
    let validation = validate_kv_page_handoff(&records, &identity).unwrap();

    assert_eq!(validation.final_decode_start_position, 128);
    assert_eq!(validation.page_count, 4);
    assert_eq!(records[0].manifest.cache_kind, "iswa");
    assert_eq!(records[0].manifest.segment_kind, "base");
    assert_eq!(records[1].manifest.segment_kind, "swa");

    let mut missing_swa = records.clone();
    missing_swa.remove(1);
    let missing_total = missing_swa.len();
    for (index, record) in missing_swa.iter_mut().enumerate() {
        record.manifest.page_index = index;
        record.manifest.total_pages = missing_total;
    }
    assert_eq!(
        validate_kv_page_handoff(&missing_swa, &identity)
            .unwrap_err()
            .reason,
        "position_gap"
    );

    let mut duplicate_base = records.clone();
    duplicate_base[1].manifest.segment_kind = "base".to_string();
    assert_eq!(
        validate_kv_page_handoff(&duplicate_base, &identity)
            .unwrap_err()
            .reason,
        "duplicate_segment"
    );

    let checks = negative_checks(&records, &identity);
    assert_eq!(checks.len(), 14);
    for check in checks {
        assert_eq!(check.status, "pass", "{check:?}");
        assert_ne!(check.failure_reason, "unexpected_pass");
    }
}

#[test]
fn full_state_blob_cannot_pass_page_handoff() {
    let identity = expected_identity();
    let records = synthetic_records(128, 64, &identity);
    let mutated = mutate_full_state_blob(&records);

    assert_eq!(
        validate_kv_page_handoff(&mutated, &identity)
            .unwrap_err()
            .reason,
        "full_state_blob"
    );
}

#[test]
fn decode_comparison_records_exact_match_without_token_arrays() {
    let comparison = compare_tokens(&[100, 101, 102], &[100, 101, 102]);

    assert!(comparison.matches);
    assert_eq!(comparison.label, "exact_token_match");
    assert_eq!(comparison.baseline_token_count, 3);
    assert_eq!(comparison.page_token_count, 3);
    assert_eq!(comparison.first_divergence_index, None);
    assert_eq!(comparison.baseline_token_id, None);
    assert_eq!(comparison.page_token_id, None);
}

#[test]
fn decode_comparison_records_bounded_divergence_metadata() {
    let comparison = compare_tokens(&[100, 101, 102], &[100, 201, 102]);

    assert!(!comparison.matches);
    assert_eq!(comparison.label, "token_divergence");
    assert_eq!(comparison.first_divergence_index, Some(1));
    assert_eq!(comparison.baseline_token_id, Some(101));
    assert_eq!(comparison.page_token_id, Some(201));
    assert_eq!(comparison.baseline_token_count, 3);
    assert_eq!(comparison.page_token_count, 3);
}

#[test]
fn baseline_unavailable_report_is_inconclusive_and_sanitized() {
    let args = coordinator_args();
    let identity = expected_identity();
    let records = synthetic_records(args.total_tokens, args.chunk_tokens, &identity);
    let validation = validate_kv_page_handoff(&records, &identity).unwrap();

    let report = baseline_unavailable_report(
        &args,
        validation,
        vec![1.0, 2.0],
        vec![0.25, 0.5],
        "no_skippy_execution_lane",
    );

    assert_eq!(report.result, "inconclusive");
    assert_eq!(
        report.recommendation,
        "fix_baseline_harness_before_streaming"
    );
    assert_eq!(report.baseline.strategy, "local_one_shot_prefill_decode");
    assert_eq!(report.baseline.one_shot_full_state_baseline, "not_used");
    assert_eq!(report.baseline.page_handoff_decode, "not_run");
    assert_eq!(report.baseline.comparison, "baseline_unavailable");
    assert_eq!(
        report.baseline.failure_reason,
        Some("no_skippy_execution_lane")
    );
    assert!(!report.runtime_path.full_state_handoff_allowed_as_pass);

    let serialized = serde_json::to_string(&report).unwrap();
    assert!(!serialized.contains("/Users/"));
    assert!(!serialized.contains("prompt text"));
    assert!(!serialized.contains("generated content"));
    assert!(!serialized.contains("private-key"));
}

#[test]
fn trim_replay_bootstrap_plan_rejects_zero_imported_tokens() {
    assert_eq!(
        trim_replay_last_token_plan(&[10, 11], 0).unwrap_err(),
        "imported_token_count_zero"
    );
}

#[test]
fn trim_replay_bootstrap_plan_rejects_missing_last_prompt_token() {
    assert_eq!(
        trim_replay_last_token_plan(&[10, 11], 3).unwrap_err(),
        "missing_last_prompt_token"
    );
}

#[test]
fn trim_replay_bootstrap_plan_uses_last_prompt_position() {
    let plan = trim_replay_last_token_plan(&[10, 11, 12], 3).unwrap();

    assert_eq!(plan.imported_token_count, 3);
    assert_eq!(plan.trim_target_position, 2);
    assert_eq!(plan.replay_token_position, 2);
    assert_eq!(plan.decode_start_position, 3);
    assert_eq!(plan.replay_token, 12);
}

#[test]
fn source_without_model_reports_foreground_requirement() {
    let report = source_skeleton_report("127.0.0.1:0".to_string(), None);

    assert_eq!(report.result, "inconclusive");
    assert_eq!(report.recommendation, RECOMMENDATION_SOURCE_UNSUPPORTED);
    assert_eq!(
        report.runtime_path.source_runtime_export_kv_page,
        "skeleton_not_started"
    );
    assert!(report
        .remaining_authorization_required
        .contains(&"start PGX foreground source process"));
}

#[test]
fn harness_frame_roundtrip_preserves_manifest_and_payload_bytes() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let identity = expected_identity();
    let record = synthetic_records(128, 64, &identity).remove(0);
    let expected_payload = record.payload.clone();
    let expected_manifest = record.manifest.clone();

    let reader = std::thread::spawn(move || {
        let (mut server, _) = listener.accept().unwrap();
        let (response, payload): (HarnessResponse, Vec<u8>) =
            read_json_frame_with_payload(&mut server).unwrap();
        assert_eq!(response.kind, "page");
        assert_eq!(response.status, "ok");
        assert_eq!(response.page_export_ms, Some(1.25));
        assert_eq!(response.manifest.unwrap(), expected_manifest);
        assert_eq!(payload, expected_payload);
    });

    let mut client = TcpStream::connect(addr).unwrap();
    write_json_frame(
        &mut client,
        &HarnessResponse::page(record.manifest, 1.25),
        &record.payload,
    )
    .unwrap();
    reader.join().unwrap();
}

#[test]
fn report_serialization_is_sanitized() {
    let args = coordinator_args();
    let report = run_local_coordinator_harness(&args);
    let serialized = serde_json::to_string(&report).unwrap();

    assert!(!serialized.contains("synthetic-page"));
    assert!(!serialized.contains("prompt text"));
    assert!(!serialized.contains("generated content"));
    assert!(!serialized.contains("token array"));
    assert!(!serialized.contains("/Users/"));
    assert!(!serialized.contains("secret"));
    assert!(!serialized.contains("private-key"));
    assert!(serialized.contains("trim_replay_last_token"));
    assert!(!serialized.contains("replay_token_id"));
}
