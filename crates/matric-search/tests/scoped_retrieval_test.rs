//! Full-migration retrieval tests with synthetic vectors, no inference service.
use matric_core::{metadata_search::MetadataPredicates, CreateNoteRequest, StrictTagFilter};
use matric_db::Database;
use matric_search::{HybridSearchConfig, HybridSearchEngine, SearchRequest};
use pgvector::Vector;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

fn assert_matched_evidence(
    hit: &matric_core::SearchHit,
    mode: &str,
    embedding_id: Uuid,
    sources: &[Option<matric_core::search_evidence::EvidenceSource>],
) {
    use matric_core::search_evidence::{EvidenceKind, EvidenceText};
    let evidence = hit
        .evidence
        .as_ref()
        .expect("canonical search must project candidate evidence");
    let units_per_source = if mode == "hybrid" { 2 } else { 1 };
    assert_eq!(evidence.locators().len(), units_per_source * sources.len());
    let wire = serde_json::to_value(evidence).unwrap();
    assert_eq!(wire["omissions"], json!([]));
    for locator in evidence.locators() {
        let unit = locator.unit();
        let text = match unit.kind {
            EvidenceKind::Current => {
                assert_ne!(mode, "semantic");
                assert_eq!(unit.id, hit.note_id.to_string());
                assert_eq!(unit.index, 0);
                "needle scoped evidence \u{1f600}"
            }
            EvidenceKind::Embedding => {
                assert_ne!(mode, "fts");
                assert_eq!(unit.id, embedding_id.to_string());
                assert_eq!(unit.index, 7);
                "needle"
            }
            other => panic!("unmatched unit kind {other:?}"),
        };
        let value = serde_json::to_value(locator).unwrap();
        let source = value
            .get("source")
            .map(|value| serde_json::from_value(value.clone()).unwrap());
        assert!(
            sources.contains(&source),
            "only selected source tuples may be projected"
        );
        assert_eq!(
            locator
                .resolve(Some(EvidenceText {
                    note_id: &hit.note_id.to_string(),
                    unit,
                    content: text,
                    source: source.as_ref()
                }))
                .unwrap(),
            text
        );
    }
}

#[sqlx::test(migrations = false)]
async fn public_search_spans_omit_query_and_filter_values(pool: PgPool) {
    use matric_search::HybridSearch;
    use std::sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    };
    use tracing::instrument::WithSubscriber;
    use tracing::{
        field::{Field, Visit},
        span::{Attributes, Id, Record},
        Event, Metadata, Subscriber,
    };

    #[derive(Clone, Default)]
    struct Capture(Arc<Mutex<String>>, Arc<AtomicU64>);
    struct Fields<'a>(&'a mut String);
    impl Visit for Fields<'_> {
        fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
            use std::fmt::Write;
            write!(self.0, "{}={value:?};", field.name()).unwrap();
        }
    }
    impl Subscriber for Capture {
        fn enabled(&self, _: &Metadata<'_>) -> bool {
            true
        }
        fn new_span(&self, attrs: &Attributes<'_>) -> Id {
            let mut output = self.0.lock().unwrap();
            output.push_str(attrs.metadata().name());
            attrs.record(&mut Fields(&mut output));
            Id::from_u64(self.1.fetch_add(1, Ordering::Relaxed) + 1)
        }
        fn record(&self, _: &Id, values: &Record<'_>) {
            values.record(&mut Fields(&mut self.0.lock().unwrap()));
        }
        fn record_follows_from(&self, _: &Id, _: &Id) {}
        fn event(&self, event: &Event<'_>) {
            event.record(&mut Fields(&mut self.0.lock().unwrap()));
        }
        fn enter(&self, _: &Id) {}
        fn exit(&self, _: &Id) {}
    }
    let capture = Capture::default();
    let mut connection = pool.acquire().await.unwrap();
    let closed = sqlx::postgres::PgPoolOptions::new()
        .connect_with((*pool.connect_options()).clone())
        .await
        .unwrap();
    closed.close().await;
    let engine = HybridSearchEngine::new(Database::new(closed));
    let config = HybridSearchConfig::default();
    let query = "private-query-sentinel";
    let filters = "tag:private-filter-sentinel";
    assert!(engine
        .search(query, None, 1, &config)
        .with_subscriber(capture.clone())
        .await
        .is_err());
    assert!(engine
        .search_filtered(query, None, filters, 1, &config)
        .with_subscriber(capture.clone())
        .await
        .is_err());
    assert!(engine
        .search_on_connection(&mut connection, query, None, filters, 0, &config)
        .with_subscriber(capture.clone())
        .await
        .unwrap()
        .is_empty());
    let output = capture.0.lock().unwrap();
    for name in [
        "search",
        "search_filtered",
        "search_on_connection",
        "query_len",
        "query_class",
    ] {
        assert!(output.contains(name), "missing captured {name}");
    }
    for secret in [query, filters, "private-filter-sentinel"] {
        assert!(
            !output.contains(secret),
            "search tracing exposed fixture input"
        );
    }
}

#[sqlx::test(migrations = false)]
async fn public_entry_points_keep_scope_without_metadata(pool: PgPool) {
    use matric_search::HybridSearch;
    use std::time::Duration;

    sqlx::query("CREATE EXTENSION IF NOT EXISTS postgis")
        .execute(&pool)
        .await
        .unwrap();
    let db = Database::new(pool.clone());
    db.migrate().await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('app.current_tenant', $1, true)")
        .bind(Uuid::nil().to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    let set: Uuid = sqlx::query_scalar("SELECT id FROM embedding_set WHERE slug = 'default'")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let vector = Vector::from(vec![1.0_f32; 768]);
    let mut wanted = Uuid::nil();
    for kind in ["wanted", "wrong-tag", "wrong-set", "deleted", "archived"] {
        for _ in 0..if kind == "wanted" { 1 } else { 4 } {
            let id = db
                .notes
                .insert_tx(
                    &mut tx,
                    CreateNoteRequest {
                        content: "needle scoped evidence \u{1f600}".into(),
                        format: "markdown".into(),
                        source: "public-search-fixture".into(),
                        collection_id: None,
                        document_type_id: None,
                        title: Some(
                            if kind == "wanted" {
                                "evidence"
                            } else {
                                "needle"
                            }
                            .into(),
                        ),
                        tags: Some(vec![if kind == "wrong-tag" {
                            "other"
                        } else {
                            "allowed"
                        }
                        .into()]),
                        metadata: None,
                    },
                )
                .await
                .unwrap();
            let mut values = vec![1.0_f32; 768];
            if kind == "wanted" {
                wanted = id;
                values[0] = 10.0;
            }
            sqlx::query("INSERT INTO embedding (note_id, chunk_index, text, vector, model, embedding_set_id) VALUES ($1, 7, 'needle', $2, 'synthetic-only', $3)")
                .bind(id).bind(Vector::from(values)).bind(set).execute(&mut *tx).await.unwrap();
            match kind {
                "wrong-set" => {
                    sqlx::query("DELETE FROM embedding_set_member WHERE note_id=$1")
                        .bind(id)
                        .execute(&mut *tx)
                        .await
                        .unwrap();
                }
                "deleted" => {
                    sqlx::query("UPDATE note SET deleted_at=now() WHERE id=$1")
                        .bind(id)
                        .execute(&mut *tx)
                        .await
                        .unwrap();
                }
                "archived" => {
                    sqlx::query("UPDATE note SET archived=true WHERE id=$1")
                        .bind(id)
                        .execute(&mut *tx)
                        .await
                        .unwrap();
                }
                _ => {}
            }
        }
    }
    // A pending embedding sorts before the usable one in the old MMR lookup.
    sqlx::query("INSERT INTO embedding (id, note_id, chunk_index, text, vector, model, embedding_set_id) VALUES ($1, $2, 1, 'pending', NULL, 'synthetic-only', $3)")
        .bind(Uuid::from_u128(1)).bind(wanted).bind(set).execute(&mut *tx).await.unwrap();
    let embedding_id: Uuid =
        sqlx::query_scalar("SELECT id FROM embedding WHERE note_id=$1 AND chunk_index=7")
            .bind(wanted)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    tx.commit().await.unwrap();

    let role = format!("public_search_{}", Uuid::new_v4().simple());
    sqlx::raw_sql(&format!("CREATE ROLE {role} LOGIN NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE NOINHERIT; GRANT USAGE ON SCHEMA public TO {role}; GRANT SELECT ON ALL TABLES IN SCHEMA public TO {role};"))
        .execute(&pool).await.unwrap();
    let runtime = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_secs(5))
        .after_connect(|connection, _| {
            Box::pin(async move {
                sqlx::query("SELECT set_config('app.current_tenant', $1, false)")
                    .bind(Uuid::nil().to_string())
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
        .connect_with((*pool.connect_options()).clone().username(&role))
        .await
        .unwrap();
    let privileged: bool = sqlx::query_scalar(
        "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&runtime)
    .await
    .unwrap();
    assert!(!privileged);
    let engine = HybridSearchEngine::new(Database::new(runtime.clone()));
    let mut checks = 0;
    for mode in ["fts", "semantic", "hybrid"] {
        for script in ["latin", "cyrillic", "emoji", "han"] {
            for entry in ["trait-strict", "trait-unified", "trait-filtered", "builder"] {
                let mut config = match mode {
                    "fts" => HybridSearchConfig::fts_only(),
                    "semantic" => HybridSearchConfig::semantic_only(),
                    _ => HybridSearchConfig::default(),
                };
                config.script_hint = if script == "emoji" {
                    None
                } else {
                    Some(script.into())
                };
                config.embedding_set_id = Some(set);
                config.diversity = Some(0.2);
                let mut tags = StrictTagFilter::new();
                tags.required_string_tags.push("allowed".into());
                if entry == "trait-unified" {
                    config.unified_filter = Some(matric_core::StrictFilter {
                        tags: Some(tags),
                        ..Default::default()
                    });
                    // Unified tags take precedence, not an accidental AND with legacy tags.
                    let mut conflicting = StrictTagFilter::new();
                    conflicting.match_none = true;
                    config.strict_filter = Some(conflicting);
                } else if entry == "trait-strict" {
                    config.strict_filter = Some(tags);
                }
                assert!(config.metadata_predicates.is_none());
                let query = if script == "emoji" {
                    "\u{1f600}"
                } else {
                    "needle"
                };
                let hits = match entry {
                    "trait-strict" | "trait-unified" => {
                        engine.search(query, Some(&vector), 1, &config).await
                    }
                    "trait-filtered" => {
                        engine
                            .search_filtered(query, Some(&vector), "tag:allowed", 1, &config)
                            .await
                    }
                    _ => {
                        SearchRequest::new(query)
                            .with_config(config)
                            .with_filters("tag:allowed")
                            .with_embedding(vector.clone())
                            .with_limit(1)
                            .execute(&engine)
                            .await
                    }
                }
                .unwrap();
                assert_eq!(hits.len(), 1, "{entry} {mode} {script}");
                assert_eq!(hits[0].hit.note_id, wanted, "{entry} {mode} {script}");
                assert_matched_evidence(&hits[0].hit, mode, embedding_id, &[None]);
                checks += 1;
            }
        }
    }
    // Pool entry points must retain an explicitly installed tenant context.
    sqlx::query("SELECT set_config('app.current_tenant', $1, false)")
        .bind(Uuid::new_v4().to_string())
        .execute(&runtime)
        .await
        .unwrap();
    for config in [
        HybridSearchConfig::fts_only(),
        HybridSearchConfig::semantic_only(),
        HybridSearchConfig::default(),
    ] {
        assert!(engine
            .search("needle", Some(&vector), 1, &config)
            .await
            .unwrap()
            .is_empty());
        assert!(engine
            .search_filtered("needle", Some(&vector), "tag:allowed", 1, &config)
            .await
            .unwrap()
            .is_empty());
        checks += 2;
    }
    runtime.close().await;
    sqlx::raw_sql(&format!("DROP OWNED BY {role}; DROP ROLE {role};"))
        .execute(&pool)
        .await
        .unwrap();
    println!("public_entry_points: {checks} no-metadata checks; non-bypass one-connection pool; synthetic vectors only");
}

#[sqlx::test(migrations = false)]
async fn metadata_and_scope_precede_limits_in_every_retrieval_mode(pool: PgPool) {
    // The deployment image provisions PostGIS before application migrations.
    sqlx::query("CREATE EXTENSION IF NOT EXISTS postgis")
        .execute(&pool)
        .await
        .unwrap();
    let db = Database::new(pool.clone());
    if let Err(error) = db.migrate().await {
        // This newly created SQLx database contains only shipped migrations,
        // before synthetic rows are inserted; retain actionable fixture errors.
        match error {
            matric_core::Error::Database(source) => panic!("full native migration chain: {source}"),
            other => panic!("full native migration chain: {other}"),
        }
    }
    let engine = HybridSearchEngine::new(db.clone());
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('app.current_tenant', $1, true)")
        .bind(Uuid::nil().to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    let set: Uuid = sqlx::query_scalar("SELECT id FROM embedding_set WHERE slug = 'default'")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let vector = Vector::from(vec![1.0_f32; 768]);
    let metadata = json!({"provider":"fixture", "model":42, "role":"assistant", "event_kind":"result", "sensitivity":"internal"});
    let mut wanted = Uuid::nil();
    let mut excluded = Vec::new();
    // Disallowed rows have equally strong or stronger lexical/vector scores.
    // A post-LIMIT filter cannot reliably recover the single authorized hit.
    for kind in [
        "wanted",
        "wrong-metadata",
        "wrong-tag",
        "wrong-set",
        "old",
        "deleted",
        "archived",
    ] {
        for i in 0..if kind == "wanted" { 1 } else { 4 } {
            let id = db.notes.insert_tx(&mut tx, CreateNoteRequest {
                content: "needle scoped evidence \u{1f600}".into(), format: "markdown".into(), source: "scoped-retrieval-test".into(),
                collection_id: None, document_type_id: None,
                title: Some(if kind == "wanted" { "evidence" } else { "needle" }.into()),
                tags: Some(vec![if kind == "wrong-tag" { "other" } else { "allowed" }.into()]),
                metadata: Some(if kind == "wrong-metadata" { json!({"provider":"other", "model":"42", "role":"other", "event_kind":"other", "sensitivity":"other"}) } else { metadata.clone() }),
            }).await.unwrap();
            if kind == "wanted" {
                wanted = id;
            } else {
                excluded.push(id);
            }
            let stored_vector = if kind == "wanted" {
                let mut values = vec![1.0_f32; 768];
                values[0] = 10.0;
                Vector::from(values)
            } else {
                vector.clone()
            };
            sqlx::query("INSERT INTO embedding (note_id, chunk_index, text, vector, model, embedding_set_id) VALUES ($1, 7, 'needle', $2, 'synthetic-only', $3)")
                .bind(id).bind(&stored_vector).bind(set).execute(&mut *tx).await.unwrap();
            sqlx::query("INSERT INTO source_identity (note_id, source_namespace, external_id, source_schema_version, content_digest, import_run_id) VALUES ($1, 'synthetic', $2, '1', $3, $4)")
                .bind(id).bind(format!("{kind}-{i}")).bind(format!("sha256:{}", "0".repeat(64)))
                .bind(if kind == "wrong-metadata" { "other" } else { "run-42" }).execute(&mut *tx).await.unwrap();
            match kind {
                "wrong-set" => {
                    sqlx::query("DELETE FROM embedding_set_member WHERE note_id = $1")
                        .bind(id)
                        .execute(&mut *tx)
                        .await
                        .unwrap();
                }
                "old" => {
                    sqlx::query("UPDATE note SET created_at_utc = '2020-01-01' WHERE id = $1")
                        .bind(id)
                        .execute(&mut *tx)
                        .await
                        .unwrap();
                }
                "deleted" => {
                    sqlx::query("UPDATE note SET deleted_at = now() WHERE id = $1")
                        .bind(id)
                        .execute(&mut *tx)
                        .await
                        .unwrap();
                }
                "archived" => {
                    sqlx::query("UPDATE note SET archived = true WHERE id = $1")
                        .bind(id)
                        .execute(&mut *tx)
                        .await
                        .unwrap();
                }
                _ => {}
            }
        }
    }
    let embedding_id: Uuid =
        sqlx::query_scalar("SELECT id FROM embedding WHERE note_id=$1 AND chunk_index=7")
            .bind(wanted)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    sqlx::query("INSERT INTO source_identity (note_id, source_namespace, external_id, source_schema_version, content_digest, import_run_id) VALUES ($1, 'synthetic', 'other-private-key', '1', $2, 'other-run')")
        .bind(wanted).bind(format!("sha256:{}", "0".repeat(64))).execute(&mut *tx).await.unwrap();
    let allowed_sources = [
        Some(matric_core::search_evidence::EvidenceSource {
            namespace: "synthetic".into(),
            external_id_hash: matric_core::source_upsert::source_identity_digest(
                Uuid::nil(),
                "public",
                "synthetic",
                "wanted-0",
            ),
            import_run_id: "run-42".into(),
            schema_version: "1".into(),
        }),
        Some(matric_core::search_evidence::EvidenceSource {
            namespace: "synthetic".into(),
            external_id_hash: matric_core::source_upsert::source_identity_digest(
                Uuid::nil(),
                "public",
                "synthetic",
                "other-private-key",
            ),
            import_run_id: "other-run".into(),
            schema_version: "1".into(),
        }),
    ];
    let role = format!("search_candidate_{}", Uuid::new_v4().simple());
    sqlx::raw_sql(&format!("CREATE ROLE {role} NOSUPERUSER NOBYPASSRLS; GRANT USAGE ON SCHEMA public TO {role}; GRANT SELECT ON ALL TABLES IN SCHEMA public TO {role}; SET LOCAL ROLE {role};")).execute(&mut *tx).await.unwrap();
    let mut checks = 0;
    for (path, value) in [
        ("provider", json!("fixture")),
        ("model", json!(42)),
        ("role", json!("assistant")),
        ("event_kind", json!("result")),
        ("sensitivity", json!("internal")),
        ("import_run_id", json!("run-42")),
    ] {
        for mode in ["fts", "semantic", "hybrid"] {
            for script in ["latin", "cyrillic", "emoji", "han"] {
                let mut config = match mode {
                    "fts" => HybridSearchConfig::fts_only(),
                    "semantic" => HybridSearchConfig::semantic_only(),
                    _ => HybridSearchConfig::default(),
                };
                config.script_hint = if script == "emoji" {
                    None
                } else {
                    Some(script.into())
                };
                config.metadata_predicates = Some(
                    MetadataPredicates::try_from(json!([{"path":path,"op":"eq","value":value}]))
                        .unwrap(),
                );
                config.embedding_set_id = Some(set);
                let mut strict = StrictTagFilter::new();
                strict.required_string_tags.push("allowed".into());
                config.strict_filter = Some(strict);
                config.diversity = Some(0.2);
                let query = if script == "emoji" {
                    "\u{1f600}"
                } else {
                    "needle"
                };
                let hits = SearchRequest::new(query)
                    .with_config(config)
                    .with_filters("tag:allowed created_after:2026-01-01T00:00:00Z")
                    .with_embedding(vector.clone())
                    .with_limit(1)
                    .execute_on_connection(&engine, &mut tx)
                    .await
                    .unwrap();
                assert_eq!(hits.len(), 1, "{path} {mode} {script}");
                assert_eq!(hits[0].hit.note_id, wanted, "{path} {mode} {script}");
                assert!(!excluded.contains(&hits[0].hit.note_id));
                assert_matched_evidence(
                    &hits[0].hit,
                    mode,
                    embedding_id,
                    if path == "import_run_id" {
                        &allowed_sources[..1]
                    } else {
                        &allowed_sources
                    },
                );
                assert!(!serde_json::to_string(&hits[0])
                    .unwrap()
                    .contains("other-private-key"));
                checks += 1;
            }
        }
    }
    // Changing the same transaction's tenant leaves no visible candidates,
    // including vector and MMR reads. No second connection may recover them.
    sqlx::query("SELECT set_config('app.current_tenant', $1, true)")
        .bind(Uuid::new_v4().to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    for mode in [
        HybridSearchConfig::fts_only(),
        HybridSearchConfig::semantic_only(),
        HybridSearchConfig::default(),
    ] {
        let hits = SearchRequest::new("needle")
            .with_config(mode)
            .with_embedding(vector.clone())
            .with_limit(1)
            .execute_on_connection(&engine, &mut tx)
            .await
            .unwrap();
        assert!(hits.is_empty());
        checks += 1;
    }
    tx.rollback().await.unwrap();
    println!("scoped_retrieval: full migrations; {checks} real retrieval checks; synthetic vectors only; role/fixture transaction rolled back");
}
