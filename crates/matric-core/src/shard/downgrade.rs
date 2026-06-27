//! Downgrade impact analysis for importing shards from newer versions.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Analysis of what will be lost when importing a newer shard
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DowngradeImpact {
    pub shard_version: String,
    pub current_version: String,
    pub features_lost: Vec<FeatureLoss>,
    pub data_loss: Vec<DataLoss>,
    pub can_proceed: bool,
    pub summary: String,
}

impl fmt::Debug for DowngradeImpact {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DowngradeImpact")
            .field("shard_version_len", &self.shard_version.chars().count())
            .field("current_version_len", &self.current_version.chars().count())
            .field("features_lost_count", &self.features_lost.len())
            .field("data_loss_count", &self.data_loss.len())
            .field("can_proceed", &self.can_proceed)
            .field("summary_len", &self.summary.chars().count())
            .finish()
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FeatureLoss {
    pub feature: String,
    pub introduced_in: String,
    pub description: String,
}

impl fmt::Debug for FeatureLoss {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FeatureLoss")
            .field("feature_len", &self.feature.chars().count())
            .field("introduced_in_len", &self.introduced_in.chars().count())
            .field("description_len", &self.description.chars().count())
            .finish()
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DataLoss {
    pub component: String,
    pub field: String,
    pub affected_count: usize,
    pub description: String,
    pub outcome: DataLossOutcome,
}

impl fmt::Debug for DataLoss {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DataLoss")
            .field("component_len", &self.component.chars().count())
            .field("field_len", &self.field.chars().count())
            .field("affected_count", &self.affected_count)
            .field("description_len", &self.description.chars().count())
            .field("outcome", &self.outcome)
            .finish()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DataLossOutcome {
    Discarded,
    Degraded,
    Ignored,
}

impl std::fmt::Display for DataLossOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Discarded => write!(f, "will be discarded"),
            Self::Degraded => write!(f, "will be degraded"),
            Self::Ignored => write!(f, "will be ignored"),
        }
    }
}

/// Analyze impact of importing a shard from a newer version
pub fn analyze_downgrade_impact(
    shard_version: &str,
    current_version: &str,
    shard_manifest: &serde_json::Value,
) -> DowngradeImpact {
    let features_lost = Vec::new();
    let mut data_loss = Vec::new();

    // Check for known fields introduced in newer versions
    // This would be driven by a feature registry in production

    // Example: Check for MRL embeddings (introduced in 2026.2.0)
    if let Some(embeddings) = shard_manifest.get("embeddings") {
        if let Some(arr) = embeddings.as_array() {
            let mrl_count = arr
                .iter()
                .filter(|e| e.get("truncate_dim").is_some())
                .count();
            if mrl_count > 0 {
                data_loss.push(DataLoss {
                    component: "embeddings".to_string(),
                    field: "truncate_dim".to_string(),
                    affected_count: mrl_count,
                    description: format!("{} embeddings use MRL truncation", mrl_count),
                    outcome: DataLossOutcome::Discarded,
                });
            }
        }
    }

    // Check for unknown fields (future-proofing)
    // In reality, compare against known schema

    let can_proceed = data_loss
        .iter()
        .all(|d| !matches!(d.outcome, DataLossOutcome::Discarded) || d.affected_count < 100);

    let summary = if data_loss.is_empty() && features_lost.is_empty() {
        "Import should proceed normally.".to_string()
    } else {
        format!(
            "Import will proceed with {} feature(s) unavailable and {} field(s) affected.",
            features_lost.len(),
            data_loss.len()
        )
    };

    DowngradeImpact {
        shard_version: shard_version.to_string(),
        current_version: current_version.to_string(),
        features_lost,
        data_loss,
        can_proceed,
        summary,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_analyze_downgrade_impact_no_data_loss() {
        let manifest = json!({
            "version": "1.1.0",
            "notes": [
                {"id": "1", "content": "test"}
            ]
        });

        let impact = analyze_downgrade_impact("1.1.0", "1.0.0", &manifest);

        assert_eq!(impact.shard_version, "1.1.0");
        assert_eq!(impact.current_version, "1.0.0");
        assert!(impact.features_lost.is_empty());
        assert!(impact.data_loss.is_empty());
        assert!(impact.can_proceed);
        assert!(impact.summary.contains("normally"));
    }

    #[test]
    fn test_analyze_downgrade_impact_with_mrl_embeddings() {
        let manifest = json!({
            "version": "1.1.0",
            "embeddings": [
                {
                    "id": "1",
                    "vector": [0.1, 0.2, 0.3],
                    "truncate_dim": 128
                },
                {
                    "id": "2",
                    "vector": [0.4, 0.5, 0.6],
                    "truncate_dim": 128
                }
            ]
        });

        let impact = analyze_downgrade_impact("1.1.0", "1.0.0", &manifest);

        assert_eq!(impact.data_loss.len(), 1);
        let loss = &impact.data_loss[0];
        assert_eq!(loss.component, "embeddings");
        assert_eq!(loss.field, "truncate_dim");
        assert_eq!(loss.affected_count, 2);
        assert_eq!(loss.outcome, DataLossOutcome::Discarded);
        assert!(loss.description.contains("MRL"));
    }

    #[test]
    fn test_analyze_downgrade_impact_large_data_loss_blocks() {
        let mut embeddings = Vec::new();
        for i in 0..150 {
            embeddings.push(json!({
                "id": i.to_string(),
                "vector": [0.1, 0.2, 0.3],
                "truncate_dim": 128
            }));
        }

        let manifest = json!({
            "version": "1.1.0",
            "embeddings": embeddings
        });

        let impact = analyze_downgrade_impact("1.1.0", "1.0.0", &manifest);

        // Should block proceed when large amount of data would be lost
        assert!(!impact.can_proceed);
        assert!(!impact.data_loss.is_empty());
    }

    #[test]
    fn test_analyze_downgrade_impact_mixed_embeddings() {
        let manifest = json!({
            "version": "1.1.0",
            "embeddings": [
                {
                    "id": "1",
                    "vector": [0.1, 0.2, 0.3],
                    "truncate_dim": 128
                },
                {
                    "id": "2",
                    "vector": [0.4, 0.5, 0.6]
                }
            ]
        });

        let impact = analyze_downgrade_impact("1.1.0", "1.0.0", &manifest);

        // Only one embedding has MRL truncation
        assert_eq!(impact.data_loss.len(), 1);
        assert_eq!(impact.data_loss[0].affected_count, 1);
    }

    #[test]
    fn test_downgrade_impact_serialization() {
        let impact = DowngradeImpact {
            shard_version: "1.1.0".to_string(),
            current_version: "1.0.0".to_string(),
            features_lost: vec![FeatureLoss {
                feature: "mrl_embeddings".to_string(),
                introduced_in: "1.1.0".to_string(),
                description: "MRL support".to_string(),
            }],
            data_loss: vec![DataLoss {
                component: "embeddings".to_string(),
                field: "truncate_dim".to_string(),
                affected_count: 5,
                description: "5 embeddings use MRL".to_string(),
                outcome: DataLossOutcome::Discarded,
            }],
            can_proceed: true,
            summary: "Test summary".to_string(),
        };

        let json = serde_json::to_string(&impact).unwrap();
        let deserialized: DowngradeImpact = serde_json::from_str(&json).unwrap();
        assert_eq!(impact, deserialized);
    }

    #[test]
    fn downgrade_impact_debug_redacts_versions_features_fields_and_summaries() {
        let impact = DowngradeImpact {
            shard_version: "2.0.0-customer@example.com-sk-live-shard".to_string(),
            current_version: "1.0.0-postgres://user:secret@db.internal/app".to_string(),
            features_lost: vec![FeatureLoss {
                feature: "private/customer@example.com-feature".to_string(),
                introduced_in: "1.9.0-sk-live-feature".to_string(),
                description: "feature uses bearer sk-live-description".to_string(),
            }],
            data_loss: vec![DataLoss {
                component: "private/path/customer@example.com".to_string(),
                field: "database_url".to_string(),
                affected_count: 5,
                description: "lost postgres://user:secret@db.internal/app field".to_string(),
                outcome: DataLossOutcome::Discarded,
            }],
            can_proceed: false,
            summary: "Import warning for customer@example.com and sk-live-summary".to_string(),
        };

        let rendered = format!("{impact:?}");
        let feature = format!("{:?}", impact.features_lost[0]);
        let data_loss = format!("{:?}", impact.data_loss[0]);
        let combined = format!("{rendered}\n{feature}\n{data_loss}");

        assert!(rendered.contains("DowngradeImpact"));
        assert!(rendered.contains("shard_version_len"));
        assert!(rendered.contains("current_version_len"));
        assert!(rendered.contains("features_lost_count"));
        assert!(rendered.contains("data_loss_count"));
        assert!(feature.contains("FeatureLoss"));
        assert!(feature.contains("feature_len"));
        assert!(data_loss.contains("DataLoss"));
        assert!(data_loss.contains("component_len"));
        assert!(data_loss.contains("field_len"));
        assert!(data_loss.contains("description_len"));

        for raw in [
            "customer@example.com",
            "sk-live",
            "postgres://",
            "db.internal",
            "private/",
            "database_url",
            "bearer",
            "Import warning",
        ] {
            assert!(
                !combined.contains(raw),
                "Debug output leaked {raw}: {combined}"
            );
        }
    }

    #[test]
    fn test_data_loss_outcome_display() {
        assert_eq!(DataLossOutcome::Discarded.to_string(), "will be discarded");
        assert_eq!(DataLossOutcome::Degraded.to_string(), "will be degraded");
        assert_eq!(DataLossOutcome::Ignored.to_string(), "will be ignored");
    }

    #[test]
    fn test_analyze_downgrade_impact_summary_format() {
        let manifest = json!({
            "version": "1.2.0",
            "embeddings": [
                {"id": "1", "vector": [0.1], "truncate_dim": 128}
            ]
        });

        let impact = analyze_downgrade_impact("1.2.0", "1.0.0", &manifest);

        assert!(impact.summary.contains("feature") || impact.summary.contains("field"));
    }
}
