//! Shard migration handlers.
//!
//! Each migration transforms shard data from one version to another.
//! Migrations are registered with the MigrationRegistry and applied
//! automatically during import.

mod v1_0_to_v1_1;
mod v1_1_to_v1_2;

pub use v1_0_to_v1_1::V1_0ToV1_1;
pub use v1_1_to_v1_2::V1_1ToV1_2;

/// Get all registered migrations
pub fn all_migrations() -> Vec<Box<dyn super::ShardMigration>> {
    vec![Box::new(V1_0ToV1_1), Box::new(V1_1ToV1_2)]
}
