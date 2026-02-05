//! Integration test for code embedding configurations (Issue #394).
//!
//! This test verifies that:
//! 1. Code-specialized embedding configs are created correctly
//! 2. Content types are properly set
//! 3. Document types reference appropriate embedding configs

use sqlx::PgPool;

/// Helper to get database connection from environment.
async fn get_test_pool() -> PgPool {
    dotenvy::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://matric:matric@localhost/matric".to_string());

    PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to test database")
}

/// Test that code embedding configurations exist with correct properties.
#[tokio::test]
#[ignore] // Requires database connection with migrations applied
async fn test_code_embedding_configs_exist() {
    let pool = get_test_pool().await;

    // Define expected configs and their properties
    let expected_configs = vec![
        (
            "code-search",
            "ollama",
            "nomic-embed-text",
            768,
            vec!["code", "technical"],
        ),
        (
            "code-bge-small",
            "ollama",
            "bge-small-en",
            384,
            vec!["code"],
        ),
        (
            "code-docs",
            "ollama",
            "nomic-embed-text",
            768,
            vec!["code", "prose", "technical"],
        ),
        (
            "api-schema",
            "ollama",
            "nomic-embed-text",
            768,
            vec!["api-spec", "database", "config"],
        ),
    ];

    for (name, provider, model, dimension, content_types) in expected_configs {
        let result: Option<(String, String, i32, Vec<String>)> = sqlx::query_as(
            "SELECT provider::text, model, dimension, content_types
             FROM embedding_config
             WHERE name = $1",
        )
        .bind(name)
        .fetch_optional(&pool)
        .await
        .unwrap_or_else(|_| panic!("Failed to query config: {}", name));

        assert!(
            result.is_some(),
            "Config '{}' should exist in database",
            name
        );

        let (db_provider, db_model, db_dimension, db_content_types) = result.unwrap();

        assert_eq!(
            db_provider, provider,
            "Config '{}' should have provider '{}'",
            name, provider
        );

        assert_eq!(
            db_model, model,
            "Config '{}' should have model '{}'",
            name, model
        );

        assert_eq!(
            db_dimension, dimension,
            "Config '{}' should have dimension {}",
            name, dimension
        );

        // Verify content types
        assert_eq!(
            db_content_types.len(),
            content_types.len(),
            "Config '{}' should have {} content types",
            name,
            content_types.len()
        );

        for ct in &content_types {
            assert!(
                db_content_types.contains(&ct.to_string()),
                "Config '{}' should include content type '{}'",
                name,
                ct
            );
        }
    }
}

/// Test that code embedding configs are not set as default.
#[tokio::test]
#[ignore] // Requires database connection
async fn test_code_configs_not_default() {
    let pool = get_test_pool().await;

    let result: Vec<(String, bool)> = sqlx::query_as(
        "SELECT name, is_default
         FROM embedding_config
         WHERE name IN ('code-search', 'code-bge-small', 'code-docs', 'api-schema')",
    )
    .fetch_all(&pool)
    .await
    .expect("Failed to query configs");

    assert!(
        !result.is_empty(),
        "Should have code embedding configs in database"
    );

    for (name, is_default) in result {
        assert!(
            !is_default,
            "Config '{}' should not be set as default",
            name
        );
    }
}

/// Test that document types are linked to appropriate embedding configs.
#[tokio::test]
#[ignore] // Requires database connection
async fn test_document_type_config_links() {
    let pool = get_test_pool().await;

    // Test code category links to code-search
    let code_types: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT dt.name, ec.name as config_name
         FROM document_type dt
         LEFT JOIN embedding_config ec ON dt.recommended_config_id = ec.id
         WHERE dt.category = 'code' AND dt.is_system = TRUE",
    )
    .fetch_all(&pool)
    .await
    .expect("Failed to query code document types");

    assert!(
        !code_types.is_empty(),
        "Should have system code document types"
    );

    for (type_name, config_name) in code_types {
        assert!(
            config_name.is_some(),
            "Code type '{}' should have recommended config",
            type_name
        );
        assert_eq!(
            config_name.unwrap(),
            "code-search",
            "Code type '{}' should recommend code-search config",
            type_name
        );
    }

    // Test API spec category links to api-schema
    let api_types: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT dt.name, ec.name as config_name
         FROM document_type dt
         LEFT JOIN embedding_config ec ON dt.recommended_config_id = ec.id
         WHERE dt.category = 'api-spec' AND dt.is_system = TRUE",
    )
    .fetch_all(&pool)
    .await
    .expect("Failed to query api-spec document types");

    for (type_name, config_name) in api_types {
        assert!(
            config_name.is_some(),
            "API spec type '{}' should have recommended config",
            type_name
        );
        assert_eq!(
            config_name.unwrap(),
            "api-schema",
            "API spec type '{}' should recommend api-schema config",
            type_name
        );
    }

    // Test database category links to api-schema
    let db_types: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT dt.name, ec.name as config_name
         FROM document_type dt
         LEFT JOIN embedding_config ec ON dt.recommended_config_id = ec.id
         WHERE dt.category = 'database' AND dt.is_system = TRUE",
    )
    .fetch_all(&pool)
    .await
    .expect("Failed to query database document types");

    for (type_name, config_name) in db_types {
        assert!(
            config_name.is_some(),
            "Database type '{}' should have recommended config",
            type_name
        );
        assert_eq!(
            config_name.unwrap(),
            "api-schema",
            "Database type '{}' should recommend api-schema config",
            type_name
        );
    }

    // Test IaC category links to code-docs
    let iac_types: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT dt.name, ec.name as config_name
         FROM document_type dt
         LEFT JOIN embedding_config ec ON dt.recommended_config_id = ec.id
         WHERE dt.category = 'iac' AND dt.is_system = TRUE",
    )
    .fetch_all(&pool)
    .await
    .expect("Failed to query iac document types");

    for (type_name, config_name) in iac_types {
        assert!(
            config_name.is_some(),
            "IaC type '{}' should have recommended config",
            type_name
        );
        assert_eq!(
            config_name.unwrap(),
            "code-docs",
            "IaC type '{}' should recommend code-docs config",
            type_name
        );
    }
}

/// Test that chunk sizes are appropriate for code.
#[tokio::test]
#[ignore] // Requires database connection
async fn test_code_config_chunk_sizes() {
    let pool = get_test_pool().await;

    let result: Vec<(String, i32, i32)> = sqlx::query_as(
        "SELECT name, chunk_size, chunk_overlap
         FROM embedding_config
         WHERE name IN ('code-search', 'code-bge-small', 'code-docs', 'api-schema')",
    )
    .fetch_all(&pool)
    .await
    .expect("Failed to query config chunk sizes");

    for (name, chunk_size, chunk_overlap) in result {
        // Verify chunk sizes are reasonable for code
        match name.as_str() {
            "code-search" | "code-bge-small" => {
                assert_eq!(
                    chunk_size, 512,
                    "Config '{}' should use 512 token chunks for function-level granularity",
                    name
                );
                assert_eq!(
                    chunk_overlap, 50,
                    "Config '{}' should use 50 token overlap for syntactic boundaries",
                    name
                );
            }
            "code-docs" | "api-schema" => {
                assert!(
                    chunk_size >= 1000,
                    "Config '{}' should use larger chunks (>=1000) to preserve context",
                    name
                );
                assert!(
                    chunk_overlap >= 100,
                    "Config '{}' should use larger overlap (>=100) for context",
                    name
                );
            }
            _ => {}
        }

        // Verify overlap is smaller than chunk size
        assert!(
            chunk_overlap < chunk_size,
            "Config '{}' overlap ({}) should be less than chunk size ({})",
            name,
            chunk_overlap,
            chunk_size
        );
    }
}

/// Test that content_types field supports GIN index queries.
#[tokio::test]
#[ignore] // Requires database connection
async fn test_content_types_gin_index() {
    let pool = get_test_pool().await;

    // Query configs by content type using array operator
    let code_configs: Vec<String> = sqlx::query_scalar(
        "SELECT name
         FROM embedding_config
         WHERE 'code' = ANY(content_types)
         ORDER BY name",
    )
    .fetch_all(&pool)
    .await
    .expect("Failed to query configs by content type");

    assert!(
        !code_configs.is_empty(),
        "Should find configs with 'code' content type"
    );

    assert!(
        code_configs.contains(&"code-search".to_string()),
        "code-search should be in results"
    );
    assert!(
        code_configs.contains(&"code-bge-small".to_string()),
        "code-bge-small should be in results"
    );
    assert!(
        code_configs.contains(&"code-docs".to_string()),
        "code-docs should be in results"
    );
}
