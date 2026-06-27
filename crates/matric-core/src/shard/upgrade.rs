//! Upgrade guidance for importing shards from older versions.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Difficulty level of an upgrade operation
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UpgradeDifficulty {
    /// Automatic, no user intervention needed
    Automatic,
    /// Simple, may need confirmation
    Simple,
    /// Moderate, requires some decisions
    Moderate,
    /// Complex, significant changes needed
    Complex,
}

impl std::fmt::Display for UpgradeDifficulty {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Automatic => write!(f, "automatic"),
            Self::Simple => write!(f, "simple"),
            Self::Moderate => write!(f, "moderate"),
            Self::Complex => write!(f, "complex"),
        }
    }
}

/// A single step in the upgrade process
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpgradeStep {
    pub order: usize,
    pub title: String,
    pub description: String,
    pub command: Option<String>,
    pub is_automatic: bool,
}

impl fmt::Debug for UpgradeStep {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UpgradeStep")
            .field("order", &self.order)
            .field("title_len", &self.title.len())
            .field("description_len", &self.description.len())
            .field("command_len", &self.command.as_ref().map(String::len))
            .field("is_automatic", &self.is_automatic)
            .finish()
    }
}

/// Complete upgrade guidance for a shard import
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpgradeGuidance {
    pub from_version: String,
    pub to_version: String,
    pub difficulty: UpgradeDifficulty,
    pub steps: Vec<UpgradeStep>,
    pub new_features_available: Vec<String>,
    pub summary: String,
}

impl fmt::Debug for UpgradeGuidance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UpgradeGuidance")
            .field("from_version_len", &self.from_version.len())
            .field("to_version_len", &self.to_version.len())
            .field("difficulty", &self.difficulty)
            .field("steps_count", &self.steps.len())
            .field(
                "new_features_available_count",
                &self.new_features_available.len(),
            )
            .field(
                "new_features_available_lens",
                &self
                    .new_features_available
                    .iter()
                    .map(String::len)
                    .collect::<Vec<_>>(),
            )
            .field("summary_len", &self.summary.len())
            .finish()
    }
}

/// Generate upgrade guidance for importing from an older shard version
pub fn generate_upgrade_guidance(from_version: &str, to_version: &str) -> UpgradeGuidance {
    let mut steps = Vec::new();
    let mut new_features = Vec::new();
    let mut difficulty = UpgradeDifficulty::Automatic;

    // Parse versions for comparison
    let from_parts: Vec<&str> = from_version.split('.').collect();
    let to_parts: Vec<&str> = to_version.split('.').collect();

    let from_major = from_parts
        .first()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(1);
    let to_major = to_parts
        .first()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(1);

    // Major version upgrades are more complex
    if to_major > from_major {
        difficulty = UpgradeDifficulty::Complex;
        steps.push(UpgradeStep {
            order: 1,
            title: "Backup current data".to_string(),
            description: "Create a backup before major version upgrade".to_string(),
            command: Some("matric backup create --name pre-upgrade".to_string()),
            is_automatic: false,
        });
    }

    // Add schema migration step
    steps.push(UpgradeStep {
        order: steps.len() + 1,
        title: "Apply schema migrations".to_string(),
        description: "Database schema will be automatically upgraded".to_string(),
        command: None,
        is_automatic: true,
    });

    // Check for known feature additions between versions
    // This would be driven by a feature registry in production
    if from_version.starts_with("1.0") && !to_version.starts_with("1.0") {
        new_features.push("MRL embeddings support".to_string());
        new_features.push("Document type registry".to_string());
    }

    // Add feature enablement step if new features available
    if !new_features.is_empty() && difficulty != UpgradeDifficulty::Complex {
        difficulty = UpgradeDifficulty::Simple;
        steps.push(UpgradeStep {
            order: steps.len() + 1,
            title: "Enable new features".to_string(),
            description: format!("Consider enabling: {}", new_features.join(", ")),
            command: None,
            is_automatic: false,
        });
    }

    let summary = match difficulty {
        UpgradeDifficulty::Automatic => {
            "Import will be processed automatically with no changes needed.".to_string()
        }
        UpgradeDifficulty::Simple => {
            format!(
                "Import will succeed. {} new feature(s) will be available.",
                new_features.len()
            )
        }
        UpgradeDifficulty::Moderate => {
            "Import will succeed but some configuration may be needed.".to_string()
        }
        UpgradeDifficulty::Complex => {
            "Major version upgrade detected. Please review the migration steps carefully."
                .to_string()
        }
    };

    UpgradeGuidance {
        from_version: from_version.to_string(),
        to_version: to_version.to_string(),
        difficulty,
        steps,
        new_features_available: new_features,
        summary,
    }
}

/// Format a human-readable upgrade message
pub fn format_upgrade_message(guidance: &UpgradeGuidance) -> String {
    let mut lines = Vec::new();

    let icon = match guidance.difficulty {
        UpgradeDifficulty::Automatic => "✅",
        UpgradeDifficulty::Simple => "ℹ️ ",
        UpgradeDifficulty::Moderate => "⚠️ ",
        UpgradeDifficulty::Complex => "🔧",
    };

    lines.push(format!(
        "{} Importing shard from older version {} to {}",
        icon, guidance.from_version, guidance.to_version
    ));
    lines.push(format!("   Difficulty: {}", guidance.difficulty));
    lines.push(String::new());

    if !guidance.steps.is_empty() {
        lines.push("Migration steps:".to_string());
        for step in &guidance.steps {
            let auto = if step.is_automatic {
                " (automatic)"
            } else {
                ""
            };
            lines.push(format!("  {}. {}{}", step.order, step.title, auto));
            lines.push(format!("     {}", step.description));
            if let Some(cmd) = &step.command {
                lines.push(format!("     $ {}", cmd));
            }
        }
        lines.push(String::new());
    }

    if !guidance.new_features_available.is_empty() {
        lines.push("New features available after import:".to_string());
        for feature in &guidance.new_features_available {
            lines.push(format!("  • {}", feature));
        }
        lines.push(String::new());
    }

    lines.push(guidance.summary.clone());

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_upgrade_guidance_same_version() {
        let guidance = generate_upgrade_guidance("1.0.0", "1.0.0");
        assert_eq!(guidance.from_version, "1.0.0");
        assert_eq!(guidance.to_version, "1.0.0");
        assert_eq!(guidance.difficulty, UpgradeDifficulty::Automatic);
    }

    #[test]
    fn test_generate_upgrade_guidance_minor_version() {
        let guidance = generate_upgrade_guidance("1.0.0", "1.1.0");
        assert_eq!(guidance.difficulty, UpgradeDifficulty::Simple);
        assert!(!guidance.new_features_available.is_empty());
    }

    #[test]
    fn test_generate_upgrade_guidance_major_version() {
        let guidance = generate_upgrade_guidance("1.0.0", "2.0.0");
        assert_eq!(guidance.difficulty, UpgradeDifficulty::Complex);
        assert!(guidance.steps.iter().any(|s| s.title.contains("Backup")));
    }

    #[test]
    fn test_format_upgrade_message() {
        let guidance = generate_upgrade_guidance("1.0.0", "1.1.0");
        let msg = format_upgrade_message(&guidance);
        assert!(msg.contains("1.0.0"));
        assert!(msg.contains("1.1.0"));
        assert!(msg.contains("Difficulty"));
    }

    #[test]
    fn test_upgrade_difficulty_display() {
        assert_eq!(UpgradeDifficulty::Automatic.to_string(), "automatic");
        assert_eq!(UpgradeDifficulty::Simple.to_string(), "simple");
        assert_eq!(UpgradeDifficulty::Moderate.to_string(), "moderate");
        assert_eq!(UpgradeDifficulty::Complex.to_string(), "complex");
    }

    #[test]
    fn test_upgrade_guidance_serialization() {
        let guidance = UpgradeGuidance {
            from_version: "1.0.0".to_string(),
            to_version: "1.1.0".to_string(),
            difficulty: UpgradeDifficulty::Simple,
            steps: vec![UpgradeStep {
                order: 1,
                title: "Test step".to_string(),
                description: "Test description".to_string(),
                command: Some("test cmd".to_string()),
                is_automatic: true,
            }],
            new_features_available: vec!["Feature A".to_string()],
            summary: "Test summary".to_string(),
        };

        let json = serde_json::to_string(&guidance).unwrap();
        let deserialized: UpgradeGuidance = serde_json::from_str(&json).unwrap();
        assert_eq!(guidance, deserialized);
    }

    #[test]
    fn upgrade_guidance_debug_redacts_versions_steps_features_and_summaries() {
        let guidance = UpgradeGuidance {
            from_version: "1.0.0-customer@example.com".to_string(),
            to_version: "2.0.0-sk-live-upgrade-token".to_string(),
            difficulty: UpgradeDifficulty::Moderate,
            steps: vec![UpgradeStep {
                order: 1,
                title: "Rotate postgres://admin:secret@db.internal/fortemi".to_string(),
                description: "Move private/archive path /srv/private/customer@example.com"
                    .to_string(),
                command: Some(
                    "fortemi import --token bearer-secret --path private/shard".to_string(),
                ),
                is_automatic: false,
            }],
            new_features_available: vec![
                "Document type registry for customer@example.com".to_string(),
                "Provider key sk-live-feature".to_string(),
            ],
            summary: "Upgrade summary contains postgres://admin:secret@db.internal/fortemi"
                .to_string(),
        };

        let guidance_debug = format!("{guidance:?}");
        let step_debug = format!("{:?}", guidance.steps[0]);

        for debug_output in [&guidance_debug, &step_debug] {
            for raw in [
                "customer@example.com",
                "sk-live",
                "postgres://",
                "db.internal",
                "/srv/private",
                "bearer-secret",
                "private/shard",
                "Document type registry",
                "Provider key",
                "Upgrade summary",
                "Rotate",
                "Move private/archive",
                "fortemi import",
            ] {
                assert!(
                    !debug_output.contains(raw),
                    "debug output leaked {raw}: {debug_output}"
                );
            }
        }

        assert!(guidance_debug.contains("UpgradeGuidance"));
        assert!(guidance_debug.contains("from_version_len"));
        assert!(guidance_debug.contains("to_version_len"));
        assert!(guidance_debug.contains("steps_count"));
        assert!(guidance_debug.contains("new_features_available_lens"));
        assert!(guidance_debug.contains("summary_len"));
        assert!(step_debug.contains("UpgradeStep"));
        assert!(step_debug.contains("title_len"));
        assert!(step_debug.contains("description_len"));
        assert!(step_debug.contains("command_len"));
    }

    #[test]
    fn test_upgrade_step_automatic_flag() {
        let step = UpgradeStep {
            order: 1,
            title: "Auto step".to_string(),
            description: "Automatic".to_string(),
            command: None,
            is_automatic: true,
        };

        assert!(step.is_automatic);
        assert!(step.command.is_none());
    }
}
