//! PKE keyset management repository (Issues #328, #332).
//!
//! Provides CRUD operations for managing named PKE keysets with encrypted private keys.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};
use uuid::Uuid;

use matric_core::{Error, Result};

/// A PKE keyset record.
#[derive(Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PkeKeyset {
    pub id: Uuid,
    pub name: String,
    #[serde(skip_serializing)]
    pub public_key: Vec<u8>,
    #[serde(skip_serializing)]
    pub encrypted_private_key: Vec<u8>,
    pub address: String,
    pub label: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl std::fmt::Debug for PkeKeyset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PkeKeyset")
            .field("id_set", &true)
            .field("name_len", &self.name.chars().count())
            .field("public_key_len", &self.public_key.len())
            .field(
                "encrypted_private_key_len",
                &self.encrypted_private_key.len(),
            )
            .field("address_len", &self.address.chars().count())
            .field("label_len", &optional_text_len(&self.label))
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .finish()
    }
}

/// Summary view of a keyset (without key material).
#[derive(Clone, Serialize, Deserialize)]
pub struct PkeKeysetSummary {
    pub id: Uuid,
    pub name: String,
    pub address: String,
    pub label: Option<String>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl std::fmt::Debug for PkeKeysetSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PkeKeysetSummary")
            .field("id_set", &true)
            .field("name_len", &self.name.chars().count())
            .field("address_len", &self.address.chars().count())
            .field("label_len", &optional_text_len(&self.label))
            .field("is_active", &self.is_active)
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .finish()
    }
}

/// Request to create a new keyset.
#[derive(Clone, Deserialize)]
pub struct CreateKeysetRequest {
    pub name: String,
    pub public_key: Vec<u8>,
    pub encrypted_private_key: Vec<u8>,
    pub address: String,
    pub label: Option<String>,
}

impl std::fmt::Debug for CreateKeysetRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CreateKeysetRequest")
            .field("name_len", &self.name.chars().count())
            .field("public_key_len", &self.public_key.len())
            .field(
                "encrypted_private_key_len",
                &self.encrypted_private_key.len(),
            )
            .field("address_len", &self.address.chars().count())
            .field("label_len", &optional_text_len(&self.label))
            .finish()
    }
}

/// Exported keyset data.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ExportedKeyset {
    pub name: String,
    pub public_key_base64: String,
    pub encrypted_private_key_base64: String,
    pub address: String,
    pub label: Option<String>,
    pub exported_at: DateTime<Utc>,
}

impl std::fmt::Debug for ExportedKeyset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExportedKeyset")
            .field("name_len", &self.name.chars().count())
            .field(
                "public_key_base64_len",
                &self.public_key_base64.chars().count(),
            )
            .field(
                "encrypted_private_key_base64_len",
                &self.encrypted_private_key_base64.chars().count(),
            )
            .field("address_len", &self.address.chars().count())
            .field("label_len", &optional_text_len(&self.label))
            .field("exported_at", &self.exported_at)
            .finish()
    }
}

fn optional_text_len(value: &Option<String>) -> Option<usize> {
    value.as_deref().map(|value| value.chars().count())
}

fn pke_keyset_text_len(value: &str) -> usize {
    value.chars().count()
}

fn pke_keyset_decode_error(field: &'static str, err: impl std::fmt::Display) -> Error {
    let diagnostic = err.to_string();
    Error::InvalidInput(format!(
        "Invalid {field}; diagnostic_class={}; diagnostic_len={}",
        pke_keyset_diagnostic_class(&diagnostic),
        pke_keyset_text_len(&diagnostic)
    ))
}

fn pke_keyset_diagnostic_class(value: &str) -> &'static str {
    let lower = value.to_ascii_lowercase();
    if value.is_empty() {
        "empty"
    } else if value.chars().any(char::is_control) {
        "control_chars"
    } else if lower.contains("secret")
        || lower.contains("token")
        || lower.contains("password")
        || lower.contains("apikey")
        || lower.contains("api_key")
        || lower.contains("sk-")
    {
        "secret_candidate"
    } else if lower.contains("://") || lower.starts_with("http") {
        "url_like"
    } else if value.contains('/') || value.contains('\\') {
        "path_like"
    } else {
        "text"
    }
}

/// PostgreSQL implementation of PKE keyset repository.
pub struct PgPkeKeysetRepository {
    pool: Pool<Postgres>,
}

impl PgPkeKeysetRepository {
    /// Create a new PgPkeKeysetRepository with the given connection pool.
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }

    /// Create a new keyset.
    ///
    /// # Arguments
    ///
    /// * `req` - Keyset creation request
    ///
    /// # Returns
    ///
    /// Returns the created keyset on success.
    pub async fn create(&self, req: CreateKeysetRequest) -> Result<PkeKeyset> {
        let keyset = sqlx::query_as::<_, PkeKeyset>(
            r#"
            INSERT INTO pke_keysets (name, public_key, encrypted_private_key, address, label)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id, name, public_key, encrypted_private_key, address, label, created_at, updated_at
            "#,
        )
        .bind(&req.name)
        .bind(&req.public_key)
        .bind(&req.encrypted_private_key)
        .bind(&req.address)
        .bind(&req.label)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(ref db_err) = e {
                if db_err.constraint() == Some("pke_keysets_name_key") {
                    return Error::InvalidInput(format!(
                        "Keyset already exists; name_len={}",
                        pke_keyset_text_len(&req.name)
                    ));
                }
            }
            Error::Database(e)
        })?;

        Ok(keyset)
    }

    /// Get a keyset by ID.
    pub async fn get_by_id(&self, id: Uuid) -> Result<Option<PkeKeyset>> {
        let keyset = sqlx::query_as::<_, PkeKeyset>(
            r#"
            SELECT id, name, public_key, encrypted_private_key, address, label, created_at, updated_at
            FROM pke_keysets
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(keyset)
    }

    /// Get a keyset by name.
    pub async fn get_by_name(&self, name: &str) -> Result<Option<PkeKeyset>> {
        let keyset = sqlx::query_as::<_, PkeKeyset>(
            r#"
            SELECT id, name, public_key, encrypted_private_key, address, label, created_at, updated_at
            FROM pke_keysets
            WHERE name = $1
            "#,
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(keyset)
    }

    /// List all keysets with active status.
    pub async fn list(&self) -> Result<Vec<PkeKeysetSummary>> {
        let keysets = sqlx::query_as::<
            _,
            (
                Uuid,
                String,
                String,
                Option<String>,
                DateTime<Utc>,
                DateTime<Utc>,
                Option<Uuid>,
            ),
        >(
            r#"
            SELECT k.id, k.name, k.address, k.label, k.created_at, k.updated_at, a.keyset_id
            FROM pke_keysets k
            LEFT JOIN pke_active_keyset a ON a.id = 1
            ORDER BY k.created_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        let summaries = keysets
            .into_iter()
            .map(
                |(id, name, address, label, created_at, updated_at, active_id)| PkeKeysetSummary {
                    id,
                    name,
                    address,
                    label,
                    is_active: active_id == Some(id),
                    created_at,
                    updated_at,
                },
            )
            .collect();

        Ok(summaries)
    }

    /// Get the active keyset.
    pub async fn get_active(&self) -> Result<Option<PkeKeyset>> {
        let keyset = sqlx::query_as::<_, PkeKeyset>(
            r#"
            SELECT k.id, k.name, k.public_key, k.encrypted_private_key, k.address, k.label, k.created_at, k.updated_at
            FROM pke_keysets k
            INNER JOIN pke_active_keyset a ON a.keyset_id = k.id
            WHERE a.id = 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(keyset)
    }

    /// Set the active keyset by ID.
    pub async fn set_active(&self, keyset_id: Uuid) -> Result<()> {
        // Verify keyset exists
        let exists = sqlx::query_scalar::<_, bool>(
            r#"SELECT EXISTS(SELECT 1 FROM pke_keysets WHERE id = $1)"#,
        )
        .bind(keyset_id)
        .fetch_one(&self.pool)
        .await
        .map_err(Error::Database)?;

        if !exists {
            return Err(Error::NotFound(format!(
                "Keyset with id '{}' not found",
                keyset_id
            )));
        }

        sqlx::query(
            r#"
            UPDATE pke_active_keyset
            SET keyset_id = $1, updated_at = NOW()
            WHERE id = 1
            "#,
        )
        .bind(keyset_id)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(())
    }

    /// Set the active keyset by name.
    pub async fn set_active_by_name(&self, name: &str) -> Result<()> {
        let keyset = self
            .get_by_name(name)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Keyset '{}' not found", name)))?;

        self.set_active(keyset.id).await
    }

    /// Delete a keyset by ID.
    pub async fn delete(&self, id: Uuid) -> Result<bool> {
        // Clear active if this is the active keyset
        sqlx::query(
            r#"
            UPDATE pke_active_keyset
            SET keyset_id = NULL, updated_at = NOW()
            WHERE keyset_id = $1
            "#,
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        let result = sqlx::query(r#"DELETE FROM pke_keysets WHERE id = $1"#)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;

        Ok(result.rows_affected() > 0)
    }

    /// Delete a keyset by name.
    pub async fn delete_by_name(&self, name: &str) -> Result<bool> {
        if let Some(keyset) = self.get_by_name(name).await? {
            self.delete(keyset.id).await
        } else {
            Ok(false)
        }
    }

    /// Export a keyset.
    pub async fn export(&self, id: Uuid) -> Result<Option<ExportedKeyset>> {
        let keyset = self.get_by_id(id).await?;

        Ok(keyset.map(|k| ExportedKeyset {
            name: k.name,
            public_key_base64: base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                &k.public_key,
            ),
            encrypted_private_key_base64: base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                &k.encrypted_private_key,
            ),
            address: k.address,
            label: k.label,
            exported_at: Utc::now(),
        }))
    }

    /// Import a keyset.
    pub async fn import(&self, name: String, exported: ExportedKeyset) -> Result<PkeKeyset> {
        let public_key = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            &exported.public_key_base64,
        )
        .map_err(|e| pke_keyset_decode_error("public_key_base64", e))?;

        let encrypted_private_key = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            &exported.encrypted_private_key_base64,
        )
        .map_err(|e| pke_keyset_decode_error("encrypted_private_key_base64", e))?;

        self.create(CreateKeysetRequest {
            name,
            public_key,
            encrypted_private_key,
            address: exported.address,
            label: exported.label,
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pool::create_pool;

    #[test]
    fn pke_keyset_debug_redacts_key_material() {
        let now = Utc::now();
        let keyset = PkeKeyset {
            id: Uuid::parse_str("018fd1a0-0000-7000-8000-000000000301").unwrap(),
            name: "tenant-secret-keyset".to_string(),
            public_key: b"public-key-material".to_vec(),
            encrypted_private_key: b"encrypted-private-key-secret".to_vec(),
            address: "mm:example-address-secret-fragment".to_string(),
            label: Some("private label".to_string()),
            created_at: now,
            updated_at: now,
        };
        let summary = PkeKeysetSummary {
            id: Uuid::parse_str("018fd1a0-0000-7000-8000-000000000302").unwrap(),
            name: "tenant-secret-keyset".to_string(),
            address: "mm:example-address-secret-fragment".to_string(),
            label: Some("private label".to_string()),
            is_active: true,
            created_at: now,
            updated_at: now,
        };
        let create = CreateKeysetRequest {
            name: "tenant-secret-keyset".to_string(),
            public_key: b"public-key-material".to_vec(),
            encrypted_private_key: b"encrypted-private-key-secret".to_vec(),
            address: "mm:example-address-secret-fragment".to_string(),
            label: Some("private label".to_string()),
        };
        let exported = ExportedKeyset {
            name: "tenant-secret-keyset".to_string(),
            public_key_base64: "public-key-material".to_string(),
            encrypted_private_key_base64: "encrypted-private-key-secret".to_string(),
            address: "mm:example-address-secret-fragment".to_string(),
            label: Some("private label".to_string()),
            exported_at: now,
        };

        let rendered = format!("{keyset:?}\n{summary:?}\n{create:?}\n{exported:?}");
        assert!(rendered.contains("id_set"));
        assert!(rendered.contains("public_key_len"));
        assert!(rendered.contains("encrypted_private_key_len"));
        assert!(rendered.contains("encrypted_private_key_base64_len"));
        assert!(rendered.contains("address_len"));
        assert!(!rendered.contains("018fd1a0-0000-7000-8000-000000000301"));
        assert!(!rendered.contains("018fd1a0-0000-7000-8000-000000000302"));
        assert!(!rendered.contains("tenant-secret-keyset"));
        assert!(!rendered.contains("public-key-material"));
        assert!(!rendered.contains("encrypted-private-key-secret"));
        assert!(!rendered.contains("example-address-secret-fragment"));
        assert!(!rendered.contains("private label"));
    }

    #[test]
    fn pke_keyset_decode_errors_report_classes_without_raw_diagnostics() {
        let diagnostic =
            "invalid token sk-live-secret from postgres://user:pass@db.internal/app at byte 3";
        let error = pke_keyset_decode_error("encrypted_private_key_base64", diagnostic);
        let message = match error {
            Error::InvalidInput(message) => message,
            other => panic!("unexpected error: {other:?}"),
        };

        assert!(message.contains("Invalid encrypted_private_key_base64"));
        assert!(message.contains("diagnostic_class=secret_candidate"));
        assert!(message.contains(&format!("diagnostic_len={}", diagnostic.chars().count())));
        for raw in [
            "sk-live-secret",
            "postgres://user:pass",
            "db.internal",
            "token ",
        ] {
            assert!(!message.contains(raw), "raw diagnostic leaked: {raw}");
        }
    }

    /// Returns pool if integration test DB is available, None to skip.
    /// These tests require a migrated database with the pke_keysets table.
    /// Set INTEGRATION_TEST_DB=1 when running with a migrated database.
    async fn setup_test_pool() -> Option<Pool<Postgres>> {
        // Skip if INTEGRATION_TEST_DB is not explicitly set.
        // This prevents failures when running locally without migrations.
        // CI sets this env var after running migrations.
        if std::env::var("INTEGRATION_TEST_DB").is_err() {
            eprintln!(
                "Skipping pke_keysets test: INTEGRATION_TEST_DB not set. \
                 These tests require a migrated database. Set INTEGRATION_TEST_DB=1 to run."
            );
            return None;
        }

        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://matric:matric@localhost/matric".to_string());
        // Note: Migrations are run by the CI workflow before tests.
        // Do NOT run migrations here - parallel tests cause race conditions.
        // See issues #349, #352.
        Some(
            create_pool(&database_url)
                .await
                .expect("Failed to create pool"),
        )
    }

    #[tokio::test]
    async fn test_create_and_get_keyset() {
        let Some(pool) = setup_test_pool().await else {
            return;
        };
        let repo = PgPkeKeysetRepository::new(pool);

        let test_id = Uuid::new_v4().to_string();
        let name = format!("test-keyset-{}", test_id);

        // Create keyset
        let keyset = repo
            .create(CreateKeysetRequest {
                name: name.clone(),
                public_key: vec![1, 2, 3, 4, 5],
                encrypted_private_key: vec![6, 7, 8, 9, 10],
                address: format!("mm:test-{}", test_id),
                label: Some("Test Keyset".to_string()),
            })
            .await
            .expect("Failed to create keyset");

        assert_eq!(keyset.name, name);
        assert_eq!(keyset.public_key, vec![1, 2, 3, 4, 5]);

        // Get by ID
        let result = repo
            .get_by_id(keyset.id)
            .await
            .expect("Failed to get keyset by ID");
        assert!(result.is_some());

        // Get by name
        let result = repo
            .get_by_name(&name)
            .await
            .expect("Failed to get keyset by name");
        assert!(result.is_some());

        // Cleanup
        repo.delete(keyset.id)
            .await
            .expect("Failed to delete keyset");
    }

    #[tokio::test]
    async fn test_list_keysets() {
        let Some(pool) = setup_test_pool().await else {
            return;
        };
        let repo = PgPkeKeysetRepository::new(pool);

        let test_id = Uuid::new_v4().to_string();
        let name = format!("test-list-{}", test_id);

        // Create keyset
        let keyset = repo
            .create(CreateKeysetRequest {
                name: name.clone(),
                public_key: vec![1, 2, 3],
                encrypted_private_key: vec![4, 5, 6],
                address: format!("mm:list-{}", test_id),
                label: None,
            })
            .await
            .expect("Failed to create keyset");

        // List keysets
        let keysets = repo.list().await.expect("Failed to list keysets");
        let our_keyset = keysets.iter().find(|k| k.name == name);
        assert!(our_keyset.is_some());
        assert!(!our_keyset.unwrap().is_active);

        // Cleanup
        repo.delete(keyset.id)
            .await
            .expect("Failed to delete keyset");
    }

    #[tokio::test]
    async fn test_active_keyset() {
        let Some(pool) = setup_test_pool().await else {
            return;
        };
        let repo = PgPkeKeysetRepository::new(pool);

        let test_id = Uuid::new_v4().to_string();
        let name = format!("test-active-{}", test_id);

        // Create keyset
        let keyset = repo
            .create(CreateKeysetRequest {
                name: name.clone(),
                public_key: vec![1, 2, 3],
                encrypted_private_key: vec![4, 5, 6],
                address: format!("mm:active-{}", test_id),
                label: None,
            })
            .await
            .expect("Failed to create keyset");

        // Initially no active keyset (or a different one)
        // Set this keyset as active
        repo.set_active(keyset.id)
            .await
            .expect("Failed to set active keyset");

        // Get active keyset
        let active = repo
            .get_active()
            .await
            .expect("Failed to get active keyset");
        assert!(active.is_some());
        assert_eq!(active.unwrap().id, keyset.id);

        // Cleanup (also clears active)
        repo.delete(keyset.id)
            .await
            .expect("Failed to delete keyset");

        // Verify active is now None
        let active = repo
            .get_active()
            .await
            .expect("Failed to get active keyset");
        assert!(active.is_none());
    }

    #[tokio::test]
    async fn test_export_import_keyset() {
        let Some(pool) = setup_test_pool().await else {
            return;
        };
        let repo = PgPkeKeysetRepository::new(pool);

        let test_id = Uuid::new_v4().to_string();
        let name = format!("test-export-{}", test_id);

        // Create keyset
        let keyset = repo
            .create(CreateKeysetRequest {
                name: name.clone(),
                public_key: vec![1, 2, 3],
                encrypted_private_key: vec![4, 5, 6],
                address: format!("mm:export-{}", test_id),
                label: Some("Export Test".to_string()),
            })
            .await
            .expect("Failed to create keyset");

        // Export
        let exported = repo
            .export(keyset.id)
            .await
            .expect("Failed to export keyset")
            .expect("Keyset not found");

        assert_eq!(exported.name, name);
        assert_eq!(exported.address, format!("mm:export-{}", test_id));

        // Import with new name
        let import_name = format!("test-import-{}", test_id);
        let imported = repo
            .import(import_name.clone(), exported)
            .await
            .expect("Failed to import keyset");

        assert_eq!(imported.name, import_name);
        assert_eq!(imported.public_key, vec![1, 2, 3]);

        // Cleanup
        repo.delete(keyset.id)
            .await
            .expect("Failed to delete original keyset");
        repo.delete(imported.id)
            .await
            .expect("Failed to delete imported keyset");
    }

    #[tokio::test]
    async fn test_duplicate_name_error() {
        let Some(pool) = setup_test_pool().await else {
            return;
        };
        let repo = PgPkeKeysetRepository::new(pool);

        let test_id = Uuid::new_v4().to_string();
        let name = format!("test-dup-{}", test_id);

        // Create keyset
        let keyset = repo
            .create(CreateKeysetRequest {
                name: name.clone(),
                public_key: vec![1, 2, 3],
                encrypted_private_key: vec![4, 5, 6],
                address: format!("mm:dup1-{}", test_id),
                label: None,
            })
            .await
            .expect("Failed to create keyset");

        // Try to create duplicate
        let result = repo
            .create(CreateKeysetRequest {
                name: name.clone(),
                public_key: vec![7, 8, 9],
                encrypted_private_key: vec![10, 11, 12],
                address: format!("mm:dup2-{}", test_id),
                label: None,
            })
            .await;

        assert!(result.is_err());
        if let Err(Error::InvalidInput(msg)) = result {
            assert!(msg.contains("already exists"));
        } else {
            panic!("Expected InvalidInput error");
        }

        // Cleanup
        repo.delete(keyset.id)
            .await
            .expect("Failed to delete keyset");
    }
}
