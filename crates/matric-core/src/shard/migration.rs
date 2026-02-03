//! Shard migration trait and registry.

use super::warning::MigrationWarning;
use serde_json::Value;
use std::collections::HashMap;
use thiserror::Error;

/// Error types for migration operations.
#[derive(Debug, Error)]
pub enum MigrationError {
    #[error("No migration path found from {from} to {to}")]
    NoMigrationPath { from: String, to: String },

    #[error("Migration failed: {0}")]
    MigrationFailed(String),

    #[error("Invalid version: {0}")]
    InvalidVersion(String),

    #[error("Circular migration detected")]
    CircularMigration,
}

#[derive(Debug)]
/// Result of a successful migration.
pub struct MigrationResult {
    pub data: Value,
    pub warnings: Vec<MigrationWarning>,
}

/// Trait for implementing shard data migrations.
#[allow(clippy::wrong_self_convention)]
pub trait ShardMigration: Send + Sync {
    /// Source version this migration applies to.
    fn from_version(&self) -> &str;

    /// Target version this migration produces.
    fn to_version(&self) -> &str;

    /// Human-readable description of the migration.
    fn description(&self) -> &str;

    /// Perform the migration on the provided data.
    fn migrate(&self, data: Value) -> Result<MigrationResult, MigrationError>;
}

/// Registry for managing available shard migrations.
pub struct MigrationRegistry {
    migrations: Vec<Box<dyn ShardMigration>>,
}

impl MigrationRegistry {
    /// Create a new empty migration registry.
    pub fn new() -> Self {
        Self {
            migrations: Vec::new(),
        }
    }

    /// Register a migration.
    pub fn register(&mut self, migration: Box<dyn ShardMigration>) {
        self.migrations.push(migration);
    }

    /// Find a migration path from one version to another.
    /// Returns a sequence of migrations to apply, or None if no path exists.
    pub fn find_path(&self, from: &str, to: &str) -> Option<Vec<&dyn ShardMigration>> {
        if from == to {
            return Some(Vec::new());
        }

        // Build adjacency map
        let mut graph: HashMap<String, Vec<&dyn ShardMigration>> = HashMap::new();
        for migration in &self.migrations {
            graph
                .entry(migration.from_version().to_string())
                .or_default()
                .push(migration.as_ref());
        }

        // BFS to find shortest path
        let mut queue = vec![(from.to_string(), Vec::new())];
        let mut visited = std::collections::HashSet::new();
        visited.insert(from.to_string());

        while let Some((current, path)) = queue.pop() {
            if current == to {
                return Some(path);
            }

            if let Some(next_migrations) = graph.get(&current) {
                for migration in next_migrations {
                    let next = migration.to_version().to_string();
                    if !visited.contains(&next) {
                        visited.insert(next.clone());
                        let mut new_path = path.clone();
                        new_path.push(*migration);
                        queue.insert(0, (next, new_path)); // Insert at front for BFS
                    }
                }
            }
        }

        None
    }

    /// Migrate data from one version to another.
    pub fn migrate(
        &self,
        data: Value,
        from: &str,
        to: &str,
    ) -> Result<MigrationResult, MigrationError> {
        let path = self
            .find_path(from, to)
            .ok_or_else(|| MigrationError::NoMigrationPath {
                from: from.to_string(),
                to: to.to_string(),
            })?;

        let mut current_data = data;
        let mut all_warnings = Vec::new();

        for migration in path {
            let result = migration.migrate(current_data)?;
            current_data = result.data;
            all_warnings.extend(result.warnings);
        }

        Ok(MigrationResult {
            data: current_data,
            warnings: all_warnings,
        })
    }
}

impl Default for MigrationRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestMigration {
        from: String,
        to: String,
        description: String,
    }

    impl ShardMigration for TestMigration {
        fn from_version(&self) -> &str {
            &self.from
        }

        fn to_version(&self) -> &str {
            &self.to
        }

        fn description(&self) -> &str {
            &self.description
        }

        fn migrate(&self, data: Value) -> Result<MigrationResult, MigrationError> {
            // Simple test migration: just pass through data
            Ok(MigrationResult {
                data,
                warnings: vec![],
            })
        }
    }

    #[test]
    fn test_registry_empty_path() {
        let registry = MigrationRegistry::new();
        let path = registry.find_path("1.0.0", "1.0.0");
        assert!(path.is_some());
        assert_eq!(path.unwrap().len(), 0);
    }

    #[test]
    fn test_registry_no_path() {
        let registry = MigrationRegistry::new();
        let path = registry.find_path("1.0.0", "2.0.0");
        assert!(path.is_none());
    }

    #[test]
    fn test_registry_single_migration() {
        let mut registry = MigrationRegistry::new();
        registry.register(Box::new(TestMigration {
            from: "1.0.0".to_string(),
            to: "1.1.0".to_string(),
            description: "Test migration".to_string(),
        }));

        let path = registry.find_path("1.0.0", "1.1.0");
        assert!(path.is_some());
        let path = path.unwrap();
        assert_eq!(path.len(), 1);
        assert_eq!(path[0].from_version(), "1.0.0");
        assert_eq!(path[0].to_version(), "1.1.0");
    }

    #[test]
    fn test_registry_multi_step_migration() {
        let mut registry = MigrationRegistry::new();
        registry.register(Box::new(TestMigration {
            from: "1.0.0".to_string(),
            to: "1.1.0".to_string(),
            description: "1.0 to 1.1".to_string(),
        }));
        registry.register(Box::new(TestMigration {
            from: "1.1.0".to_string(),
            to: "1.2.0".to_string(),
            description: "1.1 to 1.2".to_string(),
        }));

        let path = registry.find_path("1.0.0", "1.2.0");
        assert!(path.is_some());
        let path = path.unwrap();
        assert_eq!(path.len(), 2);
        assert_eq!(path[0].from_version(), "1.0.0");
        assert_eq!(path[0].to_version(), "1.1.0");
        assert_eq!(path[1].from_version(), "1.1.0");
        assert_eq!(path[1].to_version(), "1.2.0");
    }

    #[test]
    fn test_registry_no_circular() {
        let mut registry = MigrationRegistry::new();
        registry.register(Box::new(TestMigration {
            from: "1.0.0".to_string(),
            to: "1.1.0".to_string(),
            description: "Forward".to_string(),
        }));
        registry.register(Box::new(TestMigration {
            from: "1.1.0".to_string(),
            to: "1.0.0".to_string(),
            description: "Backward".to_string(),
        }));

        // Should find a path (just one step), not loop forever
        let path = registry.find_path("1.0.0", "1.1.0");
        assert!(path.is_some());
        assert_eq!(path.unwrap().len(), 1);
    }

    #[test]
    fn test_migrate_success() {
        let mut registry = MigrationRegistry::new();
        registry.register(Box::new(TestMigration {
            from: "1.0.0".to_string(),
            to: "1.1.0".to_string(),
            description: "Test".to_string(),
        }));

        let data = serde_json::json!({"test": "data"});
        let result = registry.migrate(data.clone(), "1.0.0", "1.1.0");
        assert!(result.is_ok());
        let result = result.unwrap();
        assert_eq!(result.data, data);
        assert_eq!(result.warnings.len(), 0);
    }

    #[test]
    fn test_migrate_no_path() {
        let registry = MigrationRegistry::new();
        let data = serde_json::json!({"test": "data"});
        let result = registry.migrate(data, "1.0.0", "2.0.0");
        assert!(result.is_err());
        match result {
            Err(MigrationError::NoMigrationPath { from, to }) => {
                assert_eq!(from, "1.0.0");
                assert_eq!(to, "2.0.0");
            }
            _ => panic!("Expected NoMigrationPath error"),
        }
    }
}
