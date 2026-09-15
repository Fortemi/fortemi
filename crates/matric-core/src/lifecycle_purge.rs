//! Content-free lifecycle purge contract.
//!
//! The selector is sensitive operational input. Public previews and receipts
//! expose counts and opaque operation identifiers, never target identifiers,
//! source keys, paths, payloads, or content-derived digests.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{Error, Result};

pub const LIFECYCLE_PURGE_CONTRACT_VERSION: &str = "1.0.0";
pub const LIFECYCLE_PURGE_MAX_NOTE_IDS: usize = 500;
pub const LIFECYCLE_PURGE_PREVIEW_TTL_SECONDS: i64 = 900;

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub struct PurgeSourceSelector {
    pub namespace: String,
    #[serde(default)]
    pub external_id: Option<String>,
}

impl fmt::Debug for PurgeSourceSelector {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PurgeSourceSelector")
            .field("namespace_len", &self.namespace.chars().count())
            .field(
                "external_id_len",
                &self.external_id.as_ref().map(|value| value.chars().count()),
            )
            .finish()
    }
}

#[derive(Clone, Default, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub struct PurgeSelector {
    #[serde(default)]
    pub note_ids: Vec<Uuid>,
    #[serde(default)]
    pub source: Option<PurgeSourceSelector>,
}

impl fmt::Debug for PurgeSelector {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PurgeSelector")
            .field("note_id_count", &self.note_ids.len())
            .field("source", &self.source)
            .finish()
    }
}

impl PurgeSelector {
    pub fn validate(&self) -> Result<()> {
        if self.note_ids.is_empty() && self.source.is_none() {
            return Err(Error::InvalidInput(
                "Purge selector must target note IDs or a source identity.".to_string(),
            ));
        }
        if self.note_ids.len() > LIFECYCLE_PURGE_MAX_NOTE_IDS {
            return Err(Error::InvalidInput(format!(
                "Purge selector exceeds the {} note limit.",
                LIFECYCLE_PURGE_MAX_NOTE_IDS
            )));
        }
        let mut sorted = self.note_ids.clone();
        sorted.sort_unstable();
        if sorted.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(Error::InvalidInput(
                "Purge selector contains duplicate note IDs.".to_string(),
            ));
        }
        if let Some(source) = &self.source {
            let namespace_len = source.namespace.chars().count();
            if source.namespace.trim() != source.namespace || !(1..=200).contains(&namespace_len) {
                return Err(Error::InvalidInput(
                    "Purge source namespace must contain 1 to 200 trimmed characters.".to_string(),
                ));
            }
            if let Some(external_id) = &source.external_id {
                let external_id_len = external_id.chars().count();
                if external_id.trim() != external_id || !(1..=1000).contains(&external_id_len) {
                    return Err(Error::InvalidInput(
                        "Purge external ID must contain 1 to 1000 trimmed characters.".to_string(),
                    ));
                }
            }
        }
        Ok(())
    }
}

/// Internal replay fingerprint. It must not be serialized into a public receipt.
pub fn lifecycle_purge_selector_fingerprint(selector: &PurgeSelector) -> Result<String> {
    selector.validate()?;
    let mut note_ids = selector
        .note_ids
        .iter()
        .map(Uuid::to_string)
        .collect::<Vec<_>>();
    note_ids.sort_unstable();
    let source = selector.source.as_ref().map(|value| {
        serde_json::json!({
            "namespace": value.namespace,
            "external_id": value.external_id,
        })
    });
    let canonical = serde_json::to_vec(&serde_json::json!({
        "note_ids": note_ids,
        "source": source,
    }))?;
    Ok(format!("sha256:{}", hex::encode(Sha256::digest(canonical))))
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub struct PurgeCounts {
    pub notes: u64,
    pub revisions: u64,
    pub links: u64,
    pub tags: u64,
    pub embeddings: u64,
    pub attachments: u64,
    /// Blobs that become unreferenced, not every blob touched by the selector.
    pub blobs: u64,
    pub graph_edges: u64,
    pub provenance_edges: u64,
    pub source_identities: u64,
}

impl PurgeCounts {
    pub fn is_empty(&self) -> bool {
        self.notes == 0
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub struct PurgePreview {
    pub contract_version: String,
    pub preview_id: Uuid,
    pub counts: PurgeCounts,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub struct PurgeRequest {
    pub operation_id: Uuid,
    pub preview_id: Uuid,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PurgeOutcome {
    CleanupPending,
    Completed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub struct PurgeReceiptPolicy {
    pub authority: String,
    pub mode: String,
    pub receipt_contains_content: bool,
    pub backup_disposition: String,
    pub restore_reerasure: bool,
}

impl Default for PurgeReceiptPolicy {
    fn default() -> Self {
        Self {
            authority: "Fortemi/fortemi#1092".to_string(),
            mode: "terminal_purge".to_string(),
            receipt_contains_content: false,
            backup_disposition: "beyond_use_then_reerase".to_string(),
            restore_reerasure: true,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub struct DeletionReceipt {
    pub contract_version: String,
    pub operation_id: Uuid,
    pub outcome: PurgeOutcome,
    pub counts: PurgeCounts,
    pub completed_at: DateTime<Utc>,
    pub policy: PurgeReceiptPolicy,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
pub struct PurgeStatus {
    pub contract_version: String,
    pub operation_id: Uuid,
    pub outcome: PurgeOutcome,
    pub counts: PurgeCounts,
    pub blob_cleanup_pending: u64,
    pub search_cleanup_pending: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipt: Option<DeletionReceipt>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selector_debug_and_public_outputs_do_not_disclose_targets() {
        let note_id = Uuid::now_v7();
        let selector = PurgeSelector {
            note_ids: vec![note_id],
            source: Some(PurgeSourceSelector {
                namespace: "private/provider/session".to_string(),
                external_id: Some("raw-provider-key".to_string()),
            }),
        };
        let rendered = format!("{selector:?}");
        assert!(!rendered.contains(&note_id.to_string()));
        assert!(!rendered.contains("private/provider/session"));
        assert!(!rendered.contains("raw-provider-key"));

        let receipt = DeletionReceipt {
            contract_version: LIFECYCLE_PURGE_CONTRACT_VERSION.to_string(),
            operation_id: Uuid::now_v7(),
            outcome: PurgeOutcome::Completed,
            counts: PurgeCounts::default(),
            completed_at: Utc::now(),
            policy: PurgeReceiptPolicy::default(),
        };
        let json = serde_json::to_string(&receipt).unwrap();
        assert!(!json.contains(&note_id.to_string()));
        assert!(!json.contains("private/provider/session"));
        assert!(!json.contains("raw-provider-key"));
        assert!(!json.contains("sha256:"));
    }

    #[test]
    fn selector_fingerprint_is_order_independent_and_scope_sensitive() {
        let first = Uuid::now_v7();
        let second = Uuid::now_v7();
        let selector = |note_ids| PurgeSelector {
            note_ids,
            source: Some(PurgeSourceSelector {
                namespace: "fixture.namespace".to_string(),
                external_id: Some("fixture-key".to_string()),
            }),
        };
        assert_eq!(
            lifecycle_purge_selector_fingerprint(&selector(vec![first, second])).unwrap(),
            lifecycle_purge_selector_fingerprint(&selector(vec![second, first])).unwrap()
        );
        assert_ne!(
            lifecycle_purge_selector_fingerprint(&selector(vec![first])).unwrap(),
            lifecycle_purge_selector_fingerprint(&PurgeSelector {
                note_ids: vec![first],
                source: Some(PurgeSourceSelector {
                    namespace: "other.namespace".to_string(),
                    external_id: Some("fixture-key".to_string()),
                }),
            })
            .unwrap()
        );
    }

    #[test]
    fn selector_validation_rejects_ambiguous_or_unbounded_inputs() {
        assert!(PurgeSelector::default().validate().is_err());
        let id = Uuid::now_v7();
        assert!(PurgeSelector {
            note_ids: vec![id, id],
            source: None,
        }
        .validate()
        .is_err());
        assert!(PurgeSelector {
            note_ids: Vec::new(),
            source: Some(PurgeSourceSelector {
                namespace: " namespace ".to_string(),
                external_id: None,
            }),
        }
        .validate()
        .is_err());
    }

    #[test]
    fn shared_conformance_fixture_matches_public_vocabulary() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../contracts/lifecycle-purge/conformance/v1.json"
        ))
        .unwrap();
        assert_eq!(
            fixture["contract_version"],
            LIFECYCLE_PURGE_CONTRACT_VERSION
        );

        let expected = serde_json::to_value(PurgeCounts::default()).unwrap();
        let mut expected_fields = expected
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        expected_fields.sort();
        let mut fixture_fields = fixture["count_fields"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        fixture_fields.sort();
        assert_eq!(fixture_fields, expected_fields);

        let receipt = serde_json::to_value(DeletionReceipt {
            contract_version: LIFECYCLE_PURGE_CONTRACT_VERSION.to_string(),
            operation_id: Uuid::now_v7(),
            outcome: PurgeOutcome::Completed,
            counts: PurgeCounts::default(),
            completed_at: Utc::now(),
            policy: PurgeReceiptPolicy::default(),
        })
        .unwrap();
        for forbidden in fixture["receipt_forbidden_fields"].as_array().unwrap() {
            assert!(receipt.get(forbidden.as_str().unwrap()).is_none());
        }
    }
}
