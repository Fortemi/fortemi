use std::{error::Error as StdError, str::FromStr};

use crate::{create_pool, Database};
use sqlx::migrate::MigrateDatabase;
use sqlx::postgres::PgConnectOptions;
use sqlx::{ConnectOptions, Row};
use uuid::Uuid;

static MIGRATION_REPAIR_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[tokio::test]
async fn skos_concept_status_and_relation_writers_share_final_state_coordination() {
    let _guard = MIGRATION_REPAIR_TEST_LOCK.lock().await;
    let database = DisposableDatabase::create("skos_status_coordination")
        .await
        .unwrap();
    let pool = create_pool(&database.url).await.unwrap();
    let tenant = Uuid::nil();
    sqlx::raw_sql("CREATE TABLE public.tenant_registry(id UUID PRIMARY KEY);
        INSERT INTO public.tenant_registry VALUES('00000000-0000-0000-0000-000000000000');
        CREATE TABLE public.archive_registry(schema_name TEXT);
        INSERT INTO public.archive_registry VALUES('archive_status_guard');
        CREATE TABLE public.skos_concept(id UUID PRIMARY KEY,depth INTEGER DEFAULT 0,status TEXT DEFAULT 'approved');
        CREATE TABLE public.skos_semantic_relation_edge(id UUID PRIMARY KEY,subject_id UUID,object_id UUID,relation_type TEXT);
        CREATE FUNCTION public.skos_has_circular_hierarchy(UUID) RETURNS boolean LANGUAGE sql AS 'SELECT false';
        CREATE SCHEMA archive_status_guard;
        CREATE TABLE archive_status_guard.skos_concept(LIKE public.skos_concept INCLUDING ALL);
        CREATE TABLE archive_status_guard.skos_semantic_relation_edge(LIKE public.skos_semantic_relation_edge INCLUDING ALL);")
        .execute(&pool).await.unwrap();
    for migration in [
        include_str!("../../../../migrations/20260911020400_skos_retained_relation_limits.sql"),
        include_str!("../../../../migrations/20260911020500_skos_prospective_hierarchy.sql"),
        include_str!("../../../../migrations/20260911020600_skos_atomic_relation_batches.sql"),
        include_str!(
            "../../../../migrations/20260911020700_skos_relation_writer_serialization.sql"
        ),
    ] {
        sqlx::raw_sql(migration).execute(&pool).await.unwrap();
    }
    let functions_query =
        "SELECT pg_get_functiondef(oid) FROM pg_proc WHERE pronamespace='public'::regnamespace
        AND proname IN ('skos_validate_broader_relation','skos_validate_narrower_relation',
        'skos_validate_relation_final_state','skos_serialize_relation_writes') ORDER BY proname";
    let functions = sqlx::query_scalar::<_, String>(functions_query)
        .fetch_all(&pool)
        .await
        .unwrap();
    for _ in 0..2 {
        sqlx::raw_sql(include_str!(
            "../../../../migrations/20260911020800_skos_concept_status_constraints.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();
    }
    assert_eq!(
        sqlx::query_scalar::<_, String>(functions_query)
            .fetch_all(&pool)
            .await
            .unwrap(),
        functions
    );
    for schema in ["public", "archive_status_guard"] {
        let flags:(bool,bool,bool) = sqlx::query_as("SELECT tgdeferrable,tginitdeferred,tgqual IS NULL
            FROM pg_trigger WHERE tgrelid=to_regclass($1) AND tgname='trg_skos_validate_concept_status_final_state'")
            .bind(format!("{schema}.skos_concept")).fetch_one(&pool).await.unwrap();
        assert_eq!(flags, (true, true, true));
        for isolation in ["READ COMMITTED", "REPEATABLE READ", "SERIALIZABLE"] {
            for restore in [false, true] {
                for commit_first in [false, true] {
                    for status_first in [false, true] {
                        let nodes = (0..202).map(|_| Uuid::new_v4()).collect::<Vec<_>>();
                        let mut seed = pool.begin().await.unwrap();
                        sqlx::raw_sql(&format!("SET LOCAL search_path TO {schema},public; SET LOCAL app.current_tenant='{tenant}'"))
                            .execute(&mut *seed).await.unwrap();
                        sqlx::query("INSERT INTO skos_concept(id,status) SELECT id,CASE WHEN id=$2 THEN 'candidate' ELSE 'approved' END
                            FROM unnest($1::uuid[]) AS id").bind(&nodes).bind(nodes[200]).execute(&mut *seed).await.unwrap();
                        sqlx::query("INSERT INTO skos_semantic_relation_edge SELECT gen_random_uuid(),$1,id,'narrower'
                            FROM unnest($2::uuid[]) AS id").bind(nodes[0]).bind(&nodes[1..201]).execute(&mut *seed).await.unwrap();
                        seed.commit().await.unwrap();
                        let mut first = pool.begin().await.unwrap();
                        let mut second = pool.begin().await.unwrap();
                        for tx in [&mut first, &mut second] {
                            sqlx::raw_sql(&format!("SET TRANSACTION ISOLATION LEVEL {isolation};
                                SET LOCAL search_path TO {schema},public; SET LOCAL app.current_tenant='{tenant}';
                                SET LOCAL app.shard_import='{}'; SET LOCAL statement_timeout='5s'",if restore {"on"} else {"off"}))
                                .execute(&mut **tx).await.unwrap();
                        }
                        let first_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
                            .fetch_one(&mut *first)
                            .await
                            .unwrap();
                        let second_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
                            .fetch_one(&mut *second)
                            .await
                            .unwrap();
                        let count_query="SELECT count(*) FROM skos_semantic_relation_edge e JOIN skos_concept c ON c.id=e.object_id
                            WHERE e.subject_id=$1 AND e.relation_type='narrower' AND c.status='approved'";
                        assert_eq!(
                            sqlx::query_scalar::<_, i64>(count_query)
                                .bind(nodes[0])
                                .fetch_one(&mut *second)
                                .await
                                .unwrap(),
                            199
                        );
                        let promote = "UPDATE skos_concept SET status='approved' WHERE id=$1";
                        let add="INSERT INTO skos_semantic_relation_edge VALUES(gen_random_uuid(),$1,$2,'narrower')";
                        let mut query = sqlx::query(if status_first { promote } else { add });
                        if status_first {
                            query = query.bind(nodes[200]);
                        } else {
                            query = query.bind(nodes[0]).bind(nodes[201]);
                        }
                        query.execute(&mut *first).await.unwrap();
                        let guard_query=format!("SELECT to_jsonb(g) FROM {schema}.skos_relation_write_guard g WHERE tenant_id=$1");
                        let guard_before: serde_json::Value = sqlx::query_scalar(&guard_query)
                            .bind(tenant)
                            .fetch_one(&pool)
                            .await
                            .unwrap();
                        let subject = nodes[0];
                        let candidate = nodes[200];
                        let extra = nodes[201];
                        let worker = tokio::spawn(async move {
                            let mut query = sqlx::query(if status_first { add } else { promote });
                            if status_first {
                                query = query.bind(subject).bind(extra);
                            } else {
                                query = query.bind(candidate);
                            }
                            if let Err(error) = query.execute(&mut *second).await {
                                second.rollback().await.unwrap();
                                return Err(error);
                            }
                            // Neither caller search_path nor restore flag may redirect/suppress the deferred guard.
                            sqlx::query("SET LOCAL search_path TO pg_catalog")
                                .execute(&mut *second)
                                .await
                                .unwrap();
                            sqlx::query("SET LOCAL app.shard_import='off'")
                                .execute(&mut *second)
                                .await
                                .unwrap();
                            second.commit().await
                        });
                        let deadline =
                            tokio::time::Instant::now() + std::time::Duration::from_secs(2);
                        let mut blocked = false;
                        while tokio::time::Instant::now() < deadline {
                            blocked = sqlx::query_scalar::<_, bool>(
                                "SELECT $1=ANY(pg_blocking_pids($2))",
                            )
                            .bind(first_pid)
                            .bind(second_pid)
                            .fetch_one(&pool)
                            .await
                            .unwrap();
                            if blocked || worker.is_finished() {
                                break;
                            }
                            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                        }
                        if commit_first {
                            first.commit().await.unwrap();
                        } else {
                            first.rollback().await.unwrap();
                        }
                        let result = worker.await.unwrap();
                        assert!(
                            blocked,
                            "{schema}/{isolation}/{restore}/{commit_first}/{status_first}"
                        );
                        if commit_first {
                            let error = result.unwrap_err();
                            if isolation == "READ COMMITTED" {
                                assert!(error.to_string().contains("Breadth limit"), "{error}");
                            } else {
                                assert_eq!(
                                    error.as_database_error().unwrap().code().as_deref(),
                                    Some("40001")
                                );
                            }
                        } else {
                            result.unwrap();
                        }
                        let mut verify = pool.begin().await.unwrap();
                        sqlx::raw_sql(&format!("SET LOCAL search_path TO {schema},public; SET LOCAL app.current_tenant='{tenant}'"))
                            .execute(&mut *verify).await.unwrap();
                        assert_eq!(
                            sqlx::query_scalar::<_, i64>(count_query)
                                .bind(subject)
                                .fetch_one(&mut *verify)
                                .await
                                .unwrap(),
                            200
                        );
                        verify.rollback().await.unwrap();
                        assert_eq!(
                            sqlx::query_scalar::<_, serde_json::Value>(&guard_query)
                                .bind(tenant)
                                .fetch_one(&pool)
                                .await
                                .unwrap(),
                            guard_before
                        );
                    }
                }
            }
        }
        // A promotion before a balancing demotion is valid at commit, even with
        // repeated queued status events. A subsequently deleted row is irrelevant.
        let mut tx = pool.begin().await.unwrap();
        sqlx::raw_sql(&format!(
            "SET LOCAL search_path TO {schema},public; SET LOCAL app.current_tenant='{tenant}'"
        ))
        .execute(&mut *tx)
        .await
        .unwrap();
        let nodes = (0..203).map(|_| Uuid::new_v4()).collect::<Vec<_>>();
        sqlx::query("INSERT INTO skos_concept(id,status) SELECT id,CASE WHEN id=$2 THEN 'candidate' ELSE 'approved' END
            FROM unnest($1::uuid[]) AS id").bind(&nodes).bind(nodes[201]).execute(&mut *tx).await.unwrap();
        sqlx::query(
            "INSERT INTO skos_semantic_relation_edge SELECT gen_random_uuid(),$1,id,'narrower'
            FROM unnest($2::uuid[]) AS id",
        )
        .bind(nodes[0])
        .bind(&nodes[1..202])
        .execute(&mut *tx)
        .await
        .unwrap();
        tx.commit().await.unwrap();
        for restore in [false, true] {
            let mut tx = pool.begin().await.unwrap();
            sqlx::raw_sql(&format!(
                "SET LOCAL search_path TO {schema},public; SET LOCAL app.current_tenant='{tenant}';
                SET LOCAL app.shard_import='{}'",
                if restore { "on" } else { "off" }
            ))
            .execute(&mut *tx)
            .await
            .unwrap();
            for (id, status) in [
                (nodes[201], "approved"),
                (nodes[201], "candidate"),
                (nodes[201], "approved"),
                (nodes[1], "candidate"),
            ] {
                sqlx::query("UPDATE skos_concept SET status=$2 WHERE id=$1")
                    .bind(id)
                    .bind(status)
                    .execute(&mut *tx)
                    .await
                    .unwrap();
            }
            let deleted = Uuid::new_v4();
            sqlx::query("INSERT INTO skos_concept(id) VALUES($1)")
                .bind(deleted)
                .execute(&mut *tx)
                .await
                .unwrap();
            sqlx::query("DELETE FROM skos_concept WHERE id=$1")
                .bind(deleted)
                .execute(&mut *tx)
                .await
                .unwrap();
            sqlx::query("SET LOCAL search_path TO pg_catalog")
                .execute(&mut *tx)
                .await
                .unwrap();
            tx.commit().await.unwrap();
        }
    }
    pool.close().await;
}

#[tokio::test]
async fn skos_relation_writers_serialize_across_isolation_levels_without_logical_drift() {
    let _guard = MIGRATION_REPAIR_TEST_LOCK.lock().await;
    let database = DisposableDatabase::create("skos_writer_serialization")
        .await
        .unwrap();
    let pool = create_pool(&database.url).await.unwrap();
    let tenant = Uuid::nil();
    let other_tenant = Uuid::new_v4();
    sqlx::raw_sql("CREATE TABLE public.tenant_registry(id UUID PRIMARY KEY);
        CREATE TABLE public.archive_registry(schema_name TEXT);
        INSERT INTO public.archive_registry VALUES('archive_writer_guard');
        CREATE TABLE public.skos_concept(id UUID PRIMARY KEY,depth INTEGER DEFAULT 0,status TEXT DEFAULT 'approved');
        CREATE TABLE public.skos_semantic_relation_edge(id UUID PRIMARY KEY,subject_id UUID,object_id UUID,relation_type TEXT);
        CREATE FUNCTION public.skos_has_circular_hierarchy(UUID) RETURNS boolean LANGUAGE sql AS 'SELECT false';
        CREATE SCHEMA archive_writer_guard;
        CREATE TABLE archive_writer_guard.skos_concept(LIKE public.skos_concept INCLUDING ALL);
        CREATE TABLE archive_writer_guard.skos_semantic_relation_edge(LIKE public.skos_semantic_relation_edge INCLUDING ALL);")
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO public.tenant_registry VALUES($1),($2)")
        .bind(tenant)
        .bind(other_tenant)
        .execute(&pool)
        .await
        .unwrap();
    for migration in [
        include_str!("../../../../migrations/20260911020400_skos_retained_relation_limits.sql"),
        include_str!("../../../../migrations/20260911020500_skos_prospective_hierarchy.sql"),
        include_str!("../../../../migrations/20260911020600_skos_atomic_relation_batches.sql"),
        include_str!(
            "../../../../migrations/20260911020700_skos_relation_writer_serialization.sql"
        ),
        include_str!(
            "../../../../migrations/20260911020700_skos_relation_writer_serialization.sql"
        ),
    ] {
        sqlx::raw_sql(migration).execute(&pool).await.unwrap();
    }
    for schema in ["public", "archive_writer_guard"] {
        let flags: (bool, bool, bool) = sqlx::query_as(
            "SELECT relrowsecurity,relforcerowsecurity,
            EXISTS(SELECT 1 FROM pg_policy WHERE polrelid=c.oid AND polname='tenant_isolation')
            FROM pg_class c WHERE c.oid=to_regclass($1)",
        )
        .bind(format!("{schema}.skos_relation_write_guard"))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(flags, (true, true, true));
        for isolation in ["READ COMMITTED", "REPEATABLE READ", "SERIALIZABLE"] {
            for restore in [false, true] {
                for commit_first in [false, true] {
                    let nodes = (0..5).map(|_| Uuid::new_v4()).collect::<Vec<_>>();
                    let mut seed = pool.begin().await.unwrap();
                    sqlx::raw_sql(&format!("SET LOCAL search_path TO {schema},public; SET LOCAL app.current_tenant='{tenant}'"))
                        .execute(&mut *seed).await.unwrap();
                    for object in &nodes[1..3] {
                        sqlx::query(
                            "INSERT INTO skos_semantic_relation_edge VALUES($1,$2,$3,'broader')",
                        )
                        .bind(Uuid::new_v4())
                        .bind(nodes[0])
                        .bind(object)
                        .execute(&mut *seed)
                        .await
                        .unwrap();
                    }
                    seed.commit().await.unwrap();
                    let guard_query=format!("SELECT jsonb_agg(to_jsonb(g) ORDER BY tenant_id) FROM {schema}.skos_relation_write_guard g");
                    let mut first = pool.begin().await.unwrap();
                    let mut second = pool.begin().await.unwrap();
                    for tx in [&mut first, &mut second] {
                        sqlx::raw_sql(&format!("SET TRANSACTION ISOLATION LEVEL {isolation};
                            SET LOCAL search_path TO {schema},public; SET LOCAL app.current_tenant='{tenant}';
                            SET LOCAL app.shard_import='{}'; SET LOCAL statement_timeout='5s'",if restore {"on"} else {"off"}))
                            .execute(&mut **tx).await.unwrap();
                    }
                    let first_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
                        .fetch_one(&mut *first)
                        .await
                        .unwrap();
                    let second_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
                        .fetch_one(&mut *second)
                        .await
                        .unwrap();
                    assert_eq!(
                        sqlx::query_scalar::<_, i64>(
                            "SELECT count(*) FROM skos_semantic_relation_edge WHERE subject_id=$1"
                        )
                        .bind(nodes[0])
                        .fetch_one(&mut *second)
                        .await
                        .unwrap(),
                        2
                    );
                    sqlx::query(
                        "INSERT INTO skos_semantic_relation_edge VALUES($1,$2,$3,'broader')",
                    )
                    .bind(Uuid::new_v4())
                    .bind(nodes[0])
                    .bind(nodes[3])
                    .execute(&mut *first)
                    .await
                    .unwrap();
                    // Other tenant and other archive operations must not share this guard.
                    for (probe_schema, probe_tenant) in [
                        (schema, other_tenant),
                        (
                            if schema == "public" {
                                "archive_writer_guard"
                            } else {
                                "public"
                            },
                            tenant,
                        ),
                    ] {
                        let mut probe = pool.begin().await.unwrap();
                        sqlx::raw_sql(&format!("SET LOCAL search_path TO {probe_schema},public;
                            SET LOCAL app.current_tenant='{probe_tenant}'; SET LOCAL lock_timeout='500ms'"))
                            .execute(&mut *probe).await.unwrap();
                        sqlx::query(
                            "INSERT INTO skos_semantic_relation_edge VALUES($1,$2,$3,'related')",
                        )
                        .bind(Uuid::new_v4())
                        .bind(Uuid::new_v4())
                        .bind(Uuid::new_v4())
                        .execute(&mut *probe)
                        .await
                        .unwrap();
                        probe.commit().await.unwrap();
                    }
                    let guard_before: serde_json::Value = sqlx::query_scalar(&guard_query)
                        .fetch_one(&pool)
                        .await
                        .unwrap();
                    let subject = nodes[0];
                    let object = nodes[4];
                    let worker = tokio::spawn(async move {
                        if let Err(error) = sqlx::query(
                            "INSERT INTO skos_semantic_relation_edge VALUES($1,$2,$3,'broader')",
                        )
                        .bind(Uuid::new_v4())
                        .bind(subject)
                        .bind(object)
                        .execute(&mut *second)
                        .await
                        {
                            second.rollback().await.unwrap();
                            return Err(error);
                        }
                        second.commit().await
                    });
                    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
                    let mut blocked = false;
                    while tokio::time::Instant::now() < deadline {
                        blocked =
                            sqlx::query_scalar::<_, bool>("SELECT $1=ANY(pg_blocking_pids($2))")
                                .bind(first_pid)
                                .bind(second_pid)
                                .fetch_one(&pool)
                                .await
                                .unwrap();
                        if blocked || worker.is_finished() {
                            break;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                    }
                    if commit_first {
                        first.commit().await.unwrap();
                    } else {
                        first.rollback().await.unwrap();
                    }
                    let result = worker.await.unwrap();
                    assert!(
                        blocked,
                        "writer did not wait: {schema}/{isolation}/{restore}/{commit_first}"
                    );
                    if commit_first {
                        let error = result.unwrap_err();
                        if isolation == "READ COMMITTED" {
                            assert!(error.to_string().contains("Polyhierarchy limit"), "{error}");
                        } else {
                            assert_eq!(
                                error.as_database_error().unwrap().code().as_deref(),
                                Some("40001")
                            );
                        }
                    } else {
                        result.unwrap();
                    }
                    assert_eq!(sqlx::query_scalar::<_,i64>(&format!("SELECT count(*) FROM {schema}.skos_semantic_relation_edge WHERE subject_id=$1"))
                        .bind(nodes[0]).fetch_one(&pool).await.unwrap(),3);
                    assert_eq!(
                        sqlx::query_scalar::<_, serde_json::Value>(&guard_query)
                            .fetch_one(&pool)
                            .await
                            .unwrap(),
                        guard_before
                    );
                }
            }
        }
    }
    pool.close().await;
}

#[tokio::test]
async fn skos_atomic_batches_enforce_final_state_and_preserve_native_guards() {
    let _guard = MIGRATION_REPAIR_TEST_LOCK.lock().await;
    let database = DisposableDatabase::create("skos_atomic_batches")
        .await
        .unwrap();
    let pool = create_pool(&database.url).await.unwrap();
    sqlx::raw_sql("CREATE TABLE public.archive_registry(schema_name TEXT);
        INSERT INTO public.archive_registry VALUES('archive_atomic_batches');
        CREATE TABLE public.skos_concept(id UUID PRIMARY KEY, depth INTEGER DEFAULT 0, status TEXT DEFAULT 'approved');
        CREATE TABLE public.skos_semantic_relation_edge(id UUID PRIMARY KEY,subject_id UUID,object_id UUID,relation_type TEXT);
        CREATE FUNCTION public.skos_has_circular_hierarchy(UUID) RETURNS boolean LANGUAGE sql AS 'SELECT false';
        CREATE SCHEMA archive_atomic_batches;
        CREATE TABLE archive_atomic_batches.skos_concept(LIKE public.skos_concept INCLUDING ALL);
        CREATE TABLE archive_atomic_batches.skos_semantic_relation_edge(LIKE public.skos_semantic_relation_edge INCLUDING ALL);")
        .execute(&pool).await.unwrap();
    sqlx::raw_sql(include_str!(
        "../../../../migrations/20260911020400_skos_retained_relation_limits.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::raw_sql(include_str!(
        "../../../../migrations/20260911020500_skos_prospective_hierarchy.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();
    let functions = sqlx::query_scalar::<_, String>(
        "SELECT pg_get_functiondef(oid) FROM pg_proc WHERE proname IN
        ('skos_validate_broader_relation','skos_validate_narrower_relation') ORDER BY proname",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    for _ in 0..2 {
        sqlx::raw_sql(include_str!(
            "../../../../migrations/20260911020600_skos_atomic_relation_batches.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();
    }
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT pg_get_functiondef(oid) FROM pg_proc WHERE proname IN
        ('skos_validate_broader_relation','skos_validate_narrower_relation') ORDER BY proname"
        )
        .fetch_all(&pool)
        .await
        .unwrap(),
        functions
    );
    for schema in ["public", "archive_atomic_batches"] {
        let (deferrable, deferred, unconditional): (bool, bool, bool) = sqlx::query_as(
            "SELECT tgdeferrable,tginitdeferred,tgqual IS NULL FROM pg_trigger
             WHERE tgrelid=to_regclass($1) AND tgname='trg_skos_validate_final_state'",
        )
        .bind(format!("{schema}.skos_semantic_relation_edge"))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(deferrable && deferred && unconditional);
        let nodes = (0..205).map(|_| Uuid::new_v4()).collect::<Vec<_>>();
        let edges = [Uuid::new_v4(), Uuid::new_v4()];
        sqlx::query(&format!(
            "INSERT INTO {schema}.skos_concept(id) SELECT unnest($1::uuid[])"
        ))
        .bind(&nodes)
        .execute(&pool)
        .await
        .unwrap();
        let mut tx = pool.begin().await.unwrap();
        sqlx::raw_sql(&format!(
            "SET LOCAL search_path TO {schema},public; SET LOCAL statement_timeout='3s'"
        ))
        .execute(&mut *tx)
        .await
        .unwrap();
        for (id, subject, object) in [
            (edges[0], nodes[0], nodes[3]),
            (edges[1], nodes[1], nodes[0]),
        ] {
            sqlx::query("INSERT INTO skos_semantic_relation_edge VALUES($1,$2,$3,'broader')")
                .bind(id)
                .bind(subject)
                .bind(object)
                .execute(&mut *tx)
                .await
                .unwrap();
        }
        tx.commit().await.unwrap();
        let mut tx = pool.begin().await.unwrap();
        sqlx::raw_sql(&format!("SET LOCAL search_path TO {schema},public"))
            .execute(&mut *tx)
            .await
            .unwrap();
        let error = sqlx::query("UPDATE skos_semantic_relation_edge SET object_id=$2 WHERE id=$1")
            .bind(edges[0])
            .bind(nodes[1])
            .execute(&mut *tx)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("SKOS hierarchy"));
        tx.rollback().await.unwrap();

        let mut tx = pool.begin().await.unwrap();
        sqlx::raw_sql(&format!(
            "SET LOCAL search_path TO {schema},public; SET LOCAL app.shard_import='on'"
        ))
        .execute(&mut *tx)
        .await
        .unwrap();
        sqlx::query("UPDATE skos_semantic_relation_edge SET object_id=$2 WHERE id=$1")
            .bind(edges[0])
            .bind(nodes[1])
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query(
            "UPDATE skos_semantic_relation_edge SET subject_id=$2,object_id=$3 WHERE id=$1",
        )
        .bind(edges[1])
        .bind(nodes[2])
        .bind(nodes[3])
        .execute(&mut *tx)
        .await
        .unwrap();
        // Queued events must read final state, not obsolete NEW snapshots.
        sqlx::query("UPDATE skos_semantic_relation_edge SET object_id=subject_id WHERE id=$1")
            .bind(edges[0])
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query("UPDATE skos_semantic_relation_edge SET object_id=$2 WHERE id=$1")
            .bind(edges[0])
            .bind(nodes[1])
            .execute(&mut *tx)
            .await
            .unwrap();
        let removed = Uuid::new_v4();
        sqlx::query("INSERT INTO skos_semantic_relation_edge VALUES($1,$2,$2,'broader')")
            .bind(removed)
            .bind(nodes[4])
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query("DELETE FROM skos_semantic_relation_edge WHERE id=$1")
            .bind(removed)
            .execute(&mut *tx)
            .await
            .unwrap();
        // Changing caller context cannot suppress or redirect the deferred guard.
        sqlx::raw_sql("SET LOCAL app.shard_import='off'; SET LOCAL search_path TO public")
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        let snapshot_query = format!("SELECT coalesce(jsonb_agg(to_jsonb(e) ORDER BY id),'[]'::jsonb) FROM {schema}.skos_semantic_relation_edge e");
        let baseline: serde_json::Value = sqlx::query_scalar(&snapshot_query)
            .fetch_one(&pool)
            .await
            .unwrap();
        for kind in ["cycle", "broader", "narrower"] {
            let mut tx = pool.begin().await.unwrap();
            sqlx::raw_sql(&format!("SET LOCAL search_path TO {schema},public; SET LOCAL app.shard_import='on'; SET LOCAL statement_timeout='3s'"))
                .execute(&mut *tx).await.unwrap();
            if kind == "cycle" {
                sqlx::query("INSERT INTO skos_semantic_relation_edge VALUES($1,$2,$3,'broader')")
                    .bind(Uuid::new_v4())
                    .bind(nodes[3])
                    .bind(nodes[2])
                    .execute(&mut *tx)
                    .await
                    .unwrap();
            } else {
                let count = if kind == "broader" { 4 } else { 201 };
                for object in nodes.iter().take(count) {
                    sqlx::query("INSERT INTO skos_semantic_relation_edge VALUES($1,$2,$3,$4)")
                        .bind(Uuid::new_v4())
                        .bind(nodes[204])
                        .bind(object)
                        .bind(kind)
                        .execute(&mut *tx)
                        .await
                        .unwrap();
                }
            }
            sqlx::raw_sql("SET LOCAL app.shard_import='off'; SET LOCAL search_path TO public")
                .execute(&mut *tx)
                .await
                .unwrap();
            let error = tx.commit().await.unwrap_err();
            assert!(
                error.to_string().contains(match kind {
                    "cycle" => "SKOS hierarchy",
                    "broader" => "Polyhierarchy limit",
                    _ => "Breadth limit",
                }),
                "{error}"
            );
            assert_eq!(
                sqlx::query_scalar::<_, serde_json::Value>(&snapshot_query)
                    .fetch_one(&pool)
                    .await
                    .unwrap(),
                baseline
            );
        }
    }
    pool.close().await;
}

#[tokio::test]
async fn skos_prospective_hierarchy_upgrade_preserves_binding_and_bounded_validation() {
    let _guard = MIGRATION_REPAIR_TEST_LOCK.lock().await;
    let database = DisposableDatabase::create("skos_prospective_hierarchy")
        .await
        .unwrap();
    let pool = create_pool(&database.url).await.unwrap();
    sqlx::raw_sql("CREATE TABLE public.skos_semantic_relation_edge(id UUID PRIMARY KEY,subject_id UUID,object_id UUID,relation_type TEXT,score FLOAT8);
        CREATE FUNCTION public.skos_validate_broader_relation() RETURNS trigger LANGUAGE plpgsql AS $$BEGIN RETURN NEW; END;$$;
        CREATE SCHEMA archive_prospective_hierarchy;
        CREATE TABLE archive_prospective_hierarchy.skos_semantic_relation_edge(LIKE public.skos_semantic_relation_edge INCLUDING ALL);
        CREATE TRIGGER trg_skos_validate_broader BEFORE INSERT OR UPDATE ON public.skos_semantic_relation_edge
            FOR EACH ROW EXECUTE FUNCTION public.skos_validate_broader_relation();
        CREATE TRIGGER trg_skos_validate_broader BEFORE INSERT OR UPDATE ON archive_prospective_hierarchy.skos_semantic_relation_edge
            FOR EACH ROW EXECUTE FUNCTION public.skos_validate_broader_relation();")
        .execute(&pool).await.unwrap();
    let oid: i64 = sqlx::query_scalar(
        "SELECT 'public.skos_validate_broader_relation()'::regprocedure::oid::bigint",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    for _ in 0..2 {
        sqlx::raw_sql(include_str!(
            "../../../../migrations/20260911020500_skos_prospective_hierarchy.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();
    }
    for schema in ["public", "archive_prospective_hierarchy"] {
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT tgfoid::bigint FROM pg_trigger
            WHERE tgrelid=to_regclass($1) AND tgname='trg_skos_validate_broader'"
            )
            .bind(format!("{schema}.skos_semantic_relation_edge"))
            .fetch_one(&pool)
            .await
            .unwrap(),
            oid
        );
        for restore in [false, true] {
            let mut tx = pool.begin().await.unwrap();
            sqlx::raw_sql(&format!(
                "SET LOCAL search_path TO {schema},public; SET LOCAL statement_timeout='3s'"
            ))
            .execute(&mut *tx)
            .await
            .unwrap();
            if restore {
                sqlx::query("SET LOCAL app.shard_import='on'")
                    .execute(&mut *tx)
                    .await
                    .unwrap();
            }
            let nodes = (0..7).map(|_| Uuid::new_v4()).collect::<Vec<_>>();
            let ids = (0..5).map(|_| Uuid::new_v4()).collect::<Vec<_>>();
            for i in (0..5).rev() {
                sqlx::query(
                    "INSERT INTO skos_semantic_relation_edge VALUES($1,$2,$3,'broader',0.5)",
                )
                .bind(ids[i])
                .bind(nodes[i])
                .bind(nodes[i + 1])
                .execute(&mut *tx)
                .await
                .unwrap();
            }
            sqlx::query("UPDATE skos_semantic_relation_edge SET score=0.75 WHERE id=ANY($1)")
                .bind(&ids)
                .execute(&mut *tx)
                .await
                .unwrap();
            for object in [nodes[0], nodes[6]] {
                sqlx::query("SAVEPOINT invalid_hierarchy")
                    .execute(&mut *tx)
                    .await
                    .unwrap();
                let error = sqlx::query(
                    "INSERT INTO skos_semantic_relation_edge VALUES($1,$2,$3,'broader',0.5)",
                )
                .bind(Uuid::new_v4())
                .bind(nodes[5])
                .bind(object)
                .execute(&mut *tx)
                .await
                .unwrap_err();
                assert!(error.to_string().contains("SKOS hierarchy"), "{error}");
                sqlx::query("ROLLBACK TO SAVEPOINT invalid_hierarchy")
                    .execute(&mut *tx)
                    .await
                    .unwrap();
            }
            sqlx::query("UPDATE skos_semantic_relation_edge SET object_id=$2 WHERE id=$1")
                .bind(ids[4])
                .bind(nodes[6])
                .execute(&mut *tx)
                .await
                .unwrap();
            sqlx::query("INSERT INTO skos_semantic_relation_edge SELECT * FROM skos_semantic_relation_edge WHERE id=$1
                ON CONFLICT(id) DO UPDATE SET object_id=EXCLUDED.object_id")
                .bind(ids[4]).execute(&mut *tx).await.unwrap();
            for object in [nodes[0], nodes[4]] {
                sqlx::query("SAVEPOINT invalid_reparent")
                    .execute(&mut *tx)
                    .await
                    .unwrap();
                let error =
                    sqlx::query("UPDATE skos_semantic_relation_edge SET object_id=$2 WHERE id=$1")
                        .bind(ids[4])
                        .bind(object)
                        .execute(&mut *tx)
                        .await
                        .unwrap_err();
                assert!(error.to_string().contains("SKOS hierarchy"), "{error}");
                sqlx::query("ROLLBACK TO SAVEPOINT invalid_reparent")
                    .execute(&mut *tx)
                    .await
                    .unwrap();
            }
            // Related edges are not part of the native broader hierarchy.
            sqlx::query("INSERT INTO skos_semantic_relation_edge VALUES($1,$2,$3,'related',0.5)")
                .bind(Uuid::new_v4())
                .bind(nodes[6])
                .bind(nodes[0])
                .execute(&mut *tx)
                .await
                .unwrap();
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM skos_semantic_relation_edge")
                    .fetch_one(&mut *tx)
                    .await
                    .unwrap(),
                6
            );
            tx.rollback().await.unwrap();
        }
        assert_eq!(
            sqlx::query_scalar::<_, i64>(&format!(
                "SELECT count(*) FROM {schema}.skos_semantic_relation_edge"
            ))
            .fetch_one(&pool)
            .await
            .unwrap(),
            0
        );
    }
    pool.close().await;
}

#[tokio::test]
async fn skos_retained_limits_upgrade_preserves_binding_and_cardinality() {
    let _guard = MIGRATION_REPAIR_TEST_LOCK.lock().await;
    let database = DisposableDatabase::create("skos_retained_limit")
        .await
        .unwrap();
    let pool = create_pool(&database.url).await.unwrap();
    sqlx::raw_sql("CREATE TABLE public.skos_concept(id UUID PRIMARY KEY,depth INTEGER NOT NULL DEFAULT 0,status TEXT NOT NULL DEFAULT 'approved');
        CREATE TABLE public.skos_semantic_relation_edge(id UUID PRIMARY KEY,subject_id UUID REFERENCES public.skos_concept(id),
            object_id UUID REFERENCES public.skos_concept(id),relation_type TEXT,score FLOAT8);
        CREATE FUNCTION public.skos_has_circular_hierarchy(UUID) RETURNS boolean LANGUAGE sql AS 'SELECT false';
        CREATE FUNCTION public.skos_validate_broader_relation() RETURNS trigger LANGUAGE plpgsql AS $$BEGIN RETURN NEW; END;$$;
        CREATE FUNCTION public.skos_validate_narrower_relation() RETURNS trigger LANGUAGE plpgsql AS $$BEGIN RETURN NEW; END;$$;
        CREATE SCHEMA archive_retained_limit;
        CREATE TABLE archive_retained_limit.skos_concept(LIKE public.skos_concept INCLUDING ALL);
        CREATE TABLE archive_retained_limit.skos_semantic_relation_edge(LIKE public.skos_semantic_relation_edge INCLUDING ALL);")
        .execute(&pool).await.unwrap();
    let oids: Vec<i64> = sqlx::query_scalar(
        "SELECT oid::bigint FROM pg_proc WHERE proname IN
        ('skos_validate_broader_relation','skos_validate_narrower_relation') ORDER BY proname",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    for schema in ["public", "archive_retained_limit"] {
        sqlx::raw_sql(&format!("CREATE TRIGGER trg_skos_validate_broader BEFORE INSERT OR UPDATE ON {schema}.skos_semantic_relation_edge
            FOR EACH ROW EXECUTE FUNCTION public.skos_validate_broader_relation();
            CREATE TRIGGER trg_skos_validate_narrower BEFORE INSERT OR UPDATE ON {schema}.skos_semantic_relation_edge
            FOR EACH ROW EXECUTE FUNCTION public.skos_validate_narrower_relation();"))
            .execute(&pool).await.unwrap();
    }
    for _ in 0..2 {
        sqlx::raw_sql(include_str!(
            "../../../../migrations/20260911020400_skos_retained_relation_limits.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();
    }
    for schema in ["public", "archive_retained_limit"] {
        let trigger_oids: Vec<i64> = sqlx::query_scalar("SELECT tgfoid::bigint FROM pg_trigger
            WHERE tgrelid=to_regclass($1) AND tgname IN ('trg_skos_validate_broader','trg_skos_validate_narrower') ORDER BY tgname")
            .bind(format!("{schema}.skos_semantic_relation_edge")).fetch_all(&pool).await.unwrap();
        assert_eq!(trigger_oids, oids);
        for restore in [false, true] {
            let mut tx = pool.begin().await.unwrap();
            sqlx::raw_sql(&format!("SET LOCAL search_path TO {schema},public"))
                .execute(&mut *tx)
                .await
                .unwrap();
            if restore {
                sqlx::query("SET LOCAL app.shard_import='on'")
                    .execute(&mut *tx)
                    .await
                    .unwrap();
            }
            let subject = Uuid::new_v4();
            let spare = Uuid::new_v4();
            let candidate = Uuid::new_v4();
            sqlx::query("INSERT INTO skos_concept(id) VALUES($1),($2),($3)")
                .bind(subject)
                .bind(spare)
                .bind(candidate)
                .execute(&mut *tx)
                .await
                .unwrap();
            sqlx::query("UPDATE skos_concept SET status='candidate' WHERE id=$1")
                .bind(candidate)
                .execute(&mut *tx)
                .await
                .unwrap();
            let mut broader = Vec::new();
            let mut narrower = Vec::new();
            for i in 0..200 {
                let object = Uuid::new_v4();
                sqlx::query("INSERT INTO skos_concept(id) VALUES($1)")
                    .bind(object)
                    .execute(&mut *tx)
                    .await
                    .unwrap();
                for (kind, ids) in [("broader", &mut broader), ("narrower", &mut narrower)] {
                    if kind == "broader" && i >= 3 {
                        continue;
                    }
                    let id = Uuid::new_v4();
                    sqlx::query("INSERT INTO skos_semantic_relation_edge VALUES($1,$2,$3,$4,0.5)")
                        .bind(id)
                        .bind(subject)
                        .bind(object)
                        .bind(kind)
                        .execute(&mut *tx)
                        .await
                        .unwrap();
                    ids.push(id);
                }
            }
            for ids in [&broader, &narrower] {
                sqlx::query("UPDATE skos_semantic_relation_edge SET score=0.75 WHERE id=ANY($1)")
                    .bind(ids)
                    .execute(&mut *tx)
                    .await
                    .unwrap();
                sqlx::query("INSERT INTO skos_semantic_relation_edge SELECT * FROM skos_semantic_relation_edge WHERE id=ANY($1)
                    ON CONFLICT(id) DO UPDATE SET score=EXCLUDED.score")
                    .bind(ids).execute(&mut *tx).await.unwrap();
            }
            let candidate_edge = Uuid::new_v4();
            sqlx::query("INSERT INTO skos_semantic_relation_edge VALUES($1,$2,$3,'narrower',0.5)")
                .bind(candidate_edge)
                .bind(subject)
                .bind(candidate)
                .execute(&mut *tx)
                .await
                .unwrap();
            sqlx::query("UPDATE skos_semantic_relation_edge SET score=0.75 WHERE id=$1")
                .bind(candidate_edge)
                .execute(&mut *tx)
                .await
                .unwrap();
            for kind in ["broader", "narrower"] {
                sqlx::query("SAVEPOINT invalid_edge")
                    .execute(&mut *tx)
                    .await
                    .unwrap();
                let error =
                    sqlx::query("INSERT INTO skos_semantic_relation_edge VALUES($1,$2,$3,$4,0.5)")
                        .bind(Uuid::new_v4())
                        .bind(subject)
                        .bind(spare)
                        .bind(kind)
                        .execute(&mut *tx)
                        .await
                        .unwrap_err();
                assert!(error.to_string().contains(if kind == "broader" {
                    "Polyhierarchy limit"
                } else {
                    "Breadth limit"
                }));
                sqlx::query("ROLLBACK TO SAVEPOINT invalid_edge")
                    .execute(&mut *tx)
                    .await
                    .unwrap();
            }
            // Reparenting a retained candidate to an approved child must count it.
            sqlx::query("SAVEPOINT invalid_candidate")
                .execute(&mut *tx)
                .await
                .unwrap();
            let error =
                sqlx::query("UPDATE skos_semantic_relation_edge SET object_id=$2 WHERE id=$1")
                    .bind(candidate_edge)
                    .bind(spare)
                    .execute(&mut *tx)
                    .await
                    .unwrap_err();
            assert!(error.to_string().contains("Breadth limit"));
            sqlx::query("ROLLBACK TO SAVEPOINT invalid_candidate")
                .execute(&mut *tx)
                .await
                .unwrap();
            sqlx::query("UPDATE skos_concept SET depth=5 WHERE id=$1")
                .bind(spare)
                .execute(&mut *tx)
                .await
                .unwrap();
            sqlx::query("SAVEPOINT invalid_depth")
                .execute(&mut *tx)
                .await
                .unwrap();
            let error =
                sqlx::query("UPDATE skos_semantic_relation_edge SET object_id=$2 WHERE id=$1")
                    .bind(broader[0])
                    .bind(spare)
                    .execute(&mut *tx)
                    .await
                    .unwrap_err();
            assert!(error.to_string().contains("Depth limit"));
            sqlx::query("ROLLBACK TO SAVEPOINT invalid_depth")
                .execute(&mut *tx)
                .await
                .unwrap();
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM skos_semantic_relation_edge")
                    .fetch_one(&mut *tx)
                    .await
                    .unwrap(),
                204
            );
            tx.rollback().await.unwrap();
        }
        assert_eq!(
            sqlx::query_scalar::<_, i64>(&format!(
                "SELECT count(*) FROM {schema}.skos_semantic_relation_edge"
            ))
            .fetch_one(&pool)
            .await
            .unwrap(),
            0
        );
    }
    pool.close().await;
}

#[tokio::test]
async fn skos_note_lifecycle_upgrade_preserves_native_insert_delete_and_function_identity() {
    let _guard = MIGRATION_REPAIR_TEST_LOCK.lock().await;
    let database = DisposableDatabase::create("skos_note_guard").await.unwrap();
    let pool = create_pool(&database.url).await.unwrap();
    sqlx::raw_sql("CREATE TABLE public.archive_registry(schema_name TEXT NOT NULL);
        CREATE TABLE public.note_skos_concept(note_id INTEGER,concept_id INTEGER,PRIMARY KEY(note_id,concept_id));
        CREATE TABLE public.lifecycle_calls(operation TEXT);
        CREATE SCHEMA archive_skos_note;
        CREATE TABLE archive_skos_note.note_skos_concept(LIKE public.note_skos_concept INCLUDING ALL);
        CREATE TABLE archive_skos_note.lifecycle_calls(LIKE public.lifecycle_calls INCLUDING ALL);
        INSERT INTO public.archive_registry VALUES('archive_skos_note');
        CREATE FUNCTION public.skos_update_note_count() RETURNS trigger LANGUAGE plpgsql AS $$
          BEGIN EXECUTE format('INSERT INTO %I.lifecycle_calls VALUES($1)', TG_TABLE_SCHEMA)
            USING TG_OP; RETURN NULL; END; $$;
        CREATE TRIGGER trg_skos_note_count AFTER INSERT OR DELETE ON public.note_skos_concept
          FOR EACH ROW EXECUTE FUNCTION public.skos_update_note_count();
        CREATE TRIGGER trg_skos_note_count AFTER INSERT OR DELETE ON archive_skos_note.note_skos_concept
          FOR EACH ROW EXECUTE FUNCTION public.skos_update_note_count();")
        .execute(&pool).await.unwrap();
    let oid: i64 =
        sqlx::query_scalar("SELECT 'public.skos_update_note_count()'::regprocedure::oid::bigint")
            .fetch_one(&pool)
            .await
            .unwrap();
    for _ in 0..2 {
        sqlx::raw_sql(include_str!(
            "../../../../migrations/20260911020300_restore_skos_note_lifecycle_guard.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT 'public.skos_update_note_count()'::regprocedure::oid::bigint"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        oid
    );
    for schema in ["public", "archive_skos_note"] {
        let trigger_function: i64 = sqlx::query_scalar(
            "SELECT tgfoid::bigint FROM pg_trigger
            WHERE tgrelid=to_regclass($1) AND tgname='trg_skos_note_count'",
        )
        .bind(format!("{schema}.note_skos_concept"))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(trigger_function, oid);
        for restore in [true, false] {
            let mut tx = pool.begin().await.unwrap();
            if restore {
                sqlx::query("SET LOCAL app.shard_import='on'")
                    .execute(&mut *tx)
                    .await
                    .unwrap();
            }
            sqlx::raw_sql(&format!(
                "INSERT INTO {schema}.note_skos_concept VALUES(1,1);
                UPDATE {schema}.note_skos_concept SET concept_id=2;
                DELETE FROM {schema}.note_skos_concept;"
            ))
            .execute(&mut *tx)
            .await
            .unwrap();
            let calls: Vec<String> = sqlx::query_scalar(&format!(
                "SELECT operation FROM {schema}.lifecycle_calls ORDER BY operation"
            ))
            .fetch_all(&mut *tx)
            .await
            .unwrap();
            if restore {
                assert!(calls.is_empty());
            } else {
                assert_eq!(calls, vec!["DELETE", "INSERT"]);
            }
            tx.rollback().await.unwrap();
        }
        assert_eq!(
            sqlx::query_scalar::<_, i64>(&format!("SELECT count(*) FROM {schema}.lifecycle_calls"))
                .fetch_one(&pool)
                .await
                .unwrap(),
            0
        );
    }
    pool.close().await;
}

#[tokio::test]
async fn skos_relation_authoring_upgrade_preserves_function_identity_and_native_events() {
    let _guard = MIGRATION_REPAIR_TEST_LOCK.lock().await;
    let database = DisposableDatabase::create("skos_relation_guards")
        .await
        .unwrap();
    let pool = create_pool(&database.url).await.unwrap();
    sqlx::raw_sql("CREATE TABLE public.archive_registry(schema_name TEXT NOT NULL);
        CREATE TABLE public.skos_semantic_relation_edge(id INTEGER PRIMARY KEY, is_inferred BOOL NOT NULL);
        CREATE TABLE public.authoring_calls(kind TEXT, operation TEXT);
        CREATE SCHEMA archive_skos_relation;
        CREATE TABLE archive_skos_relation.skos_semantic_relation_edge(LIKE public.skos_semantic_relation_edge INCLUDING ALL);
        CREATE TABLE archive_skos_relation.authoring_calls(LIKE public.authoring_calls INCLUDING ALL);
        INSERT INTO public.archive_registry VALUES('archive_skos_relation');
        CREATE FUNCTION public.skos_update_hierarchy_metadata() RETURNS trigger LANGUAGE plpgsql AS $$
          BEGIN EXECUTE format('INSERT INTO %I.authoring_calls VALUES($1,$2)', TG_TABLE_SCHEMA)
            USING 'hierarchy',TG_OP; RETURN NULL; END; $$;
        CREATE FUNCTION public.skos_create_reciprocal_relation() RETURNS trigger LANGUAGE plpgsql AS $$
          BEGIN EXECUTE format('INSERT INTO %I.authoring_calls VALUES($1,$2)', TG_TABLE_SCHEMA)
            USING 'reciprocal',TG_OP; RETURN NEW; END; $$;")
        .execute(&pool).await.unwrap();
    let oids: Vec<i64> = sqlx::query_scalar(
        "SELECT oid::bigint FROM pg_proc WHERE proname IN
        ('skos_update_hierarchy_metadata','skos_create_reciprocal_relation') ORDER BY proname",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(oids.len(), 2);
    for _ in 0..2 {
        sqlx::raw_sql(include_str!(
            "../../../../migrations/20260911020200_restore_skos_relation_authoring_guards.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT oid::bigint FROM pg_proc WHERE proname IN
        ('skos_update_hierarchy_metadata','skos_create_reciprocal_relation') ORDER BY proname"
        )
        .fetch_all(&pool)
        .await
        .unwrap(),
        oids
    );
    for schema in ["public", "archive_skos_relation"] {
        for restore in [true, false] {
            let mut tx = pool.begin().await.unwrap();
            if restore {
                sqlx::query("SET LOCAL app.shard_import='on'")
                    .execute(&mut *tx)
                    .await
                    .unwrap();
            }
            sqlx::raw_sql(&format!(
                "INSERT INTO {schema}.skos_semantic_relation_edge VALUES(1,false),(2,true);
                UPDATE {schema}.skos_semantic_relation_edge SET is_inferred=true WHERE id=1;
                DELETE FROM {schema}.skos_semantic_relation_edge;"
            ))
            .execute(&mut *tx)
            .await
            .unwrap();
            let calls: Vec<(String, String)> = sqlx::query_as(&format!(
                "SELECT kind,operation FROM {schema}.authoring_calls ORDER BY kind,operation"
            ))
            .fetch_all(&mut *tx)
            .await
            .unwrap();
            if restore {
                assert!(calls.is_empty());
            } else {
                assert_eq!(
                    calls,
                    vec![
                        ("hierarchy".into(), "DELETE".into()),
                        ("hierarchy".into(), "DELETE".into()),
                        ("hierarchy".into(), "INSERT".into()),
                        ("hierarchy".into(), "INSERT".into()),
                        ("hierarchy".into(), "UPDATE".into()),
                        ("reciprocal".into(), "INSERT".into())
                    ]
                );
            }
            tx.rollback().await.unwrap();
        }
        assert_eq!(
            sqlx::query_scalar::<_, i64>(&format!("SELECT count(*) FROM {schema}.authoring_calls"))
                .fetch_one(&pool)
                .await
                .unwrap(),
            0
        );
    }
    pool.close().await;
}

#[tokio::test]
async fn skos_collection_timestamp_upgrade_preserves_native_writes_and_restore() {
    let _guard = MIGRATION_REPAIR_TEST_LOCK.lock().await;
    let database = DisposableDatabase::create("skos_timestamp_upgrade")
        .await
        .unwrap();
    let pool = create_pool(&database.url).await.unwrap();
    sqlx::raw_sql("CREATE TABLE public.archive_registry(schema_name TEXT NOT NULL);
        CREATE TABLE public.skos_collection(id UUID PRIMARY KEY, updated_at TIMESTAMPTZ NOT NULL);
        CREATE SCHEMA archive_skos_time;
        CREATE TABLE archive_skos_time.skos_collection(LIKE public.skos_collection INCLUDING ALL);
        INSERT INTO public.archive_registry VALUES('archive_skos_time');
        CREATE FUNCTION public.update_skos_collection_timestamp() RETURNS trigger LANGUAGE plpgsql
          AS $$ BEGIN NEW.updated_at = NOW(); RETURN NEW; END; $$;
        CREATE TRIGGER trg_skos_collection_updated BEFORE UPDATE ON public.skos_collection
          FOR EACH ROW EXECUTE FUNCTION public.update_skos_collection_timestamp();
        CREATE TRIGGER trg_skos_collection_updated BEFORE UPDATE ON archive_skos_time.skos_collection
          FOR EACH ROW EXECUTE FUNCTION public.update_skos_collection_timestamp();
        INSERT INTO public.skos_collection VALUES('00000000-0000-0000-0000-000000000001','2000-01-01Z');
        INSERT INTO archive_skos_time.skos_collection SELECT * FROM public.skos_collection;")
        .execute(&pool).await.unwrap();
    let oid: i64 = sqlx::query_scalar(
        "SELECT 'public.update_skos_collection_timestamp()'::regprocedure::oid::bigint",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    for _ in 0..2 {
        sqlx::raw_sql(include_str!(
            "../../../../migrations/20260911020100_restore_skos_collection_timestamp_guard.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT 'public.update_skos_collection_timestamp()'::regprocedure::oid::bigint"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        oid
    );
    for schema in ["public", "archive_skos_time"] {
        let mut tx = pool.begin().await.unwrap();
        sqlx::query("SET LOCAL app.shard_import='on'")
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query(&format!(
            "UPDATE {schema}.skos_collection SET updated_at='1999-01-01Z'"
        ))
        .execute(&mut *tx)
        .await
        .unwrap();
        assert!(sqlx::query_scalar::<_, bool>(&format!(
            "SELECT updated_at='1999-01-01Z'::timestamptz FROM {schema}.skos_collection"
        ))
        .fetch_one(&mut *tx)
        .await
        .unwrap());
        tx.rollback().await.unwrap();
        assert!(sqlx::query_scalar::<_, bool>(&format!(
            "SELECT updated_at='2000-01-01Z'::timestamptz FROM {schema}.skos_collection"
        ))
        .fetch_one(&pool)
        .await
        .unwrap());
        let mut tx = pool.begin().await.unwrap();
        sqlx::query(&format!(
            "UPDATE {schema}.skos_collection SET updated_at='1999-01-01Z'"
        ))
        .execute(&mut *tx)
        .await
        .unwrap();
        assert!(
            sqlx::query_scalar::<_, bool>(&format!(
                "SELECT updated_at=now() FROM {schema}.skos_collection"
            ))
            .fetch_one(&mut *tx)
            .await
            .unwrap(),
            "native timestamp trigger remains active"
        );
        tx.rollback().await.unwrap();
    }
    pool.close().await;
}

#[tokio::test]
async fn skos_bootstrap_upgrade_preserves_existing_roots_without_inference() {
    let _guard = MIGRATION_REPAIR_TEST_LOCK.lock().await;
    let database = DisposableDatabase::create("skos_bootstrap_upgrade")
        .await
        .unwrap();
    let pool = create_pool(&database.url).await.unwrap();
    sqlx::raw_sql("CREATE TABLE public.archive_registry(schema_name TEXT NOT NULL);
        CREATE TABLE public.skos_concept_scheme(id UUID PRIMARY KEY, tenant_id UUID NOT NULL,
            notation TEXT NOT NULL, is_system BOOL NOT NULL, UNIQUE(tenant_id,id));
        CREATE SCHEMA archive_skos_old;
        CREATE TABLE archive_skos_old.skos_concept_scheme(LIKE public.skos_concept_scheme INCLUDING ALL);
        INSERT INTO public.archive_registry VALUES('public'),('archive_skos_old');
        INSERT INTO public.skos_concept_scheme VALUES
          ('00000000-0000-0000-0000-000000000001','00000000-0000-0000-0000-000000000000','default',true);
        INSERT INTO archive_skos_old.skos_concept_scheme SELECT * FROM public.skos_concept_scheme;")
        .execute(&pool).await.unwrap();
    sqlx::raw_sql(include_str!(
        "../../../../migrations/20260911020000_skos_scheme_bootstrap_custody.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();
    for schema in ["public", "archive_skos_old"] {
        let state: (i64, String, bool) = sqlx::query_as(&format!(
            "SELECT (SELECT count(*) FROM {schema}.shard_skos_scheme_bootstrap), notation, is_system
             FROM {schema}.skos_concept_scheme"))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(state, (0, "default".into(), true));
        let guards: (bool, bool, i64) = sqlx::query_as(
            "SELECT relrowsecurity,relforcerowsecurity,
            (SELECT count(*) FROM pg_policy WHERE polrelid=c.oid)
            FROM pg_class c WHERE c.oid=to_regclass($1)",
        )
        .bind(format!("{schema}.shard_skos_scheme_bootstrap"))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(guards, (true, true, 1));
    }
    pool.close().await;
}

#[tokio::test]
async fn skos_bootstrap_fresh_runner_archive_and_repair_track_only_new_seeds() {
    use crate::ArchiveRepository;
    let _guard = MIGRATION_REPAIR_TEST_LOCK.lock().await;
    let database = DisposableDatabase::create("skos_bootstrap_fresh")
        .await
        .unwrap();
    let db = Database::connect(&database.url).await.unwrap();
    sqlx::raw_sql("CREATE EXTENSION IF NOT EXISTS vector; CREATE EXTENSION IF NOT EXISTS postgis;")
        .execute(&db.pool)
        .await
        .unwrap();
    db.migrate().await.unwrap();
    db.migrate().await.unwrap();
    let public_id: Uuid = sqlx::query_scalar("SELECT scheme_id FROM shard_skos_scheme_bootstrap")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    let name = format!("skos-fresh-{}", Uuid::new_v4().simple());
    let archive = db
        .archives
        .create_archive_schema(&name, None)
        .await
        .unwrap();
    let ctx = db.for_schema(&archive.schema_name).unwrap();
    let mut tx = ctx.begin_tx().await.unwrap();
    let archive_id: Uuid = sqlx::query_scalar("SELECT scheme_id FROM shard_skos_scheme_bootstrap")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_ne!(archive_id, public_id);
    sqlx::query("UPDATE skos_concept_scheme SET description='native edit' WHERE id=$1")
        .bind(archive_id)
        .execute(&mut *tx)
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM shard_skos_scheme_bootstrap")
            .fetch_one(&mut *tx)
            .await
            .unwrap(),
        0
    );
    tx.rollback().await.unwrap();
    let mut tx = ctx.begin_tx().await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, Uuid>("SELECT scheme_id FROM shard_skos_scheme_bootstrap")
            .fetch_one(&mut *tx)
            .await
            .unwrap(),
        archive_id
    );
    sqlx::query("DELETE FROM skos_concept_scheme WHERE id=$1")
        .bind(archive_id)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("DROP TABLE shard_skos_scheme_bootstrap")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    sqlx::query("UPDATE public.archive_registry SET schema_version=0 WHERE schema_name=$1")
        .bind(&archive.schema_name)
        .execute(&db.pool)
        .await
        .unwrap();
    db.archives.sync_archive_schema(&name).await.unwrap();
    let mut tx = ctx.begin_tx().await.unwrap();
    let repaired: Uuid = sqlx::query_scalar("SELECT scheme_id FROM shard_skos_scheme_bootstrap")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_ne!(repaired, archive_id);
    sqlx::query("UPDATE skos_concept_scheme SET is_system=false WHERE id=$1")
        .bind(repaired)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    sqlx::query("UPDATE public.archive_registry SET schema_version=0 WHERE schema_name=$1")
        .bind(&archive.schema_name)
        .execute(&db.pool)
        .await
        .unwrap();
    db.archives.sync_archive_schema(&name).await.unwrap();
    let mut tx = ctx.begin_tx().await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM shard_skos_scheme_bootstrap")
            .fetch_one(&mut *tx)
            .await
            .unwrap(),
        0,
        "repair must not reclaim default-looking live state"
    );
    assert_eq!(
        sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM skos_concept_scheme WHERE notation='default'"
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap(),
        repaired
    );
    tx.rollback().await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, Uuid>("SELECT scheme_id FROM public.shard_skos_scheme_bootstrap")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        public_id,
        "archive adoption/repair cannot alter public custody"
    );
    db.archives.drop_archive_schema(&name).await.unwrap();
    db.pool.close().await;
}

#[tokio::test]
async fn embedding_stats_migration_preserves_trigger_binding_and_schema_isolation() {
    let _guard = MIGRATION_REPAIR_TEST_LOCK.lock().await;
    let database = DisposableDatabase::create("embedding_stats_upgrade")
        .await
        .expect("DATABASE_URL must select disposable PostgreSQL");
    let pool = create_pool(&database.url).await.unwrap();
    sqlx::raw_sql("CREATE TABLE public.embedding_set (id UUID PRIMARY KEY, tenant_id UUID NOT NULL,
        document_count INTEGER, embedding_count INTEGER, updated_at TIMESTAMPTZ);
        CREATE TABLE public.embedding (id UUID PRIMARY KEY, tenant_id UUID NOT NULL, embedding_set_id UUID);
        CREATE TABLE public.embedding_set_member (tenant_id UUID NOT NULL, note_id UUID, embedding_set_id UUID);
        CREATE SCHEMA archive_stats_old;
        CREATE TABLE archive_stats_old.embedding_set (LIKE public.embedding_set INCLUDING ALL);
        CREATE TABLE archive_stats_old.embedding (LIKE public.embedding INCLUDING ALL);
        CREATE TABLE archive_stats_old.embedding_set_member (LIKE public.embedding_set_member INCLUDING ALL);
        CREATE FUNCTION public.trigger_update_embedding_set_stats() RETURNS TRIGGER LANGUAGE plpgsql
          AS $$ BEGIN RETURN COALESCE(NEW, OLD); END; $$;
        CREATE TRIGGER embedding_stats_trigger AFTER INSERT OR UPDATE OR DELETE ON public.embedding
          FOR EACH ROW EXECUTE FUNCTION public.trigger_update_embedding_set_stats();
        CREATE TRIGGER embedding_stats_trigger AFTER INSERT OR UPDATE OR DELETE ON archive_stats_old.embedding
          FOR EACH ROW EXECUTE FUNCTION public.trigger_update_embedding_set_stats();")
        .execute(&pool).await.unwrap();
    let original_oid: i64 = sqlx::query_scalar(
        "SELECT 'public.trigger_update_embedding_set_stats()'::regprocedure::oid::bigint",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let tenant = Uuid::new_v4();
    let other_tenant = Uuid::new_v4();
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    let embedding = Uuid::new_v4();
    for schema in ["public", "archive_stats_old"] {
        sqlx::query(&format!(
            "INSERT INTO {schema}.embedding_set VALUES
            ($1, $3, 99, 99, '2000-01-01T00:00:00Z'), ($2, $3, 99, 99, '2000-01-01T00:00:00Z')"
        ))
        .bind(first)
        .bind(second)
        .bind(tenant)
        .execute(&pool)
        .await
        .unwrap();
    }
    sqlx::query("INSERT INTO archive_stats_old.embedding VALUES ($1,$2,$3)")
        .bind(embedding)
        .bind(tenant)
        .bind(first)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO archive_stats_old.embedding_set_member VALUES ($1,$3,$4),($2,$5,$4)")
        .bind(tenant)
        .bind(other_tenant)
        .bind(Uuid::new_v4())
        .bind(first)
        .bind(Uuid::new_v4())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::raw_sql(include_str!(
        "../../../../migrations/20260911010000_embedding_reparent_statistics.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();
    let migrated_oid: i64 = sqlx::query_scalar(
        "SELECT 'public.trigger_update_embedding_set_stats()'::regprocedure::oid::bigint",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        original_oid, migrated_oid,
        "existing archive triggers retain their function binding"
    );
    let states = |pool: sqlx::PgPool| async move {
        sqlx::query_as::<_, (i32, i32)>("SELECT embedding_count, document_count FROM archive_stats_old.embedding_set ORDER BY id")
            .fetch_all(&pool).await.unwrap()
    };
    for target in [Some(second), None, Some(first)] {
        // Default search_path is public, despite the explicitly qualified archive write.
        sqlx::query("UPDATE archive_stats_old.embedding SET embedding_set_id=$1 WHERE id=$2")
            .bind(target)
            .bind(embedding)
            .execute(&pool)
            .await
            .unwrap();
        let mismatch: i64 = sqlx::query_scalar("SELECT count(*) FROM archive_stats_old.embedding_set s
            WHERE s.embedding_count IS DISTINCT FROM (SELECT count(*) FROM archive_stats_old.embedding e
                WHERE e.embedding_set_id=s.id AND e.tenant_id=s.tenant_id)
            OR s.document_count IS DISTINCT FROM (SELECT count(DISTINCT note_id) FROM archive_stats_old.embedding_set_member m
                WHERE m.embedding_set_id=s.id AND m.tenant_id=s.tenant_id)")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(
            mismatch, 0,
            "old/new/null coordinates and tenant-scoped counts must agree"
        );
        let untouched: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM public.embedding_set
            WHERE embedding_count=99 AND document_count=99 AND updated_at='2000-01-01T00:00:00Z'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            untouched, 2,
            "archive writes must not update public sets with identical IDs"
        );
    }
    let baseline = states(pool.clone()).await;
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("DELETE FROM archive_stats_old.embedding WHERE id=$1")
        .bind(embedding)
        .execute(&mut *tx)
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT sum(embedding_count)::bigint FROM archive_stats_old.embedding_set"
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap(),
        0
    );
    tx.rollback().await.unwrap();
    assert_eq!(
        states(pool.clone()).await,
        baseline,
        "rollback restores derived counts"
    );
    pool.close().await;
}

#[tokio::test]
async fn shard_config_declaration_fresh_database_and_archive_have_distinct_roots() {
    use crate::ArchiveRepository;
    let _guard = MIGRATION_REPAIR_TEST_LOCK.lock().await;
    let database = DisposableDatabase::create("shard_config_fresh")
        .await
        .expect("DATABASE_URL must select disposable PostgreSQL for this regression");
    let db = Database::connect(&database.url).await.unwrap();
    // Match the deployment database prerequisite, not only the extension's
    // installation in the disposable server's original database.
    sqlx::raw_sql("CREATE EXTENSION IF NOT EXISTS vector; CREATE EXTENSION IF NOT EXISTS postgis;")
        .execute(&db.pool)
        .await
        .unwrap();
    db.migrate().await.unwrap();
    db.migrate().await.unwrap();
    let public_bootstrap: i64 =
        sqlx::query_scalar("SELECT count(*) FROM shard_embedding_set_bootstrap")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(
        public_bootstrap, 1,
        "fresh runner records the seed it actually created"
    );
    let (configs, declarations): (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM embedding_config WHERE shard_export_present),
            (SELECT count(*) FROM shard_embedding_config_declaration)",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert!(configs > 0);
    assert_eq!(
        configs, declarations,
        "actual fresh migration preserves public seed roots"
    );
    let name = format!("cfg-fresh-{}", Uuid::new_v4().simple());
    let archive = db
        .archives
        .create_archive_schema(&name, None)
        .await
        .unwrap();
    let ctx = db.for_schema(&archive.schema_name).unwrap();
    let mut tx = ctx.begin_tx().await.unwrap();
    let bootstrap: i64 = sqlx::query_scalar("SELECT count(*) FROM shard_embedding_set_bootstrap")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(
        bootstrap, 1,
        "new archive records its exact bootstrap identity"
    );
    let trigger: String = sqlx::query_scalar(
        "SELECT pg_get_triggerdef(oid) FROM pg_trigger
        WHERE tgrelid='note'::regclass AND tgname='trg_auto_add_note_to_embedding_sets'",
    )
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert!(trigger.contains("new.deleted_at IS NULL"));
    assert!(trigger.contains("app.shard_import"));
    sqlx::query(
        "INSERT INTO note(id,format,source,created_at_utc,updated_at_utc,deleted_at)
        VALUES ($1,'markdown','bootstrap-tombstone-test',now(),now(),now())",
    )
    .bind(Uuid::new_v4())
    .execute(&mut *tx)
    .await
    .unwrap();
    let members: i64 = sqlx::query_scalar("SELECT count(*) FROM embedding_set_member")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(
        members, 0,
        "native tombstones do not create automatic memberships"
    );
    let counts: (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM shard_embedding_config_declaration),
            (SELECT count(*) FROM embedding_set WHERE embedding_config_id IS NOT NULL)",
    )
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(
        counts,
        (0, 1),
        "new archive has a live default dependency but no copied config roots"
    );
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO embedding_config (id, name, model, dimension) VALUES ($1, $2, 'fresh-native', 768)")
        .bind(id).bind(format!("fresh-{id}")).execute(&mut *tx).await.unwrap();
    let declared: Vec<Uuid> =
        sqlx::query_scalar("SELECT config_id FROM shard_embedding_config_declaration")
            .fetch_all(&mut *tx)
            .await
            .unwrap();
    assert_eq!(declared, [id]);
    tx.rollback().await.unwrap();
    let mut tx = ctx.begin_tx().await.unwrap();
    sqlx::query(
        "UPDATE embedding_set SET description = 'native customization' WHERE slug = 'default'",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    let bootstrap: i64 = sqlx::query_scalar("SELECT count(*) FROM shard_embedding_set_bootstrap")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(bootstrap, 0, "native edit adopts ordinary live ownership");
    tx.rollback().await.unwrap();
    let mut tx = ctx.begin_tx().await.unwrap();
    let bootstrap: i64 = sqlx::query_scalar("SELECT count(*) FROM shard_embedding_set_bootstrap")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(bootstrap, 1, "rollback restores custody");
    sqlx::query("DELETE FROM embedding_set WHERE slug = 'default'")
        .execute(&mut *tx)
        .await
        .unwrap();
    // Exercise an actual older schema, not a current schema with deleted data.
    sqlx::query("DROP TABLE shard_embedding_set_bootstrap")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    sqlx::query("UPDATE public.archive_registry SET schema_version = 0 WHERE schema_name = $1")
        .bind(&archive.schema_name)
        .execute(&db.pool)
        .await
        .unwrap();
    db.archives.sync_archive_schema(&name).await.unwrap();
    let mut tx = ctx.begin_tx().await.unwrap();
    let repaired: i64 = sqlx::query_scalar("SELECT count(*) FROM shard_embedding_set_bootstrap b
        JOIN embedding_set s ON s.id = b.set_id AND s.tenant_id = b.tenant_id WHERE s.slug = 'default'")
        .fetch_one(&mut *tx).await.unwrap();
    assert_eq!(repaired, 1, "repair records only the new seed it creates");
    tx.rollback().await.unwrap();
    db.archives.drop_archive_schema(&name).await.unwrap();
    db.pool.close().await;
}

#[tokio::test]
async fn shard_bootstrap_migration_preserves_existing_defaults_without_inference() {
    let _guard = MIGRATION_REPAIR_TEST_LOCK.lock().await;
    let database = DisposableDatabase::create("shard_bootstrap_upgrade")
        .await
        .expect("DATABASE_URL must select disposable PostgreSQL");
    let pool = create_pool(&database.url).await.unwrap();
    sqlx::raw_sql(
        "CREATE TABLE public.archive_registry (schema_name TEXT NOT NULL);
        CREATE TABLE public.embedding_set (id UUID PRIMARY KEY, tenant_id UUID NOT NULL,
            name TEXT NOT NULL, slug TEXT NOT NULL, is_system BOOL NOT NULL,
            shard_export_present BOOL NOT NULL, UNIQUE(tenant_id,id));
        CREATE SCHEMA archive_bootstrap_old;
        CREATE TABLE archive_bootstrap_old.embedding_set (LIKE public.embedding_set INCLUDING ALL);
        INSERT INTO public.archive_registry VALUES ('public'), ('archive_bootstrap_old');
        INSERT INTO public.embedding_set VALUES
            ('00000000-0000-0000-0000-000000000001','00000000-0000-0000-0000-000000000000',
            'Default','default',TRUE,TRUE);
        INSERT INTO archive_bootstrap_old.embedding_set SELECT * FROM public.embedding_set;",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::raw_sql(include_str!(
        "../../../../migrations/20260910020000_embedding_set_bootstrap_custody.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();
    for schema in ["public", "archive_bootstrap_old"] {
        let count: i64 = sqlx::query_scalar(&format!(
            "SELECT count(*) FROM {schema}.shard_embedding_set_bootstrap"
        ))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            count, 0,
            "existing default/system rows are not cleanup authority"
        );
        let state: (String, bool, bool) = sqlx::query_as(&format!(
            "SELECT slug,is_system,shard_export_present FROM {schema}.embedding_set"
        ))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(state, ("default".to_string(), true, true));
        let guards: (bool, bool, i64) = sqlx::query_as(
            "SELECT relrowsecurity,relforcerowsecurity,
            (SELECT count(*) FROM pg_policy WHERE polrelid=c.oid)
            FROM pg_class c WHERE c.oid=to_regclass($1)",
        )
        .bind(format!("{schema}.shard_embedding_set_bootstrap"))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(guards, (true, true, 1));
    }
    pool.close().await;
}

#[tokio::test]
async fn shard_config_declaration_migration_preserves_roots_and_tracks_native_writes() {
    let _guard = MIGRATION_REPAIR_TEST_LOCK.lock().await;
    let database = DisposableDatabase::create("shard_config_declarations")
        .await
        .expect("DATABASE_URL must select disposable PostgreSQL for this regression");
    let pool = create_pool(&database.url).await.unwrap();
    sqlx::raw_sql("CREATE TABLE public.embedding_config (
            id UUID PRIMARY KEY, tenant_id UUID NOT NULL, shard_export_present BOOL NOT NULL,
            UNIQUE (tenant_id, id));
        CREATE TABLE public.archive_registry (schema_name TEXT NOT NULL);
        CREATE SCHEMA archive_config_old;
        CREATE TABLE archive_config_old.note (id UUID PRIMARY KEY);
        INSERT INTO public.archive_registry VALUES ('public'), ('archive_config_old');
        INSERT INTO public.embedding_config VALUES
            ('00000000-0000-0000-0000-000000000001', '00000000-0000-0000-0000-000000000000', TRUE),
            ('00000000-0000-0000-0000-000000000002', '00000000-0000-0000-0000-000000000000', FALSE);")
        .execute(&pool).await.unwrap();
    sqlx::raw_sql(include_str!(
        "../../../../migrations/20260910010000_archive_shard_config_declarations.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();
    for schema in ["public", "archive_config_old"] {
        let ids = sqlx::query_scalar::<_, Uuid>(&format!(
            "SELECT config_id FROM {schema}.shard_embedding_config_declaration ORDER BY config_id"
        ))
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(ids, [Uuid::from_u128(1)]);
        let guards: (bool, bool, i64) = sqlx::query_as(
            "SELECT relrowsecurity, relforcerowsecurity,
                (SELECT count(*) FROM pg_policy WHERE polrelid = c.oid)
             FROM pg_class c WHERE c.oid = to_regclass($1)",
        )
        .bind(format!("{schema}.shard_embedding_config_declaration"))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(guards, (true, true, 1));
    }
    let mut tx = pool.begin().await.unwrap();
    sqlx::raw_sql(
        "SET LOCAL search_path TO archive_config_old, public;
        INSERT INTO embedding_config VALUES
            ('00000000-0000-0000-0000-000000000003', '00000000-0000-0000-0000-000000000000', TRUE);
        UPDATE embedding_config SET shard_export_present = TRUE
            WHERE id = '00000000-0000-0000-0000-000000000002';",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    let local: Vec<Uuid> = sqlx::query_scalar(
        "SELECT config_id FROM shard_embedding_config_declaration ORDER BY config_id",
    )
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    assert_eq!(
        local,
        [Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(3)]
    );
    let shared: Vec<Uuid> = sqlx::query_scalar(
        "SELECT config_id FROM public.shard_embedding_config_declaration ORDER BY config_id",
    )
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    assert_eq!(shared, [Uuid::from_u128(1)]);
    tx.rollback().await.unwrap();
    let remaining: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM archive_config_old.shard_embedding_config_declaration",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        remaining, 1,
        "native registry and declaration writes roll back together"
    );
    pool.close().await;
}

struct DisposableDatabase {
    url: String,
}

impl DisposableDatabase {
    async fn create(label: &str) -> Option<Self> {
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            eprintln!("skipping migration repair test: DATABASE_URL unavailable");
            return None;
        };

        let options = PgConnectOptions::from_str(&database_url)
            .expect("DATABASE_URL must be a valid PostgreSQL URL when configured");
        let database_name = format!("fortemi_{label}_{}", Uuid::now_v7().simple());
        let url = options.database(&database_name).to_url_lossy().to_string();

        <sqlx::Postgres as MigrateDatabase>::create_database(&url)
            .await
            .expect("create disposable migration test database");

        Some(Self { url })
    }
}

impl Drop for DisposableDatabase {
    fn drop(&mut self) {
        let url = self.url.clone();
        std::thread::spawn(move || {
            if let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                runtime.block_on(async move {
                    let _ = <sqlx::Postgres as MigrateDatabase>::force_drop_database(&url).await;
                });
            }
        })
        .join()
        .expect("join disposable database cleanup thread");
    }
}

#[derive(Clone, Copy)]
enum InvalidLedger {
    Dirty,
    UnknownVersion,
    ChecksumMismatch,
}

async fn setup_repair_candidate(pool: &sqlx::PgPool, archive_schema: &str) {
    sqlx::query(
        r#"
        CREATE TABLE public.tenant_registry (
            id UUID PRIMARY KEY,
            slug TEXT NOT NULL UNIQUE,
            display_name TEXT NOT NULL,
            status TEXT NOT NULL
        )
        "#,
    )
    .execute(pool)
    .await
    .expect("create tenant registry fixture");
    sqlx::query(
        r#"
        CREATE TABLE public.archive_registry (
            id UUID PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            schema_name TEXT NOT NULL UNIQUE,
            description TEXT,
            note_count INTEGER DEFAULT 0,
            size_bytes BIGINT DEFAULT 0,
            is_default BOOLEAN DEFAULT FALSE,
            tenant_id UUID NOT NULL
        )
        "#,
    )
    .execute(pool)
    .await
    .expect("create archive registry fixture");
    sqlx::query("CREATE TABLE IF NOT EXISTS public._sqlx_migrations (version BIGINT PRIMARY KEY, description TEXT NOT NULL, installed_on TIMESTAMPTZ NOT NULL DEFAULT now(), success BOOLEAN NOT NULL, checksum BYTEA NOT NULL, execution_time BIGINT NOT NULL)")
        .execute(pool)
        .await
        .expect("create migration ledger fixture");
    sqlx::query(&format!("CREATE SCHEMA {archive_schema}"))
        .execute(pool)
        .await
        .expect("create archive schema fixture");
    sqlx::query(&format!(
        "CREATE TABLE {archive_schema}.note (id UUID PRIMARY KEY, tenant_id UUID NOT NULL)"
    ))
    .execute(pool)
    .await
    .expect("create archive note fixture");
    sqlx::query("INSERT INTO public.tenant_registry (id, slug, display_name, status) VALUES ($1, 'local', 'Local personal server', 'active')")
        .bind(Uuid::nil())
        .execute(pool)
        .await
        .expect("seed tenant fixture");
    sqlx::query("INSERT INTO public.archive_registry (id, name, schema_name, tenant_id) VALUES ($1, $2, $3, $4)")
        .bind(Uuid::now_v7())
        .bind(format!("repair-{}", Uuid::now_v7().simple()))
        .bind(archive_schema)
        .bind(Uuid::nil())
        .execute(pool)
        .await
        .expect("seed archive registry fixture");
}

async fn seed_current_migration_ledger_with_early_pending_and_late_bad_checksum(
    pool: &sqlx::PgPool,
) {
    sqlx::query("CREATE TABLE IF NOT EXISTS public._sqlx_migrations (version BIGINT PRIMARY KEY, description TEXT NOT NULL, installed_on TIMESTAMPTZ NOT NULL DEFAULT now(), success BOOLEAN NOT NULL, checksum BYTEA NOT NULL, execution_time BIGINT NOT NULL)")
        .execute(pool)
        .await
        .expect("create migration ledger fixture");

    let migrations: Vec<_> = sqlx::migrate!("../../migrations")
        .iter()
        .filter(|migration| !migration.migration_type.is_down_migration())
        .collect();
    let pending_version = migrations
        .first()
        .expect("current migration source must not be empty")
        .version;
    let late_bad_version = migrations
        .iter()
        .rev()
        .find(|migration| migration.version > pending_version)
        .expect("test requires a later applied migration after the pending version")
        .version;

    for migration in migrations {
        if migration.version == pending_version {
            continue;
        }
        let checksum: &[u8] = if migration.version == late_bad_version {
            &[0]
        } else {
            migration.checksum.as_ref()
        };
        sqlx::query("INSERT INTO public._sqlx_migrations (version, description, success, checksum, execution_time) VALUES ($1, $2, true, $3, 0)")
            .bind(migration.version)
            .bind(migration.description.to_string())
            .bind(checksum)
            .execute(pool)
            .await
            .expect("seed migration ledger row with late bad checksum");
    }
}

async fn seed_current_migration_ledger(pool: &sqlx::PgPool) {
    sqlx::query("CREATE TABLE IF NOT EXISTS public._sqlx_migrations (version BIGINT PRIMARY KEY, description TEXT NOT NULL, installed_on TIMESTAMPTZ NOT NULL DEFAULT now(), success BOOLEAN NOT NULL, checksum BYTEA NOT NULL, execution_time BIGINT NOT NULL)")
        .execute(pool)
        .await
        .expect("create migration ledger fixture");

    for migration in sqlx::migrate!("../../migrations").iter() {
        if migration.migration_type.is_down_migration() {
            continue;
        }
        sqlx::query("INSERT INTO public._sqlx_migrations (version, description, success, checksum, execution_time) VALUES ($1, $2, true, $3, 0)")
            .bind(migration.version)
            .bind(migration.description.to_string())
            .bind(migration.checksum.as_ref())
            .execute(pool)
            .await
            .expect("seed current migration ledger row");
    }
}

async fn insert_invalid_ledger(pool: &sqlx::PgPool, kind: InvalidLedger) {
    let (version, description, success) = match kind {
        InvalidLedger::Dirty => (20260903010000_i64, "dirty", false),
        InvalidLedger::UnknownVersion => (99999999999999_i64, "unknown", true),
        InvalidLedger::ChecksumMismatch => {
            let version = sqlx::migrate!("../../migrations")
                .iter()
                .find(|migration| !migration.migration_type.is_down_migration())
                .expect("current migration source must not be empty")
                .version;
            (version, "bad_checksum", true)
        }
    };

    sqlx::query("INSERT INTO public._sqlx_migrations (version, description, success, checksum, execution_time) VALUES ($1, $2, $3, decode('00', 'hex'), 0)")
        .bind(version)
        .bind(description)
        .bind(success)
        .execute(pool)
        .await
        .expect("seed invalid migration ledger");
}

async fn archive_note_repair_index_exists(pool: &sqlx::PgPool, archive_schema: &str) -> bool {
    sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1
              FROM pg_index i
              JOIN pg_class idx ON idx.oid = i.indexrelid
              JOIN pg_class tbl ON tbl.oid = i.indrelid
              JOIN pg_namespace n ON n.oid = tbl.relnamespace
             WHERE n.nspname = $1
               AND tbl.relname = 'note'
               AND idx.relname = 'uq_archive_note_tenant_id_id'
               AND i.indisunique
        )
        "#,
    )
    .bind(archive_schema)
    .fetch_one(pool)
    .await
    .expect("inspect repair index")
}

async fn advisory_lock_count(pool: &sqlx::PgPool) -> i64 {
    sqlx::query_scalar(
        r#"
        SELECT count(*)::bigint
          FROM pg_locks
         WHERE locktype = 'advisory'
           AND database = (SELECT oid FROM pg_database WHERE datname = current_database())
           AND granted
        "#,
    )
    .fetch_one(pool)
    .await
    .expect("inspect advisory locks")
}

fn error_chain_contains(error: &matric_core::Error, needle: &str) -> bool {
    let mut current: Option<&(dyn StdError + 'static)> = Some(error);
    while let Some(error) = current {
        if error.to_string().contains(needle) {
            return true;
        }
        current = error.source();
    }
    false
}

#[tokio::test]
async fn migration_runner_rejects_invalid_ledger_before_legacy_archive_repair() {
    let _guard = MIGRATION_REPAIR_TEST_LOCK.lock().await;
    let Some(database) = DisposableDatabase::create("invalid_ledger").await else {
        return;
    };
    let pool = create_pool(&database.url)
        .await
        .expect("connect to disposable migration test database");

    for (kind, expected) in [
        (InvalidLedger::Dirty, "partially applied"),
        (
            InvalidLedger::UnknownVersion,
            "previously applied but is missing",
        ),
        (
            InvalidLedger::ChecksumMismatch,
            "previously applied but has been modified",
        ),
    ] {
        let archive_schema = format!("archive_repair_{}", Uuid::now_v7().simple());
        setup_repair_candidate(&pool, &archive_schema).await;
        insert_invalid_ledger(&pool, kind).await;

        let error = Database::new(pool.clone())
            .migrate()
            .await
            .expect_err("invalid migration ledger must reject before repair");
        assert!(
            error_chain_contains(&error, expected),
            "expected error chain to contain {expected:?}, got {error:?}"
        );
        assert!(
            !archive_note_repair_index_exists(&pool, &archive_schema).await,
            "legacy archive repair must not mutate before ledger validation"
        );
        assert_eq!(
            advisory_lock_count(&pool).await,
            0,
            "migration runner must release advisory lock after rejection"
        );

        sqlx::query("DROP SCHEMA public CASCADE")
            .execute(&pool)
            .await
            .expect("drop invalid ledger fixture schema");
        sqlx::query("CREATE SCHEMA public")
            .execute(&pool)
            .await
            .expect("recreate invalid ledger fixture schema");
    }

    pool.close().await;
}

#[tokio::test]
async fn migration_runner_prevalidates_late_checksum_before_pending_apply_or_repair() {
    let _guard = MIGRATION_REPAIR_TEST_LOCK.lock().await;
    let Some(database) = DisposableDatabase::create("late_checksum").await else {
        return;
    };
    let pool = create_pool(&database.url)
        .await
        .expect("connect to disposable migration test database");
    let archive_schema = format!("archive_repair_{}", Uuid::now_v7().simple());
    setup_repair_candidate(&pool, &archive_schema).await;
    seed_current_migration_ledger_with_early_pending_and_late_bad_checksum(&pool).await;

    let error = Database::new(pool.clone())
        .migrate()
        .await
        .expect_err("late checksum mismatch must reject before pending apply or repair");
    assert!(
        error_chain_contains(&error, "previously applied but has been modified"),
        "expected checksum mismatch error, got {error:?}"
    );
    assert!(
        !archive_note_repair_index_exists(&pool, &archive_schema).await,
        "late checksum mismatch must be validated before pending apply or legacy archive repair mutates"
    );
    assert_eq!(
        advisory_lock_count(&pool).await,
        0,
        "migration runner must release advisory lock after late checksum rejection"
    );

    pool.close().await;
}

#[tokio::test]
async fn migration_runner_second_pass_is_idempotent_and_releases_lock() {
    let _guard = MIGRATION_REPAIR_TEST_LOCK.lock().await;
    let Some(database) = DisposableDatabase::create("idempotent").await else {
        return;
    };
    let pool = create_pool(&database.url)
        .await
        .expect("connect to disposable migration test database");
    seed_current_migration_ledger(&pool).await;
    let db = Database::new(pool.clone());

    db.migrate().await.expect("first idempotent migration pass");
    let after_first: (i64, i64) = sqlx::query_as(
        "SELECT COALESCE(max(version), 0), count(*) FROM public._sqlx_migrations WHERE success = true",
    )
    .fetch_one(&pool)
    .await
    .expect("inspect first migration ledger");
    assert_eq!(
        advisory_lock_count(&pool).await,
        0,
        "first migration pass must release advisory lock"
    );

    db.migrate().await.expect("second migration pass");
    let after_second: (i64, i64) = sqlx::query_as(
        "SELECT COALESCE(max(version), 0), count(*) FROM public._sqlx_migrations WHERE success = true",
    )
    .fetch_one(&pool)
    .await
    .expect("inspect second migration ledger");
    assert_eq!(after_second, after_first);
    assert_eq!(
        advisory_lock_count(&pool).await,
        0,
        "second migration pass must release advisory lock"
    );

    pool.close().await;
}

#[tokio::test]
async fn legacy_archive_note_repair_creates_tenant_qualified_unique_index() {
    let _guard = MIGRATION_REPAIR_TEST_LOCK.lock().await;
    let Some(database) = DisposableDatabase::create("repair_helper").await else {
        return;
    };
    let pool = create_pool(&database.url)
        .await
        .expect("connect to disposable migration test database");
    let archive_schema = format!("archive_repair_{}", Uuid::now_v7().simple());
    setup_repair_candidate(&pool, &archive_schema).await;

    let mut conn = pool.acquire().await.expect("acquire repair connection");
    Database::repair_legacy_archive_tenant_note_indexes(&mut conn)
        .await
        .expect("repair legacy archive note index");
    drop(conn);

    let rows = sqlx::query(
        r#"
        SELECT a.attname
          FROM pg_index i
          JOIN pg_class idx ON idx.oid = i.indexrelid
          JOIN pg_class tbl ON tbl.oid = i.indrelid
          JOIN pg_namespace n ON n.oid = tbl.relnamespace
          JOIN unnest(i.indkey) WITH ORDINALITY key(attnum, ordinality) ON true
          JOIN pg_attribute a ON a.attrelid = tbl.oid AND a.attnum = key.attnum
         WHERE n.nspname = $1
           AND tbl.relname = 'note'
           AND idx.relname = 'uq_archive_note_tenant_id_id'
           AND i.indisunique
         ORDER BY key.ordinality
        "#,
    )
    .bind(&archive_schema)
    .fetch_all(&pool)
    .await
    .expect("inspect repair index");
    let columns: Vec<String> = rows
        .into_iter()
        .map(|row| row.get::<String, _>("attname"))
        .collect();
    assert_eq!(columns, ["tenant_id", "id"]);

    pool.close().await;
}
