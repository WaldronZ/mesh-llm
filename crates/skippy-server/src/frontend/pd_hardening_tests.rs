use super::*;
use crate::cli::PdAdmissionOverLimitAction;
use axum::response::IntoResponse;

fn mvp_config() -> PdRouterValidationConfig {
    PdRouterValidationConfig {
        mode: PdServingMode::Mvp,
        prefill_addr: "127.0.0.1:19081".to_string(),
        decode_addr: "127.0.0.1:19082".to_string(),
        wire_dtype: WireActivationDType::F16,
        startup_timeout_secs: 1,
        model_id: "google_gemma-4-31B-it-bf16".to_string(),
        expected_artifact_sha256:
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
        expected_tokenizer_hash: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            .to_string(),
        expected_chat_template_hash:
            "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc".to_string(),
        source_node_id: "pgx-prefill-mvp".to_string(),
        target_node_id: "mac-decode-mvp".to_string(),
        fault_injection: PdRouterValidationFault::None,
        mvp_test_fault: PdServingMvpTestFault::None,
        admission: Some(PdAdmissionPolicy {
            max_prompt_tokens: 2048,
            max_prefill_batch: 2048,
            max_ctx_size: 8192,
            max_handoff_bytes: 1_073_741_824,
            estimated_kv_bytes_per_token: Some(902_000),
            kv_bytes_per_token_source: PdKvBytesPerTokenSource::Configured,
            over_limit_action: PdAdmissionOverLimitAction::Fallback,
            chunked_prefill: None,
        }),
        chunked_prefill: None,
    }
}

fn parse_serve_openai(args: &[&str]) -> ServeOpenAiArgs {
    use clap::Parser as _;

    let cli = crate::cli::Cli::parse_from(args);
    let crate::cli::Command::ServeOpenAi(args) = cli.command else {
        panic!("expected serve-openai command");
    };
    args
}

fn assert_generation_rate_limit(error: OpenAiError, message_fragment: &str) {
    assert_eq!(error.status(), StatusCode::TOO_MANY_REQUESTS);
    let body = error.body();
    assert_eq!(body.error.code.as_deref(), Some("rate_limit_exceeded"));
    assert!(
        body.error.message.contains(message_fragment),
        "expected {:?} to contain {:?}",
        body.error.message,
        message_fragment
    );

    let response = error.into_response();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        response
            .headers()
            .get(axum::http::header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok()),
        Some("1")
    );
}

#[test]
fn normal_and_split_paths_keep_pd_default_off() {
    let args = parse_serve_openai(&[
        "skippy-server",
        "serve-openai",
        "--config",
        "/tmp/stage.json",
        "--first-stage-addr",
        "127.0.0.1:19081",
        "--generation-concurrency",
        "4",
    ]);

    assert_eq!(pd_serving_mode_from_args(&args).unwrap(), None);
    assert!(!args.pd_router_validation);
    assert!(!args.pd_serving_mvp);
    assert!(!args.pd_serving_mvp_allow_test_faults);
    assert_eq!(args.pd_serving_mvp_test_fault, PdServingMvpTestFault::None);
    assert_eq!(generation_queue_limit_for_pd_mode(None, 4), 4);
}

#[test]
fn pd_mvp_rejects_split_serving_and_non_mvp_fault_hooks() {
    let split_args = parse_serve_openai(&[
        "skippy-server",
        "serve-openai",
        "--config",
        "/tmp/stage.json",
        "--first-stage-addr",
        "127.0.0.1:19081",
        "--pd-serving-mvp",
    ]);
    let mode = pd_serving_mode_from_args(&split_args).unwrap();
    let error = validate_pd_serving_backend_constraints(&split_args, mode).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("--pd-serving-mvp cannot be combined with --first-stage-addr"),
        "{error:?}"
    );

    let fault_args = parse_serve_openai(&[
        "skippy-server",
        "serve-openai",
        "--config",
        "/tmp/stage.json",
        "--pd-serving-mvp-allow-test-faults",
        "--pd-serving-mvp-test-fault",
        "manifest-mismatch",
    ]);
    let error = pd_serving_mode_from_args(&fault_args).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("--pd-serving-mvp-test-fault requires --pd-serving-mvp"),
        "{error:?}"
    );
}

#[test]
fn pd_mvp_requires_single_request_lane_and_ineligible_model_errors_before_content() {
    let args = parse_serve_openai(&[
        "skippy-server",
        "serve-openai",
        "--config",
        "/tmp/stage.json",
        "--pd-serving-mvp",
        "--generation-concurrency",
        "2",
    ]);
    let mode = pd_serving_mode_from_args(&args).unwrap();
    let error = validate_pd_serving_backend_constraints(&args, mode).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("--pd-serving-mvp requires --generation-concurrency 1"),
        "{error:?}"
    );

    let error = ensure_requested_model("allowed-model", "other-model").unwrap_err();
    assert_eq!(error.status(), StatusCode::NOT_FOUND);
    assert_eq!(error.body().error.code.as_deref(), Some("model_not_found"));
}

#[tokio::test]
async fn pd_mvp_busy_admission_rejects_without_queueing() {
    let generation_limit = Arc::new(Semaphore::new(0));
    let generation_queue_depth = Arc::new(AtomicUsize::new(0));
    let queue_limit = generation_queue_limit_for_pd_mode(Some(PdServingMode::Mvp), 1);

    let error = acquire_generation_permit_with_queue(
        generation_limit,
        generation_queue_depth.clone(),
        queue_limit,
        Duration::from_millis(10),
    )
    .await
    .unwrap_err();

    assert_eq!(queue_limit, 0);
    assert_generation_rate_limit(error, "queue is full");
    assert_eq!(generation_queue_depth.load(Ordering::Acquire), 0);
}

#[tokio::test]
async fn pd_mvp_admission_capacity_restores_after_terminal_drop() {
    let generation_limit = Arc::new(Semaphore::new(1));
    let generation_queue_depth = Arc::new(AtomicUsize::new(0));
    let queue_limit = generation_queue_limit_for_pd_mode(Some(PdServingMode::Mvp), 1);

    let permit = acquire_generation_permit_with_queue(
        generation_limit.clone(),
        generation_queue_depth.clone(),
        queue_limit,
        Duration::from_millis(10),
    )
    .await
    .unwrap();
    let busy = acquire_generation_permit_with_queue(
        generation_limit.clone(),
        generation_queue_depth.clone(),
        queue_limit,
        Duration::from_millis(10),
    )
    .await
    .unwrap_err();
    assert_generation_rate_limit(busy, "queue is full");

    drop(permit);
    let next = acquire_generation_permit_with_queue(
        generation_limit,
        generation_queue_depth.clone(),
        queue_limit,
        Duration::from_millis(10),
    )
    .await
    .unwrap();
    drop(next);
    assert_eq!(generation_queue_depth.load(Ordering::Acquire), 0);
}

#[test]
fn pd_mvp_status_reports_sanitized_health_capacity_and_compatibility() {
    let status = pd_serving_status_for_start(&mvp_config(), 1);
    let serialized = serde_json::to_value(&status).unwrap();

    assert_eq!(serialized["prefill_worker_health"], "configured");
    assert_eq!(serialized["decode_worker_health"], "configured");
    assert_eq!(serialized["compatibility_state"], "configured-identities");
    assert_eq!(serialized["inflight_limit"], 1);
    assert_eq!(serialized["inflight_current"], 0);
    assert_eq!(serialized["capacity_state"], "open");
    assert_eq!(serialized["admission_policy_configured"], true);
    assert_eq!(serialized["admission_over_limit_action"], "fallback");
    assert_eq!(serialized["max_prompt_tokens"], 2048);
    assert_eq!(serialized["max_prefill_batch"], 2048);
    assert_eq!(serialized["max_ctx_size"], 8192);
    assert_eq!(serialized["max_handoff_bytes"], 1_073_741_824);
    assert_eq!(serialized["estimated_kv_bytes_per_token"], 902000);
    assert_eq!(serialized["kv_bytes_per_token_source"], "configured");
    assert_eq!(
        serialized["effective_prompt_limit_without_generation"],
        1190
    );

    let text = serialized.to_string();
    for forbidden in [
        "127.0.0.1",
        "aaaaaaaaaaaaaaaa",
        "bbbbbbbbbbbbbbbb",
        "cccccccccccccccc",
        "/Users/",
        "secret",
        "http://",
    ] {
        assert!(!text.contains(forbidden), "status leaked {forbidden}");
    }
}

#[test]
fn openai_streaming_success_and_failure_shapes_are_distinct() {
    let content = generation_event_to_chat_chunk(
        Ok(GenerationStreamEvent::Delta("content".to_string())),
        "model",
    )
    .unwrap();
    assert_eq!(content.choices[0].delta.content.as_deref(), Some("content"));
    assert_eq!(content.choices[0].finish_reason, None);

    let done = generation_event_to_chat_chunk(
        Ok(GenerationStreamEvent::Done(FinishReason::Stop)),
        "model",
    )
    .unwrap();
    assert_eq!(done.choices[0].delta.content, None);
    assert_eq!(done.choices[0].finish_reason, Some(FinishReason::Stop));

    let error = generation_event_to_chat_chunk(
        Err(OpenAiError::backend(
            "PD post-content failure: transparent fallback blocked",
        )),
        "model",
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("PD post-content failure: transparent fallback blocked"),
        "{error:?}"
    );

    let serialized = serde_json::to_string(&content).unwrap();
    for forbidden in [
        "prompt text",
        "[1,2,3]",
        "native state",
        "/Users/",
        "secret",
    ] {
        assert!(!serialized.contains(forbidden), "stream leaked {forbidden}");
    }
}
