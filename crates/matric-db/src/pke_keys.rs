//! PKE public key registry repository (Issue #113).
//!
//! Provides CRUD operations for managing PKE public keys indexed by address.

use chrono::{DateTime, Utc};
use sqlx::{Pool, Postgres};

use matric_core::{Error, Result};

/// A PKE public key record from the registry.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PkePublicKey {
    pub address: String,
    pub public_key: Vec<u8>,
    pub label: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// PostgreSQL implementation of PKE key registry.
pub struct PgPkeKeyRepository {
    pool: Pool<Postgres>,
}

impl PgPkeKeyRepository {
    /// Create a new PgPkeKeyRepository with the given connection pool.
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }

    /// Register a new PKE public key or update an existing one.
    ///
    /// # Arguments
    ///
    /// * `address` - PKE address (unique identifier)
    /// * `public_key` - Raw public key bytes
    /// * `label` - Optional human-readable label
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success, or an error if the operation fails.
    pub async fn register_key(
        &self,
        address: String,
        public_key: Vec<u8>,
        label: Option<String>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO pke_public_keys (address, public_key, label)
            VALUES ($1, $2, $3)
            ON CONFLICT (address) DO UPDATE
            SET public_key = EXCLUDED.public_key,
                label = EXCLUDED.label,
                updated_at = NOW()
            "#,
        )
        .bind(&address)
        .bind(&public_key)
        .bind(&label)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(())
    }

    /// Get a PKE public key by address.
    ///
    /// # Arguments
    ///
    /// * `address` - PKE address to look up
    ///
    /// # Returns
    ///
    /// Returns `Some(PkePublicKey)` if found, `None` if not found.
    pub async fn get_key(&self, address: &str) -> Result<Option<PkePublicKey>> {
        let key = sqlx::query_as::<_, PkePublicKey>(
            r#"
            SELECT address, public_key, label, created_at, updated_at
            FROM pke_public_keys
            WHERE address = $1
            "#,
        )
        .bind(address)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(key)
    }

    /// List all PKE public keys in the registry.
    ///
    /// # Returns
    ///
    /// Returns a vector of all public keys, ordered by created_at descending.
    pub async fn list_keys(&self) -> Result<Vec<PkePublicKey>> {
        let keys = sqlx::query_as::<_, PkePublicKey>(
            r#"
            SELECT address, public_key, label, created_at, updated_at
            FROM pke_public_keys
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(keys)
    }

    /// Delete a PKE public key by address.
    ///
    /// # Arguments
    ///
    /// * `address` - PKE address to delete
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success. Does not error if the key doesn't exist.
    pub async fn delete_key(&self, address: &str) -> Result<()> {
        sqlx::query(
            r#"
            DELETE FROM pke_public_keys
            WHERE address = $1
            "#,
        )
        .bind(address)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pool::create_pool;

    async fn setup_test_pool() -> Pool<Postgres> {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://matric:matric@localhost/matric".to_string());
        create_pool(&database_url)
            .await
            .expect("Failed to create pool")
    }

    #[tokio::test]
    async fn test_register_and_get_key() {
        let pool = setup_test_pool().await;
        let repo = PgPkeKeyRepository::new(pool);

        let test_id = format!(
            "test-addr-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or(0)
        );
        let address = test_id.clone();
        let public_key = vec![1, 2, 3, 4, 5];
        let label = Some("Test Key".to_string());

        // Register key
        repo.register_key(address.clone(), public_key.clone(), label.clone())
            .await
            .expect("Failed to register key");

        // Get key
        let result = repo.get_key(&address).await.expect("Failed to get key");
        assert!(result.is_some());

        let key = result.unwrap();
        assert_eq!(key.address, address);
        assert_eq!(key.public_key, public_key);
        assert_eq!(key.label, label);

        // Cleanup
        repo.delete_key(&address)
            .await
            .expect("Failed to delete key");
    }

    #[tokio::test]
    async fn test_register_key_updates_existing() {
        let pool = setup_test_pool().await;
        let repo = PgPkeKeyRepository::new(pool);

        let test_id = format!(
            "test-addr-update-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or(0)
        );
        let address = test_id.clone();

        // Register initial key
        repo.register_key(address.clone(), vec![1, 2, 3], Some("Original".to_string()))
            .await
            .expect("Failed to register initial key");

        // Update with new key
        repo.register_key(address.clone(), vec![4, 5, 6], Some("Updated".to_string()))
            .await
            .expect("Failed to update key");

        // Verify updated
        let result = repo.get_key(&address).await.expect("Failed to get key");
        assert!(result.is_some());

        let key = result.unwrap();
        assert_eq!(key.public_key, vec![4, 5, 6]);
        assert_eq!(key.label, Some("Updated".to_string()));

        // Cleanup
        repo.delete_key(&address)
            .await
            .expect("Failed to delete key");
    }

    #[tokio::test]
    async fn test_get_key_not_found() {
        let pool = setup_test_pool().await;
        let repo = PgPkeKeyRepository::new(pool);

        let result = repo
            .get_key("nonexistent-address")
            .await
            .expect("Failed to get key");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_list_keys() {
        let pool = setup_test_pool().await;
        let repo = PgPkeKeyRepository::new(pool);

        let test_id = Utc::now().timestamp_nanos_opt().unwrap_or(0);
        let addr1 = format!("test-list-1-{}", test_id);
        let addr2 = format!("test-list-2-{}", test_id);

        // Register two keys
        repo.register_key(addr1.clone(), vec![1, 2], Some("Key 1".to_string()))
            .await
            .expect("Failed to register key 1");
        repo.register_key(addr2.clone(), vec![3, 4], Some("Key 2".to_string()))
            .await
            .expect("Failed to register key 2");

        // List all keys
        let keys = repo.list_keys().await.expect("Failed to list keys");

        // Should contain our keys
        let our_keys: Vec<_> = keys
            .iter()
            .filter(|k| k.address == addr1 || k.address == addr2)
            .collect();
        assert_eq!(our_keys.len(), 2);

        // Cleanup
        repo.delete_key(&addr1)
            .await
            .expect("Failed to delete key 1");
        repo.delete_key(&addr2)
            .await
            .expect("Failed to delete key 2");
    }

    #[tokio::test]
    async fn test_delete_key() {
        let pool = setup_test_pool().await;
        let repo = PgPkeKeyRepository::new(pool);

        let test_id = format!(
            "test-delete-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or(0)
        );
        let address = test_id.clone();

        // Register key
        repo.register_key(address.clone(), vec![1, 2, 3], None)
            .await
            .expect("Failed to register key");

        // Verify exists
        assert!(repo.get_key(&address).await.unwrap().is_some());

        // Delete
        repo.delete_key(&address)
            .await
            .expect("Failed to delete key");

        // Verify deleted
        assert!(repo.get_key(&address).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_delete_nonexistent_key() {
        let pool = setup_test_pool().await;
        let repo = PgPkeKeyRepository::new(pool);

        // Should not error
        repo.delete_key("nonexistent-address")
            .await
            .expect("Delete should not error on nonexistent key");
    }

    #[tokio::test]
    async fn test_register_key_without_label() {
        let pool = setup_test_pool().await;
        let repo = PgPkeKeyRepository::new(pool);

        let test_id = format!(
            "test-no-label-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or(0)
        );
        let address = test_id.clone();

        // Register without label
        repo.register_key(address.clone(), vec![1, 2, 3], None)
            .await
            .expect("Failed to register key without label");

        // Verify
        let key = repo.get_key(&address).await.unwrap().unwrap();
        assert_eq!(key.label, None);

        // Cleanup
        repo.delete_key(&address)
            .await
            .expect("Failed to delete key");
    }
}
