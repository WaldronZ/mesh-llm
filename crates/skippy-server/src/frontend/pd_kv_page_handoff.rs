use std::collections::BTreeSet;

use serde::Serialize;
use sha2::{Digest, Sha256};
use skippy_runtime::{RuntimeKvPageCacheKind, RuntimeKvPageSegmentKind};

use super::pd_chunked_prefill::{
    PdChunkedPrefillConfig, PdChunkedPrefillPlan, PdPrefillChunkRange,
};

const KV_PAGE_PROTOCOL_VERSION: &str = "pd-kv-page/1";
const KV_PAGE_CHECKSUM_ALGORITHM: &str = "sha256";
const KV_PAGE_RECOMMENDATION_NEEDS_SMOKE: &str = "run_foreground_runtime_smoke";

#[derive(Clone, Debug, PartialEq, Eq)]
struct KvPageIdentity {
    artifact_sha256: &'static str,
    tokenizer_hash: &'static str,
    chat_template_hash: &'static str,
    dtype: &'static str,
    layout: &'static str,
    source_role: &'static str,
    target_role: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct KvPageManifest {
    protocol_version: &'static str,
    page_index: usize,
    total_pages: usize,
    cache_kind: &'static str,
    segment_kind: &'static str,
    segment_empty: bool,
    token_start: usize,
    token_end: usize,
    layer_start: usize,
    layer_end: usize,
    dtype: &'static str,
    layout: &'static str,
    payload_bytes: u64,
    checksum_algorithm: &'static str,
    checksum: String,
    artifact_sha256: &'static str,
    tokenizer_hash: &'static str,
    chat_template_hash: &'static str,
    source_role: &'static str,
    target_role: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum KvPagePayloadKind {
    KvPage,
    FullStateBlob,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct KvPageRecord {
    manifest: KvPageManifest,
    payload: Vec<u8>,
    payload_kind: KvPagePayloadKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct KvPageValidation {
    final_decode_start_position: usize,
    page_count: usize,
    total_page_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct KvPageValidationError {
    reason: &'static str,
}

#[derive(Debug, Serialize)]
struct KvPageHandoffReport {
    result: &'static str,
    recommendation: &'static str,
    page_count: usize,
    total_page_bytes: u64,
    page_export_ms: Vec<f64>,
    page_transfer_ms: Vec<f64>,
    page_import_ms: Vec<f64>,
    decode_ttft_after_import_ms: Option<f64>,
    validation_result: &'static str,
    failure_reason: Option<&'static str>,
}

fn expected_identity() -> KvPageIdentity {
    KvPageIdentity {
        artifact_sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        tokenizer_hash: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        chat_template_hash: "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
        dtype: "f16",
        layout: "llama.cpp-kv-page",
        source_role: "prefill-worker",
        target_role: "decode-worker",
    }
}

fn manifest_for_chunk(
    chunk: PdPrefillChunkRange,
    total_pages: usize,
    identity: &KvPageIdentity,
    payload: &[u8],
) -> KvPageManifest {
    KvPageManifest {
        protocol_version: KV_PAGE_PROTOCOL_VERSION,
        page_index: chunk.index,
        total_pages,
        cache_kind: RuntimeKvPageCacheKind::Regular.as_label(),
        segment_kind: RuntimeKvPageSegmentKind::Regular.as_label(),
        segment_empty: false,
        token_start: chunk.start_position,
        token_end: chunk.end_position,
        layer_start: 0,
        layer_end: 4,
        dtype: identity.dtype,
        layout: identity.layout,
        payload_bytes: payload.len() as u64,
        checksum_algorithm: KV_PAGE_CHECKSUM_ALGORITHM,
        checksum: sha256_hex(payload),
        artifact_sha256: identity.artifact_sha256,
        tokenizer_hash: identity.tokenizer_hash,
        chat_template_hash: identity.chat_template_hash,
        source_role: identity.source_role,
        target_role: identity.target_role,
    }
}

fn records_from_plan(plan: &PdChunkedPrefillPlan, identity: &KvPageIdentity) -> Vec<KvPageRecord> {
    plan.chunks
        .iter()
        .map(|chunk| {
            let payload = format!("synthetic-kv-page-{}", chunk.index).into_bytes();
            KvPageRecord {
                manifest: manifest_for_chunk(*chunk, plan.chunks.len(), identity, &payload),
                payload,
                payload_kind: KvPagePayloadKind::KvPage,
            }
        })
        .collect()
}

fn validate_kv_page_handoff(
    records: &[KvPageRecord],
    identity: &KvPageIdentity,
) -> Result<KvPageValidation, KvPageValidationError> {
    let first = records
        .first()
        .ok_or(KvPageValidationError { reason: "no_pages" })?;
    let expected_total_pages = first.manifest.total_pages;
    if expected_total_pages == 0 {
        return Err(KvPageValidationError {
            reason: "total_pages",
        });
    }
    if records.len() != expected_total_pages {
        return Err(KvPageValidationError {
            reason: "missing_page",
        });
    }

    let mut seen = BTreeSet::new();
    let mut expected_index = 0usize;
    let mut expected_token_start = 0usize;
    let mut total_page_bytes = 0u64;
    let cache_kind = first.manifest.cache_kind;
    if cache_kind != RuntimeKvPageCacheKind::Regular.as_label()
        && cache_kind != RuntimeKvPageCacheKind::Iswa.as_label()
    {
        return Err(KvPageValidationError {
            reason: "cache_kind",
        });
    }
    let is_iswa = cache_kind == RuntimeKvPageCacheKind::Iswa.as_label();
    let mut pending_group_start: Option<usize> = None;
    let mut pending_group_end: Option<usize> = None;
    let mut pending_base = false;
    let mut pending_swa = false;

    for record in records {
        if record.payload_kind != KvPagePayloadKind::KvPage {
            return Err(KvPageValidationError {
                reason: "full_state_blob",
            });
        }
        let manifest = &record.manifest;
        if manifest.protocol_version != KV_PAGE_PROTOCOL_VERSION {
            return Err(KvPageValidationError {
                reason: "protocol_version",
            });
        }
        if manifest.total_pages != expected_total_pages {
            return Err(KvPageValidationError {
                reason: "total_pages",
            });
        }
        if manifest.cache_kind != cache_kind {
            return Err(KvPageValidationError {
                reason: "cache_kind",
            });
        }
        if is_iswa {
            if manifest.segment_kind != RuntimeKvPageSegmentKind::Base.as_label()
                && manifest.segment_kind != RuntimeKvPageSegmentKind::Swa.as_label()
            {
                return Err(KvPageValidationError {
                    reason: "segment_kind",
                });
            }
        } else if manifest.segment_kind != RuntimeKvPageSegmentKind::Regular.as_label() {
            return Err(KvPageValidationError {
                reason: "segment_kind",
            });
        }
        if !seen.insert(manifest.page_index) {
            return Err(KvPageValidationError {
                reason: "duplicate_page",
            });
        }
        if manifest.page_index != expected_index {
            return Err(KvPageValidationError {
                reason: "out_of_order_page",
            });
        }
        if manifest.token_start > expected_token_start {
            return Err(KvPageValidationError {
                reason: "position_gap",
            });
        }
        if manifest.token_start < expected_token_start {
            return Err(KvPageValidationError {
                reason: "position_overlap",
            });
        }
        if manifest.token_end <= manifest.token_start {
            return Err(KvPageValidationError {
                reason: "token_range",
            });
        }
        if manifest.layer_end <= manifest.layer_start {
            return Err(KvPageValidationError {
                reason: "layer_range",
            });
        }
        if manifest.segment_empty {
            if manifest.payload_bytes != 0 || !record.payload.is_empty() {
                return Err(KvPageValidationError {
                    reason: "segment_empty",
                });
            }
        } else if record.payload.is_empty() {
            return Err(KvPageValidationError {
                reason: "payload_bytes",
            });
        }
        if manifest.payload_bytes != record.payload.len() as u64 {
            return Err(KvPageValidationError {
                reason: "payload_bytes",
            });
        }
        if manifest.checksum_algorithm != KV_PAGE_CHECKSUM_ALGORITHM {
            return Err(KvPageValidationError {
                reason: "checksum_algorithm",
            });
        }
        if manifest.checksum != sha256_hex(&record.payload) {
            return Err(KvPageValidationError { reason: "checksum" });
        }
        if manifest.dtype != identity.dtype {
            return Err(KvPageValidationError { reason: "dtype" });
        }
        if manifest.layout != identity.layout {
            return Err(KvPageValidationError { reason: "layout" });
        }
        if manifest.artifact_sha256 != identity.artifact_sha256 {
            return Err(KvPageValidationError {
                reason: "artifact_sha256",
            });
        }
        if manifest.tokenizer_hash != identity.tokenizer_hash {
            return Err(KvPageValidationError {
                reason: "tokenizer_hash",
            });
        }
        if manifest.chat_template_hash != identity.chat_template_hash {
            return Err(KvPageValidationError {
                reason: "chat_template_hash",
            });
        }
        if !is_sanitized_role_label(manifest.source_role)
            || !is_sanitized_role_label(manifest.target_role)
        {
            return Err(KvPageValidationError {
                reason: "role_label",
            });
        }

        total_page_bytes = total_page_bytes.saturating_add(manifest.payload_bytes);
        if is_iswa {
            if pending_group_start.is_none() {
                pending_group_start = Some(manifest.token_start);
                pending_group_end = Some(manifest.token_end);
            } else if pending_group_start != Some(manifest.token_start)
                || pending_group_end != Some(manifest.token_end)
            {
                return Err(KvPageValidationError {
                    reason: "iswa_segment_range",
                });
            }
            match manifest.segment_kind {
                "base" if !pending_base => pending_base = true,
                "swa" if !pending_swa => pending_swa = true,
                _ => {
                    return Err(KvPageValidationError {
                        reason: "duplicate_segment",
                    });
                }
            }
            if pending_base && pending_swa {
                expected_token_start = pending_group_end.unwrap_or(expected_token_start);
                pending_group_start = None;
                pending_group_end = None;
                pending_base = false;
                pending_swa = false;
            }
        } else {
            expected_token_start = manifest.token_end;
        }
        expected_index += 1;
    }
    if is_iswa && (pending_base || pending_swa || pending_group_start.is_some()) {
        return Err(KvPageValidationError {
            reason: "missing_segment",
        });
    }

    Ok(KvPageValidation {
        final_decode_start_position: expected_token_start,
        page_count: records.len(),
        total_page_bytes,
    })
}

fn report_for_validation(
    result: Result<KvPageValidation, KvPageValidationError>,
) -> KvPageHandoffReport {
    match result {
        Ok(validation) => KvPageHandoffReport {
            result: "inconclusive",
            recommendation: KV_PAGE_RECOMMENDATION_NEEDS_SMOKE,
            page_count: validation.page_count,
            total_page_bytes: validation.total_page_bytes,
            page_export_ms: vec![0.0; validation.page_count],
            page_transfer_ms: vec![0.0; validation.page_count],
            page_import_ms: vec![0.0; validation.page_count],
            decode_ttft_after_import_ms: None,
            validation_result: "pass",
            failure_reason: None,
        },
        Err(error) => KvPageHandoffReport {
            result: "fail",
            recommendation: "fix_manifest_or_runtime_spike",
            page_count: 0,
            total_page_bytes: 0,
            page_export_ms: Vec::new(),
            page_transfer_ms: Vec::new(),
            page_import_ms: Vec::new(),
            decode_ttft_after_import_ms: None,
            validation_result: "fail",
            failure_reason: Some(error.reason),
        },
    }
}

fn is_sanitized_role_label(label: &str) -> bool {
    !label.is_empty()
        && label.len() <= 64
        && label
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

fn sha256_hex(payload: &[u8]) -> String {
    let digest = Sha256::digest(payload);
    format!("{digest:x}")
}

fn two_page_records() -> Vec<KvPageRecord> {
    let plan = PdChunkedPrefillPlan::new(
        16,
        PdChunkedPrefillConfig::new(8, 8).expect("valid chunk config"),
    )
    .expect("valid two-page plan");
    records_from_plan(&plan, &expected_identity())
}

fn two_chunk_iswa_records() -> Vec<KvPageRecord> {
    let identity = expected_identity();
    let ranges = [(0, 8), (8, 16)];
    let mut records = Vec::new();
    for (token_start, token_end) in ranges {
        for segment_kind in [
            RuntimeKvPageSegmentKind::Base,
            RuntimeKvPageSegmentKind::Swa,
        ] {
            let page_index = records.len();
            let payload =
                format!("synthetic-iswa-{}-{page_index}", segment_kind.as_label()).into_bytes();
            records.push(KvPageRecord {
                manifest: KvPageManifest {
                    protocol_version: KV_PAGE_PROTOCOL_VERSION,
                    page_index,
                    total_pages: 4,
                    cache_kind: RuntimeKvPageCacheKind::Iswa.as_label(),
                    segment_kind: segment_kind.as_label(),
                    segment_empty: false,
                    token_start,
                    token_end,
                    layer_start: 0,
                    layer_end: 4,
                    dtype: identity.dtype,
                    layout: identity.layout,
                    payload_bytes: payload.len() as u64,
                    checksum_algorithm: KV_PAGE_CHECKSUM_ALGORITHM,
                    checksum: sha256_hex(&payload),
                    artifact_sha256: identity.artifact_sha256,
                    tokenizer_hash: identity.tokenizer_hash,
                    chat_template_hash: identity.chat_template_hash,
                    source_role: identity.source_role,
                    target_role: identity.target_role,
                },
                payload,
                payload_kind: KvPagePayloadKind::KvPage,
            });
        }
    }
    records
}

#[test]
fn positive_two_page_manifest_validation_passes() {
    let records = two_page_records();
    let validation = validate_kv_page_handoff(&records, &expected_identity()).unwrap();

    assert_eq!(validation.page_count, 2);
    assert_eq!(validation.final_decode_start_position, 16);
    assert_eq!(
        validation.total_page_bytes,
        records
            .iter()
            .map(|record| record.payload.len() as u64)
            .sum::<u64>()
    );
}

#[test]
fn missing_page_fails_closed() {
    let mut records = two_page_records();
    records.pop();

    assert_eq!(
        validate_kv_page_handoff(&records, &expected_identity())
            .unwrap_err()
            .reason,
        "missing_page"
    );
}

#[test]
fn duplicate_page_fails_closed() {
    let mut records = two_page_records();
    records[1].manifest.page_index = records[0].manifest.page_index;

    assert_eq!(
        validate_kv_page_handoff(&records, &expected_identity())
            .unwrap_err()
            .reason,
        "duplicate_page"
    );
}

#[test]
fn out_of_order_page_fails_closed() {
    let mut records = two_page_records();
    records.swap(0, 1);

    assert_eq!(
        validate_kv_page_handoff(&records, &expected_identity())
            .unwrap_err()
            .reason,
        "out_of_order_page"
    );
}

#[test]
fn position_gap_fails_closed() {
    let mut records = two_page_records();
    records[1].manifest.token_start += 1;

    assert_eq!(
        validate_kv_page_handoff(&records, &expected_identity())
            .unwrap_err()
            .reason,
        "position_gap"
    );
}

#[test]
fn position_overlap_fails_closed() {
    let mut records = two_page_records();
    records[1].manifest.token_start -= 1;

    assert_eq!(
        validate_kv_page_handoff(&records, &expected_identity())
            .unwrap_err()
            .reason,
        "position_overlap"
    );
}

#[test]
fn checksum_mismatch_fails_closed() {
    let mut records = two_page_records();
    records[1].manifest.checksum = "bad-checksum".to_string();

    assert_eq!(
        validate_kv_page_handoff(&records, &expected_identity())
            .unwrap_err()
            .reason,
        "checksum"
    );
}

#[test]
fn dtype_and_layout_mismatch_fail_closed() {
    let mut records = two_page_records();
    records[0].manifest.dtype = "q8_0";
    assert_eq!(
        validate_kv_page_handoff(&records, &expected_identity())
            .unwrap_err()
            .reason,
        "dtype"
    );

    let mut records = two_page_records();
    records[0].manifest.layout = "other-layout";
    assert_eq!(
        validate_kv_page_handoff(&records, &expected_identity())
            .unwrap_err()
            .reason,
        "layout"
    );
}

#[test]
fn identity_mismatch_fails_closed() {
    let mut records = two_page_records();
    records[0].manifest.artifact_sha256 =
        "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";
    assert_eq!(
        validate_kv_page_handoff(&records, &expected_identity())
            .unwrap_err()
            .reason,
        "artifact_sha256"
    );

    let mut records = two_page_records();
    records[0].manifest.tokenizer_hash =
        "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
    assert_eq!(
        validate_kv_page_handoff(&records, &expected_identity())
            .unwrap_err()
            .reason,
        "tokenizer_hash"
    );

    let mut records = two_page_records();
    records[0].manifest.chat_template_hash =
        "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";
    assert_eq!(
        validate_kv_page_handoff(&records, &expected_identity())
            .unwrap_err()
            .reason,
        "chat_template_hash"
    );
}

#[test]
fn full_state_blob_cannot_satisfy_page_handoff_proof() {
    let mut records = two_page_records();
    records[0].payload_kind = KvPagePayloadKind::FullStateBlob;

    assert_eq!(
        validate_kv_page_handoff(&records, &expected_identity())
            .unwrap_err()
            .reason,
        "full_state_blob"
    );
}

#[test]
fn iswa_records_require_base_and_swa_segments() {
    let records = two_chunk_iswa_records();
    let validation = validate_kv_page_handoff(&records, &expected_identity()).unwrap();

    assert_eq!(validation.page_count, 4);
    assert_eq!(validation.final_decode_start_position, 16);

    let mut missing_swa = records.clone();
    missing_swa.remove(1);
    let total_pages = missing_swa.len();
    for (index, record) in missing_swa.iter_mut().enumerate() {
        record.manifest.page_index = index;
        record.manifest.total_pages = total_pages;
    }
    assert_eq!(
        validate_kv_page_handoff(&missing_swa, &expected_identity())
            .unwrap_err()
            .reason,
        "position_gap"
    );

    let mut duplicate_base = records.clone();
    duplicate_base[1].manifest.segment_kind = RuntimeKvPageSegmentKind::Base.as_label();
    assert_eq!(
        validate_kv_page_handoff(&duplicate_base, &expected_identity())
            .unwrap_err()
            .reason,
        "duplicate_segment"
    );
}

#[test]
fn sanitized_report_contains_no_sensitive_payload_or_prompt_material() {
    let records = two_page_records();
    let report = report_for_validation(validate_kv_page_handoff(&records, &expected_identity()));
    let serialized = serde_json::to_string(&report).unwrap();

    assert!(serialized.contains("\"validation_result\":\"pass\""));
    assert!(serialized.contains(KV_PAGE_RECOMMENDATION_NEEDS_SMOKE));
    assert!(!serialized.contains("synthetic-kv-page"));
    assert!(!serialized.contains("prompt"));
    assert!(!serialized.contains("generated"));
    assert!(!serialized.contains("token array"));
    assert!(!serialized.contains("/Users/"));
    assert!(!serialized.contains("127.0.0.1"));
    assert!(!serialized.contains("credential"));
}
