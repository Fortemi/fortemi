//! Shard migration handlers.
//!
//! Each migration transforms shard data from one version to another.
//! Migrations are registered with the MigrationRegistry and applied
//! automatically during import.

// Future migrations will be added here:
// pub mod v1_0_to_v1_1;
// pub mod v1_1_to_v2_0;

/// Get all registered migrations
pub fn all_migrations() -> Vec<Box<dyn super::ShardMigration>> {
    vec![
        // Migrations will be added as schema evolves
    ]
}
