//! Scale check for index-backed lexical candidates.
//!
//! Seeds synthetic notes (short titles, JSON-ish pointer bodies, a few
//! attachments) into a disposable database, then reports the latency of the
//! production lexical path and the plan node types it uses.
//!
//! ```text
//! DATABASE_URL=postgres://... cargo run -p matric-db --example fts_scale_check -- \
//!     --migrate --seed 200000
//! ```
//!
//! Never point this at a shared or production database: `--seed` disables
//! triggers for speed and inserts rows.

use std::time::Instant;

use matric_db::{
    create_pool,
    search_candidates::{
        explain_lexical_compiled, lexical_on_connection, normalize_query, LexicalStrategy,
        SearchCandidateScope,
    },
    Database,
};

const SEED_SQL: &str = r#"
SET session_replication_role = replica;
CREATE TEMP TABLE seed AS SELECT uuidv7() id, g FROM generate_series(1, {count}) g;
INSERT INTO note (id, format, source, created_at_utc, updated_at_utc, title)
SELECT id, 'markdown', 'scale', now(), now(),
  CASE WHEN g % 10000 = 0 THEN 'Michigan Technological University record ' || g
       ELSE (ARRAY['Quarterly report','Meeting notes','Reading list','Project plan','Customer call','Design review'])[1 + g % 6]
            || ' ' || (ARRAY['alpha','bravo','charlie','delta','echo','foxtrot','golf','hotel'])[1 + g % 8] || ' ' || g END
FROM seed;
INSERT INTO note_original (note_id, content, hash)
SELECT id, '{"ref":"s3://bucket/obj-' || g || '","kind":"pointer","size":' || (g * 7) || '}', md5(g::text) FROM seed;
INSERT INTO note_revised_current (note_id, content)
SELECT id, '{"ref":"s3://bucket/obj-' || g || '","kind":"pointer","size":' || (g * 7) || ',"note":"'
  || CASE WHEN g % 20000 = 7 THEN 'content only zeppelin mention' ELSE 'plain' END || '"}' FROM seed;
INSERT INTO attachment_blob (id, content_hash, content_type, size_bytes, data)
SELECT id, md5(g::text) || md5((g + 1)::text), 'text/plain', 1, '\x01' FROM seed WHERE g % 100 = 0;
INSERT INTO attachment (note_id, blob_id, filename, status, extracted_text)
SELECT id, id, 'f' || g || '.txt', 'completed',
  CASE WHEN g % 20000 = 100 THEN 'attachment only quokka text' ELSE 'extracted text body ' || g END
FROM seed WHERE g % 100 = 0;
"#;

fn nodes(plan: &serde_json::Value, out: &mut Vec<String>) {
    match plan {
        serde_json::Value::Object(map) => {
            if let Some(kind) = map.get("Node Type").and_then(|v| v.as_str()) {
                let target = map
                    .get("Index Name")
                    .or_else(|| map.get("Relation Name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                out.push(format!("{kind}({target})"));
            }
            map.values().for_each(|v| nodes(v, out));
        }
        serde_json::Value::Array(items) => items.iter().for_each(|v| nodes(v, out)),
        _ => {}
    }
}

#[tokio::main]
async fn main() {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
    let args: Vec<String> = std::env::args().skip(1).collect();
    let pool = create_pool(&url).await.expect("pool");
    if args.iter().any(|a| a == "--migrate") {
        let started = Instant::now();
        Database::new(pool.clone())
            .migrate()
            .await
            .expect("migrate");
        println!("migrate: {:?}", started.elapsed());
    }
    let mut conn = pool.acquire().await.expect("connection");
    sqlx::query(
        "SELECT set_config('app.current_tenant', '00000000-0000-0000-0000-000000000000', false)",
    )
    .execute(&mut *conn)
    .await
    .unwrap();
    if let Some(pos) = args.iter().position(|a| a == "--seed") {
        let count: i32 = args[pos + 1].parse().expect("seed count");
        let started = Instant::now();
        let seed = SEED_SQL.replace("{count}", &count.to_string());
        for statement in seed.split(";\n").filter(|s| !s.trim().is_empty()) {
            sqlx::query(statement)
                .persistent(false)
                .execute(&mut *conn)
                .await
                .unwrap_or_else(|e| panic!("seed failed: {e}\n{statement}"));
        }
        println!("seed {count}: {:?}", started.elapsed());
        for table in ["note", "note_revised_current", "attachment"] {
            sqlx::query(&format!("ANALYZE {table}"))
                .execute(&mut *conn)
                .await
                .unwrap();
        }
    }
    let notes: i64 = sqlx::query_scalar("SELECT count(*) FROM note")
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    println!("notes: {notes}");
    let scope = SearchCandidateScope::default();
    let cases = [
        ("title", "Michigan Technological University"),
        ("content", "zeppelin"),
        ("attachment", "quokka"),
        ("explicit OR", "Michigan OR zeppelin OR quokka"),
        ("explicit NOT", "Michigan -University"),
    ];
    for strategy in [LexicalStrategy::English, LexicalStrategy::Simple] {
        for (label, query) in cases {
            // Warm once, then report the median of five runs.
            let mut times = Vec::new();
            let mut hits = 0;
            for _ in 0..6 {
                let started = Instant::now();
                hits = lexical_on_connection(&mut conn, query, 10, strategy, &scope)
                    .await
                    .unwrap()
                    .len();
                times.push(started.elapsed());
            }
            times.remove(0);
            times.sort();
            println!(
                "{strategy:?} {label:<12} {query:?}: hits={hits} median={:?} max={:?}",
                times[times.len() / 2],
                times[times.len() - 1]
            );
        }
        let parsed =
            matric_db::search_query::ParsedQuery::parse("Michigan Technological University")
                .unwrap();
        let tsquery = normalize_query(&mut conn, &parsed.tsquery().unwrap(), strategy)
            .await
            .unwrap();
        let plan = explain_lexical_compiled(&mut conn, &tsquery, 10, strategy, &scope)
            .await
            .unwrap();
        let mut found = Vec::new();
        nodes(&plan, &mut found);
        println!("{strategy:?} plan nodes: {}", found.join(", "));
    }
}
