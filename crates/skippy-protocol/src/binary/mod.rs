mod activation;
mod codec;
mod types;

pub use activation::{
    activation_payload_multiplier_from_state_flags, activation_wire_bytes,
    activation_wire_bytes_with_state_flags, encode_f32_activation_payload,
    encode_f32_activation_payload_with_state_flags,
};
pub use codec::{
    read_stage_message, read_stage_message_timed, recv_ready, recv_reply, send_ready,
    send_reply_ack, send_reply_ack_with_stats, send_reply_predicted,
    send_reply_predicted_tokens_with_stats, send_reply_predicted_with_stats,
    state_import_framing_kind_for_len, write_stage_message, write_stage_message_timed,
    write_state_import_message_timed, LargeStateFramingOptions, StageMessageReadTiming,
    StageMessageWriteTiming, StateImportFramingKind,
};
pub use types::{
    activation_frame_flags_from_state_flags, activation_state_flags_from_frame_flags, state_flags,
    StageLogitBias, StageReply, StageReplyStats, StageSamplingConfig, StageStateHeader,
    StageWireMessage, WireActivationDType, WireMessageKind, WireReplyKind, WireStagePhase,
    ACTIVATION_FLAG_GEMMA3N_ALTUP, ACTIVATION_FLAG_RWKV7_V_FIRST, DEFAULT_LARGE_STATE_FRAME_BYTES,
    LARGE_STATE_FRAMING_CAPABILITY, LARGE_STATE_FRAMING_PROTOCOL_VERSION,
    LEGACY_STATE_IMPORT_MAX_BYTES, LLAMA_TOKEN_NULL, MAX_LARGE_STATE_FRAME_BYTES,
    MAX_LARGE_STATE_PAYLOAD_BYTES, MAX_STAGE_LOGIT_BIAS, READY_MAGIC, STAGE_LOGIT_BIAS_WIRE_BYTES,
    STAGE_SAMPLING_CONFIG_BASE_BYTES, STAGE_STATE_HEADER_BYTES, STAGE_STATE_VERSION,
    STAGE_WIRE_FIXED_HEADER_BYTES,
};

pub(crate) fn invalid_data(message: &'static str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message)
}

pub(crate) fn invalid_input(message: &'static str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidInput, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn ready_round_trips() {
        let mut bytes = Vec::new();
        send_ready(&mut bytes).unwrap();
        recv_ready(Cursor::new(bytes)).unwrap();
    }

    #[test]
    fn reply_round_trips() {
        let mut bytes = Vec::new();
        send_reply_predicted(&mut bytes, 42).unwrap();
        let reply = recv_reply(Cursor::new(bytes)).unwrap();
        assert_eq!(reply.kind, WireReplyKind::PredictedToken);
        assert_eq!(reply.predicted, 42);
        assert_eq!(reply.predicted_tokens, vec![42]);
    }

    #[test]
    fn token_vector_reply_round_trips() {
        let mut bytes = Vec::new();
        send_reply_predicted_tokens_with_stats(&mut bytes, &[1, 2, 3], StageReplyStats::default())
            .unwrap();
        let reply = recv_reply(Cursor::new(bytes)).unwrap();
        assert_eq!(reply.kind, WireReplyKind::PredictedTokens);
        assert_eq!(reply.predicted, 1);
        assert_eq!(reply.predicted_tokens, vec![1, 2, 3]);
    }

    #[test]
    fn stage_message_round_trips_f32() {
        let mut state =
            StageStateHeader::new(WireMessageKind::DecodeEmbd, WireActivationDType::F32);
        state.prompt_token_count = 1;
        state.decode_step = 0;
        state.current_token = 11;
        state.source_stage_index = 0;
        let activation = vec![1, 2, 3, 4, 5, 6, 7, 8];
        let message = StageWireMessage {
            kind: WireMessageKind::DecodeEmbd,
            pos_start: 1,
            token_count: 1,
            state,
            request_id: 7,
            session_id: 11,
            sampling: Some(StageSamplingConfig {
                flags: 1,
                seed: 42,
                temperature: 0.8,
                top_p: 0.9,
                top_k: 40,
                logit_bias: vec![StageLogitBias {
                    token_id: 123,
                    bias: -50.0,
                }],
                ..StageSamplingConfig::default()
            }),
            chat_sampling_metadata: None,
            tokens: vec![11],
            positions: Vec::new(),
            activation: activation.clone(),
            raw_bytes: Vec::new(),
        };
        let mut bytes = Vec::new();
        write_stage_message(&mut bytes, &message, WireActivationDType::F32).unwrap();
        let decoded = read_stage_message(Cursor::new(bytes), 2).unwrap();
        assert_eq!(decoded.kind, WireMessageKind::DecodeEmbd);
        assert_eq!(decoded.tokens, vec![11]);
        assert_eq!(decoded.activation, activation);
        assert_eq!(decoded.state.source_stage_index, 0);
        assert_eq!(decoded.request_id, 7);
        assert_eq!(decoded.session_id, 11);
        assert_ne!(decoded.state.flags & state_flags::SAMPLING, 0);
        assert_eq!(decoded.state.flags & state_flags::CHAT_SAMPLING_METADATA, 0);
        assert_eq!(decoded.chat_sampling_metadata, None);
        let sampling = decoded.sampling.expect("sampling extension round-tripped");
        assert_eq!(sampling.seed, 42);
        assert_eq!(sampling.top_k, 40);
        assert_eq!(sampling.logit_bias.len(), 1);
        assert_eq!(sampling.logit_bias[0].token_id, 123);
        assert_eq!(sampling.logit_bias[0].bias, -50.0);
    }

    #[test]
    fn generation_config_round_trips_sampling_metadata() {
        let message = StageWireMessage::configure_generation(
            WireActivationDType::F32,
            7,
            11,
            123,
            Some(StageSamplingConfig {
                flags: 1,
                seed: 42,
                temperature: 0.8,
                top_p: 0.9,
                top_k: 40,
                ..StageSamplingConfig::default()
            }),
            Some("{\"grammar\":\"root ::= \\\"x\\\"\"}".to_string()),
        );
        let mut bytes = Vec::new();
        write_stage_message(&mut bytes, &message, WireActivationDType::F32).unwrap();
        let decoded = read_stage_message(Cursor::new(bytes), 2).unwrap();
        assert_eq!(decoded.kind, WireMessageKind::ConfigureGeneration);
        assert_eq!(decoded.token_count, 0);
        assert_eq!(decoded.tokens, Vec::<i32>::new());
        assert_eq!(decoded.activation, Vec::<u8>::new());
        assert_eq!(decoded.request_id, 7);
        assert_eq!(decoded.session_id, 11);
        assert_eq!(decoded.state.prompt_token_count, 123);
        assert_ne!(decoded.state.flags & state_flags::SAMPLING, 0);
        assert_ne!(decoded.state.flags & state_flags::CHAT_SAMPLING_METADATA, 0);
        assert_eq!(
            decoded.chat_sampling_metadata.as_deref(),
            Some("{\"grammar\":\"root ::= \\\"x\\\"\"}")
        );
        let sampling = decoded.sampling.expect("sampling extension round-tripped");
        assert_eq!(sampling.seed, 42);
        assert_eq!(sampling.top_k, 40);
    }

    #[test]
    fn driver_origin_message_round_trips_without_activation() {
        let mut state =
            StageStateHeader::new(WireMessageKind::PrefillEmbd, WireActivationDType::F32);
        state.prompt_token_count = 2;
        state.current_token = 22;
        state.source_stage_index = -1;
        let message = StageWireMessage {
            kind: WireMessageKind::PrefillEmbd,
            pos_start: 0,
            token_count: 2,
            state,
            request_id: 13,
            session_id: 17,
            sampling: None,
            chat_sampling_metadata: None,
            tokens: vec![11, 22],
            positions: Vec::new(),
            activation: Vec::new(),
            raw_bytes: Vec::new(),
        };
        let mut bytes = Vec::new();
        write_stage_message(&mut bytes, &message, WireActivationDType::F32).unwrap();
        let decoded = read_stage_message(Cursor::new(bytes), 2048).unwrap();
        assert_eq!(decoded.tokens, vec![11, 22]);
        assert!(decoded.activation.is_empty());
        assert_eq!(decoded.state.source_stage_index, -1);
        assert_eq!(decoded.request_id, 13);
        assert_eq!(decoded.session_id, 17);
        assert_eq!(decoded.state.flags & state_flags::SAMPLING, 0);
        assert!(decoded.sampling.is_none());
    }

    #[test]
    fn prefill_wire_overhead_is_fixed_and_bounded() {
        let mut state =
            StageStateHeader::new(WireMessageKind::PrefillEmbd, WireActivationDType::F32);
        state.prompt_token_count = 128;
        state.current_token = 127;
        state.source_stage_index = -1;
        let tokens: Vec<i32> = (0..128).collect();
        let message = StageWireMessage {
            kind: WireMessageKind::PrefillEmbd,
            pos_start: 0,
            token_count: tokens.len() as i32,
            state,
            request_id: u64::MAX - 1,
            session_id: u64::MAX,
            sampling: None,
            chat_sampling_metadata: None,
            tokens,
            positions: Vec::new(),
            activation: Vec::new(),
            raw_bytes: Vec::new(),
        };
        let mut bytes = Vec::new();
        write_stage_message(&mut bytes, &message, WireActivationDType::F32).unwrap();

        assert_eq!(STAGE_STATE_HEADER_BYTES, 40);
        assert_eq!(STAGE_SAMPLING_CONFIG_BASE_BYTES, 40);
        assert_eq!(STAGE_WIRE_FIXED_HEADER_BYTES, 76);
        assert_eq!(
            bytes.len(),
            STAGE_WIRE_FIXED_HEADER_BYTES + message.tokens.len() * 4
        );
        const { assert!(STAGE_WIRE_FIXED_HEADER_BYTES <= 80) };
    }

    #[test]
    fn session_control_messages_are_fixed_header_only() {
        for kind in [
            WireMessageKind::CheckpointSession,
            WireMessageKind::RestoreSession,
            WireMessageKind::TrimSession,
        ] {
            let message = StageWireMessage {
                kind,
                pos_start: 0,
                token_count: 0,
                state: StageStateHeader::new(kind, WireActivationDType::F32),
                request_id: 23,
                session_id: 29,
                sampling: None,
                chat_sampling_metadata: None,
                tokens: Vec::new(),
                positions: Vec::new(),
                activation: Vec::new(),
                raw_bytes: Vec::new(),
            };
            let mut bytes = Vec::new();
            write_stage_message(&mut bytes, &message, WireActivationDType::F32).unwrap();
            assert_eq!(bytes.len(), STAGE_WIRE_FIXED_HEADER_BYTES);
            let decoded = read_stage_message(Cursor::new(bytes), 2048).unwrap();
            assert_eq!(decoded.kind, kind);
            assert_eq!(decoded.request_id, 23);
            assert_eq!(decoded.session_id, 29);
            assert!(decoded.tokens.is_empty());
            assert!(decoded.activation.is_empty());
        }
    }

    #[test]
    fn state_import_message_round_trips_raw_bytes() {
        let state = StageStateHeader::new(WireMessageKind::StateImport, WireActivationDType::F32);
        let message = StageWireMessage {
            kind: WireMessageKind::StateImport,
            pos_start: 0,
            token_count: 4,
            state,
            request_id: 31,
            session_id: 37,
            sampling: None,
            chat_sampling_metadata: None,
            tokens: Vec::new(),
            positions: Vec::new(),
            activation: Vec::new(),
            raw_bytes: vec![1, 2, 3, 4],
        };
        let mut bytes = Vec::new();
        write_stage_message(&mut bytes, &message, WireActivationDType::F32).unwrap();
        let decoded = read_stage_message(Cursor::new(bytes), 2048).unwrap();
        assert_eq!(decoded.kind, WireMessageKind::StateImport);
        assert_eq!(decoded.raw_bytes, vec![1, 2, 3, 4]);
        assert!(decoded.tokens.is_empty());
        assert!(decoded.activation.is_empty());
    }

    #[test]
    fn timed_state_import_message_records_raw_payload_timing() {
        let state = StageStateHeader::new(WireMessageKind::StateImport, WireActivationDType::F32);
        let message = StageWireMessage {
            kind: WireMessageKind::StateImport,
            pos_start: 0,
            token_count: 4,
            state,
            request_id: 31,
            session_id: 37,
            sampling: None,
            chat_sampling_metadata: None,
            tokens: Vec::new(),
            positions: Vec::new(),
            activation: Vec::new(),
            raw_bytes: vec![1, 2, 3, 4],
        };
        let mut bytes = Vec::new();
        let write_timing =
            write_stage_message_timed(&mut bytes, &message, WireActivationDType::F32).unwrap();
        assert_eq!(bytes.len(), STAGE_WIRE_FIXED_HEADER_BYTES + 4);
        assert!(write_timing.total_ms >= write_timing.raw_payload_ms);

        let (decoded, read_timing) = read_stage_message_timed(Cursor::new(bytes), 2048).unwrap();
        assert_eq!(decoded.kind, WireMessageKind::StateImport);
        assert_eq!(decoded.raw_bytes, vec![1, 2, 3, 4]);
        assert!(read_timing.total_ms >= read_timing.raw_payload_ms);
        assert!(read_timing.total_ms >= read_timing.header_ms);
    }

    #[test]
    fn state_import_framing_kind_preserves_legacy_limit() {
        assert_eq!(
            state_import_framing_kind_for_len(
                LEGACY_STATE_IMPORT_MAX_BYTES - 1,
                LargeStateFramingOptions::disabled()
            )
            .unwrap(),
            StateImportFramingKind::Legacy
        );
        assert!(state_import_framing_kind_for_len(
            LEGACY_STATE_IMPORT_MAX_BYTES + 1,
            LargeStateFramingOptions::disabled()
        )
        .is_err());
        assert_eq!(
            state_import_framing_kind_for_len(
                LEGACY_STATE_IMPORT_MAX_BYTES + 1,
                LargeStateFramingOptions::enabled()
            )
            .unwrap(),
            StateImportFramingKind::Large
        );
    }

    #[test]
    fn large_state_import_frames_round_trip_over_configured_threshold() {
        let mut state =
            StageStateHeader::new(WireMessageKind::StateImport, WireActivationDType::F16);
        state.flags |= state_flags::LARGE_STATE_FRAMING;
        let message = StageWireMessage {
            kind: WireMessageKind::StateImport,
            pos_start: 0,
            token_count: 0,
            state,
            request_id: 31,
            session_id: 37,
            sampling: None,
            chat_sampling_metadata: None,
            tokens: Vec::new(),
            positions: Vec::new(),
            activation: Vec::new(),
            raw_bytes: vec![1, 2, 3, 4, 5],
        };
        let options = LargeStateFramingOptions::enabled()
            .with_threshold_bytes(3)
            .with_max_frame_bytes(2);
        let mut bytes = Vec::new();
        let write_timing = write_state_import_message_timed(
            &mut bytes,
            &message,
            WireActivationDType::F16,
            options,
        )
        .unwrap();
        assert_eq!(i32::from_le_bytes(bytes[0..4].try_into().unwrap()), 20);
        assert_eq!(write_timing.large_state_frame_count, 3);
        assert_eq!(write_timing.large_state_frame_bytes, 2);
        assert_eq!(write_timing.large_state_payload_bytes, 5);

        let (decoded, read_timing) = read_stage_message_timed(Cursor::new(bytes), 2048).unwrap();
        assert_eq!(decoded.kind, WireMessageKind::StateImport);
        assert_eq!(decoded.raw_bytes, vec![1, 2, 3, 4, 5]);
        assert_eq!(decoded.request_id, 31);
        assert_eq!(decoded.session_id, 37);
        assert_eq!(decoded.state.dtype().unwrap(), WireActivationDType::F16);
        assert_eq!(read_timing.large_state_frame_count, 3);
        assert_eq!(read_timing.large_state_frame_bytes, 2);
        assert_eq!(read_timing.large_state_payload_bytes, 5);
    }

    #[test]
    fn large_state_import_requires_capability_flag() {
        let state = StageStateHeader::new(WireMessageKind::StateImport, WireActivationDType::F16);
        let message = StageWireMessage {
            kind: WireMessageKind::StateImport,
            pos_start: 0,
            token_count: 0,
            state,
            request_id: 31,
            session_id: 37,
            sampling: None,
            chat_sampling_metadata: None,
            tokens: Vec::new(),
            positions: Vec::new(),
            activation: Vec::new(),
            raw_bytes: vec![1, 2, 3, 4, 5],
        };
        let options = LargeStateFramingOptions::enabled()
            .with_threshold_bytes(3)
            .with_max_frame_bytes(2);
        let mut bytes = Vec::new();
        let error = write_state_import_message_timed(
            &mut bytes,
            &message,
            WireActivationDType::F16,
            options,
        )
        .unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    }

    #[test]
    fn truncated_large_state_import_fails_closed() {
        let bytes = large_state_import_test_bytes();
        let truncated = &bytes[..bytes.len() - 1];
        assert!(read_stage_message(Cursor::new(truncated), 2048).is_err());
    }

    #[test]
    fn checksum_mismatch_large_state_import_fails_closed() {
        let mut bytes = large_state_import_test_bytes();
        let first_payload = first_large_state_payload_offset();
        bytes[first_payload] ^= 0xff;
        let error = read_stage_message(Cursor::new(bytes), 2048).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    fn out_of_order_large_state_import_fails_closed() {
        let mut bytes = large_state_import_test_bytes();
        let first_frame_index = first_large_state_frame_index_offset();
        bytes[first_frame_index..first_frame_index + 8].copy_from_slice(&1_u64.to_le_bytes());
        let error = read_stage_message(Cursor::new(bytes), 2048).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    }

    fn large_state_import_test_bytes() -> Vec<u8> {
        let mut state =
            StageStateHeader::new(WireMessageKind::StateImport, WireActivationDType::F32);
        state.flags |= state_flags::LARGE_STATE_FRAMING;
        let message = StageWireMessage {
            kind: WireMessageKind::StateImport,
            pos_start: 0,
            token_count: 0,
            state,
            request_id: 31,
            session_id: 37,
            sampling: None,
            chat_sampling_metadata: None,
            tokens: Vec::new(),
            positions: Vec::new(),
            activation: Vec::new(),
            raw_bytes: vec![1, 2, 3, 4, 5],
        };
        let options = LargeStateFramingOptions::enabled()
            .with_threshold_bytes(3)
            .with_max_frame_bytes(2);
        let mut bytes = Vec::new();
        write_state_import_message_timed(&mut bytes, &message, WireActivationDType::F32, options)
            .unwrap();
        bytes
    }

    fn first_large_state_frame_index_offset() -> usize {
        let start_metadata = 4 + 8 + 8 + 8 + 4 + 32;
        STAGE_WIRE_FIXED_HEADER_BYTES + start_metadata + STAGE_WIRE_FIXED_HEADER_BYTES + 4
    }

    fn first_large_state_payload_offset() -> usize {
        first_large_state_frame_index_offset() + 8 + 8 + 8
    }

    #[test]
    fn state_export_message_round_trips_without_payload() {
        let state = StageStateHeader::new(WireMessageKind::StateExport, WireActivationDType::F32);
        let message = StageWireMessage {
            kind: WireMessageKind::StateExport,
            pos_start: 0,
            token_count: 0,
            state,
            request_id: 41,
            session_id: 43,
            sampling: None,
            chat_sampling_metadata: None,
            tokens: Vec::new(),
            positions: Vec::new(),
            activation: Vec::new(),
            raw_bytes: Vec::new(),
        };
        let mut bytes = Vec::new();
        write_stage_message(&mut bytes, &message, WireActivationDType::F32).unwrap();
        let decoded = read_stage_message(Cursor::new(bytes), 2048).unwrap();
        assert_eq!(decoded.kind, WireMessageKind::StateExport);
        assert!(decoded.raw_bytes.is_empty());
        assert!(decoded.tokens.is_empty());
        assert!(decoded.activation.is_empty());
    }

    #[test]
    fn q8_payload_decodes_to_f32_bytes() {
        let mut payload = Vec::new();
        payload.extend_from_slice(&0.5_f32.to_le_bytes());
        payload.extend_from_slice(&[2_u8, 254_u8]);
        let decoded = activation::decode_q8_to_f32_bytes(&payload, 1, 2).unwrap();
        let first = f32::from_le_bytes(decoded[0..4].try_into().unwrap());
        let second = f32::from_le_bytes(decoded[4..8].try_into().unwrap());
        assert_eq!(first, 1.0);
        assert_eq!(second, -1.0);
    }

    #[test]
    fn f32_payload_encodes_to_q8_and_decodes() {
        let mut input = Vec::new();
        input.extend_from_slice(&1.0_f32.to_le_bytes());
        input.extend_from_slice(&(-1.0_f32).to_le_bytes());
        let encoded = encode_f32_activation_payload(WireActivationDType::Q8, 1, 2, &input).unwrap();
        let decoded = activation::decode_q8_to_f32_bytes(&encoded, 1, 2).unwrap();
        let first = f32::from_le_bytes(decoded[0..4].try_into().unwrap());
        let second = f32::from_le_bytes(decoded[4..8].try_into().unwrap());
        assert!((first - 1.0).abs() < 0.01);
        assert!((second + 1.0).abs() < 0.01);
    }

    #[test]
    fn rwkv7_sideband_activation_round_trips() {
        let mut state =
            StageStateHeader::new(WireMessageKind::DecodeEmbd, WireActivationDType::F32);
        state.source_stage_index = 0;
        state.flags |= state_flags::RWKV7_V_FIRST_SIDEBAND;
        let mut activation = Vec::new();
        for value in [1.0_f32, 2.0, 3.0, 4.0] {
            activation.extend_from_slice(&value.to_le_bytes());
        }
        let message = StageWireMessage {
            kind: WireMessageKind::DecodeEmbd,
            pos_start: 0,
            token_count: 1,
            state,
            request_id: 7,
            session_id: 9,
            sampling: None,
            chat_sampling_metadata: None,
            tokens: vec![42],
            positions: Vec::new(),
            activation,
            raw_bytes: Vec::new(),
        };
        let mut bytes = Vec::new();
        write_stage_message(&mut bytes, &message, WireActivationDType::F32).unwrap();
        let decoded = read_stage_message(Cursor::new(bytes), 2).unwrap();
        assert_eq!(decoded.activation.len(), 16);
        assert_eq!(
            activation_frame_flags_from_state_flags(decoded.state.flags),
            ACTIVATION_FLAG_RWKV7_V_FIRST
        );
        assert_eq!(
            decoded.activation_f32_payload(2).unwrap(),
            message.activation
        );
    }

    #[test]
    fn gemma3n_altup_sideband_activation_round_trips() {
        let mut state =
            StageStateHeader::new(WireMessageKind::DecodeEmbd, WireActivationDType::F32);
        state.source_stage_index = 0;
        state.flags |= state_flags::GEMMA3N_ALTUP_SIDEBAND;
        let mut activation = Vec::new();
        for value in 0..8 {
            activation.extend_from_slice(&(value as f32).to_le_bytes());
        }
        let message = StageWireMessage {
            kind: WireMessageKind::DecodeEmbd,
            pos_start: 0,
            token_count: 1,
            state,
            request_id: 7,
            session_id: 9,
            sampling: None,
            chat_sampling_metadata: None,
            tokens: vec![42],
            positions: Vec::new(),
            activation,
            raw_bytes: Vec::new(),
        };
        let mut bytes = Vec::new();
        write_stage_message(&mut bytes, &message, WireActivationDType::F32).unwrap();
        let decoded = read_stage_message(Cursor::new(bytes), 2).unwrap();
        assert_eq!(decoded.activation.len(), 32);
        assert_eq!(
            activation_frame_flags_from_state_flags(decoded.state.flags),
            ACTIVATION_FLAG_GEMMA3N_ALTUP
        );
        assert_eq!(
            decoded.activation_f32_payload(2).unwrap(),
            message.activation
        );
    }
}
