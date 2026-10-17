//! Production engine tests on disposable, fully migrated PostgreSQL databases.
//! Synthetic vectors below test constraints/ranking only, never semantic quality.
mod relevance;
use matric_db::Database;
use matric_search::{HybridSearchConfig, HybridSearchEngine, SearchRequest};
use pgvector::Vector;
use serde_json::json;
use sqlx::{PgConnection, PgPool};
use std::collections::HashMap;
use uuid::Uuid;

async fn database(pool: PgPool) -> Database {
    sqlx::query("CREATE EXTENSION IF NOT EXISTS postgis")
        .execute(&pool)
        .await
        .unwrap();
    let db = Database::new(pool);
    db.migrate().await.unwrap();
    db
}

async fn seed(
    connection: &mut PgConnection,
    id: Uuid,
    title: &str,
    content: &str,
    tags: &[String],
) {
    sqlx::query("INSERT INTO note (id, title, format, source, created_at_utc, updated_at_utc) VALUES ($1, $2, 'markdown', 'relevance-fixture', now(), now())")
        .bind(id).bind(title).execute(&mut *connection).await.unwrap();
    sqlx::query("INSERT INTO note_revised_current (note_id, content) VALUES ($1, $2)")
        .bind(id)
        .bind(content)
        .execute(&mut *connection)
        .await
        .unwrap();
    for tag in tags {
        sqlx::query(
            "INSERT INTO tag (name, created_at_utc) VALUES ($1, now()) ON CONFLICT DO NOTHING",
        )
        .bind(tag)
        .execute(&mut *connection)
        .await
        .unwrap();
        sqlx::query("INSERT INTO note_tag (note_id, tag_name) VALUES ($1, $2)")
            .bind(id)
            .bind(tag)
            .execute(&mut *connection)
            .await
            .unwrap();
    }
}

#[sqlx::test(migrations = false)]
async fn explicit_syntax_and_final_match_tiers(pool: PgPool) {
    let db = database(pool).await;
    let engine = HybridSearchEngine::new(db.clone());
    let mut tx = db.pool().begin().await.unwrap();
    sqlx::query("SELECT set_config('app.current_tenant', $1, true)")
        .bind(Uuid::nil().to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    let set: Uuid = sqlx::query_scalar("SELECT id FROM embedding_set WHERE slug='default'")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let query_vector = Vector::from(vec![1.0_f32; 768]);
    for (index, content) in [
        "alpha beta",
        "alpha gamma",
        "alphabet gamma",
        "beta gamma",
        "alpha the beta",
        "irrelevant",
    ]
    .iter()
    .enumerate()
    {
        let id = Uuid::from_u128(index as u128 + 1);
        seed(&mut tx, id, "fixture", content, &[]).await;
        let mut values = vec![1.0_f32; 768];
        if index == 0 {
            values[0] = 10.0;
        }
        sqlx::query("INSERT INTO embedding (note_id, chunk_index, text, vector, model, embedding_set_id) VALUES ($1, 0, $2, $3, 'synthetic-constraint-test', $4)")
            .bind(id).bind(content).bind(Vector::from(values)).bind(set).execute(&mut *tx).await.unwrap();
    }
    for (query, expected) in [
        ("alpha AND beta", vec![1, 5]),
        ("alpha OR beta", vec![1, 2, 4, 5]),
        ("alpha NOT beta", vec![2]),
        ("alpha -beta", vec![2]),
        ("\"alpha beta\"", vec![1]),
        ("\"alpha the beta\"", vec![5]),
        ("alph*", vec![1, 2, 3, 5]),
        ("alph* NOT beta", vec![2, 3]),
        ("NOT alpha", vec![3, 4, 6]),
        ("(alpha OR beta) NOT gamma", vec![1, 5]),
        ("alpha OR beta AND gamma", vec![1, 2, 4, 5]),
        ("alpha AND nonexistent", vec![]),
        ("NOT the", vec![]),
        ("the AND alpha", vec![1, 2, 5]),
        ("\"the and\"", vec![]),
        (r"\alpha", vec![1, 2, 5]),
    ] {
        for config in [
            HybridSearchConfig::fts_only(),
            HybridSearchConfig::default().with_diversity(0.9),
            HybridSearchConfig::semantic_only(),
        ] {
            let results = SearchRequest::new(query)
                .with_config(config)
                .with_embedding(query_vector.clone())
                .with_limit(20)
                .execute_on_connection(&engine, &mut tx)
                .await
                .unwrap();
            let mut actual: Vec<_> = results.iter().map(|h| h.hit.note_id.as_u128()).collect();
            actual.sort();
            assert_eq!(actual, expected, "query {query}");
        }
    }
    for query in ["alpha OR", "(alpha", "\"alpha", "prefix**"] {
        assert!(SearchRequest::new(query)
            .with_embedding(query_vector.clone())
            .execute_on_connection(&engine, &mut tx)
            .await
            .is_err());
    }
    for diversity in [0.0, 0.9, 1.0] {
        for dedup in [false, true] {
            let config = HybridSearchConfig::default()
                .with_diversity(diversity)
                .with_deduplication_enabled(dedup);
            let results = SearchRequest::new("alpha beta")
                .with_config(config)
                .with_embedding(query_vector.clone())
                .with_limit(20)
                .execute_on_connection(&engine, &mut tx)
                .await
                .unwrap();
            let ids: Vec<_> = results.iter().map(|h| h.hit.note_id.as_u128()).collect();
            assert_eq!(ids.len(), 4);
            assert!(
                ids[..2].contains(&1) && ids[..2].contains(&5),
                "full matches must lead: {ids:?}"
            );
            assert!(!ids.contains(&6));
        }
    }
    let strict = HybridSearchConfig {
        query_policy: matric_search::query_policy::QueryPolicyConfig {
            scarcity_threshold: 2,
            ..Default::default()
        },
        ..HybridSearchConfig::fts_only()
    };
    let hits = SearchRequest::new("alpha beta")
        .with_config(strict)
        .execute_on_connection(&engine, &mut tx)
        .await
        .unwrap();
    assert_eq!(hits.len(), 2, "sufficient short query stays strict");
    let minimum = HybridSearchConfig {
        query_policy: matric_search::query_policy::QueryPolicyConfig {
            minimum_should_match_ratio: 0.66,
            ..Default::default()
        },
        ..HybridSearchConfig::fts_only()
    };
    let hits = SearchRequest::new("alpha beta gamma")
        .with_config(minimum)
        .execute_on_connection(&engine, &mut tx)
        .await
        .unwrap();
    assert_eq!(hits.len(), 4);
    // Exclusions cover every field; phrases cannot bridge title/body units.
    seed(&mut tx, Uuid::from_u128(7), "alpha", "beta", &[]).await;
    for query in ["alpha NOT beta", "\"alpha beta\""] {
        let hits = SearchRequest::new(query)
            .with_config(HybridSearchConfig::fts_only())
            .execute_on_connection(&engine, &mut tx)
            .await
            .unwrap();
        assert!(!hits.iter().any(|h| h.hit.note_id == Uuid::from_u128(7)));
    }
    let empty = SearchRequest::new("the and")
        .with_config(HybridSearchConfig::fts_only())
        .execute_on_connection(&engine, &mut tx)
        .await
        .unwrap();
    assert!(empty.is_empty());
    let long_query = "cobalt silver nickel copper zinc iron";
    for id in 10..16 {
        seed(
            &mut tx,
            Uuid::from_u128(id),
            "metals",
            if id == 15 { "cobalt" } else { long_query },
            &[],
        )
        .await;
    }
    for (threshold, expected) in [(6, 6), (7, 5)] {
        let config = HybridSearchConfig {
            query_policy: matric_search::query_policy::QueryPolicyConfig {
                long_query_terms: threshold,
                ..Default::default()
            },
            ..HybridSearchConfig::fts_only()
        };
        let hits = SearchRequest::new(long_query)
            .with_config(config)
            .with_limit(100)
            .execute_on_connection(&engine, &mut tx)
            .await
            .unwrap();
        assert_eq!(hits.len(), expected, "long-query boundary");
    }
    let config = HybridSearchConfig {
        query_policy: matric_search::query_policy::QueryPolicyConfig {
            candidate_limit: 2,
            ..Default::default()
        },
        ..HybridSearchConfig::fts_only()
    };
    let hits = SearchRequest::new(long_query)
        .with_config(config)
        .with_limit(100)
        .execute_on_connection(&engine, &mut tx)
        .await
        .unwrap();
    assert!(hits.len() <= 4, "strict + expanded reservoirs are bounded");
}

#[sqlx::test(migrations = false)]
async fn golden_relevance_report(pool: PgPool) {
    let fixture: relevance::Fixture =
        serde_json::from_str(include_str!("fixtures/golden_test_set.json")).unwrap();
    assert_eq!(fixture.notes.len(), 15);
    assert_eq!(fixture.queries.len(), 8);
    let db = database(pool).await;
    let engine = HybridSearchEngine::new(db.clone());
    let mut tx = db.pool().begin().await.unwrap();
    sqlx::query("SELECT set_config('app.current_tenant', $1, true)")
        .bind(Uuid::nil().to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    // UUID order equals fixture-key order, independent of insertion/hash order.
    let mut keys: Vec<_> = fixture.notes.iter().map(|n| n.id.clone()).collect();
    keys.sort();
    let names: HashMap<_, _> = keys
        .iter()
        .enumerate()
        .map(|(i, key)| (Uuid::from_u128(i as u128 + 1), key.clone()))
        .collect();
    for note in &fixture.notes {
        let id = *names.iter().find(|(_, name)| *name == &note.id).unwrap().0;
        seed(&mut tx, id, &note.title, &note.content, &note.tags).await;
    }
    let mut runs = Vec::new();
    for mode in ["strict", "or", "adaptive"] {
        let mut per_query = Vec::new();
        let mut metrics = Vec::new();
        let mut latencies = Vec::new();
        for query in &fixture.queries {
            let text = if mode == "or" {
                query
                    .text
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" OR ")
            } else {
                query.text.clone()
            };
            let mut config = HybridSearchConfig {
                query_policy: Default::default(),
                ..HybridSearchConfig::fts_only()
            };
            config.query_policy.enabled = mode == "adaptive";
            let mut ranking = None;
            // Repeat the actual engine to detect unstable fusion/dedup ties.
            for _ in 0..10 {
                let start = std::time::Instant::now();
                let hits = SearchRequest::new(&text)
                    .with_config(config.clone())
                    .with_limit(100)
                    .execute_on_connection(&engine, &mut tx)
                    .await
                    .unwrap();
                latencies.push(start.elapsed().as_secs_f64() * 1000.0);
                let ids: Vec<_> = hits.iter().map(|h| names[&h.hit.note_id].clone()).collect();
                if let Some(previous) = &ranking {
                    assert_eq!(&ids, previous, "nondeterministic {mode}/{}", query.id);
                }
                ranking = Some(ids);
            }
            let ranking = ranking.unwrap();
            let measured = relevance::evaluate(&ranking, &query.relevant_notes);
            per_query.push(json!({"id": query.id, "text": query.text, "ranking": ranking, "metrics": measured}));
            metrics.push(measured);
        }
        latencies.sort_by(f64::total_cmp);
        runs.push(json!({"mode": mode, "requested_mode": "fts", "effective_mode": "fts", "degraded": false,
            "metrics": relevance::mean(&metrics), "queries": per_query,
            "zero_result_rate": per_query.iter().filter(|q| q["ranking"].as_array().unwrap().is_empty()).count() as f64 / fixture.queries.len() as f64,
            "p95_ms": latencies[(latencies.len() * 95).div_ceil(100) - 1]}));
    }
    let report = json!({"evaluator": "binary-v1", "route": "SearchRequest::execute_on_connection", "fixture_version": "1.0", "repetitions": 10, "runs": runs});
    println!(
        "RELEVANCE_REPORT={}",
        serde_json::to_string(&report).unwrap()
    );
    if let Ok(path) = std::env::var("FORTEMI_RELEVANCE_REPORT") {
        std::fs::write(path, serde_json::to_string_pretty(&report).unwrap()).unwrap();
    }
}
