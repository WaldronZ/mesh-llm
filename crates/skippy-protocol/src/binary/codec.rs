use std::{
    io::{self, Read, Write},
    time::Instant,
};

use sha2::{Digest, Sha256};

use super::{
    activation::activation_wire_bytes_with_state_flags, invalid_data, invalid_input,
    StageLogitBias, StageReply, StageReplyStats, StageSamplingConfig, StageStateHeader,
    StageWireMessage, WireActivationDType, WireMessageKind, WireReplyKind,
    DEFAULT_LARGE_STATE_FRAME_BYTES, LARGE_STATE_FRAMING_PROTOCOL_VERSION,
    LEGACY_STATE_IMPORT_MAX_BYTES, MAX_LARGE_STATE_FRAME_BYTES, MAX_LARGE_STATE_PAYLOAD_BYTES,
    MAX_STAGE_LOGIT_BIAS, READY_MAGIC, STAGE_STATE_VERSION,
};

pub fn send_ready(mut writer: impl Write) -> io::Result<()> {
    write_i32(&mut writer, READY_MAGIC)
}

pub fn recv_ready(mut reader: impl Read) -> io::Result<()> {
    let magic = read_i32(&mut reader)?;
    if magic != READY_MAGIC {
        return Err(invalid_data("stage ready magic mismatch"));
    }
    Ok(())
}

pub fn send_reply_ack(mut writer: impl Write) -> io::Result<()> {
    send_reply_ack_with_stats(&mut writer, StageReplyStats::default())
}

pub fn send_reply_ack_with_stats(mut writer: impl Write, stats: StageReplyStats) -> io::Result<()> {
    write_i32(&mut writer, WireReplyKind::Ack as i32)?;
    write_i32(&mut writer, 0)?;
    write_i32(&mut writer, 0)?;
    write_reply_stats(&mut writer, stats)
}

pub fn send_reply_predicted(mut writer: impl Write, predicted: i32) -> io::Result<()> {
    send_reply_predicted_with_stats(&mut writer, predicted, StageReplyStats::default())
}

pub fn send_reply_predicted_with_stats(
    mut writer: impl Write,
    predicted: i32,
    stats: StageReplyStats,
) -> io::Result<()> {
    write_i32(&mut writer, WireReplyKind::PredictedToken as i32)?;
    write_i32(&mut writer, predicted)?;
    write_i32(&mut writer, 1)?;
    write_i32(&mut writer, predicted)?;
    write_reply_stats(&mut writer, stats)
}

pub fn send_reply_predicted_tokens_with_stats(
    mut writer: impl Write,
    predicted_tokens: &[i32],
    stats: StageReplyStats,
) -> io::Result<()> {
    let predicted = predicted_tokens.first().copied().unwrap_or(0);
    write_i32(&mut writer, WireReplyKind::PredictedTokens as i32)?;
    write_i32(&mut writer, predicted)?;
    write_i32(
        &mut writer,
        i32::try_from(predicted_tokens.len())
            .map_err(|_| invalid_input("too many predicted tokens"))?,
    )?;
    for token in predicted_tokens {
        write_i32(&mut writer, *token)?;
    }
    write_reply_stats(&mut writer, stats)
}

pub fn recv_reply(mut reader: impl Read) -> io::Result<StageReply> {
    let kind = WireReplyKind::try_from(read_i32(&mut reader)?)?;
    let predicted = read_i32(&mut reader)?;
    let predicted_count = read_i32(&mut reader)?;
    if predicted_count < 0 {
        return Err(invalid_data("negative predicted token count"));
    }
    let mut predicted_tokens = Vec::with_capacity(predicted_count as usize);
    for _ in 0..predicted_count {
        predicted_tokens.push(read_i32(&mut reader)?);
    }
    let stats = read_reply_stats(&mut reader)?;
    Ok(StageReply {
        kind,
        predicted,
        predicted_tokens,
        stats,
    })
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct StageMessageWriteTiming {
    pub total_ms: f64,
    pub raw_payload_ms: f64,
    pub activation_payload_ms: f64,
    pub large_state_checksum_ms: f64,
    pub large_state_frame_count: u64,
    pub large_state_payload_bytes: u64,
    pub large_state_frame_bytes: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct StageMessageReadTiming {
    pub total_ms: f64,
    pub header_ms: f64,
    pub raw_payload_ms: f64,
    pub activation_payload_ms: f64,
    pub large_state_checksum_ms: f64,
    pub large_state_frame_count: u64,
    pub large_state_payload_bytes: u64,
    pub large_state_frame_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateImportFramingKind {
    Legacy,
    Large,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LargeStateFramingOptions {
    pub enabled: bool,
    pub threshold_bytes: u64,
    pub max_frame_bytes: usize,
    pub max_payload_bytes: u64,
}

impl LargeStateFramingOptions {
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            threshold_bytes: LEGACY_STATE_IMPORT_MAX_BYTES,
            max_frame_bytes: DEFAULT_LARGE_STATE_FRAME_BYTES,
            max_payload_bytes: MAX_LARGE_STATE_PAYLOAD_BYTES,
        }
    }

    pub fn enabled() -> Self {
        Self {
            enabled: true,
            threshold_bytes: LEGACY_STATE_IMPORT_MAX_BYTES,
            max_frame_bytes: DEFAULT_LARGE_STATE_FRAME_BYTES,
            max_payload_bytes: MAX_LARGE_STATE_PAYLOAD_BYTES,
        }
    }

    pub fn with_threshold_bytes(mut self, threshold_bytes: u64) -> Self {
        self.threshold_bytes = threshold_bytes;
        self
    }

    pub fn with_max_frame_bytes(mut self, max_frame_bytes: usize) -> Self {
        self.max_frame_bytes = max_frame_bytes;
        self
    }
}

pub fn state_import_framing_kind_for_len(
    payload_len: u64,
    options: LargeStateFramingOptions,
) -> io::Result<StateImportFramingKind> {
    validate_large_state_options(options)?;
    if payload_len > options.max_payload_bytes {
        return Err(invalid_input(
            "large state payload exceeds configured limit",
        ));
    }
    if payload_len <= options.threshold_bytes && payload_len <= LEGACY_STATE_IMPORT_MAX_BYTES {
        return Ok(StateImportFramingKind::Legacy);
    }
    if !options.enabled {
        return Err(invalid_input("large state framing capability missing"));
    }
    Ok(StateImportFramingKind::Large)
}

pub fn write_stage_message(
    writer: impl Write,
    message: &StageWireMessage,
    dtype: WireActivationDType,
) -> io::Result<()> {
    write_stage_message_timed(writer, message, dtype).map(|_| ())
}

pub fn write_state_import_message_timed(
    writer: impl Write,
    message: &StageWireMessage,
    dtype: WireActivationDType,
    options: LargeStateFramingOptions,
) -> io::Result<StageMessageWriteTiming> {
    if message.kind != WireMessageKind::StateImport {
        return Err(invalid_input("large-state writer requires StateImport"));
    }
    let payload_len = u64::try_from(message.raw_bytes.len())
        .map_err(|_| invalid_input("state payload length overflows u64"))?;
    match state_import_framing_kind_for_len(payload_len, options)? {
        StateImportFramingKind::Legacy => {
            let mut legacy = message.clone();
            legacy.token_count = i32::try_from(legacy.raw_bytes.len())
                .map_err(|_| invalid_input("state import payload exceeds i32 length"))?;
            write_stage_message_timed(writer, &legacy, dtype)
        }
        StateImportFramingKind::Large => {
            if (message.state.flags & super::state_flags::LARGE_STATE_FRAMING) == 0 {
                return Err(invalid_input("large state framing capability missing"));
            }
            write_large_state_import_message_timed(writer, message, dtype, options)
        }
    }
}

pub fn write_stage_message_timed(
    mut writer: impl Write,
    message: &StageWireMessage,
    dtype: WireActivationDType,
) -> io::Result<StageMessageWriteTiming> {
    let total_started = Instant::now();
    let mut timing = StageMessageWriteTiming::default();
    // Wire v4 fixed prefix, little-endian:
    // kind, pos_start, token_count, token_sideband_count, position_sideband_count (5 x i32);
    // StageStateHeader (10 x i32); request_id, session_id (2 x u64);
    // optional StageSamplingConfig follows when state_flags::SAMPLING is set.
    // Token sideband, raw StateImport bytes, or activation bytes follow this
    // prefix, so prefill overhead stays independent of ID string length.
    write_i32(&mut writer, message.kind as i32)?;
    write_i32(&mut writer, message.pos_start)?;
    write_i32(&mut writer, message.token_count)?;
    write_i32(
        &mut writer,
        i32::try_from(message.tokens.len()).map_err(|_| invalid_input("too many tokens"))?,
    )?;
    write_i32(
        &mut writer,
        i32::try_from(message.positions.len())
            .map_err(|_| invalid_input("too many position sideband values"))?,
    )?;

    let mut state = message.state;
    state.reserved = dtype as i32;
    if message.sampling.is_some() {
        state.flags |= super::state_flags::SAMPLING;
    } else {
        state.flags &= !super::state_flags::SAMPLING;
    }
    if message.chat_sampling_metadata.is_some() {
        state.flags |= super::state_flags::CHAT_SAMPLING_METADATA;
    } else {
        state.flags &= !super::state_flags::CHAT_SAMPLING_METADATA;
    }
    write_state_header(&mut writer, state)?;
    write_u64(&mut writer, message.request_id)?;
    write_u64(&mut writer, message.session_id)?;
    if let Some(sampling) = message.sampling.as_ref() {
        write_sampling_config(&mut writer, sampling)?;
    }
    if let Some(metadata) = message.chat_sampling_metadata.as_ref() {
        let bytes = metadata.as_bytes();
        write_u32(
            &mut writer,
            u32::try_from(bytes.len())
                .map_err(|_| invalid_input("chat sampling metadata is too large"))?,
        )?;
        writer.write_all(bytes)?;
    }

    if message.kind == WireMessageKind::StateImport {
        let payload_started = Instant::now();
        writer.write_all(&message.raw_bytes)?;
        timing.raw_payload_ms = elapsed_ms(payload_started);
        timing.total_ms = elapsed_ms(total_started);
        return Ok(timing);
    }
    for token in &message.tokens {
        write_i32(&mut writer, *token)?;
    }
    for position in &message.positions {
        write_i32(&mut writer, *position)?;
    }
    let payload_started = Instant::now();
    writer.write_all(&message.activation)?;
    timing.activation_payload_ms = elapsed_ms(payload_started);
    timing.total_ms = elapsed_ms(total_started);
    Ok(timing)
}

pub fn read_stage_message(reader: impl Read, n_embd: i32) -> io::Result<StageWireMessage> {
    read_stage_message_timed(reader, n_embd).map(|(message, _)| message)
}

pub fn read_stage_message_timed(
    mut reader: impl Read,
    n_embd: i32,
) -> io::Result<(StageWireMessage, StageMessageReadTiming)> {
    let total_started = Instant::now();
    let header_started = Instant::now();
    let mut timing = StageMessageReadTiming::default();
    let kind = WireMessageKind::try_from(read_i32(&mut reader)?)?;
    let pos_start = read_i32(&mut reader)?;
    let token_count = read_i32(&mut reader)?;
    let token_sideband_count = read_i32(&mut reader)?;
    let position_sideband_count = read_i32(&mut reader)?;
    let state = read_state_header(&mut reader)?;
    if state.version != STAGE_STATE_VERSION {
        return Err(invalid_data("unsupported stage state version"));
    }
    let request_id = read_u64(&mut reader)?;
    let session_id = read_u64(&mut reader)?;
    let sampling = if (state.flags & super::state_flags::SAMPLING) != 0 {
        Some(read_sampling_config(&mut reader)?)
    } else {
        None
    };
    let chat_sampling_metadata = if (state.flags & super::state_flags::CHAT_SAMPLING_METADATA) != 0
    {
        let len = usize::try_from(read_u32(&mut reader)?)
            .map_err(|_| invalid_data("chat sampling metadata length overflows usize"))?;
        let mut bytes = vec![0_u8; len];
        reader.read_exact(&mut bytes)?;
        Some(
            String::from_utf8(bytes)
                .map_err(|_| invalid_data("chat sampling metadata is not UTF-8"))?,
        )
    } else {
        None
    };
    timing.header_ms = elapsed_ms(header_started);
    let dtype = state.dtype()?;
    if kind == WireMessageKind::LargeStateStart {
        return read_large_state_import_message_timed(
            &mut reader,
            LargeStateStartPrefix {
                pos_start,
                token_count,
                token_sideband_count,
                position_sideband_count,
                state,
                request_id,
                session_id,
                sampling,
                chat_sampling_metadata,
                dtype,
            },
            timing,
            total_started,
        );
    }
    if matches!(
        kind,
        WireMessageKind::LargeStateData | WireMessageKind::LargeStateEnd
    ) {
        return Err(invalid_data("large state frame without start"));
    }
    if kind == WireMessageKind::Stop {
        timing.total_ms = elapsed_ms(total_started);
        return Ok((
            StageWireMessage {
                kind,
                pos_start,
                token_count,
                state,
                request_id,
                session_id,
                sampling,
                chat_sampling_metadata,
                tokens: Vec::new(),
                positions: Vec::new(),
                activation: Vec::new(),
                raw_bytes: Vec::new(),
            },
            timing,
        ));
    }
    if token_count < 0 || token_sideband_count < 0 || position_sideband_count < 0 {
        return Err(invalid_data("negative wire count"));
    }
    if kind == WireMessageKind::StateImport {
        let mut raw_bytes = vec![0; token_count as usize];
        let payload_started = Instant::now();
        reader.read_exact(&mut raw_bytes)?;
        timing.raw_payload_ms = elapsed_ms(payload_started);
        timing.total_ms = elapsed_ms(total_started);
        return Ok((
            StageWireMessage {
                kind,
                pos_start,
                token_count,
                state,
                request_id,
                session_id,
                sampling,
                chat_sampling_metadata,
                tokens: Vec::new(),
                positions: Vec::new(),
                activation: Vec::new(),
                raw_bytes,
            },
            timing,
        ));
    }

    let mut tokens = Vec::with_capacity(token_sideband_count as usize);
    for _ in 0..token_sideband_count {
        tokens.push(read_i32(&mut reader)?);
    }
    let mut positions = Vec::with_capacity(position_sideband_count as usize);
    for _ in 0..position_sideband_count {
        positions.push(read_i32(&mut reader)?);
    }
    let activation_bytes =
        if state.source_stage_index < 0 || kind.is_activationless_prefix_cache_control() {
            0
        } else {
            activation_wire_bytes_with_state_flags(dtype, token_count, n_embd, state.flags)?
        };
    let mut activation = vec![0; activation_bytes];
    if activation_bytes > 0 {
        let payload_started = Instant::now();
        reader.read_exact(&mut activation)?;
        timing.activation_payload_ms = elapsed_ms(payload_started);
    }
    timing.total_ms = elapsed_ms(total_started);
    Ok((
        StageWireMessage {
            kind,
            pos_start,
            token_count,
            state,
            request_id,
            session_id,
            sampling,
            chat_sampling_metadata,
            tokens,
            positions,
            activation,
            raw_bytes: Vec::new(),
        },
        timing,
    ))
}

struct LargeStateStartPrefix {
    pos_start: i32,
    token_count: i32,
    token_sideband_count: i32,
    position_sideband_count: i32,
    state: StageStateHeader,
    request_id: u64,
    session_id: u64,
    sampling: Option<StageSamplingConfig>,
    chat_sampling_metadata: Option<String>,
    dtype: WireActivationDType,
}

#[derive(Debug, Clone)]
struct LargeStateStartMetadata {
    total_bytes: u64,
    frame_count: u64,
    max_frame_bytes: u64,
    checksum: [u8; 32],
}

fn write_large_state_import_message_timed(
    mut writer: impl Write,
    message: &StageWireMessage,
    dtype: WireActivationDType,
    options: LargeStateFramingOptions,
) -> io::Result<StageMessageWriteTiming> {
    validate_large_state_options(options)?;
    let total_started = Instant::now();
    let checksum_started = Instant::now();
    let checksum = sha256_bytes(&message.raw_bytes);
    let checksum_ms = elapsed_ms(checksum_started);
    let total_bytes = u64::try_from(message.raw_bytes.len())
        .map_err(|_| invalid_input("state payload length overflows u64"))?;
    if total_bytes > options.max_payload_bytes {
        return Err(invalid_input(
            "large state payload exceeds configured limit",
        ));
    }
    let frame_bytes = options.max_frame_bytes;
    let frame_count = frame_count(total_bytes, frame_bytes)?;
    let payload_started = Instant::now();
    write_large_state_frame_prefix(
        &mut writer,
        WireMessageKind::LargeStateStart,
        message.state,
        message.request_id,
        message.session_id,
        dtype,
    )?;
    write_u32(&mut writer, LARGE_STATE_FRAMING_PROTOCOL_VERSION)?;
    write_u64(&mut writer, total_bytes)?;
    write_u64(&mut writer, frame_count)?;
    write_u64(
        &mut writer,
        u64::try_from(frame_bytes).map_err(|_| invalid_input("frame size overflows u64"))?,
    )?;
    write_u32(&mut writer, checksum.len() as u32)?;
    writer.write_all(&checksum)?;

    for (frame_index, chunk) in message.raw_bytes.chunks(frame_bytes).enumerate() {
        let offset = frame_index
            .checked_mul(frame_bytes)
            .ok_or_else(|| invalid_input("large state frame offset overflow"))?;
        write_large_state_frame_prefix(
            &mut writer,
            WireMessageKind::LargeStateData,
            message.state,
            message.request_id,
            message.session_id,
            dtype,
        )?;
        write_u32(&mut writer, LARGE_STATE_FRAMING_PROTOCOL_VERSION)?;
        write_u64(&mut writer, frame_index as u64)?;
        write_u64(
            &mut writer,
            u64::try_from(offset).map_err(|_| invalid_input("frame offset overflows u64"))?,
        )?;
        write_u64(
            &mut writer,
            u64::try_from(chunk.len()).map_err(|_| invalid_input("frame length overflows u64"))?,
        )?;
        writer.write_all(chunk)?;
    }

    write_large_state_frame_prefix(
        &mut writer,
        WireMessageKind::LargeStateEnd,
        message.state,
        message.request_id,
        message.session_id,
        dtype,
    )?;
    write_u32(&mut writer, LARGE_STATE_FRAMING_PROTOCOL_VERSION)?;
    write_u64(&mut writer, total_bytes)?;
    write_u64(&mut writer, frame_count)?;

    Ok(StageMessageWriteTiming {
        total_ms: elapsed_ms(total_started),
        raw_payload_ms: elapsed_ms(payload_started),
        large_state_checksum_ms: checksum_ms,
        large_state_frame_count: frame_count,
        large_state_payload_bytes: total_bytes,
        large_state_frame_bytes: frame_bytes as u64,
        ..StageMessageWriteTiming::default()
    })
}

fn read_large_state_import_message_timed(
    mut reader: impl Read,
    prefix: LargeStateStartPrefix,
    mut timing: StageMessageReadTiming,
    total_started: Instant,
) -> io::Result<(StageWireMessage, StageMessageReadTiming)> {
    validate_large_state_prefix(&prefix)?;
    let metadata = read_large_state_start_metadata(&mut reader)?;
    if metadata.total_bytes > MAX_LARGE_STATE_PAYLOAD_BYTES {
        return Err(invalid_data("large state payload exceeds configured limit"));
    }
    let max_frame_bytes = usize::try_from(metadata.max_frame_bytes)
        .map_err(|_| invalid_data("large state frame size overflows usize"))?;
    if max_frame_bytes == 0 || max_frame_bytes > MAX_LARGE_STATE_FRAME_BYTES {
        return Err(invalid_data(
            "large state frame size exceeds configured limit",
        ));
    }
    let expected_frame_count = frame_count(metadata.total_bytes, max_frame_bytes)?;
    if metadata.frame_count != expected_frame_count {
        return Err(invalid_data("large state frame count mismatch"));
    }
    let payload_len = usize::try_from(metadata.total_bytes)
        .map_err(|_| invalid_data("large state payload length overflows usize"))?;
    let mut raw_bytes = Vec::new();
    raw_bytes
        .try_reserve_exact(payload_len)
        .map_err(|_| invalid_data("large state payload allocation failed"))?;
    let payload_started = Instant::now();
    let mut offset = 0_u64;
    for expected_index in 0..metadata.frame_count {
        let data_prefix = read_large_state_frame_prefix(&mut reader)?;
        validate_large_state_data_prefix(&data_prefix, &prefix)?;
        let version = read_u32(&mut reader)?;
        if version != LARGE_STATE_FRAMING_PROTOCOL_VERSION {
            return Err(invalid_data("unsupported large state frame version"));
        }
        let frame_index = read_u64(&mut reader)?;
        let frame_offset = read_u64(&mut reader)?;
        let frame_len_u64 = read_u64(&mut reader)?;
        if frame_index != expected_index {
            return Err(invalid_data("large state frame index mismatch"));
        }
        if frame_offset != offset {
            return Err(invalid_data("large state frame offset mismatch"));
        }
        if frame_len_u64 == 0 || frame_len_u64 > metadata.max_frame_bytes {
            return Err(invalid_data(
                "large state frame size exceeds configured limit",
            ));
        }
        let remaining = metadata.total_bytes.saturating_sub(offset);
        if frame_len_u64 > remaining {
            return Err(invalid_data(
                "large state frame length exceeds remaining payload",
            ));
        }
        let frame_len = usize::try_from(frame_len_u64)
            .map_err(|_| invalid_data("large state frame length overflows usize"))?;
        let mut chunk = vec![0_u8; frame_len];
        reader.read_exact(&mut chunk)?;
        raw_bytes.extend_from_slice(&chunk);
        offset = offset
            .checked_add(frame_len_u64)
            .ok_or_else(|| invalid_data("large state frame offset overflow"))?;
    }
    if offset != metadata.total_bytes || raw_bytes.len() != payload_len {
        return Err(invalid_data("large state total length mismatch"));
    }
    let end_prefix = read_large_state_frame_prefix(&mut reader)?;
    validate_large_state_end_prefix(&end_prefix, &prefix)?;
    let version = read_u32(&mut reader)?;
    if version != LARGE_STATE_FRAMING_PROTOCOL_VERSION {
        return Err(invalid_data("unsupported large state frame version"));
    }
    let end_total_bytes = read_u64(&mut reader)?;
    let end_frame_count = read_u64(&mut reader)?;
    if end_total_bytes != metadata.total_bytes || end_frame_count != metadata.frame_count {
        return Err(invalid_data("large state end metadata mismatch"));
    }
    timing.raw_payload_ms = elapsed_ms(payload_started);
    let checksum_started = Instant::now();
    let actual_checksum = sha256_bytes(&raw_bytes);
    timing.large_state_checksum_ms = elapsed_ms(checksum_started);
    if actual_checksum != metadata.checksum {
        return Err(invalid_data("large state checksum mismatch"));
    }
    timing.large_state_frame_count = metadata.frame_count;
    timing.large_state_payload_bytes = metadata.total_bytes;
    timing.large_state_frame_bytes = metadata.max_frame_bytes;
    timing.total_ms = elapsed_ms(total_started);
    Ok((
        StageWireMessage {
            kind: WireMessageKind::StateImport,
            pos_start: prefix.pos_start,
            token_count: i32::try_from(metadata.total_bytes).unwrap_or(i32::MAX),
            state: prefix.state,
            request_id: prefix.request_id,
            session_id: prefix.session_id,
            sampling: prefix.sampling,
            chat_sampling_metadata: prefix.chat_sampling_metadata,
            tokens: Vec::new(),
            positions: Vec::new(),
            activation: Vec::new(),
            raw_bytes,
        },
        timing,
    ))
}

fn write_large_state_frame_prefix(
    mut writer: impl Write,
    kind: WireMessageKind,
    mut state: StageStateHeader,
    request_id: u64,
    session_id: u64,
    dtype: WireActivationDType,
) -> io::Result<()> {
    state.reserved = dtype as i32;
    write_i32(&mut writer, kind as i32)?;
    write_i32(&mut writer, 0)?;
    write_i32(&mut writer, 0)?;
    write_i32(&mut writer, 0)?;
    write_i32(&mut writer, 0)?;
    write_state_header(&mut writer, state)?;
    write_u64(&mut writer, request_id)?;
    write_u64(&mut writer, session_id)
}

#[derive(Debug, Clone, Copy)]
struct LargeStateFramePrefix {
    kind: WireMessageKind,
    token_count: i32,
    token_sideband_count: i32,
    position_sideband_count: i32,
    state: StageStateHeader,
    request_id: u64,
    session_id: u64,
}

fn read_large_state_frame_prefix(mut reader: impl Read) -> io::Result<LargeStateFramePrefix> {
    let kind = WireMessageKind::try_from(read_i32(&mut reader)?)?;
    let _pos_start = read_i32(&mut reader)?;
    let token_count = read_i32(&mut reader)?;
    let token_sideband_count = read_i32(&mut reader)?;
    let position_sideband_count = read_i32(&mut reader)?;
    let state = read_state_header(&mut reader)?;
    if state.version != STAGE_STATE_VERSION {
        return Err(invalid_data("unsupported stage state version"));
    }
    let request_id = read_u64(&mut reader)?;
    let session_id = read_u64(&mut reader)?;
    Ok(LargeStateFramePrefix {
        kind,
        token_count,
        token_sideband_count,
        position_sideband_count,
        state,
        request_id,
        session_id,
    })
}

fn read_large_state_start_metadata(mut reader: impl Read) -> io::Result<LargeStateStartMetadata> {
    let version = read_u32(&mut reader)?;
    if version != LARGE_STATE_FRAMING_PROTOCOL_VERSION {
        return Err(invalid_data("unsupported large state frame version"));
    }
    let total_bytes = read_u64(&mut reader)?;
    let frame_count = read_u64(&mut reader)?;
    let max_frame_bytes = read_u64(&mut reader)?;
    let checksum_len = read_u32(&mut reader)?;
    if checksum_len != 32 {
        return Err(invalid_data("large state checksum length mismatch"));
    }
    let mut checksum = [0_u8; 32];
    reader.read_exact(&mut checksum)?;
    Ok(LargeStateStartMetadata {
        total_bytes,
        frame_count,
        max_frame_bytes,
        checksum,
    })
}

fn validate_large_state_prefix(prefix: &LargeStateStartPrefix) -> io::Result<()> {
    if prefix.token_count != 0
        || prefix.token_sideband_count != 0
        || prefix.position_sideband_count != 0
        || prefix.sampling.is_some()
        || prefix.chat_sampling_metadata.is_some()
    {
        return Err(invalid_data("invalid large state start prefix"));
    }
    if (prefix.state.flags & super::state_flags::LARGE_STATE_FRAMING) == 0 {
        return Err(invalid_data("large state framing capability missing"));
    }
    let _ = prefix.dtype;
    Ok(())
}

fn validate_large_state_data_prefix(
    data: &LargeStateFramePrefix,
    start: &LargeStateStartPrefix,
) -> io::Result<()> {
    validate_large_state_common_prefix(data, start, WireMessageKind::LargeStateData)
}

fn validate_large_state_end_prefix(
    end: &LargeStateFramePrefix,
    start: &LargeStateStartPrefix,
) -> io::Result<()> {
    validate_large_state_common_prefix(end, start, WireMessageKind::LargeStateEnd)
}

fn validate_large_state_common_prefix(
    frame: &LargeStateFramePrefix,
    start: &LargeStateStartPrefix,
    expected_kind: WireMessageKind,
) -> io::Result<()> {
    if frame.kind != expected_kind {
        return Err(invalid_data("large state frame kind mismatch"));
    }
    if frame.token_count != 0
        || frame.token_sideband_count != 0
        || frame.position_sideband_count != 0
    {
        return Err(invalid_data("invalid large state frame prefix"));
    }
    if frame.request_id != start.request_id || frame.session_id != start.session_id {
        return Err(invalid_data("large state frame identity mismatch"));
    }
    if frame.state.version != start.state.version
        || frame.state.flags != start.state.flags
        || frame.state.prompt_token_count != start.state.prompt_token_count
    {
        return Err(invalid_data("large state frame state mismatch"));
    }
    Ok(())
}

fn validate_large_state_options(options: LargeStateFramingOptions) -> io::Result<()> {
    if options.max_frame_bytes == 0 || options.max_frame_bytes > MAX_LARGE_STATE_FRAME_BYTES {
        return Err(invalid_input("invalid large state frame size"));
    }
    if options.max_payload_bytes == 0 || options.max_payload_bytes > MAX_LARGE_STATE_PAYLOAD_BYTES {
        return Err(invalid_input("invalid large state payload limit"));
    }
    Ok(())
}

fn frame_count(total_bytes: u64, frame_bytes: usize) -> io::Result<u64> {
    if frame_bytes == 0 {
        return Err(invalid_input("invalid large state frame size"));
    }
    let frame_bytes =
        u64::try_from(frame_bytes).map_err(|_| invalid_input("frame size overflows u64"))?;
    if total_bytes == 0 {
        return Ok(0);
    }
    Ok(total_bytes.div_ceil(frame_bytes))
}

fn sha256_bytes(payload: &[u8]) -> [u8; 32] {
    Sha256::digest(payload).into()
}

fn elapsed_ms(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1000.0
}

fn write_state_header(mut writer: impl Write, state: StageStateHeader) -> io::Result<()> {
    write_i32(&mut writer, state.version)?;
    write_i32(&mut writer, state.seq_id)?;
    write_i32(&mut writer, state.phase)?;
    write_i32(&mut writer, state.flags)?;
    write_i32(&mut writer, state.checkpoint_generation)?;
    write_i32(&mut writer, state.prompt_token_count)?;
    write_i32(&mut writer, state.decode_step)?;
    write_i32(&mut writer, state.current_token)?;
    write_i32(&mut writer, state.source_stage_index)?;
    write_i32(&mut writer, state.reserved)
}

fn read_state_header(mut reader: impl Read) -> io::Result<StageStateHeader> {
    Ok(StageStateHeader {
        version: read_i32(&mut reader)?,
        seq_id: read_i32(&mut reader)?,
        phase: read_i32(&mut reader)?,
        flags: read_i32(&mut reader)?,
        checkpoint_generation: read_i32(&mut reader)?,
        prompt_token_count: read_i32(&mut reader)?,
        decode_step: read_i32(&mut reader)?,
        current_token: read_i32(&mut reader)?,
        source_stage_index: read_i32(&mut reader)?,
        reserved: read_i32(&mut reader)?,
    })
}

fn write_sampling_config(mut writer: impl Write, sampling: &StageSamplingConfig) -> io::Result<()> {
    write_u32(&mut writer, sampling.flags)?;
    write_u32(&mut writer, sampling.seed)?;
    write_f32(&mut writer, sampling.temperature)?;
    write_f32(&mut writer, sampling.top_p)?;
    write_i32(&mut writer, sampling.top_k)?;
    write_f32(&mut writer, sampling.min_p)?;
    write_f32(&mut writer, sampling.presence_penalty)?;
    write_f32(&mut writer, sampling.frequency_penalty)?;
    write_f32(&mut writer, sampling.repeat_penalty)?;
    write_i32(&mut writer, sampling.penalty_last_n)?;
    let count = sampling.logit_bias.len().min(MAX_STAGE_LOGIT_BIAS);
    write_u32(&mut writer, count as u32)?;
    for bias in sampling.logit_bias.iter().take(count) {
        write_i32(&mut writer, bias.token_id)?;
        write_f32(&mut writer, bias.bias)?;
    }
    Ok(())
}

fn read_sampling_config(mut reader: impl Read) -> io::Result<StageSamplingConfig> {
    let mut sampling = StageSamplingConfig {
        flags: read_u32(&mut reader)?,
        seed: read_u32(&mut reader)?,
        temperature: read_f32(&mut reader)?,
        top_p: read_f32(&mut reader)?,
        top_k: read_i32(&mut reader)?,
        min_p: read_f32(&mut reader)?,
        presence_penalty: read_f32(&mut reader)?,
        frequency_penalty: read_f32(&mut reader)?,
        repeat_penalty: read_f32(&mut reader)?,
        penalty_last_n: read_i32(&mut reader)?,
        logit_bias: Vec::new(),
    };
    let logit_bias_count = usize::try_from(read_u32(&mut reader)?)
        .map_err(|_| invalid_data("logit bias count overflows usize"))?;
    if logit_bias_count > MAX_STAGE_LOGIT_BIAS {
        return Err(invalid_data("logit bias count exceeds maximum"));
    }
    sampling.logit_bias.reserve(logit_bias_count);
    for _ in 0..logit_bias_count {
        sampling.logit_bias.push(StageLogitBias {
            token_id: read_i32(&mut reader)?,
            bias: read_f32(&mut reader)?,
        });
    }
    Ok(sampling)
}

fn write_reply_stats(mut writer: impl Write, stats: StageReplyStats) -> io::Result<()> {
    write_i64(&mut writer, stats.kv_lookup_hits)?;
    write_i64(&mut writer, stats.kv_lookup_misses)?;
    write_i64(&mut writer, stats.kv_lookup_errors)?;
    write_i64(&mut writer, stats.kv_imported_pages)?;
    write_i64(&mut writer, stats.kv_imported_tokens)?;
    write_i64(&mut writer, stats.kv_recorded_pages)?;
    write_i64(&mut writer, stats.kv_recorded_bytes)?;
    write_i64(&mut writer, stats.kv_hit_stage_mask)?;
    write_i64(&mut writer, stats.kv_record_stage_mask)?;
    write_i64(&mut writer, stats.checkpoint_flush_us)?;
    write_i64(&mut writer, stats.checkpoint_prefill_drain_us)?;
    write_i64(&mut writer, stats.checkpoint_local_us)?;
    write_i64(&mut writer, stats.checkpoint_downstream_write_us)?;
    write_i64(&mut writer, stats.checkpoint_downstream_wait_us)?;
    write_i64(&mut writer, stats.checkpoint_total_us)?;
    write_i64(&mut writer, stats.checkpoint_prefill_drained_replies)?;
    write_i64(&mut writer, stats.restore_flush_us)?;
    write_i64(&mut writer, stats.restore_prefill_drain_us)?;
    write_i64(&mut writer, stats.restore_local_us)?;
    write_i64(&mut writer, stats.restore_downstream_write_us)?;
    write_i64(&mut writer, stats.restore_downstream_wait_us)?;
    write_i64(&mut writer, stats.restore_total_us)?;
    write_i64(&mut writer, stats.restore_prefill_drained_replies)?;
    write_i64(&mut writer, stats.verify_span_compute_us)?;
    write_i64(&mut writer, stats.verify_span_forward_write_us)?;
    write_i64(&mut writer, stats.verify_span_downstream_wait_us)?;
    write_i64(&mut writer, stats.verify_span_total_us)?;
    write_i64(&mut writer, stats.verify_span_stage_count)?;
    write_i64(&mut writer, stats.verify_span_request_count)?;
    write_i64(&mut writer, stats.verify_span_token_count)?;
    write_i64(&mut writer, stats.verify_span_max_tokens)?;
    write_i64(&mut writer, stats.verify_span_checkpointed_requests)?;
    write_i64(&mut writer, stats.verify_span_skip_checkpoint_requests)
}

fn read_reply_stats(mut reader: impl Read) -> io::Result<StageReplyStats> {
    Ok(StageReplyStats {
        kv_lookup_hits: read_i64(&mut reader)?,
        kv_lookup_misses: read_i64(&mut reader)?,
        kv_lookup_errors: read_i64(&mut reader)?,
        kv_imported_pages: read_i64(&mut reader)?,
        kv_imported_tokens: read_i64(&mut reader)?,
        kv_recorded_pages: read_i64(&mut reader)?,
        kv_recorded_bytes: read_i64(&mut reader)?,
        kv_hit_stage_mask: read_i64(&mut reader)?,
        kv_record_stage_mask: read_i64(&mut reader)?,
        checkpoint_flush_us: read_i64(&mut reader)?,
        checkpoint_prefill_drain_us: read_i64(&mut reader)?,
        checkpoint_local_us: read_i64(&mut reader)?,
        checkpoint_downstream_write_us: read_i64(&mut reader)?,
        checkpoint_downstream_wait_us: read_i64(&mut reader)?,
        checkpoint_total_us: read_i64(&mut reader)?,
        checkpoint_prefill_drained_replies: read_i64(&mut reader)?,
        restore_flush_us: read_i64(&mut reader)?,
        restore_prefill_drain_us: read_i64(&mut reader)?,
        restore_local_us: read_i64(&mut reader)?,
        restore_downstream_write_us: read_i64(&mut reader)?,
        restore_downstream_wait_us: read_i64(&mut reader)?,
        restore_total_us: read_i64(&mut reader)?,
        restore_prefill_drained_replies: read_i64(&mut reader)?,
        verify_span_compute_us: read_i64(&mut reader)?,
        verify_span_forward_write_us: read_i64(&mut reader)?,
        verify_span_downstream_wait_us: read_i64(&mut reader)?,
        verify_span_total_us: read_i64(&mut reader)?,
        verify_span_stage_count: read_i64(&mut reader)?,
        verify_span_request_count: read_i64(&mut reader)?,
        verify_span_token_count: read_i64(&mut reader)?,
        verify_span_max_tokens: read_i64(&mut reader)?,
        verify_span_checkpointed_requests: read_i64(&mut reader)?,
        verify_span_skip_checkpoint_requests: read_i64(&mut reader)?,
    })
}

fn read_i32(mut reader: impl Read) -> io::Result<i32> {
    let mut bytes = [0_u8; 4];
    reader.read_exact(&mut bytes)?;
    Ok(i32::from_le_bytes(bytes))
}

fn write_i32(mut writer: impl Write, value: i32) -> io::Result<()> {
    writer.write_all(&value.to_le_bytes())
}

fn read_u32(mut reader: impl Read) -> io::Result<u32> {
    let mut bytes = [0_u8; 4];
    reader.read_exact(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}

fn write_u32(mut writer: impl Write, value: u32) -> io::Result<()> {
    writer.write_all(&value.to_le_bytes())
}

fn read_f32(mut reader: impl Read) -> io::Result<f32> {
    let mut bytes = [0_u8; 4];
    reader.read_exact(&mut bytes)?;
    Ok(f32::from_le_bytes(bytes))
}

fn write_f32(mut writer: impl Write, value: f32) -> io::Result<()> {
    writer.write_all(&value.to_le_bytes())
}

fn read_i64(mut reader: impl Read) -> io::Result<i64> {
    let mut bytes = [0_u8; 8];
    reader.read_exact(&mut bytes)?;
    Ok(i64::from_le_bytes(bytes))
}

fn write_i64(mut writer: impl Write, value: i64) -> io::Result<()> {
    writer.write_all(&value.to_le_bytes())
}

fn read_u64(mut reader: impl Read) -> io::Result<u64> {
    let mut bytes = [0_u8; 8];
    reader.read_exact(&mut bytes)?;
    Ok(u64::from_le_bytes(bytes))
}

fn write_u64(mut writer: impl Write, value: u64) -> io::Result<()> {
    writer.write_all(&value.to_le_bytes())
}
