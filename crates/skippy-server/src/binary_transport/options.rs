use std::{net::SocketAddr, path::PathBuf};

use anyhow::{bail, Context, Result};
use skippy_protocol::{binary::WireActivationDType, StageConfig, StageTopology};

use crate::{
    cli::ServeBinaryArgs,
    config::load_json,
    frontend::pd_streaming_kv_production::{
        validate_pd_streaming_kv_capacity, PdStreamingKvCapacity,
    },
    telemetry::TelemetryLevel,
};

use super::{pd_streaming_kv_source::PdStreamingKvSourceOptions, WireCondition};

#[derive(Clone)]
pub struct BinaryStageOptions {
    pub config: StageConfig,
    pub topology: Option<StageTopology>,
    pub bind_addr: SocketAddr,
    pub activation_width: i32,
    pub wire_dtype: WireActivationDType,
    pub metrics_otlp_grpc: Option<String>,
    pub telemetry_queue_capacity: usize,
    pub telemetry_level: TelemetryLevel,
    pub max_inflight: usize,
    pub reply_credit_limit: Option<usize>,
    pub async_prefill_forward: bool,
    pub downstream_wire_condition: WireCondition,
    pub downstream_connect_timeout_secs: u64,
    pub openai: Option<EmbeddedOpenAiStageOptions>,
    pub(crate) pd_streaming_kv_source: Option<PdStreamingKvSourceOptions>,
}

#[derive(Clone)]
pub struct EmbeddedOpenAiStageOptions {
    pub bind_addr: SocketAddr,
    pub model_id: Option<String>,
    pub default_max_tokens: u32,
    pub generation_concurrency: usize,
    pub prefill_chunk_size: usize,
    pub prefill_chunk_policy: String,
    pub prefill_chunk_schedule: Option<String>,
    pub prefill_adaptive_start: usize,
    pub prefill_adaptive_step: usize,
    pub prefill_adaptive_max: usize,
    pub draft_model_path: Option<PathBuf>,
    pub speculative_window: usize,
    pub adaptive_speculative_window: bool,
    pub draft_n_gpu_layers: Option<i32>,
}

impl BinaryStageOptions {
    pub fn from_cli_args(args: ServeBinaryArgs) -> Result<Self> {
        if args.activation_width <= 0 {
            bail!("activation_width must be greater than zero");
        }
        if args.openai_generation_concurrency == 0 {
            bail!("--openai-generation-concurrency must be greater than zero");
        }
        if args.openai_prefill_chunk_size == 0 {
            bail!("--openai-prefill-chunk-size must be greater than zero");
        }
        let wire_dtype = parse_wire_dtype(&args.activation_wire_dtype)?;
        let downstream_wire_condition =
            WireCondition::new(args.downstream_wire_delay_ms, args.downstream_wire_mbps)?;
        let config = load_json::<StageConfig>(&args.config)
            .with_context(|| format!("load stage config {}", args.config.display()))?;
        let topology = match args.topology.as_ref() {
            Some(path) => Some(
                load_json::<StageTopology>(path)
                    .with_context(|| format!("load topology {}", path.display()))?,
            ),
            None => None,
        };
        let bind_addr = args.bind_addr.unwrap_or(config.bind_addr.parse()?);
        let pd_streaming_kv_source = pd_streaming_kv_source_options_from_args(&args)?;
        let openai = args
            .openai_bind_addr
            .map(|bind_addr| EmbeddedOpenAiStageOptions {
                bind_addr,
                model_id: args.openai_model_id,
                default_max_tokens: args.openai_default_max_tokens,
                generation_concurrency: args.openai_generation_concurrency,
                prefill_chunk_size: args.openai_prefill_chunk_size,
                prefill_chunk_policy: args.openai_prefill_chunk_policy,
                prefill_chunk_schedule: args.openai_prefill_chunk_schedule,
                prefill_adaptive_start: args.openai_prefill_adaptive_start,
                prefill_adaptive_step: args.openai_prefill_adaptive_step,
                prefill_adaptive_max: args.openai_prefill_adaptive_max,
                draft_model_path: args.openai_draft_model_path,
                speculative_window: args.openai_speculative_window,
                adaptive_speculative_window: args.openai_adaptive_speculative_window,
                draft_n_gpu_layers: args.openai_draft_n_gpu_layers,
            });
        Ok(Self {
            config,
            topology,
            bind_addr,
            activation_width: args.activation_width,
            wire_dtype,
            metrics_otlp_grpc: args.metrics_otlp_grpc,
            telemetry_queue_capacity: args.telemetry_queue_capacity,
            telemetry_level: args.telemetry_level,
            max_inflight: args.max_inflight,
            reply_credit_limit: args.reply_credit_limit,
            async_prefill_forward: args.async_prefill_forward || !args.no_async_prefill_forward,
            downstream_wire_condition,
            downstream_connect_timeout_secs: args.downstream_connect_timeout_secs,
            openai,
            pd_streaming_kv_source,
        })
    }
}

fn pd_streaming_kv_source_options_from_args(
    args: &ServeBinaryArgs,
) -> Result<Option<PdStreamingKvSourceOptions>> {
    let has_source_config = args.pd_stream_control_bind_addr.is_some()
        || args.pd_stream_page_bind_addr.is_some()
        || args.pd_stream_max_in_flight_chunks != 2
        || args.pd_stream_max_in_flight_bytes != 536_870_912
        || args.pd_stream_max_frame_bytes != 67_108_864
        || args.pd_stream_max_queue_depth != 4;
    if has_source_config && !args.pd_streaming_kv_source {
        bail!("PD streaming KV source options require --pd-streaming-kv-source");
    }
    if !args.pd_streaming_kv_source {
        return Ok(None);
    }
    let control_bind_addr = args.pd_stream_control_bind_addr.ok_or_else(|| {
        anyhow::anyhow!("--pd-streaming-kv-source requires --pd-stream-control-bind-addr")
    })?;
    let page_bind_addr = args.pd_stream_page_bind_addr.ok_or_else(|| {
        anyhow::anyhow!("--pd-streaming-kv-source requires --pd-stream-page-bind-addr")
    })?;
    let capacity = PdStreamingKvCapacity {
        max_in_flight_chunks: args.pd_stream_max_in_flight_chunks,
        max_in_flight_bytes: args.pd_stream_max_in_flight_bytes,
        max_frame_bytes: args.pd_stream_max_frame_bytes,
        max_queue_depth: args.pd_stream_max_queue_depth,
    };
    validate_pd_streaming_kv_capacity(capacity)?;
    Ok(Some(PdStreamingKvSourceOptions {
        control_bind_addr,
        page_bind_addr,
        max_in_flight_chunks: args.pd_stream_max_in_flight_chunks,
        max_in_flight_bytes: args.pd_stream_max_in_flight_bytes,
        max_frame_bytes: args.pd_stream_max_frame_bytes,
        max_queue_depth: args.pd_stream_max_queue_depth,
    }))
}

pub fn parse_wire_dtype(value: &str) -> Result<WireActivationDType> {
    match value {
        "fp32" | "f32" => Ok(WireActivationDType::F32),
        "fp16" | "f16" => Ok(WireActivationDType::F16),
        "q8" | "int8" | "i8" => Ok(WireActivationDType::Q8),
        _ => bail!("unsupported activation wire dtype {value}"),
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use crate::cli::{Cli, Command};

    use super::pd_streaming_kv_source_options_from_args;

    #[test]
    fn streaming_kv_source_options_are_default_off() {
        let cli = Cli::parse_from([
            "skippy-server",
            "serve-binary",
            "--config",
            "/tmp/stage.json",
            "--activation-width",
            "4096",
        ]);
        let Command::ServeBinary(args) = cli.command else {
            panic!("expected serve-binary command");
        };
        assert!(pd_streaming_kv_source_options_from_args(&args)
            .unwrap()
            .is_none());
    }

    #[test]
    fn streaming_kv_source_rejects_missing_bind_addrs() {
        let cli = Cli::parse_from([
            "skippy-server",
            "serve-binary",
            "--config",
            "/tmp/stage.json",
            "--activation-width",
            "4096",
            "--pd-streaming-kv-source",
        ]);
        let Command::ServeBinary(args) = cli.command else {
            panic!("expected serve-binary command");
        };
        let error = pd_streaming_kv_source_options_from_args(&args).unwrap_err();
        assert!(
            error.to_string().contains("--pd-stream-control-bind-addr"),
            "{error:?}"
        );
    }

    #[test]
    fn streaming_kv_source_rejects_invalid_capacity() {
        let cli = Cli::parse_from([
            "skippy-server",
            "serve-binary",
            "--config",
            "/tmp/stage.json",
            "--activation-width",
            "4096",
            "--pd-streaming-kv-source",
            "--pd-stream-control-bind-addr",
            "127.0.0.1:19430",
            "--pd-stream-page-bind-addr",
            "127.0.0.1:19431",
            "--pd-stream-max-frame-bytes",
            "8",
            "--pd-stream-max-in-flight-bytes",
            "4",
        ]);
        let Command::ServeBinary(args) = cli.command else {
            panic!("expected serve-binary command");
        };
        let error = pd_streaming_kv_source_options_from_args(&args).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("--pd-stream-max-frame-bytes cannot exceed"),
            "{error:?}"
        );
    }
}
