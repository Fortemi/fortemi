use matric_core::{
    search_evidence::{EvidenceKind, EvidenceText, MAX_EVIDENCE_BYTES},
    CreateNoteRequest,
};
use matric_db::{
    search_candidates::{
        lexical_on_connection, vector_on_connection, LexicalStrategy, SearchCandidateScope,
    },
    Database,
};
use pgvector::Vector;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

#[sqlx::test(migrations = false)]
async fn matched_units_limits_and_current_state_are_projected_in_native_sql(pool: PgPool) {
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
    let body = "\u{feff}raw needle body \u{1f642}\r\n";
    let attachment_text = "\u{feff}raw needle attachment \u{1f642}\r\n";
    let embedding_text = "\u{feff}raw needle stored chunk \u{1f642}\r\n";
    let note = db
        .notes
        .insert_tx(
            &mut tx,
            CreateNoteRequest {
                content: body.into(),
                format: "markdown".into(),
                source: "synthetic-evidence".into(),
                collection_id: None,
                document_type_id: None,
                title: Some("needle title".into()),
                tags: None,
                metadata: None,
            },
        )
        .await
        .unwrap();
    let blob = Uuid::new_v4();
    let attachment = Uuid::new_v4();
    sqlx::query("INSERT INTO attachment_blob (id, content_hash, content_type, size_bytes, data) VALUES ($1, $2, 'text/plain', 1, $3)").bind(blob).bind("1".repeat(64)).bind(vec![1_u8]).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO attachment (id, note_id, blob_id, filename, status, extracted_text) VALUES ($1, $2, $3, 'synthetic.txt', 'completed', $4)").bind(attachment).bind(note).bind(blob).bind(attachment_text).execute(&mut *tx).await.unwrap();
    let embedding = Uuid::new_v4();
    let vector = Vector::from(vec![1.0_f32; 768]);
    sqlx::query("INSERT INTO embedding (id, note_id, chunk_index, text, vector, model) VALUES ($1, $2, 7, $3, $4, 'synthetic-only')").bind(embedding).bind(note).bind(embedding_text).bind(&vector).execute(&mut *tx).await.unwrap();
    let scope = SearchCandidateScope::default();
    for strategy in [
        LexicalStrategy::English,
        LexicalStrategy::Simple,
        LexicalStrategy::Trigram,
        LexicalStrategy::Bigram,
    ] {
        let hits = lexical_on_connection(&mut tx, "needle", 1, strategy, &scope)
            .await
            .unwrap();
        assert_eq!(hits.len(), 1);
        let evidence = hits[0].evidence.as_ref().unwrap();
        assert_eq!(evidence.locators().len(), 3, "{strategy:?}");
        for locator in evidence.locators() {
            let (id, text) = match locator.unit().kind {
                EvidenceKind::Title => (note, "needle title"),
                EvidenceKind::Current => (note, body),
                EvidenceKind::Attachment => (attachment, attachment_text),
                _ => panic!("unexpected lexical unit"),
            };
            assert_eq!(locator.unit().id, id.to_string());
            assert_eq!(
                locator
                    .resolve(Some(EvidenceText {
                        note_id: &note.to_string(),
                        unit: locator.unit(),
                        content: text,
                        source: None
                    }))
                    .unwrap(),
                text
            );
        }
    }
    let hits = vector_on_connection(&mut tx, &vector, 1, &scope)
        .await
        .unwrap();
    let locator = &hits[0].evidence.as_ref().unwrap().locators()[0];
    assert_eq!(locator.unit().id, embedding.to_string());
    assert_eq!(locator.unit().index, 7);
    assert_eq!(
        locator
            .resolve(Some(EvidenceText {
                note_id: &note.to_string(),
                unit: locator.unit(),
                content: embedding_text,
                source: None
            }))
            .unwrap(),
        embedding_text
    );
    sqlx::query("UPDATE note SET title='other' WHERE id=$1")
        .bind(note)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("UPDATE note_revised_current SET content='other' WHERE note_id=$1")
        .bind(note)
        .execute(&mut *tx)
        .await
        .unwrap();
    for strategy in [
        LexicalStrategy::English,
        LexicalStrategy::Simple,
        LexicalStrategy::Trigram,
        LexicalStrategy::Bigram,
    ] {
        let hits = lexical_on_connection(&mut tx, "needle", 1, strategy, &scope)
            .await
            .unwrap();
        assert_eq!(
            hits.len(),
            1,
            "completed attachment-only match {strategy:?}"
        );
        assert_eq!(hits[0].evidence.as_ref().unwrap().locators().len(), 1);
        assert_eq!(
            hits[0].evidence.as_ref().unwrap().locators()[0].unit().kind,
            EvidenceKind::Attachment
        );
    }
    sqlx::query("UPDATE attachment SET status='failed' WHERE id=$1")
        .bind(attachment)
        .execute(&mut *tx)
        .await
        .unwrap();
    assert!(
        lexical_on_connection(&mut tx, "needle", 1, LexicalStrategy::English, &scope)
            .await
            .unwrap()
            .is_empty()
    );
    sqlx::query("INSERT INTO source_identity (note_id, source_namespace, external_id, source_schema_version, content_digest, import_run_id) SELECT $1, 'synthetic', 'key-' || i, '1', $2, 'run' FROM generate_series(1,65) i").bind(note).bind(format!("sha256:{}", "0".repeat(64))).execute(&mut *tx).await.unwrap();
    let hits = vector_on_connection(&mut tx, &vector, 1, &scope)
        .await
        .unwrap();
    let evidence = serde_json::to_value(&hits[0].evidence).unwrap();
    assert_eq!(evidence["locators"].as_array().unwrap().len(), 64);
    assert_eq!(evidence["omissions"], json!(["locator-limit"]));
    sqlx::query("UPDATE embedding SET text=$1 WHERE id=$2")
        .bind("x".repeat(MAX_EVIDENCE_BYTES + 1))
        .bind(embedding)
        .execute(&mut *tx)
        .await
        .unwrap();
    let hits = vector_on_connection(&mut tx, &vector, 1, &scope)
        .await
        .unwrap();
    let evidence = serde_json::to_value(&hits[0].evidence).unwrap();
    assert_eq!(evidence["locators"], json!([]));
    assert_eq!(
        evidence["omissions"],
        json!(["unavailable-unit", "locator-limit"])
    );
    sqlx::query("UPDATE note SET deleted_at=now() WHERE id=$1")
        .bind(note)
        .execute(&mut *tx)
        .await
        .unwrap();
    assert!(vector_on_connection(&mut tx, &vector, 1, &scope)
        .await
        .unwrap()
        .is_empty());
    tx.rollback().await.unwrap();
}
