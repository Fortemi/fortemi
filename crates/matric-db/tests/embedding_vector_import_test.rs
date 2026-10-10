#![cfg(feature = "migrations")]

use std::{fs, path::Path};

use matric_core::embedding_space_id;
use matric_db::{
    find_similar_profiles_for_note_tx, note_id_for_source_identity_tx, AppUserUpsert, Database,
    EntitySimilarityFilter, PgAppUserRepository, PgEmbeddingImportRepository, LOCAL_TENANT_ID,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use uuid::Uuid;

async fn set_with_vector_source(db: &Database, slug: &str, vector_source: &str) -> (Uuid, String) {
    let contract = json!({
        "provider": "external-fixture",
        "model": "unit-3",
        "normalization": "l2"
    });
    let space_id = embedding_space_id(&contract);
    let config_id = Uuid::new_v4();
    let set_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO embedding_config (
            id, name, model, dimension, vector_type, chunk_size, chunk_overlap,
            provider, provider_config, content_types, document_composition,
            space_contract, space_id
         ) VALUES (
            $1, $2, 'unit-3', 3, 'vector', 512, 0,
            'custom'::embedding_provider, '{}'::jsonb, '{}'::text[], '{}'::jsonb,
            $3, $4
         )",
    )
    .bind(config_id)
    .bind(format!("external fixture {slug}"))
    .bind(&contract)
    .bind(&space_id)
    .execute(&db.pool)
    .await
    .expect("insert embedding config");
    sqlx::query(
        "INSERT INTO embedding_set (
            id, name, slug, set_type, mode, criteria, embedding_config_id,
            auto_embed_rules, vector_source, defer_index_build, agent_metadata
         ) VALUES (
            $1, $2, $3, 'full'::embedding_set_type, 'manual'::embedding_set_mode,
            '{}'::jsonb, $4, '{}'::jsonb, $5, TRUE, '{}'::jsonb
         )",
    )
    .bind(set_id)
    .bind(format!("external profiles {slug}"))
    .bind(slug)
    .bind(config_id)
    .bind(vector_source)
    .execute(&db.pool)
    .await
    .expect("insert embedding set");
    (set_id, space_id)
}

async fn external_set(db: &Database, slug: &str) -> (Uuid, String) {
    set_with_vector_source(db, slug, "external").await
}

fn profile_hash(template: &str, text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(template.as_bytes());
    hasher.update(b"\n");
    hasher.update(text.as_bytes());
    hex::encode(hasher.finalize())
}

fn write_run(
    root: &Path,
    run_id: &str,
    previous_run_id: Option<&str>,
    space_id: &str,
    profiles: Vec<serde_json::Value>,
    deletions: Vec<serde_json::Value>,
) -> TempDir {
    let dir = tempfile::tempdir_in(root).expect("temp run dir");
    fs::create_dir_all(dir.path().join("profiles")).expect("profiles dir");
    let profiles_body = profiles
        .iter()
        .map(|row| serde_json::to_string(row).unwrap())
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    fs::write(
        dir.path().join("profiles/profiles.jsonl"),
        profiles_body.as_bytes(),
    )
    .expect("write profiles");
    let profile_sha = hex::encode(Sha256::digest(profiles_body.as_bytes()));
    let mut files = vec![json!({
        "path": "profiles/profiles.jsonl",
        "sha256": profile_sha,
        "rows": profiles.len()
    })];
    if !deletions.is_empty() {
        let deletions_body = deletions
            .iter()
            .map(|row| serde_json::to_string(row).unwrap())
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        fs::write(
            dir.path().join("deletions.jsonl"),
            deletions_body.as_bytes(),
        )
        .expect("write deletions");
        let deletion_sha = hex::encode(Sha256::digest(deletions_body.as_bytes()));
        files.push(json!({
            "path": "deletions.jsonl",
            "sha256": deletion_sha,
            "rows": deletions.len()
        }));
    }
    fs::write(
        dir.path().join("manifest.json"),
        serde_json::to_vec_pretty(&json!({
            "run_id": run_id,
            "previous_run_id": previous_run_id,
            "space_id": space_id,
            "created_at": "2026-10-09T04:00:00Z",
            "counts": { "profiles": profiles.len(), "deletions": deletions.len() },
            "files": files,
            "template_version": "profile-v1"
        }))
        .unwrap(),
    )
    .expect("write manifest");
    dir
}

fn profile(
    entity: &str,
    source: &str,
    text: &str,
    vector: [f32; 3],
    space_id: &str,
) -> serde_json::Value {
    json!({
        "entity_id": entity,
        "source": source,
        "profile_hash": profile_hash("profile-v1", text),
        "profile_text": text,
        "template_version": "profile-v1",
        "space_id": space_id,
        "dims": 3,
        "embedding": vector,
        "metadata": { "fixture": true }
    })
}

#[tokio::test]
async fn external_embedding_run_imports_idempotently_orders_and_deletes() {
    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must select a migrated test database");
    let db = Database::connect(&database_url)
        .await
        .expect("connect migrated test database");
    let slug = format!("external-import-{}", Uuid::new_v4().simple());
    let internal_slug = format!("{slug}-internal");
    let (set_id, space_id) = external_set(&db, &slug).await;
    let (_internal_set_id, _) = set_with_vector_source(&db, &internal_slug, "internal").await;
    let repository = PgEmbeddingImportRepository::new(db.pool.clone());
    let tenant_id = Uuid::parse_str(LOCAL_TENANT_ID).expect("local tenant id");
    let user = PgAppUserRepository::new(db.pool.clone())
        .upsert_oidc(
            AppUserUpsert {
                tenant_id,
                iss: "https://issuer.example".to_string(),
                sub: format!("vector-import-{}", Uuid::new_v4()),
                email: None,
                email_verified: false,
                display_name: None,
                groups: Vec::new(),
                current_scopes: vec!["admin".to_string()],
                kind: "user".to_string(),
                azp: Some("fortemi-web".to_string()),
            },
            true,
        )
        .await
        .expect("create import initiator");
    let root = tempfile::tempdir().expect("root tempdir");

    let before_jobs: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM job_queue WHERE job_type = 'embedding'")
            .fetch_one(&db.pool)
            .await
            .expect("job count");

    let run1 = write_run(
        root.path(),
        "run-1",
        None,
        &space_id,
        (0..1000)
            .map(|i| {
                let vector = if i == 0 {
                    [1.0, 0.0, 0.0]
                } else {
                    [0.0, 1.0, 0.0]
                };
                profile(
                    &format!("entity-{i}"),
                    "fixture-source",
                    "profile text",
                    vector,
                    &space_id,
                )
            })
            .collect(),
        Vec::new(),
    );
    assert!(repository
        .import_run_folder("public", &internal_slug, run1.path(), None, None)
        .await
        .is_err());

    let report = repository
        .import_run_folder("public", &slug, run1.path(), Some(user.id), Some("user"))
        .await
        .expect("import run 1");
    assert_eq!(report.inserted, 1000);
    assert_eq!(report.rejected.total, 0);
    let stored: (Option<Uuid>, Option<String>) = sqlx::query_as(
        "SELECT initiated_by_user_id, initiated_by_kind
         FROM embedding_import_run
         WHERE run_id = 'run-1' AND set_id = $1",
    )
    .bind(set_id)
    .fetch_one(&db.pool)
    .await
    .expect("import run initiator");
    assert_eq!(stored, (Some(user.id), Some("user".to_string())));

    let replay = repository
        .import_run_folder("public", &slug, run1.path(), None, None)
        .await
        .expect("replay run 1");
    assert_eq!(replay.status, "already_applied");

    let checksum_mismatch = write_run(
        root.path(),
        "run-checksum-mismatch",
        Some("run-1"),
        &space_id,
        vec![profile(
            "checksum-row",
            "fixture-source",
            "checksum profile",
            [1.0, 0.0, 0.0],
            &space_id,
        )],
        Vec::new(),
    );
    fs::write(
        checksum_mismatch.path().join("profiles/profiles.jsonl"),
        b"{\"tampered\":true}\n",
    )
    .expect("tamper profile file");
    assert!(repository
        .import_run_folder("public", &slug, checksum_mismatch.path(), None, None)
        .await
        .is_err());

    let out_of_order = write_run(
        root.path(),
        "run-out-of-order",
        Some("missing"),
        &space_id,
        Vec::new(),
        Vec::new(),
    );
    assert!(repository
        .import_run_folder("public", &slug, out_of_order.path(), None, None)
        .await
        .is_err());

    let run2 = write_run(
        root.path(),
        "run-2",
        Some("run-1"),
        &space_id,
        vec![
            profile(
                "entity-1000",
                "fixture-source",
                "new profile",
                [0.0, 0.0, 1.0],
                &space_id,
            ),
            json!({
                "entity_id": "bad-dims",
                "source": "fixture-source",
                "profile_hash": profile_hash("profile-v1", "bad dims"),
                "profile_text": "bad dims",
                "template_version": "profile-v1",
                "space_id": space_id,
                "dims": 2,
                "embedding": [1.0, 0.0],
                "metadata": {}
            }),
            json!({
                "entity_id": "bad-hash",
                "source": "fixture-source",
                "profile_hash": "0".repeat(64),
                "profile_text": "hash mismatch",
                "template_version": "profile-v1",
                "space_id": space_id,
                "dims": 3,
                "embedding": [1.0, 0.0, 0.0],
                "metadata": {}
            }),
            json!({
                "entity_id": "bad-space",
                "source": "fixture-source",
                "profile_hash": profile_hash("profile-v1", "bad space"),
                "profile_text": "bad space",
                "template_version": "profile-v1",
                "space_id": "0000000000000000000000000000000000000000000000000000000000000000",
                "dims": 3,
                "embedding": [1.0, 0.0, 0.0],
                "metadata": {}
            }),
            json!({
                "entity_id": "non-unit",
                "source": "fixture-source",
                "profile_hash": profile_hash("profile-v1", "non unit"),
                "profile_text": "non unit",
                "template_version": "profile-v1",
                "space_id": space_id,
                "dims": 3,
                "embedding": [0.5, 0.0, 0.0],
                "metadata": {}
            }),
            json!({
                "entity_id": "numeric-bounds",
                "source": "fixture-source",
                "profile_hash": profile_hash("profile-v1", "numeric bounds"),
                "profile_text": "numeric bounds",
                "template_version": "profile-v1",
                "space_id": space_id,
                "dims": 3,
                "embedding": [1.0e39, 0.0, 0.0],
                "metadata": {}
            }),
        ],
        vec![json!({
            "entity_id": "entity-999",
            "source": "fixture-source",
            "run_id": "run-2"
        })],
    );
    let report2 = repository
        .import_run_folder("public", &slug, run2.path(), None, None)
        .await
        .expect("import run 2");
    assert_eq!(report2.inserted, 1);
    assert_eq!(report2.deleted, 1);
    assert_eq!(report2.rejected.total, 5);
    let reasons = report2
        .rejected
        .rows
        .iter()
        .map(|row| row.reason.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        reasons.contains("dims") || reasons.contains("dimension") || reasons.contains("values")
    );
    assert!(reasons.contains("space_id"));
    assert!(reasons.contains("unit"));
    assert!(reasons.contains("hash"));

    let after_jobs: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM job_queue WHERE job_type = 'embedding'")
            .fetch_one(&db.pool)
            .await
            .expect("job count");
    assert_eq!(before_jobs, after_jobs);

    let mut connection = db.pool.acquire().await.expect("acquire");
    let query_note = note_id_for_source_identity_tx(&mut connection, "fixture-source", "entity-0")
        .await
        .expect("resolve query");
    let hits = find_similar_profiles_for_note_tx(
        &mut connection,
        query_note,
        set_id,
        5,
        EntitySimilarityFilter {
            metadata: None,
            strict: None,
            legacy_filters: String::new(),
        },
        Vec::new(),
    )
    .await
    .expect("similar profiles");
    assert!(!hits.is_empty());
}
