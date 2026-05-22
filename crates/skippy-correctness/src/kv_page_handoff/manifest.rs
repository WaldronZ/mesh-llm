use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use skippy_runtime::{RuntimeKvPageCacheKind, RuntimeKvPageDesc, RuntimeKvPageSegmentKind};

use super::{
    NegativeCheckReport, CHECKSUM_ALGORITHM, PROTOCOL_VERSION, SOURCE_CAPABILITY, TARGET_CAPABILITY,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct KvPageIdentity {
    pub(super) artifact_sha256: String,
    pub(super) tokenizer_hash: String,
    pub(super) chat_template_hash: String,
    pub(super) dtype: String,
    pub(super) layout: String,
    pub(super) source_capability: String,
    pub(super) target_capability: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(super) struct KvPageManifest {
    pub(super) protocol_version: String,
    pub(super) page_index: usize,
    pub(super) total_pages: usize,
    pub(super) cache_kind: String,
    pub(super) segment_kind: String,
    pub(super) segment_empty: bool,
    pub(super) token_start: usize,
    pub(super) token_end: usize,
    pub(super) layer_start: usize,
    pub(super) layer_end: usize,
    pub(super) dtype: String,
    pub(super) layout: String,
    pub(super) payload_bytes: u64,
    pub(super) checksum_algorithm: String,
    pub(super) checksum: String,
    pub(super) artifact_sha256: String,
    pub(super) tokenizer_hash: String,
    pub(super) chat_template_hash: String,
    pub(super) source_capability: String,
    pub(super) target_capability: String,
    pub(super) native_desc: Option<KvPageNativeDesc>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(super) struct KvPageNativeDesc {
    pub(super) version: u32,
    pub(super) layer_start: i32,
    pub(super) layer_end: i32,
    pub(super) token_start: u64,
    pub(super) token_count: u64,
    pub(super) layer_count: u32,
    pub(super) k_type: u32,
    pub(super) v_type: u32,
    pub(super) k_row_bytes: u32,
    pub(super) v_row_bytes: u32,
    pub(super) v_element_bytes: u32,
    pub(super) payload_bytes: u64,
    pub(super) flags: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum KvPagePayloadKind {
    KvPage,
    FullStateBlob,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct KvPageRecord {
    pub(super) manifest: KvPageManifest,
    pub(super) payload: Vec<u8>,
    pub(super) payload_kind: KvPagePayloadKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct KvPageValidation {
    pub(super) final_decode_start_position: usize,
    pub(super) page_count: usize,
    pub(super) total_page_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct KvPageValidationError {
    pub(super) reason: &'static str,
}

pub(super) fn manifest_from_runtime_page(
    page_index: usize,
    total_pages: usize,
    token_start: usize,
    token_end: usize,
    identity: &KvPageIdentity,
    desc: &RuntimeKvPageDesc,
    payload: &[u8],
) -> KvPageManifest {
    KvPageManifest {
        protocol_version: PROTOCOL_VERSION.to_string(),
        page_index,
        total_pages,
        cache_kind: desc.cache_kind().as_label().to_string(),
        segment_kind: desc.segment_kind().as_label().to_string(),
        segment_empty: desc.segment_empty(),
        token_start,
        token_end,
        layer_start: usize::try_from(desc.layer_start.max(0)).unwrap_or_default(),
        layer_end: usize::try_from(desc.layer_end.max(0)).unwrap_or_default(),
        dtype: identity.dtype.clone(),
        layout: identity.layout.clone(),
        payload_bytes: payload.len() as u64,
        checksum_algorithm: CHECKSUM_ALGORITHM.to_string(),
        checksum: sha256_hex(payload),
        artifact_sha256: identity.artifact_sha256.clone(),
        tokenizer_hash: identity.tokenizer_hash.clone(),
        chat_template_hash: identity.chat_template_hash.clone(),
        source_capability: identity.source_capability.clone(),
        target_capability: identity.target_capability.clone(),
        native_desc: Some(KvPageNativeDesc {
            version: desc.version,
            layer_start: desc.layer_start,
            layer_end: desc.layer_end,
            token_start: desc.token_start,
            token_count: desc.token_count,
            layer_count: desc.layer_count,
            k_type: desc.k_type,
            v_type: desc.v_type,
            k_row_bytes: desc.k_row_bytes,
            v_row_bytes: desc.v_row_bytes,
            v_element_bytes: desc.v_element_bytes,
            payload_bytes: desc.payload_bytes,
            flags: desc.flags,
        }),
    }
}

pub(super) fn runtime_desc_from_manifest(manifest: &KvPageManifest) -> Option<RuntimeKvPageDesc> {
    let desc = manifest.native_desc.as_ref()?;
    Some(RuntimeKvPageDesc {
        version: desc.version,
        layer_start: desc.layer_start,
        layer_end: desc.layer_end,
        token_start: desc.token_start,
        token_count: desc.token_count,
        layer_count: desc.layer_count,
        k_type: desc.k_type,
        v_type: desc.v_type,
        k_row_bytes: desc.k_row_bytes,
        v_row_bytes: desc.v_row_bytes,
        v_element_bytes: desc.v_element_bytes,
        payload_bytes: desc.payload_bytes,
        flags: desc.flags,
    })
}

pub(super) fn negative_checks(
    records: &[KvPageRecord],
    identity: &KvPageIdentity,
) -> Vec<NegativeCheckReport> {
    let cases = [
        ("missing_page", mutate_missing_page(records)),
        ("duplicate_page", mutate_duplicate_page(records)),
        ("out_of_order_page", mutate_out_of_order(records)),
        ("position_gap", mutate_position_gap(records)),
        ("position_overlap", mutate_position_overlap(records)),
        ("checksum_mismatch", mutate_checksum(records)),
        ("dtype_mismatch", mutate_dtype(records)),
        ("layout_mismatch", mutate_layout(records)),
        ("artifact_mismatch", mutate_artifact(records)),
        ("tokenizer_mismatch", mutate_tokenizer(records)),
        ("chat_template_mismatch", mutate_chat_template(records)),
        ("cache_kind_mismatch", mutate_cache_kind(records)),
        ("segment_kind_mismatch", mutate_segment_kind(records)),
        ("full_state_blob", mutate_full_state_blob(records)),
    ];

    cases
        .into_iter()
        .map(
            |(case, mutated)| match validate_kv_page_handoff(&mutated, identity) {
                Ok(_) => NegativeCheckReport {
                    case,
                    status: "fail",
                    failure_reason: "unexpected_pass",
                },
                Err(error) => NegativeCheckReport {
                    case,
                    status: "pass",
                    failure_reason: error.reason,
                },
            },
        )
        .collect()
}

pub(super) fn expected_identity() -> KvPageIdentity {
    KvPageIdentity {
        artifact_sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            .to_string(),
        tokenizer_hash: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            .to_string(),
        chat_template_hash: "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
            .to_string(),
        dtype: "f16".to_string(),
        layout: "llama.cpp-kv-page".to_string(),
        source_capability: SOURCE_CAPABILITY.to_string(),
        target_capability: TARGET_CAPABILITY.to_string(),
    }
}

pub(super) fn synthetic_records(
    total_tokens: usize,
    chunk_tokens: usize,
    identity: &KvPageIdentity,
) -> Vec<KvPageRecord> {
    let chunk_tokens = chunk_tokens.max(1);
    let mut records = Vec::new();
    let mut token_start = 0usize;
    while token_start < total_tokens {
        let token_count = (total_tokens - token_start).min(chunk_tokens);
        let token_end = token_start + token_count;
        let page_index = records.len();
        let payload = format!("synthetic-page-{page_index}").into_bytes();
        records.push(KvPageRecord {
            manifest: KvPageManifest {
                protocol_version: PROTOCOL_VERSION.to_string(),
                page_index,
                total_pages: 0,
                cache_kind: RuntimeKvPageCacheKind::Regular.as_label().to_string(),
                segment_kind: RuntimeKvPageSegmentKind::Regular.as_label().to_string(),
                segment_empty: false,
                token_start,
                token_end,
                layer_start: 0,
                layer_end: 4,
                dtype: identity.dtype.clone(),
                layout: identity.layout.clone(),
                payload_bytes: payload.len() as u64,
                checksum_algorithm: CHECKSUM_ALGORITHM.to_string(),
                checksum: sha256_hex(&payload),
                artifact_sha256: identity.artifact_sha256.clone(),
                tokenizer_hash: identity.tokenizer_hash.clone(),
                chat_template_hash: identity.chat_template_hash.clone(),
                source_capability: identity.source_capability.clone(),
                target_capability: identity.target_capability.clone(),
                native_desc: None,
            },
            payload,
            payload_kind: KvPagePayloadKind::KvPage,
        });
        token_start = token_end;
    }
    let total_pages = records.len();
    for record in &mut records {
        record.manifest.total_pages = total_pages;
    }
    records
}

#[cfg(test)]
pub(super) fn synthetic_iswa_records(
    total_tokens: usize,
    chunk_tokens: usize,
    identity: &KvPageIdentity,
) -> Vec<KvPageRecord> {
    let chunk_tokens = chunk_tokens.max(1);
    let mut records = Vec::new();
    let mut token_start = 0usize;
    while token_start < total_tokens {
        let token_count = (total_tokens - token_start).min(chunk_tokens);
        let token_end = token_start + token_count;
        for segment_kind in [
            RuntimeKvPageSegmentKind::Base,
            RuntimeKvPageSegmentKind::Swa,
        ] {
            let page_index = records.len();
            let payload =
                format!("synthetic-iswa-{}-{page_index}", segment_kind.as_label()).into_bytes();
            records.push(KvPageRecord {
                manifest: KvPageManifest {
                    protocol_version: PROTOCOL_VERSION.to_string(),
                    page_index,
                    total_pages: 0,
                    cache_kind: RuntimeKvPageCacheKind::Iswa.as_label().to_string(),
                    segment_kind: segment_kind.as_label().to_string(),
                    segment_empty: false,
                    token_start,
                    token_end,
                    layer_start: 0,
                    layer_end: 4,
                    dtype: identity.dtype.clone(),
                    layout: identity.layout.clone(),
                    payload_bytes: payload.len() as u64,
                    checksum_algorithm: CHECKSUM_ALGORITHM.to_string(),
                    checksum: sha256_hex(&payload),
                    artifact_sha256: identity.artifact_sha256.clone(),
                    tokenizer_hash: identity.tokenizer_hash.clone(),
                    chat_template_hash: identity.chat_template_hash.clone(),
                    source_capability: identity.source_capability.clone(),
                    target_capability: identity.target_capability.clone(),
                    native_desc: None,
                },
                payload,
                payload_kind: KvPagePayloadKind::KvPage,
            });
        }
        token_start = token_end;
    }
    let total_pages = records.len();
    for record in &mut records {
        record.manifest.total_pages = total_pages;
    }
    records
}

pub(super) fn validate_kv_page_handoff(
    records: &[KvPageRecord],
    identity: &KvPageIdentity,
) -> Result<KvPageValidation, KvPageValidationError> {
    let first = records
        .first()
        .ok_or(KvPageValidationError { reason: "no_pages" })?;
    let expected_total_pages = first.manifest.total_pages;
    if expected_total_pages == 0 || records.len() != expected_total_pages {
        return Err(KvPageValidationError {
            reason: "missing_page",
        });
    }

    let mut seen = BTreeSet::new();
    let mut expected_index = 0usize;
    let mut expected_token_start = 0usize;
    let mut total_page_bytes = 0u64;
    let cache_kind = first.manifest.cache_kind.as_str();
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
        if manifest.protocol_version != PROTOCOL_VERSION {
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
        if manifest.checksum_algorithm != CHECKSUM_ALGORITHM {
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
        if manifest.source_capability != identity.source_capability {
            return Err(KvPageValidationError {
                reason: "source_capability",
            });
        }
        if manifest.target_capability != identity.target_capability {
            return Err(KvPageValidationError {
                reason: "target_capability",
            });
        }

        total_page_bytes = total_page_bytes.saturating_add(manifest.payload_bytes);
        if is_iswa {
            if pending_group_start.is_none() {
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
                pending_group_start = Some(manifest.token_start);
                pending_group_end = Some(manifest.token_end);
            } else if pending_group_start != Some(manifest.token_start)
                || pending_group_end != Some(manifest.token_end)
            {
                return Err(KvPageValidationError {
                    reason: "iswa_segment_range",
                });
            }
            match manifest.segment_kind.as_str() {
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

pub(super) fn mutate_full_state_blob(records: &[KvPageRecord]) -> Vec<KvPageRecord> {
    let mut records = records.to_vec();
    if let Some(record) = records.first_mut() {
        record.payload_kind = KvPagePayloadKind::FullStateBlob;
    }
    records
}

fn mutate_missing_page(records: &[KvPageRecord]) -> Vec<KvPageRecord> {
    records
        .iter()
        .take(records.len().saturating_sub(1))
        .cloned()
        .collect()
}

fn mutate_duplicate_page(records: &[KvPageRecord]) -> Vec<KvPageRecord> {
    let mut records = records.to_vec();
    if records.len() > 1 {
        records[1].manifest.page_index = records[0].manifest.page_index;
    }
    records
}

fn mutate_out_of_order(records: &[KvPageRecord]) -> Vec<KvPageRecord> {
    let mut records = records.to_vec();
    if records.len() > 1 {
        records.swap(0, 1);
    }
    records
}

fn mutate_position_gap(records: &[KvPageRecord]) -> Vec<KvPageRecord> {
    let mut records = records.to_vec();
    if let Some(index) = first_next_token_range_index(&records) {
        records[index].manifest.token_start += 1;
    }
    records
}

fn mutate_position_overlap(records: &[KvPageRecord]) -> Vec<KvPageRecord> {
    let mut records = records.to_vec();
    if let Some(index) = first_next_token_range_index(&records) {
        records[index].manifest.token_start = records[index].manifest.token_start.saturating_sub(1);
    }
    records
}

fn first_next_token_range_index(records: &[KvPageRecord]) -> Option<usize> {
    let first_start = records.first()?.manifest.token_start;
    records
        .iter()
        .position(|record| record.manifest.token_start > first_start)
}

fn mutate_checksum(records: &[KvPageRecord]) -> Vec<KvPageRecord> {
    let mut records = records.to_vec();
    if let Some(record) = records.first_mut() {
        record.manifest.checksum = "bad-checksum".to_string();
    }
    records
}

fn mutate_dtype(records: &[KvPageRecord]) -> Vec<KvPageRecord> {
    let mut records = records.to_vec();
    if let Some(record) = records.first_mut() {
        record.manifest.dtype = "q8_0".to_string();
    }
    records
}

fn mutate_layout(records: &[KvPageRecord]) -> Vec<KvPageRecord> {
    let mut records = records.to_vec();
    if let Some(record) = records.first_mut() {
        record.manifest.layout = "other-layout".to_string();
    }
    records
}

fn mutate_artifact(records: &[KvPageRecord]) -> Vec<KvPageRecord> {
    let mut records = records.to_vec();
    if let Some(record) = records.first_mut() {
        record.manifest.artifact_sha256 =
            "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd".to_string();
    }
    records
}

fn mutate_tokenizer(records: &[KvPageRecord]) -> Vec<KvPageRecord> {
    let mut records = records.to_vec();
    if let Some(record) = records.first_mut() {
        record.manifest.tokenizer_hash =
            "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee".to_string();
    }
    records
}

fn mutate_chat_template(records: &[KvPageRecord]) -> Vec<KvPageRecord> {
    let mut records = records.to_vec();
    if let Some(record) = records.first_mut() {
        record.manifest.chat_template_hash =
            "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff".to_string();
    }
    records
}

pub(super) fn mutate_cache_kind(records: &[KvPageRecord]) -> Vec<KvPageRecord> {
    let mut records = records.to_vec();
    if let Some(record) = records.first_mut() {
        record.manifest.cache_kind = "other-cache".to_string();
    }
    records
}

pub(super) fn mutate_segment_kind(records: &[KvPageRecord]) -> Vec<KvPageRecord> {
    let mut records = records.to_vec();
    if let Some(record) = records.first_mut() {
        record.manifest.segment_kind = "other-segment".to_string();
    }
    records
}

fn sha256_hex(payload: &[u8]) -> String {
    let digest = Sha256::digest(payload);
    format!("{digest:x}")
}
