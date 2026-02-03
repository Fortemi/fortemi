//! User-facing messages for shard migration operations.

use super::downgrade::{DataLossOutcome, DowngradeImpact};

/// Format a human-readable downgrade warning message
pub fn format_downgrade_message(impact: &DowngradeImpact) -> String {
    let mut lines = Vec::new();

    lines.push(format!(
        "⚠️  Importing shard from newer version {} into {}",
        impact.shard_version, impact.current_version
    ));
    lines.push(String::new());

    if !impact.features_lost.is_empty() {
        lines.push("Features not available in this version:".to_string());
        for feature in &impact.features_lost {
            lines.push(format!(
                "  • {} (introduced in {}): {}",
                feature.feature, feature.introduced_in, feature.description
            ));
        }
        lines.push(String::new());
    }

    if !impact.data_loss.is_empty() {
        lines.push("Data that will be affected:".to_string());
        for loss in &impact.data_loss {
            let action = match loss.outcome {
                DataLossOutcome::Discarded => "❌ DISCARDED",
                DataLossOutcome::Degraded => "⚠️  degraded",
                DataLossOutcome::Ignored => "ℹ️  ignored",
            };
            lines.push(format!(
                "  • {}.{} ({} items) - {} - {}",
                loss.component, loss.field, loss.affected_count, action, loss.description
            ));
        }
        lines.push(String::new());
    }

    if impact.can_proceed {
        lines.push("✅ Import can proceed with the above limitations.".to_string());
    } else {
        lines.push("❌ Import blocked due to significant data loss.".to_string());
        lines.push("   Consider upgrading matric-memory before importing this shard.".to_string());
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shard::downgrade::{DataLoss, FeatureLoss};

    #[test]
    fn test_format_downgrade_message_empty() {
        let impact = DowngradeImpact {
            shard_version: "1.1.0".to_string(),
            current_version: "1.0.0".to_string(),
            features_lost: vec![],
            data_loss: vec![],
            can_proceed: true,
            summary: "No issues".to_string(),
        };

        let msg = format_downgrade_message(&impact);
        assert!(msg.contains("1.1.0"));
        assert!(msg.contains("1.0.0"));
        assert!(msg.contains("✅"));
    }

    #[test]
    fn test_format_downgrade_message_with_features() {
        let impact = DowngradeImpact {
            shard_version: "1.2.0".to_string(),
            current_version: "1.0.0".to_string(),
            features_lost: vec![FeatureLoss {
                feature: "mrl_support".to_string(),
                introduced_in: "1.1.0".to_string(),
                description: "Matryoshka embeddings".to_string(),
            }],
            data_loss: vec![],
            can_proceed: true,
            summary: "".to_string(),
        };

        let msg = format_downgrade_message(&impact);
        assert!(msg.contains("mrl_support"));
        assert!(msg.contains("1.1.0"));
        assert!(msg.contains("Matryoshka"));
    }

    #[test]
    fn test_format_downgrade_message_with_data_loss() {
        let impact = DowngradeImpact {
            shard_version: "1.2.0".to_string(),
            current_version: "1.0.0".to_string(),
            features_lost: vec![],
            data_loss: vec![DataLoss {
                component: "embeddings".to_string(),
                field: "truncate_dim".to_string(),
                affected_count: 50,
                description: "MRL dimension truncation".to_string(),
                outcome: DataLossOutcome::Discarded,
            }],
            can_proceed: true,
            summary: "".to_string(),
        };

        let msg = format_downgrade_message(&impact);
        assert!(msg.contains("embeddings.truncate_dim"));
        assert!(msg.contains("50 items"));
        assert!(msg.contains("DISCARDED"));
    }

    #[test]
    fn test_format_downgrade_message_blocked() {
        let impact = DowngradeImpact {
            shard_version: "2.0.0".to_string(),
            current_version: "1.0.0".to_string(),
            features_lost: vec![],
            data_loss: vec![DataLoss {
                component: "notes".to_string(),
                field: "new_format".to_string(),
                affected_count: 1000,
                description: "New note format".to_string(),
                outcome: DataLossOutcome::Discarded,
            }],
            can_proceed: false,
            summary: "Blocked".to_string(),
        };

        let msg = format_downgrade_message(&impact);
        assert!(msg.contains("❌ Import blocked"));
        assert!(msg.contains("upgrading matric-memory"));
    }
}
