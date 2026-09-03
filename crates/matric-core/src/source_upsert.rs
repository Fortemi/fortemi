//! Provider-neutral contract for source-addressed, atomic note upsert.
//!
//! This is a live persistence contract. It is intentionally independent from
//! the Knowledge Shard export/import profiles.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use utoipa::ToSchema;
use uuid::Uuid;

pub const SOURCE_UPSERT_CONTRACT_VERSION: &str = "1.0.0";
pub const SOURCE_UPSERT_MAX_ITEMS: usize = 500;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SourceUpsertPolicy {
    Replace,
    #[default]
    Version,
    Conflict,
}

#[derive(Clone, Deserialize, Serialize, ToSchema)]
pub struct SourceUpsertItem {
    pub external_id: String,
    pub content: String,
    #[serde(default)]
    pub content_digest: Option<String>,
    #[serde(default)]
    pub caller_stable_id: Option<Uuid>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default = "default_note_format")]
    pub format: String,
    #[serde(default)]
    pub metadata: serde_json::Value,
    #[serde(default)]
    pub policy: Option<SourceUpsertPolicy>,
}

impl fmt::Debug for SourceUpsertItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SourceUpsertItem")
            .field("external_id_present", &!self.external_id.is_empty())
            .field("external_id_len", &self.external_id.chars().count())
            .field("content_len", &self.content.chars().count())
            .field("content_digest_present", &self.content_digest.is_some())
            .field("caller_stable_id_present", &self.caller_stable_id.is_some())
            .field(
                "title_len",
                &self.title.as_ref().map(|value| value.chars().count()),
            )
            .field("format_len", &self.format.chars().count())
            .field("metadata_present", &!self.metadata.is_null())
            .field("policy", &self.policy)
            .finish()
    }
}

#[derive(Clone, Deserialize, Serialize, ToSchema)]
pub struct SourceUpsertRequest {
    pub source_namespace: String,
    #[serde(default)]
    pub source_id: Option<String>,
    pub source_schema_version: String,
    pub import_run_id: String,
    #[serde(default)]
    pub batch_id: Option<String>,
    #[serde(default)]
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub checkpoint: Option<serde_json::Value>,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub policy: SourceUpsertPolicy,
    pub items: Vec<SourceUpsertItem>,
}

impl fmt::Debug for SourceUpsertRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SourceUpsertRequest")
            .field(
                "source_namespace_len",
                &self.source_namespace.chars().count(),
            )
            .field(
                "source_id_len",
                &self.source_id.as_ref().map(|value| value.chars().count()),
            )
            .field(
                "source_schema_version_len",
                &self.source_schema_version.chars().count(),
            )
            .field("import_run_id_len", &self.import_run_id.chars().count())
            .field(
                "batch_id_len",
                &self.batch_id.as_ref().map(|value| value.chars().count()),
            )
            .field(
                "workspace_id_len",
                &self
                    .workspace_id
                    .as_ref()
                    .map(|value| value.chars().count()),
            )
            .field("checkpoint_present", &self.checkpoint.is_some())
            .field("dry_run", &self.dry_run)
            .field("policy", &self.policy)
            .field("item_count", &self.items.len())
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SourceUpsertItemOutcome {
    Inserted,
    Unchanged,
    Versioned,
    Replaced,
    Conflict,
    Rejected,
}

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
pub struct SourceUpsertItemResult {
    pub index: usize,
    pub outcome: SourceUpsertItemOutcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note_id: Option<Uuid>,
    pub external_id_hash: String,
    pub content_digest: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SourceUpsertBatchOutcome {
    Committed,
    Duplicate,
    Preview,
    Rejected,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, ToSchema)]
pub struct SourceUpsertCounts {
    pub inserted: usize,
    pub unchanged: usize,
    pub versioned: usize,
    pub replaced: usize,
    pub conflict: usize,
    pub rejected: usize,
}

impl SourceUpsertCounts {
    pub fn observe(&mut self, outcome: SourceUpsertItemOutcome) {
        match outcome {
            SourceUpsertItemOutcome::Inserted => self.inserted += 1,
            SourceUpsertItemOutcome::Unchanged => self.unchanged += 1,
            SourceUpsertItemOutcome::Versioned => self.versioned += 1,
            SourceUpsertItemOutcome::Replaced => self.replaced += 1,
            SourceUpsertItemOutcome::Conflict => self.conflict += 1,
            SourceUpsertItemOutcome::Rejected => self.rejected += 1,
        }
    }

    pub fn material_changes(&self) -> usize {
        self.inserted + self.versioned + self.replaced
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
pub struct SourceUpsertResponse {
    pub contract_version: String,
    pub import_run_id: String,
    pub batch_id: String,
    pub dry_run: bool,
    pub outcome: SourceUpsertBatchOutcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<serde_json::Value>,
    pub counts: SourceUpsertCounts,
    pub items: Vec<SourceUpsertItemResult>,
}

pub fn source_content_digest(content: &str) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(content.as_bytes())))
}

/// Opaque identity used in receipts and operational responses.
///
/// The memory name participates in the digest so equal source keys in two
/// memories cannot collide. The raw key is never returned or logged.
pub fn source_identity_digest(
    tenant_id: Uuid,
    memory_name: &str,
    namespace: &str,
    external_id: &str,
) -> String {
    let mut hasher = Sha256::new();
    let tenant = tenant_id.to_string();
    for part in [tenant.as_str(), memory_name, namespace, external_id] {
        hasher.update(part.as_bytes());
        hasher.update([0]);
    }
    format!("sha256:{}", hex::encode(hasher.finalize()))
}

pub fn source_request_digest(request: &SourceUpsertRequest) -> String {
    let encoded = serde_json::to_vec(request).expect("source upsert request is serializable");
    format!("sha256:{}", hex::encode(Sha256::digest(encoded)))
}

fn default_note_format() -> String {
    "markdown".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_output_redacts_external_keys_and_content() {
        let item = SourceUpsertItem {
            external_id: "sensitive-key".to_string(),
            content: "sensitive-content".to_string(),
            content_digest: None,
            caller_stable_id: None,
            title: None,
            format: default_note_format(),
            metadata: serde_json::json!({"secret": "sensitive-metadata"}),
            policy: None,
        };
        let rendered = format!("{item:?}");
        assert!(!rendered.contains("sensitive-key"));
        assert!(!rendered.contains("sensitive-content"));
        assert!(!rendered.contains("sensitive-metadata"));
    }

    #[test]
    fn identity_digest_is_tenant_and_memory_bound() {
        let tenant = Uuid::nil();
        let base = source_identity_digest(tenant, "public", "example", "one");
        assert_ne!(
            base,
            source_identity_digest(Uuid::new_v4(), "public", "example", "one")
        );
        assert_ne!(
            base,
            source_identity_digest(tenant, "archive_alpha", "example", "one")
        );
        assert_ne!(
            base,
            source_identity_digest(tenant, "public", "example", "two")
        );
    }
}
