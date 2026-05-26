use std::{net::SocketAddr, path::PathBuf};

use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(name = "skippy-correctness")]
#[command(about = "Validate staged llama execution against full-model execution")]
pub struct Cli {
    #[command(subcommand)]
    pub command: CommandKind,
}

#[derive(Subcommand)]
pub enum CommandKind {
    SingleStep(SingleStepArgs),
    Chain(ChainArgs),
    SplitScan(SplitScanArgs),
    DtypeMatrix(DtypeMatrixArgs),
    StateHandoff(StateHandoffArgs),
    RouterValidation(RouterValidationArgs),
    KvPageHandoff(KvPageHandoffArgs),
    KvStreamingHandoff(KvStreamingHandoffArgs),
}

#[derive(Args, Clone)]
pub struct RuntimeArgs {
    #[arg(long, alias = "model-path")]
    pub model: PathBuf,
    #[arg(
        long,
        help = "Model coordinate for local model paths, for example org/repo:Q4_K_M. If omitted, Hugging Face cache paths are resolved from cache provenance."
    )]
    pub model_id: Option<String>,
    #[arg(long)]
    pub stage_model: Option<PathBuf>,
    #[arg(long, value_enum, default_value = "runtime-slice")]
    pub stage_load_mode: StageLoadMode,
    #[arg(long, default_value_t = 30)]
    pub layer_end: u32,
    #[arg(long, default_value_t = 128)]
    pub ctx_size: u32,
    #[arg(long, default_value_t = 0)]
    pub n_gpu_layers: i32,
    #[arg(long)]
    pub n_batch: Option<u32>,
    #[arg(long)]
    pub n_ubatch: Option<u32>,
    #[arg(long, default_value = "Hello")]
    pub prompt: String,
    #[arg(long = "flash-attn", value_enum, default_value = "auto")]
    pub flash_attn: FlashAttentionArg,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum StageLoadMode {
    RuntimeSlice,
    ArtifactSlice,
    LayerPackage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum FlashAttentionArg {
    Auto,
    Disabled,
    Enabled,
}

#[derive(Args, Clone)]
pub struct ServerArgs {
    #[arg(long, default_value = "target/debug/skippy-server")]
    pub stage_server_bin: PathBuf,
    #[arg(long)]
    pub child_logs: bool,
    #[arg(long, default_value_t = 60)]
    pub startup_timeout_secs: u64,
}

#[derive(Args)]
pub struct OutputArgs {
    #[arg(long)]
    pub report_out: Option<PathBuf>,
}

#[derive(Args)]
pub struct SingleStepArgs {
    #[command(flatten)]
    pub runtime: RuntimeArgs,
    #[command(flatten)]
    pub server: ServerArgs,
    #[command(flatten)]
    pub output: OutputArgs,
    #[arg(long, default_value_t = 15)]
    pub split_layer: u32,
    #[arg(long, default_value = "127.0.0.1:19021")]
    pub stage1_bind_addr: SocketAddr,
    #[arg(long, default_value = "f16")]
    pub activation_wire_dtype: String,
    #[arg(long)]
    pub allow_mismatch: bool,
}

#[derive(Args)]
pub struct ChainArgs {
    #[command(flatten)]
    pub runtime: RuntimeArgs,
    #[command(flatten)]
    pub server: ServerArgs,
    #[command(flatten)]
    pub output: OutputArgs,
    #[arg(long, default_value = "10,20")]
    pub splits: String,
    #[arg(long, default_value = "127.0.0.1:19031")]
    pub stage1_bind_addr: SocketAddr,
    #[arg(long, default_value = "127.0.0.1:19032")]
    pub stage2_bind_addr: SocketAddr,
    #[arg(long, default_value = "f16")]
    pub activation_wire_dtype: String,
    #[arg(long)]
    pub allow_mismatch: bool,
}

#[derive(Args)]
pub struct SplitScanArgs {
    #[command(flatten)]
    pub runtime: RuntimeArgs,
    #[command(flatten)]
    pub server: ServerArgs,
    #[command(flatten)]
    pub output: OutputArgs,
    #[arg(long, default_value = "1..30")]
    pub splits: String,
    #[arg(long, default_value = "127.0.0.1:19041")]
    pub stage1_bind_addr: SocketAddr,
    #[arg(long, default_value = "f16")]
    pub activation_wire_dtype: String,
    #[arg(long)]
    pub allow_mismatch: bool,
}

#[derive(Args)]
pub struct DtypeMatrixArgs {
    #[command(flatten)]
    pub runtime: RuntimeArgs,
    #[command(flatten)]
    pub server: ServerArgs,
    #[command(flatten)]
    pub output: OutputArgs,
    #[arg(long, default_value_t = 15)]
    pub split_layer: u32,
    #[arg(long, default_value = "127.0.0.1:19051")]
    pub stage1_bind_addr: SocketAddr,
    #[arg(long, default_value = "f32,f16,q8")]
    pub dtypes: String,
    #[arg(long)]
    pub allow_mismatch: bool,
}

#[derive(Args)]
pub struct StateHandoffArgs {
    #[command(flatten)]
    pub runtime: RuntimeArgs,
    #[command(flatten)]
    pub server: ServerArgs,
    #[command(flatten)]
    pub output: OutputArgs,
    #[arg(long, default_value = "127.0.0.1:19061")]
    pub source_bind_addr: SocketAddr,
    #[arg(long, default_value = "127.0.0.1:19062")]
    pub restore_bind_addr: SocketAddr,
    #[arg(long, default_value_t = 2048)]
    pub activation_width: i32,
    #[arg(long, default_value = "f16")]
    pub activation_wire_dtype: String,
    #[arg(long, default_value_t = 0)]
    pub state_layer_start: u32,
    #[arg(long)]
    pub state_layer_end: Option<u32>,
    #[arg(long)]
    pub state_stage_index: Option<u32>,
    #[arg(long, value_enum, default_value = "full-state")]
    pub state_payload_kind: StatePayloadKind,
    #[arg(long)]
    pub prefix_token_count: Option<usize>,
    #[arg(long, default_value_t = 1)]
    pub cache_hit_repeats: usize,
    #[arg(long)]
    pub runtime_lane_count: Option<u32>,
    #[arg(long)]
    pub borrow_resident_hits: bool,
    #[arg(long)]
    pub cache_decoded_result_hits: bool,
    #[arg(long)]
    pub synthetic_input_activation: bool,
    #[arg(long)]
    pub binary_control: bool,
    #[arg(
        long,
        help = "Connect to already-running binary source/restore servers instead of spawning local servers. Intended for cross-machine state handoff spikes."
    )]
    pub external_binary_control: bool,
    #[arg(long)]
    pub allow_mismatch: bool,
}

#[derive(Args)]
pub struct RouterValidationArgs {
    #[command(flatten)]
    pub output: OutputArgs,
    #[arg(long)]
    pub markdown_out: Option<PathBuf>,
    #[arg(long, default_value = "pd-router-validation-local")]
    pub request_id: String,
    #[arg(long, default_value = "handoff-local-001")]
    pub handoff_id: String,
    #[arg(long, default_value = "pgx-prefill-validation")]
    pub source_node_id: String,
    #[arg(long, default_value = "mac-decode-validation")]
    pub target_node_id: String,
    #[arg(long, default_value = "google/gemma-4-31b-it:bf16")]
    pub model_id: String,
    #[arg(long, default_value_t = 4096)]
    pub synthetic_payload_bytes: usize,
}

#[derive(Args)]
pub struct KvPageHandoffArgs {
    #[command(subcommand)]
    pub role: KvPageHandoffRole,
}

#[derive(Subcommand)]
pub enum KvPageHandoffRole {
    Source(KvPageHandoffSourceArgs),
    Coordinator(KvPageHandoffCoordinatorArgs),
}

#[derive(Args)]
pub struct KvPageHandoffSourceArgs {
    #[command(flatten)]
    pub output: OutputArgs,
    #[arg(long, default_value = "127.0.0.1:19430")]
    pub bind_addr: SocketAddr,
    #[arg(long)]
    pub model: Option<PathBuf>,
    #[arg(long, value_enum, default_value = "runtime-slice")]
    pub stage_load_mode: StageLoadMode,
    #[arg(long, default_value_t = 30)]
    pub layer_end: u32,
    #[arg(long, default_value_t = 512)]
    pub ctx_size: u32,
    #[arg(long, default_value_t = 0)]
    pub n_gpu_layers: i32,
    #[arg(long)]
    pub n_batch: Option<u32>,
    #[arg(long)]
    pub n_ubatch: Option<u32>,
    #[arg(long = "flash-attn", value_enum, default_value = "auto")]
    pub flash_attn: FlashAttentionArg,
    #[arg(long, default_value = "source")]
    pub session_id: String,
    #[arg(
        long,
        default_value = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    )]
    pub artifact_sha256: String,
    #[arg(
        long,
        default_value = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    )]
    pub tokenizer_hash: String,
    #[arg(
        long,
        default_value = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
    )]
    pub chat_template_hash: String,
    #[arg(long, default_value_t = 64)]
    pub chunk_tokens: usize,
    #[arg(long, default_value_t = 2)]
    pub chunk_count: usize,
}

#[derive(Args)]
pub struct KvPageHandoffCoordinatorArgs {
    #[command(flatten)]
    pub output: OutputArgs,
    #[arg(long)]
    pub markdown_out: Option<PathBuf>,
    #[arg(long, default_value = "127.0.0.1:19430")]
    pub source_addr: SocketAddr,
    #[arg(long)]
    pub model: Option<PathBuf>,
    #[arg(long, value_enum, default_value = "runtime-slice")]
    pub stage_load_mode: StageLoadMode,
    #[arg(long, default_value_t = 30)]
    pub layer_end: u32,
    #[arg(long, default_value_t = 512)]
    pub ctx_size: u32,
    #[arg(long, default_value_t = 0)]
    pub n_gpu_layers: i32,
    #[arg(long)]
    pub n_batch: Option<u32>,
    #[arg(long)]
    pub n_ubatch: Option<u32>,
    #[arg(long = "flash-attn", value_enum, default_value = "auto")]
    pub flash_attn: FlashAttentionArg,
    #[arg(long, default_value = "source")]
    pub session_id: String,
    #[arg(
        long,
        default_value = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    )]
    pub artifact_sha256: String,
    #[arg(
        long,
        default_value = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    )]
    pub tokenizer_hash: String,
    #[arg(
        long,
        default_value = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
    )]
    pub chat_template_hash: String,
    #[arg(long, default_value = "synthetic-two-chunk")]
    pub prompt_id: String,
    #[arg(long, default_value_t = 128)]
    pub total_tokens: usize,
    #[arg(long, default_value_t = 64)]
    pub chunk_tokens: usize,
    #[arg(long, default_value_t = 16)]
    pub max_tokens: usize,
    #[arg(long, default_value_t = 42)]
    pub seed: u64,
    #[arg(long, value_enum, default_value = "trim-replay-last-token")]
    pub bootstrap_strategy: KvPageBootstrapStrategy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum KvPageBootstrapStrategy {
    TrimReplayLastToken,
}

#[derive(Args)]
pub struct KvStreamingHandoffArgs {
    #[command(subcommand)]
    pub role: KvStreamingHandoffRole,
}

#[derive(Subcommand)]
pub enum KvStreamingHandoffRole {
    Local(KvStreamingHandoffLocalArgs),
    Source(KvStreamingHandoffSourceArgs),
    Coordinator(KvStreamingHandoffCoordinatorArgs),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum KvStreamingPipelineMode {
    Serial,
    Async,
    SplitChannel,
}

#[derive(Args)]
pub struct KvStreamingHandoffLocalArgs {
    #[command(flatten)]
    pub output: OutputArgs,
    #[arg(long, value_enum, default_value = "serial")]
    pub pipeline_mode: KvStreamingPipelineMode,
    #[arg(long, default_value_t = 128)]
    pub total_tokens: usize,
    #[arg(long, default_value_t = 64)]
    pub chunk_tokens: usize,
    #[arg(long, default_value_t = 1)]
    pub max_in_flight_chunks: usize,
    #[arg(long, default_value_t = 1_048_576)]
    pub max_in_flight_bytes: u64,
    #[arg(long, default_value_t = 524_288)]
    pub max_frame_bytes: u64,
    #[arg(long, default_value_t = 2)]
    pub max_queue_depth: usize,
    #[arg(long, default_value_t = 4096)]
    pub page_bytes_per_chunk: u64,
}

#[derive(Args)]
pub struct KvStreamingHandoffSourceArgs {
    #[command(flatten)]
    pub output: OutputArgs,
    #[arg(long, value_enum, default_value = "serial")]
    pub pipeline_mode: KvStreamingPipelineMode,
    #[arg(long, alias = "control-bind-addr", default_value = "127.0.0.1:19430")]
    pub bind_addr: SocketAddr,
    #[arg(long, default_value = "127.0.0.1:19431")]
    pub page_bind_addr: SocketAddr,
    #[arg(long)]
    pub model: Option<PathBuf>,
    #[arg(long, value_enum, default_value = "runtime-slice")]
    pub stage_load_mode: StageLoadMode,
    #[arg(long, default_value_t = 60)]
    pub layer_end: u32,
    #[arg(long, default_value_t = 8192)]
    pub ctx_size: u32,
    #[arg(long, default_value_t = 0)]
    pub n_gpu_layers: i32,
    #[arg(long)]
    pub n_batch: Option<u32>,
    #[arg(long)]
    pub n_ubatch: Option<u32>,
    #[arg(long = "flash-attn", value_enum, default_value = "auto")]
    pub flash_attn: FlashAttentionArg,
    #[arg(long, default_value = "source")]
    pub session_id: String,
    #[arg(
        long,
        default_value = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    )]
    pub artifact_sha256: String,
    #[arg(
        long,
        default_value = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    )]
    pub tokenizer_hash: String,
    #[arg(
        long,
        default_value = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
    )]
    pub chat_template_hash: String,
    #[arg(long, default_value_t = 2)]
    pub max_queue_depth: usize,
}

#[derive(Args)]
pub struct KvStreamingHandoffCoordinatorArgs {
    #[command(flatten)]
    pub output: OutputArgs,
    #[arg(long)]
    pub markdown_out: Option<PathBuf>,
    #[arg(long, value_enum, default_value = "serial")]
    pub pipeline_mode: KvStreamingPipelineMode,
    #[arg(long, alias = "control-addr", default_value = "127.0.0.1:19430")]
    pub source_addr: SocketAddr,
    #[arg(long, default_value = "127.0.0.1:19431")]
    pub page_addr: SocketAddr,
    #[arg(long)]
    pub model: Option<PathBuf>,
    #[arg(long, value_enum, default_value = "runtime-slice")]
    pub stage_load_mode: StageLoadMode,
    #[arg(long, default_value_t = 60)]
    pub layer_end: u32,
    #[arg(long, default_value_t = 8192)]
    pub ctx_size: u32,
    #[arg(long, default_value_t = 0)]
    pub n_gpu_layers: i32,
    #[arg(long)]
    pub n_batch: Option<u32>,
    #[arg(long)]
    pub n_ubatch: Option<u32>,
    #[arg(long = "flash-attn", value_enum, default_value = "auto")]
    pub flash_attn: FlashAttentionArg,
    #[arg(long, default_value = "source")]
    pub session_id: String,
    #[arg(
        long,
        default_value = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    )]
    pub artifact_sha256: String,
    #[arg(
        long,
        default_value = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    )]
    pub tokenizer_hash: String,
    #[arg(
        long,
        default_value = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
    )]
    pub chat_template_hash: String,
    #[arg(long, default_value = "synthetic-streaming-two-chunk")]
    pub prompt_id: String,
    #[arg(long, default_value_t = 128)]
    pub total_tokens: usize,
    #[arg(long, default_value_t = 64)]
    pub chunk_tokens: usize,
    #[arg(long, default_value_t = 16)]
    pub max_tokens: usize,
    #[arg(long, default_value_t = 42)]
    pub seed: u64,
    #[arg(long, value_enum, default_value = "trim-replay-last-token")]
    pub bootstrap_strategy: KvPageBootstrapStrategy,
    #[arg(long, default_value_t = 1)]
    pub max_in_flight_chunks: usize,
    #[arg(long, default_value_t = 1_073_741_824)]
    pub max_in_flight_bytes: u64,
    #[arg(long, default_value_t = 1_073_741_824)]
    pub max_frame_bytes: u64,
    #[arg(long, default_value_t = 2)]
    pub max_queue_depth: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum StatePayloadKind {
    ResidentKv,
    FullState,
    RecurrentOnly,
    KvRecurrent,
}
