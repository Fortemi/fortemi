//! Shard migration infrastructure for versioned knowledge shard imports/exports.

pub mod compatibility;
pub mod downgrade;
pub mod messages;
pub mod migration;
pub mod migrations;
pub mod reserved;
pub mod upgrade;
pub mod version;
pub mod warning;

#[cfg(test)]
mod tests;

pub use compatibility::{check_shard_compatibility, CompatibilityResult};
pub use downgrade::{
    analyze_downgrade_impact, DataLoss, DataLossOutcome, DowngradeImpact, FeatureLoss,
};
pub use messages::format_downgrade_message;
pub use migration::{MigrationError, MigrationRegistry, MigrationResult, ShardMigration};
pub use reserved::{
    is_reserved, validate_no_reserved_fields, FieldComponent, ReservedField, ReservedFieldError,
};
pub use upgrade::{
    format_upgrade_message, generate_upgrade_guidance, UpgradeDifficulty, UpgradeGuidance,
    UpgradeStep,
};
pub use version::{Version, CURRENT_SHARD_VERSION};
pub use warning::MigrationWarning;
