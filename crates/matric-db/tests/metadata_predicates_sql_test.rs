//! Native PostgreSQL counterpart of the candidate shared SQL corpus.
//! SQLx creates a fresh database; the migration never targets a developer database.

use matric_core::metadata_search::MetadataPredicates;
use matric_db::{metadata_predicates::MetadataPredicateQueryBuilder, strict_filter::QueryParam};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection, PgPool};

const CORPUS: &str =
    include_str!("../../../contracts/metadata-search/candidate/1.0.0/predicate-vectors.json");
const SCOPES: &str =
    include_str!("../../../contracts/metadata-search/candidate/1.0.0/sql-scope-vectors.json");
const MIGRATION: &str =
    include_str!("../../../migrations/20260912000000_typed_metadata_search_indexes.sql");

async fn insert(conn: &mut PgConnection, rows: &[Value]) {
    for row in rows {
        let mut metadata = row.get("metadata").cloned().unwrap_or(json!({}));
        if row.get("appendFixture").is_some() {
            assert_eq!(row["appendFixture"], "long-uncompressible-ascii");
            let suffix: String = (0..512)
                .map(|i| {
                    hex::encode(Sha256::digest(
                        format!("synthetic-long-metadata-{i}").as_bytes(),
                    ))
                })
                .collect();
            metadata["model"] = json!(metadata["model"].as_str().unwrap().to_owned() + &suffix);
        }
        let raw = row
            .get("metadataJson")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| metadata.to_string());
        sqlx::query("INSERT INTO note VALUES ($1, 'tenant-a', $2::jsonb)")
            .bind(row["id"].as_str().unwrap())
            .bind(raw)
            .execute(&mut *conn)
            .await
            .unwrap();
        let identities = row.get("identities").cloned().unwrap_or_else(|| {
            row.get("importRunId")
                .map(|run| json!([{"tenant": "tenant-a", "run": run}]))
                .unwrap_or(json!([]))
        });
        for identity in identities.as_array().unwrap() {
            sqlx::query("INSERT INTO source_identity VALUES ($1, $2, $3)")
                .bind(identity["tenant"].as_str().unwrap())
                .bind(row["id"].as_str().unwrap())
                .bind(identity["run"].as_str().unwrap())
                .execute(&mut *conn)
                .await
                .unwrap();
        }
    }
}

fn compile(case: &Value) -> (String, Vec<String>) {
    let predicates = MetadataPredicates::try_from(case["predicates"].clone()).unwrap();
    let (sql, params) = MetadataPredicateQueryBuilder::new(&predicates, 0).build();
    (
        sql,
        params
            .into_iter()
            .map(|param| match param {
                QueryParam::String(value) => value,
                _ => panic!("unexpected metadata parameter type"),
            })
            .collect(),
    )
}

async fn ids(conn: &mut PgConnection, case: &Value, tenant_where: bool) -> Vec<String> {
    let (clause, params) = compile(case);
    let sql = format!(
        "SELECT n.id FROM note n WHERE {}{clause}",
        if tenant_where {
            "n.tenant_id = 'tenant-a' AND "
        } else {
            ""
        }
    );
    let mut query = sqlx::query_scalar::<_, String>(&sql);
    for param in params {
        query = query.bind(param);
    }
    let mut result = query.fetch_all(conn).await.unwrap();
    result.sort();
    result
}

fn uses_index(plan: &Value, name: &str) -> bool {
    plan["Index Name"] == name
        || plan["Plans"]
            .as_array()
            .is_some_and(|children| children.iter().any(|child| uses_index(child, name)))
}

#[sqlx::test(migrations = false)]
async fn metadata_predicates_native_sql(pool: PgPool) {
    let corpus: Value = serde_json::from_str(CORPUS).unwrap();
    let scopes: Value = serde_json::from_str(SCOPES).unwrap();
    let mut conn = pool.acquire().await.unwrap();
    sqlx::raw_sql("CREATE TABLE public.archive_registry (schema_name text PRIMARY KEY);
        INSERT INTO public.archive_registry VALUES ('public'), ('archive_existing');
        CREATE TABLE public.note (id text PRIMARY KEY, tenant_id text NOT NULL, metadata jsonb NOT NULL);
        CREATE TABLE public.source_identity (tenant_id text NOT NULL, note_id text NOT NULL, import_run_id text NOT NULL CHECK (length(import_run_id) BETWEEN 1 AND 200));
        CREATE SCHEMA archive_existing;
        CREATE TABLE archive_existing.note (LIKE public.note INCLUDING ALL);
        CREATE TABLE archive_existing.source_identity (LIKE public.source_identity INCLUDING ALL);")
        .execute(&mut *conn).await.unwrap();
    let mut large_rows = scopes["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["id"] == "numeric-zero-recheck")
        .unwrap()["rows"]
        .as_array()
        .unwrap()
        .clone();
    for row in scopes["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["id"] == "string-prefix-equality-recheck")
        .unwrap()["rows"]
        .as_array()
        .unwrap()
    {
        let mut row = row.clone();
        row["id"] = json!(format!("large-{}", row["id"].as_str().unwrap()));
        large_rows.push(row);
    }
    insert(&mut conn, &large_rows).await;
    sqlx::raw_sql("INSERT INTO archive_existing.note SELECT * FROM public.note")
        .execute(&mut *conn)
        .await
        .unwrap();
    let mut migration_tx = conn.begin().await.unwrap();
    sqlx::raw_sql(MIGRATION)
        .execute(&mut *migration_tx)
        .await
        .unwrap();
    migration_tx.commit().await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_indexes WHERE schemaname IN ('public', 'archive_existing') AND (indexname LIKE 'idx_note_metadata_%_v1' OR indexname LIKE 'idx_source_identity_metadata_%_v1')")
        .fetch_one(&mut *conn).await.unwrap();
    assert_eq!(count, 14);
    sqlx::raw_sql(
        "CREATE SCHEMA archive_new;
        CREATE TABLE archive_new.note (LIKE public.note INCLUDING ALL);
        CREATE TABLE archive_new.source_identity (LIKE public.source_identity INCLUDING ALL);",
    )
    .execute(&mut *conn)
    .await
    .unwrap();
    let cloned: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_indexes WHERE schemaname = 'archive_new' AND indexdef LIKE '%metadata_search_order_key_v1%'")
        .fetch_one(&mut *conn).await.unwrap();
    assert_eq!(cloned, 5);
    let mut checked = 0;
    for schema in ["public", "archive_existing", "archive_new"] {
        sqlx::raw_sql(&format!("SET search_path TO {schema}, public"))
            .execute(&mut *conn)
            .await
            .unwrap();
        for case in corpus["cases"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|case| case["valid"] == true)
            .chain(scopes["cases"].as_array().unwrap())
        {
            sqlx::raw_sql("TRUNCATE source_identity, note")
                .execute(&mut *conn)
                .await
                .unwrap();
            insert(
                &mut conn,
                case.get("rows")
                    .unwrap_or(&corpus["rows"])
                    .as_array()
                    .unwrap(),
            )
            .await;
            let mut expected: Vec<String> =
                serde_json::from_value(case["expectedIds"].clone()).unwrap();
            expected.sort();
            assert_eq!(
                ids(&mut conn, case, true).await,
                expected,
                "{schema}:{}",
                case["id"]
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 120);
    sqlx::raw_sql("SET search_path TO public; TRUNCATE source_identity, note")
        .execute(&mut *conn)
        .await
        .unwrap();
    insert(
        &mut conn,
        &[json!({"id": "same-note-id", "metadata": {"model": 2}, "importRunId": "run-1"})],
    )
    .await;
    sqlx::raw_sql("SET search_path TO archive_existing, public; TRUNCATE source_identity, note")
        .execute(&mut *conn)
        .await
        .unwrap();
    insert(
        &mut conn,
        &[json!({"id": "same-note-id", "metadata": {"model": "2"}})],
    )
    .await;
    for name in ["numeric-equality", "import-run-equality"] {
        let case = corpus["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["id"] == name)
            .unwrap();
        assert!(
            ids(&mut conn, case, false).await.is_empty(),
            "cross-archive {name}"
        );
    }
    sqlx::raw_sql("SET search_path TO public; TRUNCATE source_identity, note;
        INSERT INTO note SELECT 'plan-' || i, 'tenant-a', jsonb_build_object('model', i, 'provider', 'provider-' || i) FROM generate_series(1, 8192) i;
        INSERT INTO source_identity SELECT 'tenant-a', 'plan-' || i, 'run-' || i FROM generate_series(1, 8192) i;
        ANALYZE note; ANALYZE source_identity;")
        .execute(&mut *conn).await.unwrap();
    for case in scopes["planPredicates"].as_array().unwrap() {
        assert_eq!(ids(&mut conn, case, true).await, ["plan-777"]);
        let (clause, params) = compile(case);
        let sql = format!("EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) SELECT n.id FROM note n WHERE n.tenant_id = 'tenant-a' AND {clause}");
        let mut query = sqlx::query_scalar::<_, Value>(&sql);
        for param in params {
            query = query.bind(param);
        }
        let plan = query.fetch_one(&mut *conn).await.unwrap();
        assert!(
            uses_index(&plan[0]["Plan"], case["expectedIndex"].as_str().unwrap()),
            "index plan {}",
            case["id"]
        );
    }

    // Role DDL and policy changes roll back, including on assertion failure.
    let mut tx = conn.begin().await.unwrap();
    let role = format!("metadata_reader_{}", uuid::Uuid::new_v4().simple());
    sqlx::raw_sql(&format!("INSERT INTO note VALUES ('other-tenant-777', 'tenant-b', '{{\"model\":777,\"provider\":\"provider-777\"}}'::jsonb);
        INSERT INTO source_identity VALUES ('tenant-b', 'other-tenant-777', 'run-777');
        CREATE ROLE {role}; GRANT USAGE ON SCHEMA public TO {role}; GRANT SELECT ON note, source_identity TO {role};
        ALTER TABLE note ENABLE ROW LEVEL SECURITY; ALTER TABLE note FORCE ROW LEVEL SECURITY;
        ALTER TABLE source_identity ENABLE ROW LEVEL SECURITY; ALTER TABLE source_identity FORCE ROW LEVEL SECURITY;
        CREATE POLICY metadata_test_tenant ON note USING (tenant_id = current_setting('app.current_tenant', true));
        CREATE POLICY metadata_test_tenant ON source_identity USING (tenant_id = current_setting('app.current_tenant', true));
        SET LOCAL ROLE {role};"))
        .execute(&mut *tx).await.unwrap();
    for (tenant, expected) in [
        ("tenant-a", vec!["plan-777"]),
        ("tenant-b", vec!["other-tenant-777"]),
        ("", vec![]),
    ] {
        sqlx::query("SELECT set_config('app.current_tenant', $1, true)")
            .bind(tenant)
            .execute(&mut *tx)
            .await
            .unwrap();
        for case in scopes["planPredicates"].as_array().unwrap() {
            assert_eq!(
                ids(&mut tx, case, false).await,
                expected,
                "role {tenant}:{}",
                case["id"]
            );
        }
    }
    tx.rollback().await.unwrap();
    let residual: Option<String> =
        sqlx::query_scalar("SELECT current_setting('app.current_tenant', true)")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
    assert!(residual.is_none_or(|tenant| tenant.is_empty()));
    println!("122 native SQL cases, three natural index plans, nine RLS query checks and transaction-local reset passed");
}
