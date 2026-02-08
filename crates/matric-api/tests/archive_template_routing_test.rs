//! Integration tests for archive schema routing in template, versioning, and export handlers.
//!
//! Tests that template, versioning, and export operations correctly route to
//! archive-specific schemas and maintain data isolation between archives.

use matric_core::{
    ArchiveRepository, CreateNoteRequest, CreateTemplateRequest, TemplateRepository,
};
use matric_db::{Database, PgNoteRepository, PgTemplateRepository, VersioningRepository};
use uuid::Uuid;

fn database_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://matric:matric@localhost/matric".to_string())
}

/// Test helper to create a test archive.
async fn create_test_archive(db: &Database, name: &str) -> String {
    let archive = db
        .archives
        .create_archive_schema(name, Some("Test archive"))
        .await
        .expect("Failed to create test archive");
    archive.schema_name
}

/// Test helper to cleanup test archives.
async fn cleanup_archive(db: &Database, name: &str) {
    let _ = db.archives.drop_archive_schema(name).await;
}

// =============================================================================
// TEMPLATE TESTS
// =============================================================================

#[tokio::test]
async fn test_template_create_in_archive_schema() {
    let db = Database::connect(&database_url())
        .await
        .expect("Failed to connect to database");

    let archive_name = format!("tpl_c_{}", Uuid::now_v7());
    let schema = create_test_archive(&db, &archive_name).await;

    // Create template in archive schema
    let ctx = db
        .for_schema(&schema)
        .expect("Failed to create schema context");
    let templates = PgTemplateRepository::new(db.pool.clone());

    let req = CreateTemplateRequest {
        name: "Test Template".to_string(),
        description: Some("A test template".to_string()),
        content: "Template content with {{variable}}".to_string(),
        format: Some("markdown".to_string()),
        default_tags: Some(vec!["template".to_string()]),
        collection_id: None,
    };

    let template_id = ctx
        .execute(move |tx| Box::pin(async move { templates.create_tx(tx, req).await }))
        .await
        .expect("Failed to create template");

    // Verify template exists in archive schema
    let templates2 = PgTemplateRepository::new(db.pool.clone());
    let template = ctx
        .query(move |tx| Box::pin(async move { templates2.get_tx(tx, template_id).await }))
        .await
        .expect("Failed to fetch template")
        .expect("Template not found");

    assert_eq!(template.id, template_id);
    assert_eq!(template.name, "Test Template");
    assert_eq!(template.content, "Template content with {{variable}}");

    // Verify template does NOT exist in public schema
    let public_result = db.templates.get(template_id).await;
    assert!(
        public_result.unwrap().is_none(),
        "Template should not exist in public schema"
    );

    cleanup_archive(&db, &archive_name).await;
}

#[tokio::test]
async fn test_template_list_isolation() {
    let db = Database::connect(&database_url())
        .await
        .expect("Failed to connect to database");

    let archive1_name = format!("tpl_l1_{}", Uuid::now_v7());
    let archive2_name = format!("tpl_l2_{}", Uuid::now_v7());

    let schema1 = create_test_archive(&db, &archive1_name).await;
    let schema2 = create_test_archive(&db, &archive2_name).await;

    // Create template in archive1
    let ctx1 = db
        .for_schema(&schema1)
        .expect("Failed to create schema context");
    let templates1 = PgTemplateRepository::new(db.pool.clone());
    let req1 = CreateTemplateRequest {
        name: "Template in Archive 1".to_string(),
        description: None,
        content: "Content 1".to_string(),
        format: None,
        default_tags: None,
        collection_id: None,
    };

    ctx1.execute(move |tx| Box::pin(async move { templates1.create_tx(tx, req1).await }))
        .await
        .expect("Failed to create template in archive1");

    // Create template in archive2
    let ctx2 = db
        .for_schema(&schema2)
        .expect("Failed to create schema context");
    let templates2 = PgTemplateRepository::new(db.pool.clone());
    let req2 = CreateTemplateRequest {
        name: "Template in Archive 2".to_string(),
        description: None,
        content: "Content 2".to_string(),
        format: None,
        default_tags: None,
        collection_id: None,
    };

    ctx2.execute(move |tx| Box::pin(async move { templates2.create_tx(tx, req2).await }))
        .await
        .expect("Failed to create template in archive2");

    // List templates in archive1 - should only see template1
    let templates1_list = PgTemplateRepository::new(db.pool.clone());
    let list1 = ctx1
        .query(move |tx| Box::pin(async move { templates1_list.list_tx(tx).await }))
        .await
        .expect("Failed to list templates in archive1");

    assert_eq!(list1.len(), 1);
    assert_eq!(list1[0].name, "Template in Archive 1");

    // List templates in archive2 - should only see template2
    let templates2_list = PgTemplateRepository::new(db.pool.clone());
    let list2 = ctx2
        .query(move |tx| Box::pin(async move { templates2_list.list_tx(tx).await }))
        .await
        .expect("Failed to list templates in archive2");

    assert_eq!(list2.len(), 1);
    assert_eq!(list2[0].name, "Template in Archive 2");

    cleanup_archive(&db, &archive1_name).await;
    cleanup_archive(&db, &archive2_name).await;
}

#[tokio::test]
async fn test_template_update_in_archive_schema() {
    let db = Database::connect(&database_url())
        .await
        .expect("Failed to connect to database");

    let archive_name = format!("tpl_u_{}", Uuid::now_v7());
    let schema = create_test_archive(&db, &archive_name).await;

    // Create template
    let ctx = db
        .for_schema(&schema)
        .expect("Failed to create schema context");
    let templates1 = PgTemplateRepository::new(db.pool.clone());
    let req = CreateTemplateRequest {
        name: "Original Name".to_string(),
        description: None,
        content: "Original content".to_string(),
        format: None,
        default_tags: None,
        collection_id: None,
    };

    let template_id = ctx
        .execute(move |tx| Box::pin(async move { templates1.create_tx(tx, req).await }))
        .await
        .expect("Failed to create template");

    // Update template
    let templates2 = PgTemplateRepository::new(db.pool.clone());
    let update_req = matric_core::UpdateTemplateRequest {
        name: Some("Updated Name".to_string()),
        description: Some("Updated description".to_string()),
        content: Some("Updated content".to_string()),
        default_tags: None,
        collection_id: None,
    };

    ctx.execute(move |tx| {
        Box::pin(async move { templates2.update_tx(tx, template_id, update_req).await })
    })
    .await
    .expect("Failed to update template");

    // Verify update
    let templates3 = PgTemplateRepository::new(db.pool.clone());
    let updated = ctx
        .query(move |tx| Box::pin(async move { templates3.get_tx(tx, template_id).await }))
        .await
        .expect("Failed to fetch updated template")
        .expect("Template not found");

    assert_eq!(updated.name, "Updated Name");
    assert_eq!(updated.content, "Updated content");
    assert_eq!(updated.description, Some("Updated description".to_string()));

    cleanup_archive(&db, &archive_name).await;
}

#[tokio::test]
async fn test_template_delete_in_archive_schema() {
    let db = Database::connect(&database_url())
        .await
        .expect("Failed to connect to database");

    let archive_name = format!("tpl_d_{}", Uuid::now_v7());
    let schema = create_test_archive(&db, &archive_name).await;

    // Create template
    let ctx = db
        .for_schema(&schema)
        .expect("Failed to create schema context");
    let templates1 = PgTemplateRepository::new(db.pool.clone());
    let req = CreateTemplateRequest {
        name: "To be deleted".to_string(),
        description: None,
        content: "Content".to_string(),
        format: None,
        default_tags: None,
        collection_id: None,
    };

    let template_id = ctx
        .execute(move |tx| Box::pin(async move { templates1.create_tx(tx, req).await }))
        .await
        .expect("Failed to create template");

    // Delete template
    let templates2 = PgTemplateRepository::new(db.pool.clone());
    ctx.execute(move |tx| Box::pin(async move { templates2.delete_tx(tx, template_id).await }))
        .await
        .expect("Failed to delete template");

    // Verify deletion
    let templates3 = PgTemplateRepository::new(db.pool.clone());
    let result = ctx
        .query(move |tx| Box::pin(async move { templates3.get_tx(tx, template_id).await }))
        .await
        .expect("Failed to query template");

    assert!(result.is_none(), "Template should be deleted");

    cleanup_archive(&db, &archive_name).await;
}

// =============================================================================
// VERSIONING TESTS
// =============================================================================

#[tokio::test]
async fn test_versioning_list_in_archive_schema() {
    let db = Database::connect(&database_url())
        .await
        .expect("Failed to connect to database");

    let archive_name = format!("ver_l_{}", Uuid::now_v7());
    let schema = create_test_archive(&db, &archive_name).await;

    // Create note in archive
    let ctx = db
        .for_schema(&schema)
        .expect("Failed to create schema context");
    let notes = PgNoteRepository::new(db.pool.clone());
    let req = CreateNoteRequest {
        content: "Initial content".to_string(),
        format: "markdown".to_string(),
        source: "test".to_string(),
        collection_id: None,
        tags: None,
        metadata: None,
        document_type_id: None,
    };

    let note_id = ctx
        .execute(move |tx| Box::pin(async move { notes.insert_tx(tx, req).await }))
        .await
        .expect("Failed to create note");

    // List versions
    let versioning = VersioningRepository::new(db.pool.clone());
    let versions = ctx
        .query(move |tx| Box::pin(async move { versioning.list_versions_tx(tx, note_id).await }))
        .await
        .expect("Failed to list versions");

    assert_eq!(versions.note_id, note_id);
    assert_eq!(versions.current_original_version, 1);
    assert_eq!(versions.original_versions.len(), 1);

    cleanup_archive(&db, &archive_name).await;
}

#[tokio::test]
async fn test_versioning_get_in_archive_schema() {
    let db = Database::connect(&database_url())
        .await
        .expect("Failed to connect to database");

    let archive_name = format!("ver_g_{}", Uuid::now_v7());
    let schema = create_test_archive(&db, &archive_name).await;

    // Create note in archive
    let ctx = db
        .for_schema(&schema)
        .expect("Failed to create schema context");
    let notes = PgNoteRepository::new(db.pool.clone());
    let req = CreateNoteRequest {
        content: "Version 1 content".to_string(),
        format: "markdown".to_string(),
        source: "test".to_string(),
        collection_id: None,
        tags: None,
        metadata: None,
        document_type_id: None,
    };

    let note_id = ctx
        .execute(move |tx| Box::pin(async move { notes.insert_tx(tx, req).await }))
        .await
        .expect("Failed to create note");

    // Get version 1
    let versioning = VersioningRepository::new(db.pool.clone());
    let version = ctx
        .query(move |tx| {
            Box::pin(async move { versioning.get_original_version_tx(tx, note_id, 1).await })
        })
        .await
        .expect("Failed to get version")
        .expect("Version not found");

    assert_eq!(version.note_id, note_id);
    assert_eq!(version.version_number, 1);
    assert_eq!(version.content, "Version 1 content");

    cleanup_archive(&db, &archive_name).await;
}
