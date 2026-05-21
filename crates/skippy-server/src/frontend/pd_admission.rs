use super::*;

use crate::cli::PdAdmissionOverLimitAction;
#[cfg(test)]
use crate::telemetry::TelemetryLevel;

use super::pd_chunked_prefill::PdChunkedPrefillConfig;

const PD_GEMMA4_31B_IT_BF16_MODEL_ID: &str = "google_gemma-4-31B-it-bf16";
const PD_GEMMA4_NATIVE_FULL_STATE_KV_BYTES_PER_TOKEN: u64 = 902_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PdKvBytesPerTokenSource {
    Configured,
    Measured,
    Missing,
}

impl PdKvBytesPerTokenSource {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Configured => "configured",
            Self::Measured => "measured",
            Self::Missing => "missing",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PdAdmissionPolicy {
    pub(super) max_prompt_tokens: usize,
    pub(super) max_prefill_batch: usize,
    pub(super) max_ctx_size: usize,
    pub(super) max_handoff_bytes: u64,
    pub(super) estimated_kv_bytes_per_token: Option<u64>,
    pub(super) kv_bytes_per_token_source: PdKvBytesPerTokenSource,
    pub(super) over_limit_action: PdAdmissionOverLimitAction,
    pub(super) chunked_prefill: Option<PdChunkedPrefillConfig>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PdAdmissionResult {
    Admitted,
    Fallback,
    Rejected,
    PdUnavailable,
}

impl PdAdmissionResult {
    fn label(self) -> &'static str {
        match self {
            Self::Admitted => "admitted",
            Self::Fallback => "fallback",
            Self::Rejected => "rejected",
            Self::PdUnavailable => "pd_unavailable",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PdAdmissionReason {
    WithinLimits,
    PromptTokensExceeded,
    PrefillBatchExceeded,
    CtxSizeExceeded,
    EstimatedHandoffBytesExceeded,
    EstimatedHandoffBytesUnavailable,
    AdmissionConfigMissing,
}

impl PdAdmissionReason {
    fn label(self) -> &'static str {
        match self {
            Self::WithinLimits => "within_limits",
            Self::PromptTokensExceeded => "prompt_tokens_exceeded",
            Self::PrefillBatchExceeded => "prefill_batch_exceeded",
            Self::CtxSizeExceeded => "ctx_size_exceeded",
            Self::EstimatedHandoffBytesExceeded => "estimated_handoff_bytes_exceeded",
            Self::EstimatedHandoffBytesUnavailable => "estimated_handoff_bytes_unavailable",
            Self::AdmissionConfigMissing => "admission_config_missing",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PdAdmissionOutcome {
    pub(super) result: PdAdmissionResult,
    pub(super) reason: PdAdmissionReason,
    pub(super) prompt_token_count: usize,
    pub(super) requested_max_tokens: u32,
    pub(super) estimated_kv_bytes: Option<u64>,
    pub(super) token_context_prefill_limit: usize,
    pub(super) effective_prompt_limit: Option<usize>,
    pub(super) policy: PdAdmissionPolicy,
}

impl PdAdmissionOutcome {
    fn with_result(mut self, result: PdAdmissionResult) -> Self {
        self.result = result;
        self
    }

    pub(super) fn result_label(&self) -> &'static str {
        self.result.label()
    }

    pub(super) fn reason_label(&self) -> &'static str {
        self.reason.label()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PdAdmissionDecision {
    Admit(PdAdmissionOutcome),
    Fallback(PdAdmissionOutcome),
    Reject(PdAdmissionOutcome),
    Unavailable(PdAdmissionOutcome),
}

impl PdAdmissionDecision {
    pub(super) fn outcome(&self) -> &PdAdmissionOutcome {
        match self {
            Self::Admit(outcome)
            | Self::Fallback(outcome)
            | Self::Reject(outcome)
            | Self::Unavailable(outcome) => outcome,
        }
    }

    #[cfg(test)]
    pub(super) fn should_start_prefill(&self) -> bool {
        matches!(self, Self::Admit(_))
    }
}

impl PdAdmissionPolicy {
    pub(super) fn evaluate(
        self,
        prompt_token_count: usize,
        requested_max_tokens: u32,
    ) -> PdAdmissionDecision {
        let token_context_prefill_limit = self.token_context_prefill_limit(requested_max_tokens);
        let effective_prompt_limit = self.effective_prompt_limit(requested_max_tokens);
        let pre_kv_reason = self.pre_kv_over_limit_reason(prompt_token_count, requested_max_tokens);
        let estimated_kv_bytes = self.estimated_kv_bytes(prompt_token_count);
        let reason = match pre_kv_reason {
            Some(reason) => reason,
            None => match estimated_kv_bytes {
                Some(bytes) if bytes > self.max_handoff_bytes => {
                    PdAdmissionReason::EstimatedHandoffBytesExceeded
                }
                Some(_) => PdAdmissionReason::WithinLimits,
                None => PdAdmissionReason::EstimatedHandoffBytesUnavailable,
            },
        };
        let outcome = PdAdmissionOutcome {
            result: PdAdmissionResult::Admitted,
            reason,
            prompt_token_count,
            requested_max_tokens,
            estimated_kv_bytes,
            token_context_prefill_limit,
            effective_prompt_limit,
            policy: self,
        };
        if reason == PdAdmissionReason::WithinLimits {
            PdAdmissionDecision::Admit(outcome)
        } else if reason == PdAdmissionReason::EstimatedHandoffBytesUnavailable {
            PdAdmissionDecision::Unavailable(outcome.with_result(PdAdmissionResult::PdUnavailable))
        } else {
            match self.over_limit_action {
                PdAdmissionOverLimitAction::Fallback => {
                    PdAdmissionDecision::Fallback(outcome.with_result(PdAdmissionResult::Fallback))
                }
                PdAdmissionOverLimitAction::Reject => {
                    PdAdmissionDecision::Reject(outcome.with_result(PdAdmissionResult::Rejected))
                }
            }
        }
    }

    pub(super) fn token_context_prefill_limit(self, requested_max_tokens: u32) -> usize {
        let requested_max_tokens = usize::try_from(requested_max_tokens).unwrap_or(usize::MAX);
        let mut limit = self
            .max_prompt_tokens
            .min(self.max_ctx_size.saturating_sub(requested_max_tokens));
        if self.chunked_prefill.is_none() {
            limit = limit.min(self.max_prefill_batch);
        }
        limit
    }

    pub(super) fn effective_prompt_limit(self, requested_max_tokens: u32) -> Option<usize> {
        let bytes_per_token = self.estimated_kv_bytes_per_token?;
        let handoff_token_limit = usize::try_from(self.max_handoff_bytes / bytes_per_token).ok()?;
        Some(
            self.token_context_prefill_limit(requested_max_tokens)
                .min(handoff_token_limit),
        )
    }

    fn estimated_kv_bytes(self, prompt_token_count: usize) -> Option<u64> {
        let bytes_per_token = self.estimated_kv_bytes_per_token?;
        u64::try_from(prompt_token_count)
            .ok()?
            .checked_mul(bytes_per_token)
    }

    fn pre_kv_over_limit_reason(
        self,
        prompt_token_count: usize,
        requested_max_tokens: u32,
    ) -> Option<PdAdmissionReason> {
        if prompt_token_count > self.max_prompt_tokens {
            return Some(PdAdmissionReason::PromptTokensExceeded);
        }
        let requested_max_tokens = usize::try_from(requested_max_tokens).unwrap_or(usize::MAX);
        if requested_max_tokens > self.max_ctx_size
            || prompt_token_count > self.max_ctx_size.saturating_sub(requested_max_tokens)
        {
            return Some(PdAdmissionReason::CtxSizeExceeded);
        }
        if self.chunked_prefill.is_none() && prompt_token_count > self.max_prefill_batch {
            return Some(PdAdmissionReason::PrefillBatchExceeded);
        }
        None
    }
}

pub(super) fn pd_admission_policy_from_args(
    args: &ServeOpenAiArgs,
    mode: PdServingMode,
    model_id: &str,
) -> Result<Option<PdAdmissionPolicy>> {
    let has_pd_admission_args = args.pd_max_prompt_tokens.is_some()
        || args.pd_max_prefill_batch.is_some()
        || args.pd_max_ctx_size.is_some()
        || args.pd_max_handoff_bytes.is_some()
        || args.pd_estimated_kv_bytes_per_token.is_some()
        || args.pd_chunked_prefill
        || args.pd_prefill_chunk_size.is_some()
        || args.pd_admission_over_limit != PdAdmissionOverLimitAction::Fallback;
    if mode != PdServingMode::Mvp {
        if has_pd_admission_args {
            bail!("PD admission policy flags require --pd-serving-mvp");
        }
        return Ok(None);
    }

    let max_prompt_tokens =
        required_positive_usize(args.pd_max_prompt_tokens, "--pd-max-prompt-tokens")?;
    let max_prefill_batch =
        required_positive_usize(args.pd_max_prefill_batch, "--pd-max-prefill-batch")?;
    let max_ctx_size = required_positive_usize(args.pd_max_ctx_size, "--pd-max-ctx-size")?;
    let max_handoff_bytes =
        required_positive_u64(args.pd_max_handoff_bytes, "--pd-max-handoff-bytes")?;
    let (estimated_kv_bytes_per_token, kv_bytes_per_token_source) =
        pd_kv_bytes_per_token_from_args(args, model_id)?;
    if args.pd_prefill_chunk_size.is_some() && !args.pd_chunked_prefill {
        bail!("--pd-prefill-chunk-size requires --pd-chunked-prefill");
    }
    let chunked_prefill = if args.pd_chunked_prefill {
        Some(PdChunkedPrefillConfig::new(
            args.pd_prefill_chunk_size.unwrap_or(max_prefill_batch),
            max_prefill_batch,
        )?)
    } else {
        None
    };

    Ok(Some(PdAdmissionPolicy {
        max_prompt_tokens,
        max_prefill_batch,
        max_ctx_size,
        max_handoff_bytes,
        estimated_kv_bytes_per_token,
        kv_bytes_per_token_source,
        over_limit_action: args.pd_admission_over_limit,
        chunked_prefill,
    }))
}

fn pd_kv_bytes_per_token_from_args(
    args: &ServeOpenAiArgs,
    model_id: &str,
) -> Result<(Option<u64>, PdKvBytesPerTokenSource)> {
    if let Some(value) = args.pd_estimated_kv_bytes_per_token {
        return Ok((
            Some(required_positive_u64(
                Some(value),
                "--pd-estimated-kv-bytes-per-token",
            )?),
            PdKvBytesPerTokenSource::Configured,
        ));
    }
    if model_id == PD_GEMMA4_31B_IT_BF16_MODEL_ID {
        return Ok((
            Some(PD_GEMMA4_NATIVE_FULL_STATE_KV_BYTES_PER_TOKEN),
            PdKvBytesPerTokenSource::Measured,
        ));
    }
    bail!(
        "--pd-serving-mvp requires --pd-estimated-kv-bytes-per-token for model '{model_id}' because no calibrated default is available"
    )
}

pub(super) fn pd_admission_missing_config_decision(
    prompt_token_count: usize,
    requested_max_tokens: u32,
) -> PdAdmissionDecision {
    PdAdmissionDecision::Unavailable(PdAdmissionOutcome {
        result: PdAdmissionResult::PdUnavailable,
        reason: PdAdmissionReason::AdmissionConfigMissing,
        prompt_token_count,
        requested_max_tokens,
        estimated_kv_bytes: None,
        policy: PdAdmissionPolicy {
            max_prompt_tokens: 0,
            max_prefill_batch: 0,
            max_ctx_size: 0,
            max_handoff_bytes: 0,
            estimated_kv_bytes_per_token: None,
            kv_bytes_per_token_source: PdKvBytesPerTokenSource::Missing,
            over_limit_action: PdAdmissionOverLimitAction::Reject,
            chunked_prefill: None,
        },
        token_context_prefill_limit: 0,
        effective_prompt_limit: None,
    })
}

fn required_positive_usize(value: Option<usize>, flag: &str) -> Result<usize> {
    match value {
        Some(value) if value > 0 => Ok(value),
        Some(_) => bail!("{flag} must be greater than zero"),
        None => bail!("--pd-serving-mvp requires {flag}"),
    }
}

fn required_positive_u64(value: Option<u64>, flag: &str) -> Result<u64> {
    match value {
        Some(value) if value > 0 => Ok(value),
        Some(_) => bail!("{flag} must be greater than zero"),
        None => bail!("--pd-serving-mvp requires {flag}"),
    }
}

pub(super) fn insert_pd_admission_status_attrs(
    attrs: &mut BTreeMap<String, Value>,
    policy: Option<&PdAdmissionPolicy>,
) {
    attrs.insert(
        "pd.admission.policy_configured".to_string(),
        json!(policy.is_some()),
    );
    if let Some(policy) = policy {
        attrs.insert(
            "pd.admission.over_limit_action".to_string(),
            json!(policy.over_limit_action.as_label()),
        );
        attrs.insert(
            "pd.max_prompt_tokens".to_string(),
            json!(policy.max_prompt_tokens),
        );
        attrs.insert(
            "pd.max_prefill_batch".to_string(),
            json!(policy.max_prefill_batch),
        );
        attrs.insert("pd.max_ctx_size".to_string(), json!(policy.max_ctx_size));
        attrs.insert(
            "pd.max_handoff_bytes".to_string(),
            json!(policy.max_handoff_bytes),
        );
        attrs.insert(
            "pd.estimated_kv_bytes_per_token".to_string(),
            json!(policy.estimated_kv_bytes_per_token),
        );
        attrs.insert(
            "pd.kv_bytes_per_token_source".to_string(),
            json!(policy.kv_bytes_per_token_source.label()),
        );
        attrs.insert(
            "pd.chunked_prefill.enabled".to_string(),
            json!(policy.chunked_prefill.is_some()),
        );
        if let Some(chunked_prefill) = policy.chunked_prefill {
            attrs.insert(
                "pd.prefill.chunk_size".to_string(),
                json!(chunked_prefill.chunk_size),
            );
            attrs.insert(
                "pd.chunked_prefill.capability".to_string(),
                json!(super::pd_chunked_prefill::PD_CHUNKED_PREFILL_CAPABILITY),
            );
        }
    }
}

impl StageOpenAiBackend {
    pub(super) fn emit_pd_admission(
        &self,
        ids: &OpenAiGenerationIds,
        mode: PdServingMode,
        outcome: &PdAdmissionOutcome,
        elapsed_ms: f64,
    ) {
        let mut attrs = self.openai_attrs(ids);
        attrs.insert("pd.mode".to_string(), json!(mode.backend_label()));
        attrs.insert(
            "pd.admission.result".to_string(),
            json!(outcome.result_label()),
        );
        attrs.insert(
            "pd.admission.reason".to_string(),
            json!(outcome.reason_label()),
        );
        attrs.insert(
            "pd.prompt_token_count".to_string(),
            json!(outcome.prompt_token_count),
        );
        attrs.insert(
            "pd.estimated_kv_bytes".to_string(),
            json!(outcome.estimated_kv_bytes),
        );
        attrs.insert(
            "pd.estimated_kv_bytes_per_token".to_string(),
            json!(outcome.policy.estimated_kv_bytes_per_token),
        );
        attrs.insert(
            "pd.kv_bytes_per_token_source".to_string(),
            json!(outcome.policy.kv_bytes_per_token_source.label()),
        );
        attrs.insert(
            "pd.token_context_prefill_limit".to_string(),
            json!(outcome.token_context_prefill_limit),
        );
        attrs.insert(
            "pd.effective_prompt_limit".to_string(),
            json!(outcome.effective_prompt_limit),
        );
        attrs.insert(
            "pd.max_prompt_tokens".to_string(),
            json!(outcome.policy.max_prompt_tokens),
        );
        attrs.insert(
            "pd.max_prefill_batch".to_string(),
            json!(outcome.policy.max_prefill_batch),
        );
        attrs.insert(
            "pd.max_ctx_size".to_string(),
            json!(outcome.policy.max_ctx_size),
        );
        attrs.insert(
            "pd.max_handoff_bytes".to_string(),
            json!(outcome.policy.max_handoff_bytes),
        );
        attrs.insert(
            "pd.requested_max_tokens".to_string(),
            json!(outcome.requested_max_tokens),
        );
        attrs.insert(
            "pd.chunked_prefill.enabled".to_string(),
            json!(outcome.policy.chunked_prefill.is_some()),
        );
        if let Some(chunked_prefill) = outcome.policy.chunked_prefill {
            attrs.insert(
                "pd.prefill.chunk_size".to_string(),
                json!(chunked_prefill.chunk_size),
            );
            attrs.insert(
                "pd.chunked_prefill.capability".to_string(),
                json!(super::pd_chunked_prefill::PD_CHUNKED_PREFILL_CAPABILITY),
            );
        }
        match outcome.result {
            PdAdmissionResult::Admitted => {}
            PdAdmissionResult::Fallback => {
                attrs.insert("pd.validation_or_mvp.result".to_string(), json!("fallback"));
                attrs.insert(mode.result_attr().to_string(), json!("fallback"));
                attrs.insert(
                    mode.fallback_attr().to_string(),
                    json!(outcome.reason_label()),
                );
                attrs.insert("pd.pre_content".to_string(), json!(true));
            }
            PdAdmissionResult::Rejected | PdAdmissionResult::PdUnavailable => {
                attrs.insert("pd.validation_or_mvp.result".to_string(), json!("fail"));
                attrs.insert(mode.result_attr().to_string(), json!("fail"));
                attrs.insert(mode.failure_phase_attr().to_string(), json!("admission"));
                attrs.insert(
                    mode.failure_reason_attr().to_string(),
                    json!(outcome.reason_label()),
                );
                attrs.insert("pd.pre_content".to_string(), json!(true));
            }
        }
        attrs.insert("llama_stage.elapsed_ms".to_string(), json!(elapsed_ms));
        self.telemetry.emit(mode.telemetry_event(), attrs);
    }
}

pub(super) fn pd_admission_rejection_error(outcome: &PdAdmissionOutcome) -> OpenAiError {
    let message = format!(
        "PD admission rejected request before prefill: {}",
        outcome.reason_label()
    );
    match outcome.reason {
        PdAdmissionReason::EstimatedHandoffBytesExceeded => OpenAiError::payload_too_large(message),
        PdAdmissionReason::EstimatedHandoffBytesUnavailable
        | PdAdmissionReason::AdmissionConfigMissing => OpenAiError::backend(message),
        _ => OpenAiError::context_length_exceeded(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> PdAdmissionPolicy {
        PdAdmissionPolicy {
            max_prompt_tokens: 16,
            max_prefill_batch: 16,
            max_ctx_size: 32,
            max_handoff_bytes: 16_000,
            estimated_kv_bytes_per_token: Some(100),
            kv_bytes_per_token_source: PdKvBytesPerTokenSource::Configured,
            over_limit_action: PdAdmissionOverLimitAction::Fallback,
            chunked_prefill: None,
        }
    }

    #[test]
    fn below_threshold_is_admitted() {
        let decision = policy().evaluate(8, 4);
        assert!(decision.should_start_prefill());
        assert_eq!(decision.outcome().result, PdAdmissionResult::Admitted);
        assert_eq!(decision.outcome().reason, PdAdmissionReason::WithinLimits);
        assert_eq!(decision.outcome().estimated_kv_bytes, Some(800));
        assert_eq!(decision.outcome().token_context_prefill_limit, 16);
        assert_eq!(decision.outcome().effective_prompt_limit, Some(16));
    }

    #[test]
    fn exactly_at_effective_threshold_is_admitted() {
        let decision = policy().evaluate(16, 16);
        assert!(decision.should_start_prefill());
        assert_eq!(decision.outcome().result, PdAdmissionResult::Admitted);
        assert_eq!(decision.outcome().reason, PdAdmissionReason::WithinLimits);
    }

    #[test]
    fn above_prompt_threshold_fallback_does_not_start_prefill() {
        let decision = policy().evaluate(17, 4);
        assert!(!decision.should_start_prefill());
        assert!(matches!(decision, PdAdmissionDecision::Fallback(_)));
        assert_eq!(
            decision.outcome().reason,
            PdAdmissionReason::PromptTokensExceeded
        );
    }

    #[test]
    fn above_prefill_batch_threshold_is_bounded() {
        let mut policy = policy();
        policy.max_prompt_tokens = 64;
        let decision = policy.evaluate(17, 4);
        assert!(!decision.should_start_prefill());
        assert_eq!(
            decision.outcome().reason,
            PdAdmissionReason::PrefillBatchExceeded
        );
    }

    #[test]
    fn context_budget_uses_requested_max_tokens() {
        let mut policy = policy();
        policy.max_prompt_tokens = 64;
        policy.max_prefill_batch = 64;
        let decision = policy.evaluate(17, 16);
        assert!(!decision.should_start_prefill());
        assert_eq!(
            decision.outcome().reason,
            PdAdmissionReason::CtxSizeExceeded
        );
    }

    #[test]
    fn effective_prompt_limit_uses_prompt_prefill_and_ctx_budget() {
        let mut policy = policy();
        policy.max_prompt_tokens = 64;
        policy.max_prefill_batch = 12;
        policy.max_ctx_size = 32;
        policy.max_handoff_bytes = 100_000;

        assert_eq!(policy.token_context_prefill_limit(8), 12);
        assert_eq!(policy.effective_prompt_limit(8), Some(12));
    }

    #[test]
    fn chunked_capability_removes_whole_prompt_prefill_batch_gate() {
        let mut policy = policy();
        policy.max_prompt_tokens = 8192;
        policy.max_prefill_batch = 1800;
        policy.max_ctx_size = 8192;
        policy.max_handoff_bytes = 10_000_000;
        policy.chunked_prefill = Some(PdChunkedPrefillConfig::new(1800, 1800).unwrap());

        let decision = policy.evaluate(4000, 32);

        assert!(decision.should_start_prefill());
        assert_eq!(decision.outcome().reason, PdAdmissionReason::WithinLimits);
        assert_eq!(decision.outcome().token_context_prefill_limit, 8192 - 32);
    }

    #[test]
    fn missing_chunked_capability_keeps_prefill_batch_rejection() {
        let mut policy = policy();
        policy.max_prompt_tokens = 8192;
        policy.max_prefill_batch = 1800;
        policy.max_ctx_size = 8192;
        policy.max_handoff_bytes = 10_000_000;

        let decision = policy.evaluate(4000, 32);

        assert!(!decision.should_start_prefill());
        assert_eq!(
            decision.outcome().reason,
            PdAdmissionReason::PrefillBatchExceeded
        );
    }

    #[test]
    fn effective_prompt_limit_includes_handoff_budget() {
        let mut policy = policy();
        policy.max_prompt_tokens = 64;
        policy.max_prefill_batch = 64;
        policy.max_ctx_size = 128;
        policy.max_handoff_bytes = 1_000;

        assert_eq!(policy.token_context_prefill_limit(8), 64);
        assert_eq!(policy.effective_prompt_limit(8), Some(10));
    }

    #[test]
    fn gate_order_checks_context_before_prefill_batch() {
        let mut policy = policy();
        policy.max_prompt_tokens = 64;
        policy.max_prefill_batch = 16;
        policy.max_ctx_size = 20;
        let decision = policy.evaluate(17, 4);

        assert_eq!(
            decision.outcome().reason,
            PdAdmissionReason::CtxSizeExceeded
        );
    }

    #[test]
    fn reject_policy_returns_pre_content_rejection() {
        let mut policy = policy();
        policy.over_limit_action = PdAdmissionOverLimitAction::Reject;
        let decision = policy.evaluate(17, 4);
        assert!(!decision.should_start_prefill());
        assert!(matches!(decision, PdAdmissionDecision::Reject(_)));
        assert_eq!(decision.outcome().result, PdAdmissionResult::Rejected);
    }

    #[test]
    fn kv_bytes_hard_guard_blocks_prefill() {
        let mut policy = policy();
        policy.max_prompt_tokens = 64;
        policy.max_prefill_batch = 64;
        policy.max_handoff_bytes = 1_000;
        let decision = policy.evaluate(11, 4);
        assert!(!decision.should_start_prefill());
        assert_eq!(
            decision.outcome().reason,
            PdAdmissionReason::EstimatedHandoffBytesExceeded
        );
    }

    #[test]
    fn unavailable_kv_estimate_fails_safe() {
        let mut policy = policy();
        policy.estimated_kv_bytes_per_token = None;
        policy.kv_bytes_per_token_source = PdKvBytesPerTokenSource::Missing;
        let decision = policy.evaluate(8, 4);
        assert!(!decision.should_start_prefill());
        assert!(matches!(decision, PdAdmissionDecision::Unavailable(_)));
        assert_eq!(
            decision.outcome().reason,
            PdAdmissionReason::EstimatedHandoffBytesUnavailable
        );
    }

    #[test]
    fn configured_kv_bytes_per_token_takes_precedence() {
        let mut args = valid_mvp_args();
        args.pd_estimated_kv_bytes_per_token = Some(1_000_000);
        let policy =
            pd_admission_policy_from_args(&args, PdServingMode::Mvp, "custom-model").unwrap();
        let policy = policy.unwrap();

        assert_eq!(policy.estimated_kv_bytes_per_token, Some(1_000_000));
        assert_eq!(
            policy.kv_bytes_per_token_source,
            PdKvBytesPerTokenSource::Configured
        );
    }

    #[test]
    fn gemma4_uses_measured_kv_bytes_per_token_when_not_configured() {
        let mut args = valid_mvp_args();
        args.pd_estimated_kv_bytes_per_token = None;
        let policy = pd_admission_policy_from_args(
            &args,
            PdServingMode::Mvp,
            PD_GEMMA4_31B_IT_BF16_MODEL_ID,
        )
        .unwrap()
        .unwrap();

        assert_eq!(
            policy.estimated_kv_bytes_per_token,
            Some(PD_GEMMA4_NATIVE_FULL_STATE_KV_BYTES_PER_TOKEN)
        );
        assert_eq!(
            policy.kv_bytes_per_token_source,
            PdKvBytesPerTokenSource::Measured
        );
    }

    #[test]
    fn unknown_model_without_kv_calibration_fails_safe() {
        let mut args = valid_mvp_args();
        args.pd_estimated_kv_bytes_per_token = None;
        let error =
            pd_admission_policy_from_args(&args, PdServingMode::Mvp, "custom-model").unwrap_err();

        assert!(
            error
                .to_string()
                .contains("--pd-estimated-kv-bytes-per-token"),
            "{error:?}"
        );
    }

    #[test]
    fn policy_flags_require_pd_serving_mvp() {
        let args = ServeOpenAiArgs {
            config: PathBuf::from("/tmp/stage.json"),
            topology: None,
            bind_addr: "127.0.0.1:9337".parse().unwrap(),
            model_id: None,
            default_max_tokens: 16,
            generation_concurrency: 1,
            first_stage_addr: None,
            pd_router_validation: true,
            pd_serving_mvp: false,
            pd_prefill_addr: None,
            pd_decode_addr: None,
            pd_expected_artifact_sha256: None,
            pd_expected_tokenizer_hash: None,
            pd_expected_chat_template_hash: None,
            pd_source_node_id: "pgx-prefill-validation".to_string(),
            pd_target_node_id: "mac-decode-validation".to_string(),
            pd_fault_injection: PdRouterValidationFault::None,
            pd_serving_mvp_test_fault: PdServingMvpTestFault::None,
            pd_serving_mvp_allow_test_faults: false,
            pd_admission_over_limit: PdAdmissionOverLimitAction::Fallback,
            pd_max_prompt_tokens: Some(16),
            pd_max_prefill_batch: None,
            pd_max_ctx_size: None,
            pd_max_handoff_bytes: None,
            pd_estimated_kv_bytes_per_token: None,
            pd_chunked_prefill: false,
            pd_prefill_chunk_size: None,
            prefill_chunk_size: 256,
            prefill_chunk_policy: "fixed".to_string(),
            prefill_chunk_schedule: None,
            prefill_adaptive_start: 128,
            prefill_adaptive_step: 128,
            prefill_adaptive_max: 384,
            activation_wire_dtype: "f32".to_string(),
            startup_timeout_secs: 60,
            metrics_otlp_grpc: None,
            telemetry_queue_capacity: 1024,
            telemetry_level: TelemetryLevel::Summary,
        };
        let error =
            pd_admission_policy_from_args(&args, PdServingMode::Validation, "model").unwrap_err();
        assert!(
            error
                .to_string()
                .contains("PD admission policy flags require --pd-serving-mvp"),
            "{error:?}"
        );
    }

    fn valid_mvp_args() -> ServeOpenAiArgs {
        ServeOpenAiArgs {
            config: PathBuf::from("/tmp/stage.json"),
            topology: None,
            bind_addr: "127.0.0.1:9337".parse().unwrap(),
            model_id: None,
            default_max_tokens: 16,
            generation_concurrency: 1,
            first_stage_addr: None,
            pd_router_validation: false,
            pd_serving_mvp: true,
            pd_prefill_addr: Some("127.0.0.1:19081".to_string()),
            pd_decode_addr: Some("127.0.0.1:19082".to_string()),
            pd_expected_artifact_sha256: Some(
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            ),
            pd_expected_tokenizer_hash: Some(
                "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
            ),
            pd_expected_chat_template_hash: Some(
                "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc".to_string(),
            ),
            pd_source_node_id: "pgx-prefill-mvp".to_string(),
            pd_target_node_id: "mac-decode-mvp".to_string(),
            pd_fault_injection: PdRouterValidationFault::None,
            pd_serving_mvp_test_fault: PdServingMvpTestFault::None,
            pd_serving_mvp_allow_test_faults: false,
            pd_admission_over_limit: PdAdmissionOverLimitAction::Fallback,
            pd_max_prompt_tokens: Some(16),
            pd_max_prefill_batch: Some(16),
            pd_max_ctx_size: Some(32),
            pd_max_handoff_bytes: Some(16_000),
            pd_estimated_kv_bytes_per_token: Some(100),
            pd_chunked_prefill: false,
            pd_prefill_chunk_size: None,
            prefill_chunk_size: 256,
            prefill_chunk_policy: "fixed".to_string(),
            prefill_chunk_schedule: None,
            prefill_adaptive_start: 128,
            prefill_adaptive_step: 128,
            prefill_adaptive_max: 384,
            activation_wire_dtype: "f32".to_string(),
            startup_timeout_secs: 60,
            metrics_otlp_grpc: None,
            telemetry_queue_capacity: 1024,
            telemetry_level: TelemetryLevel::Summary,
        }
    }
}
