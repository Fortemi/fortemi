//! Lexical candidate selection must be index-backed (Core 2026.11.1 fix).
//!
//! Each arm (title, current content, completed attachment) is served by its own
//! GIN index; the arms are combined as a union of `(tenant_id, note_id)` keys
//! and only that set is joined and ranked. These tests pin the observable
//! results of every arm, tenant isolation per arm, and the plan shape.

use std::collections::BTreeSet;

use matric_core::CreateNoteRequest;
use matric_db::{
    search_candidates::{
        explain_lexical_compiled, lexical_on_connection, normalize_expression, normalize_query,
        LexicalStrategy, SearchCandidateScope,
    },
    Database,
};
use serde_json::Value;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

const TENANT_A: Uuid = Uuid::nil();

async fn migrated(pool: &PgPool) -> Database {
    sqlx::query("CREATE EXTENSION IF NOT EXISTS postgis")
        .execute(pool)
        .await
        .unwrap();
    let db = Database::new(pool.clone());
    db.migrate().await.unwrap();
    db
}

async fn use_tenant(tx: &mut PgConnection, tenant: Uuid) {
    sqlx::query("SELECT set_config('app.current_tenant', $1, true)")
        .bind(tenant.to_string())
        .execute(tx)
        .await
        .unwrap();
}

async fn add_note(db: &Database, tx: &mut PgConnection, title: &str, content: &str) -> Uuid {
    db.notes
        .insert_tx(
            tx,
            CreateNoteRequest {
                content: content.into(),
                format: "markdown".into(),
                source: "fts-arm-test".into(),
                collection_id: None,
                document_type_id: None,
                title: Some(title.into()),
                tags: None,
                metadata: None,
            },
        )
        .await
        .unwrap()
}

async fn add_attachment(tx: &mut PgConnection, note: Uuid, text: &str) {
    let blob = Uuid::new_v4();
    let hash = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    sqlx::query("INSERT INTO attachment_blob (id, content_hash, content_type, size_bytes, data) VALUES ($1, $2, 'text/plain', 1, $3)")
        .bind(blob)
        .bind(hash)
        .bind(vec![1_u8])
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO attachment (id, note_id, blob_id, filename, status, extracted_text) VALUES ($1, $2, $3, 'arm.txt', 'completed', $4)")
        .bind(Uuid::new_v4())
        .bind(note)
        .bind(blob)
        .bind(text)
        .execute(tx)
        .await
        .unwrap();
}

async fn ids(tx: &mut PgConnection, query: &str, strategy: LexicalStrategy) -> BTreeSet<Uuid> {
    lexical_on_connection(tx, query, 100, strategy, &SearchCandidateScope::default())
        .await
        .unwrap()
        .into_iter()
        .map(|hit| hit.note_id)
        .collect()
}

struct Fixture {
    title_only: Uuid,
    content_only: Uuid,
    attachment_only: Uuid,
    combined: Uuid,
    phrase: Uuid,
    scattered: Uuid,
}

async fn seed_arms(db: &Database, tx: &mut PgConnection) -> Fixture {
    let title_only = add_note(db, tx, "zephyrtitle handbook", "unrelated filler body").await;
    let content_only = add_note(db, tx, "plain heading", "body mentions zephyrbody here").await;
    let attachment_only = add_note(db, tx, "another heading", "nothing to see").await;
    add_attachment(tx, attachment_only, "scanned zephyrattach page").await;
    let combined = add_note(db, tx, "zephyrtitle combined", "also zephyrbody inside").await;
    add_attachment(tx, combined, "plus zephyrattach text").await;
    let phrase = add_note(db, tx, "phrase note", "the quick red fox jumps").await;
    let scattered = add_note(db, tx, "scattered note", "fox that is quick and red").await;
    Fixture {
        title_only,
        content_only,
        attachment_only,
        combined,
        phrase,
        scattered,
    }
}

fn set(items: &[Uuid]) -> BTreeSet<Uuid> {
    items.iter().copied().collect()
}

#[sqlx::test(migrations = false)]
async fn every_arm_boolean_and_phrase_still_match(pool: PgPool) {
    let db = migrated(&pool).await;
    let mut tx = pool.begin().await.unwrap();
    use_tenant(&mut tx, TENANT_A).await;
    let f = seed_arms(&db, &mut tx).await;

    // Plain (non-explicit) queries exercise the candidate `matching` arms.
    for strategy in [
        LexicalStrategy::English,
        LexicalStrategy::Simple,
        LexicalStrategy::Trigram,
    ] {
        assert_eq!(
            ids(&mut tx, "zephyrtitle", strategy).await,
            set(&[f.title_only, f.combined]),
            "title arm {strategy:?}"
        );
        assert_eq!(
            ids(&mut tx, "zephyrbody", strategy).await,
            set(&[f.content_only, f.combined]),
            "content arm {strategy:?}"
        );
        assert_eq!(
            ids(&mut tx, "zephyrattach", strategy).await,
            set(&[f.attachment_only, f.combined]),
            "attachment arm {strategy:?}"
        );
    }

    // Explicit expressions exercise the set-algebra constraint path.
    for strategy in [LexicalStrategy::English, LexicalStrategy::Simple] {
        assert_eq!(
            ids(
                &mut tx,
                "zephyrtitle OR zephyrbody OR zephyrattach",
                strategy
            )
            .await,
            set(&[f.title_only, f.content_only, f.attachment_only, f.combined]),
            "OR {strategy:?}"
        );
        // Explicit AND is evaluated per leaf across units; a plain multi-word
        // query remains unit-local (all words must match inside one unit).
        assert_eq!(
            ids(&mut tx, "zephyrtitle AND zephyrbody", strategy).await,
            set(&[f.combined]),
            "AND across title+content {strategy:?}"
        );
        assert!(
            ids(&mut tx, "zephyrtitle zephyrbody", strategy)
                .await
                .is_empty(),
            "plain multi-word query is unit-local {strategy:?}"
        );
        assert_eq!(
            ids(
                &mut tx,
                "zephyrtitle (zephyrbody OR zephyrattach)",
                strategy
            )
            .await,
            set(&[f.combined]),
            "grouped {strategy:?}"
        );
        assert_eq!(
            ids(&mut tx, "zephyrbody -zephyrattach", strategy).await,
            set(&[f.content_only]),
            "NOT attachment arm {strategy:?}"
        );
        assert_eq!(
            ids(&mut tx, "zephyrattach -zephyrtitle", strategy).await,
            set(&[f.attachment_only]),
            "NOT title arm {strategy:?}"
        );
        assert_eq!(
            ids(&mut tx, "\"quick red fox\"", strategy).await,
            set(&[f.phrase]),
            "phrase {strategy:?}"
        );
        assert!(
            ids(&mut tx, "\"quick red fox\" -jumps", strategy)
                .await
                .is_empty(),
            "phrase with NOT {strategy:?}"
        );
    }
    assert!(
        !ids(&mut tx, "fox -\"quick red fox\"", LexicalStrategy::English)
            .await
            .contains(&f.phrase)
    );
    assert!(
        ids(&mut tx, "fox -\"quick red fox\"", LexicalStrategy::English)
            .await
            .contains(&f.scattered)
    );

    // An attachment that is not completed never matches.
    sqlx::query("UPDATE attachment SET status='failed' WHERE note_id=$1")
        .bind(f.attachment_only)
        .execute(&mut *tx)
        .await
        .unwrap();
    assert_eq!(
        ids(&mut tx, "zephyrattach", LexicalStrategy::English).await,
        set(&[f.combined])
    );
    // Soft-deleted notes are excluded.
    sqlx::query("UPDATE note SET deleted_at = now() WHERE id=$1")
        .bind(f.title_only)
        .execute(&mut *tx)
        .await
        .unwrap();
    assert_eq!(
        ids(&mut tx, "zephyrtitle", LexicalStrategy::English).await,
        set(&[f.combined])
    );
    tx.rollback().await.unwrap();
}

#[sqlx::test(migrations = false)]
async fn each_arm_is_tenant_isolated(pool: PgPool) {
    let db = migrated(&pool).await;
    let tenant_b = Uuid::new_v4();
    sqlx::query("INSERT INTO tenant_registry (id, slug, display_name, status) VALUES ($1, $2, $2, 'active')")
        .bind(tenant_b)
        .bind(format!("fts-arm-{}", tenant_b.simple()))
        .execute(&pool)
        .await
        .unwrap();
    // A role without BYPASSRLS so the forced tenant policies apply to the search.
    let role = format!("fts_arm_{}", Uuid::new_v4().simple());
    for statement in [
        format!("CREATE ROLE {role} NOSUPERUSER NOBYPASSRLS NOLOGIN"),
        format!("GRANT USAGE ON SCHEMA public TO {role}"),
        format!("GRANT SELECT ON ALL TABLES IN SCHEMA public TO {role}"),
    ] {
        sqlx::query(&statement).execute(&pool).await.unwrap();
    }

    let mut tx = pool.begin().await.unwrap();
    // Tenant B owns one match per arm; tenant A owns none of these tokens.
    use_tenant(&mut tx, tenant_b).await;
    let b_title = add_note(&db, &mut tx, "isotitle only", "filler").await;
    let b_content = add_note(&db, &mut tx, "heading", "isobody only").await;
    let b_attachment = add_note(&db, &mut tx, "heading two", "filler").await;
    add_attachment(&mut tx, b_attachment, "isoattach only").await;
    use_tenant(&mut tx, TENANT_A).await;
    let a_note = add_note(&db, &mut tx, "isotitle shared", "isobody shared").await;
    add_attachment(&mut tx, a_note, "isoattach shared").await;

    sqlx::query(&format!("SET LOCAL ROLE {role}"))
        .execute(&mut *tx)
        .await
        .unwrap();
    for strategy in [
        LexicalStrategy::English,
        LexicalStrategy::Simple,
        LexicalStrategy::Trigram,
    ] {
        use_tenant(&mut tx, TENANT_A).await;
        for token in ["isotitle", "isobody", "isoattach"] {
            assert_eq!(
                ids(&mut tx, token, strategy).await,
                set(&[a_note]),
                "tenant A sees only its own {token} match ({strategy:?})"
            );
        }
        if matches!(strategy, LexicalStrategy::Trigram) {
            continue;
        }
        // Boolean/NOT paths must not leak or be widened by other tenants' rows.
        assert_eq!(
            ids(&mut tx, "isotitle OR isobody OR isoattach", strategy).await,
            set(&[a_note])
        );
        assert!(
            ids(&mut tx, "isotitle -isobody", strategy).await.is_empty(),
            "NOT excludes the only visible match for tenant A ({strategy:?})"
        );
        use_tenant(&mut tx, tenant_b).await;
        assert_eq!(ids(&mut tx, "isotitle", strategy).await, set(&[b_title]));
        assert_eq!(ids(&mut tx, "isobody", strategy).await, set(&[b_content]));
        assert_eq!(
            ids(&mut tx, "isoattach", strategy).await,
            set(&[b_attachment])
        );
        assert_eq!(
            ids(&mut tx, "isotitle OR isobody OR isoattach", strategy).await,
            set(&[b_title, b_content, b_attachment])
        );
    }
    tx.rollback().await.unwrap();
    for statement in [format!("DROP OWNED BY {role}"), format!("DROP ROLE {role}")] {
        sqlx::query(&statement).execute(&pool).await.unwrap();
    }
}

fn collect(plan: &Value, nodes: &mut Vec<(String, Option<String>, Option<String>)>) {
    match plan {
        Value::Object(map) => {
            if let Some(Value::String(kind)) = map.get("Node Type") {
                nodes.push((
                    kind.clone(),
                    map.get("Index Name")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    map.get("Relation Name")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                ));
            }
            map.values().for_each(|value| collect(value, nodes));
        }
        Value::Array(items) => items.iter().for_each(|value| collect(value, nodes)),
        _ => {}
    }
}

/// Seed enough rows for the planner to prefer the GIN indexes on cost alone.
async fn seed_scale(pool: &PgPool) {
    let mut tx = pool.begin().await.unwrap();
    use_tenant(&mut tx, TENANT_A).await;
    sqlx::query("CREATE TEMP TABLE plan_seed ON COMMIT DROP AS SELECT uuidv7() AS id, g FROM generate_series(1, 30000) g")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO note (id, format, source, created_at_utc, updated_at_utc, title) SELECT id, 'markdown', 'plan', now(), now(), CASE WHEN g % 10000 = 1 THEN 'plantitle ' || g ELSE 'Meeting notes ' || g END FROM plan_seed")
        .execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO note_revised_current (note_id, content) SELECT id, '{\"ref\":\"obj-' || g || '\",\"kind\":\"pointer\"' || CASE WHEN g % 10000 = 2 THEN ' planbody' ELSE '' END || '}' FROM plan_seed")
        .execute(&mut *tx).await.unwrap();
    let blob = Uuid::new_v4();
    sqlx::query("INSERT INTO attachment_blob (id, content_hash, content_type, size_bytes, data) VALUES ($1, $2, 'text/plain', 1, $3)")
        .bind(blob)
        .bind("f".repeat(64))
        .bind(vec![1_u8])
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO attachment (note_id, blob_id, filename, status, extracted_text) SELECT id, $1, 'plan.txt', 'completed', CASE WHEN g % 10000 = 3 THEN 'planattach text' ELSE 'extracted words ' || g END FROM plan_seed")
        .bind(blob)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    for table in ["note", "note_revised_current", "attachment"] {
        sqlx::query(&format!("ANALYZE {table}"))
            .execute(pool)
            .await
            .unwrap();
    }
}

type PlanNodes = Vec<(String, Option<String>, Option<String>)>;

/// Plan the production candidate statement. A single plain word exercises the
/// `matching` arms; an explicit `OR` expression exercises the set-algebra
/// constraint path.
async fn plan_nodes(tx: &mut PgConnection, query: &str, strategy: LexicalStrategy) -> PlanNodes {
    let plan = if query.contains(" OR ") {
        let parsed = matric_db::search_query::ParsedQuery::parse(query).unwrap();
        let expression = normalize_expression(&mut *tx, &parsed, strategy)
            .await
            .unwrap();
        let compiled = expression.tsquery().unwrap();
        let scope = SearchCandidateScope {
            lexical_constraint: Some((expression, strategy)),
            ..Default::default()
        };
        explain_lexical_compiled(tx, &compiled, 10, strategy, &scope)
            .await
            .unwrap()
    } else {
        let tsquery = normalize_query(&mut *tx, query, strategy).await.unwrap();
        explain_lexical_compiled(tx, &tsquery, 10, strategy, &SearchCandidateScope::default())
            .await
            .unwrap()
    };
    let mut nodes = Vec::new();
    collect(&plan, &mut nodes);
    nodes
}

fn has_bitmap_index(nodes: &PlanNodes, index: &str) -> bool {
    nodes
        .iter()
        .any(|(kind, name, _)| kind == "Bitmap Index Scan" && name.as_deref() == Some(index))
}

fn seq_scans_on(nodes: &PlanNodes, tables: &[&str]) -> Vec<String> {
    nodes
        .iter()
        .filter(|(kind, _, relation)| {
            kind == "Seq Scan"
                && relation
                    .as_deref()
                    .is_some_and(|name| tables.contains(&name))
        })
        .map(|(_, _, relation)| relation.clone().unwrap_or_default())
        .collect()
}

#[sqlx::test(migrations = false)]
async fn candidate_plan_uses_bitmap_index_scans_and_no_content_seq_scan(pool: PgPool) {
    let _db = migrated(&pool).await;
    seed_scale(&pool).await;
    let mut tx = pool.begin().await.unwrap();
    use_tenant(&mut tx, TENANT_A).await;

    let cases = [
        (
            LexicalStrategy::English,
            [
                "idx_note_revised_tsv",
                "idx_note_title_tsv_english",
                "idx_attachment_extracted_fts_english",
            ],
        ),
        (
            LexicalStrategy::Simple,
            [
                "idx_note_revised_tsv_simple",
                "idx_note_title_tsv_simple",
                "idx_attachment_extracted_fts_simple",
            ],
        ),
    ];
    for (strategy, expected) in cases {
        for query in ["plantitle", "plantitle OR planbody OR planattach"] {
            // Index-match proof, independent of table-size cost heuristics: with
            // sequential and plain index scans disabled, a Bitmap Index Scan on
            // the intended GIN index is only possible when the arm's expression
            // matches the index expression exactly. Natural-cost planning at
            // scale is covered by the `fts_scale_check` example.
            for setting in ["enable_seqscan", "enable_indexscan", "enable_indexonlyscan"] {
                sqlx::query(&format!("SET LOCAL {setting} = off"))
                    .execute(&mut *tx)
                    .await
                    .unwrap();
            }
            let nodes = plan_nodes(&mut tx, query, strategy).await;
            for index in expected {
                assert!(
                    has_bitmap_index(&nodes, index),
                    "{strategy:?} {query:?}: expected Bitmap Index Scan on {index}: {nodes:?}"
                );
            }
            assert!(
                seq_scans_on(&nodes, &["note", "note_revised_current", "attachment"]).is_empty(),
                "{strategy:?} {query:?}: arms fell back to Seq Scan: {nodes:?}"
            );
        }
    }
    tx.rollback().await.unwrap();
}

async fn federated_ids(tx: &mut PgConnection, query: &str) -> BTreeSet<Uuid> {
    sqlx::query_scalar::<_, Uuid>(matric_db::search::FEDERATED_FTS_SQL)
        .bind(query)
        .bind(100_i64)
        .fetch_all(tx)
        .await
        .unwrap()
        .into_iter()
        .collect()
}

/// The federated handler's statement (title arm + stored-tsv content arm, no
/// attachments) keeps its results, tenant isolation and archive scoping.
#[sqlx::test(migrations = false)]
async fn federated_search_union_matches_isolates_tenants_and_scopes_archives(pool: PgPool) {
    use matric_core::ArchiveRepository;
    let db = migrated(&pool).await;
    let tenant_b = Uuid::new_v4();
    sqlx::query("INSERT INTO tenant_registry (id, slug, display_name, status) VALUES ($1, $2, $2, 'active')")
        .bind(tenant_b)
        .bind(format!("fts-fed-{}", tenant_b.simple()))
        .execute(&pool)
        .await
        .unwrap();
    let role = format!("fts_fed_{}", Uuid::new_v4().simple());
    for statement in [
        format!("CREATE ROLE {role} NOSUPERUSER NOBYPASSRLS NOLOGIN"),
        format!("GRANT USAGE ON SCHEMA public TO {role}"),
        format!("GRANT SELECT ON ALL TABLES IN SCHEMA public TO {role}"),
    ] {
        sqlx::query(&statement).execute(&pool).await.unwrap();
    }
    // Archive DDL reads `app.current_tenant` from the session, like `create_pool`.
    let archive_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .after_connect(|connection, _| {
            Box::pin(async move {
                sqlx::query("SELECT set_config('app.current_tenant', $1, false)")
                    .bind(TENANT_A.to_string())
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
        .connect_with((*pool.connect_options()).clone())
        .await
        .unwrap();
    let archive_db = Database::new(archive_pool);
    let archive_name = format!("fed-{}", Uuid::new_v4());
    let archive = archive_db
        .archives
        .create_archive_schema(&archive_name, None)
        .await
        .unwrap();

    let mut tx = pool.begin().await.unwrap();
    use_tenant(&mut tx, tenant_b).await;
    let b_title = add_note(&db, &mut tx, "fedtitle only", "filler").await;
    let b_content = add_note(&db, &mut tx, "heading", "fedbody only").await;
    use_tenant(&mut tx, TENANT_A).await;
    let a_title = add_note(&db, &mut tx, "fedtitle mine", "filler").await;
    let a_content = add_note(&db, &mut tx, "heading", "fedbody mine").await;
    let a_both = add_note(&db, &mut tx, "fedtitle fedbody", "fedbody").await;
    let a_deleted = add_note(&db, &mut tx, "fedtitle gone", "fedbody gone").await;
    sqlx::query("UPDATE note SET deleted_at = now() WHERE id = $1")
        .bind(a_deleted)
        .execute(&mut *tx)
        .await
        .unwrap();

    // Superuser view: both arms match, deleted notes excluded.
    assert_eq!(
        federated_ids(&mut tx, "fedtitle").await,
        set(&[a_title, a_both, b_title])
    );
    assert_eq!(
        federated_ids(&mut tx, "fedbody").await,
        set(&[a_content, a_both, b_content])
    );
    // websearch operators keep working.
    assert_eq!(
        federated_ids(&mut tx, "fedtitle OR fedbody").await,
        set(&[a_title, a_content, a_both, b_title, b_content])
    );
    assert_eq!(
        // Unit-local NOT: a_both's content (without "fedtitle") still matches.
        federated_ids(&mut tx, "fedbody -fedtitle").await,
        set(&[a_content, a_both, b_content])
    );

    // Tenant isolation per arm under a NOBYPASSRLS role.
    sqlx::query(&format!("SET LOCAL ROLE {role}"))
        .execute(&mut *tx)
        .await
        .unwrap();
    use_tenant(&mut tx, TENANT_A).await;
    assert_eq!(
        federated_ids(&mut tx, "fedtitle").await,
        set(&[a_title, a_both])
    );
    assert_eq!(
        federated_ids(&mut tx, "fedbody").await,
        set(&[a_content, a_both])
    );
    use_tenant(&mut tx, tenant_b).await;
    assert_eq!(federated_ids(&mut tx, "fedtitle").await, set(&[b_title]));
    assert_eq!(federated_ids(&mut tx, "fedbody").await, set(&[b_content]));
    tx.rollback().await.unwrap();

    // Archive scoping: with the memory's search_path only that schema's notes match.
    let mut tx = pool.begin().await.unwrap();
    use_tenant(&mut tx, TENANT_A).await;
    let public_note = add_note(&db, &mut tx, "scopeword public", "filler").await;
    sqlx::query(&format!(
        "SET LOCAL search_path TO {}, public",
        archive.schema_name
    ))
    .execute(&mut *tx)
    .await
    .unwrap();
    let archived_note = add_note(&db, &mut tx, "scopeword archived", "filler").await;
    assert_eq!(
        federated_ids(&mut tx, "scopeword").await,
        set(&[archived_note])
    );
    assert!(!federated_ids(&mut tx, "scopeword")
        .await
        .contains(&public_note));
    tx.rollback().await.unwrap();

    archive_db
        .archives
        .drop_archive_schema(&archive_name)
        .await
        .unwrap();
    for statement in [format!("DROP OWNED BY {role}"), format!("DROP ROLE {role}")] {
        sqlx::query(&statement).execute(&pool).await.unwrap();
    }
}

#[sqlx::test(migrations = false)]
async fn federated_plan_uses_bitmap_index_scans(pool: PgPool) {
    let _db = migrated(&pool).await;
    seed_scale(&pool).await;
    let mut tx = pool.begin().await.unwrap();
    use_tenant(&mut tx, TENANT_A).await;
    for setting in ["enable_seqscan", "enable_indexscan", "enable_indexonlyscan"] {
        sqlx::query(&format!("SET LOCAL {setting} = off"))
            .execute(&mut *tx)
            .await
            .unwrap();
    }
    let explain = format!(
        "EXPLAIN (FORMAT JSON) {}",
        matric_db::search::FEDERATED_FTS_SQL
    );
    let plan: Value = sqlx::query_scalar(&explain)
        .bind("plantitle")
        .bind(10_i64)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let mut nodes = Vec::new();
    collect(&plan, &mut nodes);
    for index in ["idx_note_revised_tsv", "idx_note_title_tsv_english"] {
        assert!(has_bitmap_index(&nodes, index), "{index}: {nodes:?}");
    }
    assert!(
        seq_scans_on(&nodes, &["note", "note_revised_current"]).is_empty(),
        "{nodes:?}"
    );
    tx.rollback().await.unwrap();
}
