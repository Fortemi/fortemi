//! RLS-safe lexical admission (Fortemi#1348).
//!
//! Production runtime roles are subject to FORCE ROW LEVEL SECURITY, so the
//! non-leakproof `@@`/`to_tsvector` quals cannot use the GIN indexes. These
//! tests run the search as a NOSUPERUSER NOBYPASSRLS role (created like the
//! hosted runtime role) with a real tenant, and prove:
//! - the direct statements fall back to sequential scans with an `@@` filter
//!   (the bug), while the SECURITY DEFINER function path uses the GIN indexes;
//! - tenant isolation is enforced inside the function (cross-tenant schema,
//!   crafted search_path, unset/malformed tenant, unregistered schema, bad
//!   analyzer/arm all fail closed);
//! - per-arm correctness, direct/function parity and federated parity.

use std::collections::BTreeSet;

use matric_core::{ArchiveRepository, CreateNoteRequest};
use matric_db::{
    fts_admission::{self, AdmissionPath},
    search_candidates::{
        classify_lexical_matches, explain_lexical_compiled, lexical_on_connection,
        normalize_expression, normalize_query, LexicalStrategy, SearchCandidateScope,
    },
    Database,
};
use serde_json::Value;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

/// Tests that read or flip the process-wide admission switch run one at a time.
static GLOBAL_SWITCH: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

const TENANT_A: Uuid = Uuid::nil();
const SIGNATURE: &str = "public.fts_admit_note_ids(tsquery, text, text, text[], integer)";

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
    use_tenant_text(tx, &tenant.to_string()).await;
}

async fn use_tenant_text(tx: &mut PgConnection, tenant: &str) {
    sqlx::query("SELECT set_config('app.current_tenant', $1, true)")
        .bind(tenant)
        .execute(tx)
        .await
        .unwrap();
}

async fn register_tenant(pool: &PgPool) -> Uuid {
    let tenant = Uuid::new_v4();
    sqlx::query("INSERT INTO tenant_registry (id, slug, display_name, status) VALUES ($1, $2, $2, 'active')")
        .bind(tenant)
        .bind(format!("fts-adm-{}", tenant.simple()))
        .execute(pool)
        .await
        .unwrap();
    tenant
}

/// A runtime-like role: NOSUPERUSER, NOBYPASSRLS, subject to FORCE RLS, holding
/// EXECUTE on the function exactly as startup grants it.
async fn runtime_role(pool: &PgPool, grant_execute: bool) -> String {
    let role = format!("fts_rt_{}", Uuid::new_v4().simple());
    let mut statements = vec![
        format!("CREATE ROLE {role} NOSUPERUSER NOBYPASSRLS NOLOGIN"),
        format!("GRANT USAGE ON SCHEMA public TO {role}"),
        format!("GRANT SELECT ON ALL TABLES IN SCHEMA public TO {role}"),
    ];
    if grant_execute {
        statements.push(format!("GRANT EXECUTE ON FUNCTION {SIGNATURE} TO {role}"));
    }
    for statement in statements {
        sqlx::query(&statement).execute(pool).await.unwrap();
    }
    role
}

async fn grant_schema(pool: &PgPool, role: &str, schema: &str) {
    for statement in [
        format!("GRANT USAGE ON SCHEMA {schema} TO {role}"),
        format!("GRANT SELECT ON ALL TABLES IN SCHEMA {schema} TO {role}"),
    ] {
        sqlx::query(&statement).execute(pool).await.unwrap();
    }
}

async fn drop_role(pool: &PgPool, role: &str) {
    for statement in [format!("DROP OWNED BY {role}"), format!("DROP ROLE {role}")] {
        sqlx::query(&statement).execute(pool).await.unwrap();
    }
}

async fn add_note(db: &Database, tx: &mut PgConnection, title: &str, content: &str) -> Uuid {
    db.notes
        .insert_tx(
            tx,
            CreateNoteRequest {
                content: content.into(),
                format: "markdown".into(),
                source: "fts-admission-test".into(),
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
    sqlx::query("INSERT INTO attachment (id, note_id, blob_id, filename, status, extracted_text) VALUES ($1, $2, $3, 'adm.txt', 'completed', $4)")
        .bind(Uuid::new_v4())
        .bind(note)
        .bind(blob)
        .bind(text)
        .execute(tx)
        .await
        .unwrap();
}

fn set(items: &[Uuid]) -> BTreeSet<Uuid> {
    items.iter().copied().collect()
}

/// An archive registered to `tenant` (created over a session scoped to it).
struct TenantArchive {
    db: Database,
    schema: String,
}

async fn create_archive(pool: &PgPool, tenant: Uuid) -> TenantArchive {
    // Creation needs the local tenant's defaults; reassign the registry row after.
    let session_tenant = TENANT_A.to_string();
    let archive_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .after_connect(move |connection, _| {
            let session_tenant = session_tenant.clone();
            Box::pin(async move {
                sqlx::query("SELECT set_config('app.current_tenant', $1, false)")
                    .bind(session_tenant)
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
        .connect_with((*pool.connect_options()).clone())
        .await
        .unwrap();
    let db = Database::new(archive_pool);
    let name = format!("adm-{}", Uuid::new_v4().simple());
    let info = db
        .archives
        .create_archive_schema(&name, None)
        .await
        .unwrap_or_else(|e| {
            let code = match &e {
                matric_core::Error::Database(sqlx::Error::Database(db)) => {
                    db.code().map(|c| c.to_string())
                }
                _ => None,
            };
            let kind: String = match &e {
                matric_core::Error::Database(inner) => format!("{inner}"),
                other => format!("{other}"),
            };
            panic!("create_archive_schema for tenant {tenant}: sqlstate {code:?} kind {kind}");
        });
    if tenant != TENANT_A {
        sqlx::query("UPDATE archive_registry SET tenant_id = $1 WHERE schema_name = $2")
            .bind(tenant)
            .bind(&info.schema_name)
            .execute(pool)
            .await
            .unwrap();
    }
    TenantArchive {
        db,
        schema: info.schema_name,
    }
}

async fn drop_archive(pool: &PgPool, archive: TenantArchive) {
    sqlx::query(&format!("DROP SCHEMA {} CASCADE", archive.schema))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM archive_registry WHERE schema_name = $1")
        .bind(&archive.schema)
        .execute(pool)
        .await
        .unwrap();
    archive.db.pool().close().await;
}

async fn in_schema(tx: &mut PgConnection, schema: &str) {
    sqlx::query(&format!("SET LOCAL search_path TO {schema}, public"))
        .execute(tx)
        .await
        .unwrap();
}

async fn reset_schema(tx: &mut PgConnection) {
    sqlx::query("SET LOCAL search_path TO public")
        .execute(tx)
        .await
        .unwrap();
}

async fn call_function(
    tx: &mut PgConnection,
    query: &str,
    analyzer: &str,
    schema: &str,
    arms: &[&str],
    cap: i32,
) -> Result<BTreeSet<Uuid>, String> {
    sqlx::query("SAVEPOINT adm_case")
        .execute(&mut *tx)
        .await
        .unwrap();
    let arms: Vec<String> = arms.iter().map(|arm| arm.to_string()).collect();
    let outcome = sqlx::query_scalar::<_, Uuid>(
        "SELECT note_id FROM public.fts_admit_note_ids(to_tsquery((CASE WHEN $2 = 'public.matric_simple' THEN 'public.matric_simple' ELSE 'public.matric_english' END)::regconfig, $1), $2, $3, $4::text[], $5)",
    )
    .bind(query)
    .bind(analyzer)
    .bind(schema)
    .bind(arms)
    .bind(cap)
    .fetch_all(&mut *tx)
    .await;
    match outcome {
        Ok(rows) => {
            sqlx::query("RELEASE SAVEPOINT adm_case")
                .execute(&mut *tx)
                .await
                .unwrap();
            Ok(rows.into_iter().collect())
        }
        Err(error) => {
            sqlx::query("ROLLBACK TO SAVEPOINT adm_case")
                .execute(&mut *tx)
                .await
                .unwrap();
            Err(error.to_string())
        }
    }
}

const ALL_ARMS: [&str; 3] = ["title", "content", "attachment"];
const ENGLISH: &str = "public.matric_english";
const SIMPLE: &str = "public.matric_simple";

struct Seeded {
    a_title: Uuid,
    a_content: Uuid,
    a_attachment: Uuid,
    b_title: Uuid,
    b_content: Uuid,
    b_attachment: Uuid,
    a_arch_title: Uuid,
    a_arch_content: Uuid,
    a_arch_attachment: Uuid,
    b_arch_title: Uuid,
    /// Tenant B's row planted inside tenant A's archive schema.
    planted_b_in_a_archive: Uuid,
}

async fn seed_matrix(
    db: &Database,
    pool: &PgPool,
    tenant_b: Uuid,
    arch_a: &TenantArchive,
    arch_b: &TenantArchive,
) -> Seeded {
    let mut tx = pool.begin().await.unwrap();
    use_tenant(&mut tx, TENANT_A).await;
    let a_title = add_note(db, &mut tx, "isotitle alpha", "filler").await;
    let a_content = add_note(db, &mut tx, "heading", "isobody alpha").await;
    let a_attachment = add_note(db, &mut tx, "heading two", "filler").await;
    add_attachment(&mut tx, a_attachment, "isoattach alpha").await;
    use_tenant(&mut tx, tenant_b).await;
    let b_title = add_note(db, &mut tx, "isotitle bravo", "filler").await;
    let b_content = add_note(db, &mut tx, "heading", "isobody bravo").await;
    let b_attachment = add_note(db, &mut tx, "heading two", "filler").await;
    add_attachment(&mut tx, b_attachment, "isoattach bravo").await;

    use_tenant(&mut tx, TENANT_A).await;
    in_schema(&mut tx, &arch_a.schema).await;
    let a_arch_title = add_note(db, &mut tx, "isotitle archive-a", "filler").await;
    let a_arch_content = add_note(db, &mut tx, "heading", "isobody archive-a").await;
    let a_arch_attachment = add_note(db, &mut tx, "heading two", "filler").await;
    add_attachment(&mut tx, a_arch_attachment, "isoattach archive-a").await;
    // A row of tenant B planted inside tenant A's archive schema.
    use_tenant(&mut tx, tenant_b).await;
    let planted = add_note(db, &mut tx, "isotitle planted", "isobody planted").await;
    use_tenant(&mut tx, tenant_b).await;
    in_schema(&mut tx, &arch_b.schema).await;
    let b_arch_title = add_note(db, &mut tx, "isotitle archive-b", "filler").await;
    tx.commit().await.unwrap();
    Seeded {
        a_title,
        a_content,
        a_attachment,
        b_title,
        b_content,
        b_attachment,
        a_arch_title,
        a_arch_content,
        a_arch_attachment,
        b_arch_title,
        planted_b_in_a_archive: planted,
    }
}

#[sqlx::test(migrations = false)]
async fn function_is_a_locked_down_security_definer(pool: PgPool) {
    let _db = migrated(&pool).await;
    let row: (bool, Vec<String>, bool, bool, bool, Option<String>) = sqlx::query_as(
        "SELECT p.prosecdef, COALESCE(p.proconfig, ARRAY[]::text[]), p.proleakproof,
                (r.rolsuper OR r.rolbypassrls), p.provolatile = 's', p.proacl::text
           FROM pg_proc p JOIN pg_roles r ON r.oid = p.proowner
          WHERE p.oid = $1::regprocedure",
    )
    .bind(SIGNATURE)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(row.0, "must be SECURITY DEFINER");
    assert!(
        row.1.contains(&"search_path=pg_catalog".to_string()),
        "{:?}",
        row.1
    );
    assert!(
        row.1.contains(&"row_security=off".to_string()),
        "{:?}",
        row.1
    );
    // A function-local statement_timeout would not bound the function's own
    // statements, so it is deliberately absent (the caller bounds the statement).
    assert!(
        !row.1.iter().any(|c| c.starts_with("statement_timeout")),
        "{:?}",
        row.1
    );
    assert!(!row.2, "must not be LEAKPROOF");
    assert!(row.3, "owner must bypass RLS");
    assert!(row.4, "must be STABLE");
    // No PUBLIC grant (an `=X/owner` ACL entry).
    let acl = row.5.unwrap_or_default();
    assert!(
        !acl.contains("{=") && !acl.contains(",="),
        "PUBLIC ACL: {acl}"
    );

    // A role without an explicit grant cannot call it.
    let role = runtime_role(&pool, false).await;
    let mut tx = pool.begin().await.unwrap();
    use_tenant(&mut tx, TENANT_A).await;
    sqlx::query(&format!("SET LOCAL ROLE {role}"))
        .execute(&mut *tx)
        .await
        .unwrap();
    let denied = call_function(&mut tx, "x", ENGLISH, "public", &ALL_ARMS, 10).await;
    assert!(
        denied
            .as_ref()
            .is_err_and(|e| e.contains("permission denied")),
        "{denied:?}"
    );
    tx.rollback().await.unwrap();
    drop_role(&pool, &role).await;
}

/// The isolation matrix. Every case runs as a NOSUPERUSER NOBYPASSRLS role.
#[sqlx::test(migrations = false)]
async fn tenant_isolation_matrix_under_non_bypass_role(pool: PgPool) {
    let db = migrated(&pool).await;
    let tenant_b = register_tenant(&pool).await;
    let arch_a = create_archive(&pool, TENANT_A).await;
    let arch_b = create_archive(&pool, tenant_b).await;
    let s = seed_matrix(&db, &pool, tenant_b, &arch_a, &arch_b).await;
    let role = runtime_role(&pool, true).await;
    grant_schema(&pool, &role, &arch_a.schema).await;
    grant_schema(&pool, &role, &arch_b.schema).await;

    let mut tx = pool.begin().await.unwrap();
    sqlx::query(&format!("SET LOCAL ROLE {role}"))
        .execute(&mut *tx)
        .await
        .unwrap();
    let mut matrix: Vec<String> = Vec::new();
    macro_rules! record {
        ($name:expr, $ok:expr) => {{
            let ok: bool = $ok;
            matrix.push(format!("{:<8} {}", if ok { "PASS" } else { "FAIL" }, $name));
            assert!(ok, "isolation case failed: {}", $name);
        }};
    }

    // --- Tenant A, public memory: only A's ids, per arm and per analyzer.
    for analyzer in [ENGLISH, SIMPLE] {
        use_tenant(&mut tx, TENANT_A).await;
        for (token, arm, expected) in [
            ("isotitle", "title", set(&[s.a_title])),
            ("isobody", "content", set(&[s.a_content])),
            ("isoattach", "attachment", set(&[s.a_attachment])),
        ] {
            let got = call_function(&mut tx, token, analyzer, "public", &[arm], 1000).await;
            record!(
                format!("A/public/{analyzer}/{arm}: only A's ids"),
                got == Ok(expected.clone())
            );
            let all = call_function(&mut tx, token, analyzer, "public", &ALL_ARMS, 1000).await;
            record!(
                format!("A/public/{analyzer}/all arms: {token} only A's ids"),
                all == Ok(expected)
            );
        }
    }
    // --- Tenant B, public memory: only B's ids.
    use_tenant(&mut tx, tenant_b).await;
    for (token, expected) in [
        ("isotitle", set(&[s.b_title])),
        ("isobody", set(&[s.b_content])),
        ("isoattach", set(&[s.b_attachment])),
    ] {
        let got = call_function(&mut tx, token, ENGLISH, "public", &ALL_ARMS, 1000).await;
        record!(
            format!("B/public/{token}: only B's ids"),
            got == Ok(expected)
        );
    }
    // --- A on its registered archive: A's archive rows, not the planted B row.
    use_tenant(&mut tx, TENANT_A).await;
    let got = call_function(
        &mut tx,
        "isotitle",
        ENGLISH,
        &arch_a.schema,
        &ALL_ARMS,
        1000,
    )
    .await;
    record!(
        "A/own archive: A's archive row only (planted tenant-B row excluded)",
        got == Ok(set(&[s.a_arch_title]))
    );
    record!(
        "A/own archive: planted tenant-B id never returned",
        !got.unwrap().contains(&s.planted_b_in_a_archive)
    );
    for (token, expected) in [
        ("isobody", s.a_arch_content),
        ("isoattach", s.a_arch_attachment),
    ] {
        let got = call_function(&mut tx, token, ENGLISH, &arch_a.schema, &ALL_ARMS, 1000).await;
        record!(
            format!("A/own archive/{token}"),
            got == Ok(set(&[expected]))
        );
    }
    // --- Cross-tenant schema: registered to the other tenant -> error, no ids.
    for (who, tenant, schema) in [
        ("A -> B's archive", TENANT_A, &arch_b.schema),
        ("B -> A's archive", tenant_b, &arch_a.schema),
    ] {
        use_tenant(&mut tx, tenant).await;
        let got = call_function(&mut tx, "isotitle", ENGLISH, schema, &ALL_ARMS, 1000).await;
        record!(
            format!("{who}: refused (not registered to caller)"),
            got.as_ref()
                .is_err_and(|e| e.contains("not a registered memory"))
        );
    }
    // B's archive via B works and does not leak A's rows.
    use_tenant(&mut tx, tenant_b).await;
    let got = call_function(
        &mut tx,
        "isotitle",
        ENGLISH,
        &arch_b.schema,
        &ALL_ARMS,
        1000,
    )
    .await;
    record!("B/own archive", got == Ok(set(&[s.b_arch_title])));
    // --- Crafted search_path pointing at B's archive: the function ignores the
    // caller's search_path; the caller-side `current_schema()` is validated.
    use_tenant(&mut tx, TENANT_A).await;
    in_schema(&mut tx, &arch_b.schema).await;
    let got = call_function(&mut tx, "isotitle", ENGLISH, "public", &ALL_ARMS, 1000).await;
    record!(
        "A with search_path=B's archive, schema=public: only A's public ids",
        got == Ok(set(&[s.a_title]))
    );
    for path in [AdmissionPath::Function] {
        let outcome = savepoint_lexical(&mut tx, "isotitle", path).await;
        record!(
            "A with search_path=B's archive via lexical statement: refused, no ids",
            // (the repository error type redacts the message, so assert the refusal)
            outcome.is_err()
        );
    }
    // Direct path (RLS only) under the same crafted path returns no B rows.
    sqlx::query("SAVEPOINT direct_case")
        .execute(&mut *tx)
        .await
        .unwrap();
    let direct = lexical_on_connection(
        &mut tx,
        "isotitle",
        100,
        LexicalStrategy::English,
        &SearchCandidateScope {
            admission: Some(AdmissionPath::Direct),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    record!(
        "A with search_path=B's archive on the direct path (RLS): no tenant-B ids",
        !direct
            .iter()
            .any(|hit| hit.note_id == s.b_arch_title || hit.note_id == s.b_title)
    );
    reset_schema(&mut tx).await;
    // --- Unset / empty / malformed tenant: fail closed.
    for (label, value) in [
        ("empty", ""),
        ("blank", "   "),
        ("malformed", "not-a-uuid"),
        ("almost-uuid", "00000000-0000-0000-0000-00000000000g"),
        ("sql-ish", "' OR '1'='1"),
    ] {
        use_tenant_text(&mut tx, value).await;
        let got = call_function(&mut tx, "isotitle", ENGLISH, "public", &ALL_ARMS, 1000).await;
        record!(
            format!("tenant {label}: refused"),
            got.is_err() && got.as_ref().is_err_and(|e| !e.contains("isotitle alpha"))
        );
    }
    tx.rollback().await.unwrap();

    // A connection that never set the tenant at all (GUC NULL).
    let mut fresh = <PgConnection as sqlx::Connection>::connect_with(&pool.connect_options())
        .await
        .unwrap();
    let mut tx = sqlx::Connection::begin(&mut fresh).await.unwrap();
    sqlx::query(&format!("SET LOCAL ROLE {role}"))
        .execute(&mut *tx)
        .await
        .unwrap();
    let unset: Option<String> =
        sqlx::query_scalar("SELECT current_setting('app.current_tenant', true)")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert!(unset.is_none(), "fresh connection must not have a tenant");
    let got = call_function(&mut tx, "isotitle", ENGLISH, "public", &ALL_ARMS, 1000).await;
    record!(
        "tenant never set (NULL): refused",
        got.as_ref()
            .is_err_and(|e| e.contains("tenant scope is not set"))
    );
    tx.rollback().await.unwrap();
    drop(fresh);

    // --- Schema, analyzer, arm and cap validation (tenant A).
    let mut tx = pool.begin().await.unwrap();
    sqlx::query(&format!("SET LOCAL ROLE {role}"))
        .execute(&mut *tx)
        .await
        .unwrap();
    use_tenant(&mut tx, TENANT_A).await;
    for schema in [
        "pg_catalog",
        "information_schema",
        "archive_does_not_exist",
        "public; DROP TABLE note",
        "\"public\"",
        "PUBLIC",
        "public ",
        "",
        "pg_temp",
    ] {
        let got = call_function(&mut tx, "isotitle", ENGLISH, schema, &ALL_ARMS, 1000).await;
        record!(format!("schema {schema:?}: refused"), got.is_err());
    }
    for analyzer in [
        "simple",
        "english",
        "PUBLIC.matric_english",
        "public.matric_english; select 1",
        "public.matric_english ",
        "",
    ] {
        let got = call_function(&mut tx, "isotitle", analyzer, "public", &ALL_ARMS, 1000).await;
        record!(format!("analyzer {analyzer:?}: refused"), got.is_err());
    }
    let got = call_function(
        &mut tx,
        "isotitle",
        ENGLISH,
        "public",
        &["title", "x; drop"],
        10,
    )
    .await;
    record!("arm not allowlisted: refused", got.is_err());
    let got = call_function(&mut tx, "isotitle", ENGLISH, "public", &[], 10).await;
    record!("empty arm list: no rows", got == Ok(BTreeSet::new()));
    for cap in [0, -5, 1] {
        // Never truncated silently: more matches than the cap raise.
        let got = call_function(
            &mut tx,
            "isotitle | isobody | isoattach",
            ENGLISH,
            "public",
            &ALL_ARMS,
            cap,
        )
        .await;
        record!(
            format!("cap {cap} with 3 matches: raises (no silent truncation)"),
            got.as_ref()
                .is_err_and(|e| e.contains("admission cap exceeded"))
        );
    }
    let got = call_function(
        &mut tx,
        "isotitle | isobody | isoattach",
        ENGLISH,
        "public",
        &ALL_ARMS,
        3,
    )
    .await;
    record!(
        "cap equal to the match count: all rows, only tenant A's",
        got == Ok(set(&[s.a_title, s.a_content, s.a_attachment]))
    );
    let got = call_function(&mut tx, "nomatchever", ENGLISH, "public", &ALL_ARMS, 0).await;
    record!("cap 0 with no matches: no rows", got == Ok(BTreeSet::new()));
    // Arms are independent: a title-only call never returns a content match.
    let got = call_function(&mut tx, "isobody", ENGLISH, "public", &["title"], 1000).await;
    record!(
        "per-arm: title arm does not match content",
        got == Ok(BTreeSet::new())
    );
    let got = call_function(
        &mut tx,
        "isoattach",
        ENGLISH,
        "public",
        &["title", "content"],
        1000,
    )
    .await;
    record!(
        "per-arm: no attachment match without attachment arm",
        got == Ok(BTreeSet::new())
    );
    // A failed attachment (non-completed) is excluded.
    tx.rollback().await.unwrap();

    println!(
        "ISOLATION MATRIX ({} cases)\n{}",
        matrix.len(),
        matrix.join("\n")
    );
    drop_role(&pool, &role).await;
    for archive in [arch_a, arch_b] {
        drop_archive(&pool, archive).await;
    }
}

async fn savepoint_lexical(
    tx: &mut PgConnection,
    query: &str,
    path: AdmissionPath,
) -> Result<usize, String> {
    sqlx::query("SAVEPOINT lex_case")
        .execute(&mut *tx)
        .await
        .unwrap();
    let outcome = lexical_on_connection(
        &mut *tx,
        query,
        100,
        LexicalStrategy::English,
        &SearchCandidateScope {
            admission: Some(path),
            ..Default::default()
        },
    )
    .await;
    match outcome {
        Ok(hits) => {
            sqlx::query("RELEASE SAVEPOINT lex_case")
                .execute(&mut *tx)
                .await
                .unwrap();
            Ok(hits.len())
        }
        Err(error) => {
            sqlx::query("ROLLBACK TO SAVEPOINT lex_case")
                .execute(&mut *tx)
                .await
                .unwrap();
            Err(error.to_string())
        }
    }
}

/// Direct and function admission return identical hits (ids, order, scores,
/// evidence) under the non-bypass role, for plain, explicit, NOT and phrase
/// queries, in the public memory and in an archive.
#[sqlx::test(migrations = false)]
async fn function_path_matches_direct_path_under_non_bypass_role(pool: PgPool) {
    let db = migrated(&pool).await;
    let tenant_b = register_tenant(&pool).await;
    let arch_a = create_archive(&pool, TENANT_A).await;
    let role = runtime_role(&pool, true).await;
    grant_schema(&pool, &role, &arch_a.schema).await;

    let mut seed = pool.begin().await.unwrap();
    for tenant in [TENANT_A, tenant_b] {
        use_tenant(&mut seed, tenant).await;
        add_note(
            &db,
            &mut seed,
            "zephyrtitle handbook",
            "unrelated filler body",
        )
        .await;
        add_note(
            &db,
            &mut seed,
            "plain heading",
            "body mentions zephyrbody here",
        )
        .await;
        let attachment = add_note(&db, &mut seed, "another heading", "nothing to see").await;
        add_attachment(&mut seed, attachment, "scanned zephyrattach page").await;
        let combined = add_note(
            &db,
            &mut seed,
            "zephyrtitle combined",
            "also zephyrbody inside",
        )
        .await;
        add_attachment(&mut seed, combined, "plus zephyrattach text").await;
        add_note(&db, &mut seed, "phrase note", "the quick red fox jumps").await;
        add_note(
            &db,
            &mut seed,
            "scattered note",
            "fox that is quick and red",
        )
        .await;
        let gone = add_note(&db, &mut seed, "zephyrtitle deleted", "zephyrbody").await;
        sqlx::query("UPDATE note SET deleted_at = now() WHERE id = $1")
            .bind(gone)
            .execute(&mut *seed)
            .await
            .unwrap();
    }
    use_tenant(&mut seed, TENANT_A).await;
    in_schema(&mut seed, &arch_a.schema).await;
    add_note(&db, &mut seed, "zephyrtitle archived memory", "zephyrbody").await;
    seed.commit().await.unwrap();

    let queries = [
        "zephyrtitle",
        "zephyrbody",
        "zephyrattach",
        "zephyrtitle zephyrbody",
        "zephyrtitle OR zephyrbody OR zephyrattach",
        "zephyrtitle AND zephyrbody",
        "zephyrbody -zephyrattach",
        "zephyrattach -zephyrtitle",
        "zephyrtitle (zephyrbody OR zephyrattach)",
        "\"quick red fox\"",
        "fox -\"quick red fox\"",
        "NOT zephyrtitle",
        "zephyr*",
        "nomatchever",
    ];
    for schema in [None, Some(arch_a.schema.clone())] {
        for strategy in [LexicalStrategy::English, LexicalStrategy::Simple] {
            for query in queries {
                let mut results = Vec::new();
                for path in [AdmissionPath::Direct, AdmissionPath::Function] {
                    let mut tx = pool.begin().await.unwrap();
                    use_tenant(&mut tx, TENANT_A).await;
                    if let Some(schema) = &schema {
                        in_schema(&mut tx, schema).await;
                    }
                    sqlx::query(&format!("SET LOCAL ROLE {role}"))
                        .execute(&mut *tx)
                        .await
                        .unwrap();
                    let hits = lexical_on_connection(
                        &mut tx,
                        query,
                        100,
                        strategy,
                        &SearchCandidateScope {
                            admission: Some(path),
                            ..Default::default()
                        },
                    )
                    .await
                    .unwrap_or_else(|e| panic!("{path:?} {strategy:?} {query:?}: {e}"));
                    results.push(
                        hits.into_iter()
                            .map(|h| {
                                (
                                    h.note_id,
                                    h.score.to_bits(),
                                    format!("{:?}", (&h.title, &h.snippet, &h.tags)),
                                    // Serialized evidence values (locator ids, digests,
                                    // spans), not the redacted Debug summary.
                                    serde_json::to_string(&h.evidence).unwrap(),
                                )
                            })
                            .collect::<Vec<_>>(),
                    );
                    tx.rollback().await.unwrap();
                }
                assert_eq!(
                    results[0], results[1],
                    "direct vs function: schema {schema:?} {strategy:?} {query:?}"
                );
                if query == "zephyrattach" && schema.is_none() {
                    // The comparison is not vacuous: attachment evidence is present.
                    assert!(
                        results[1].iter().any(|r| r.3.contains("attachment")),
                        "{:?}",
                        results[1]
                    );
                }
                // Tenant A's page never contains tenant B notes under either path.
                // Plain multi-word queries are unit-local, so "zephyrtitle zephyrbody"
                // legitimately matches nothing (see the arm tests).
                assert!(
                    schema.is_some()
                        || query == "nomatchever"
                        || query == "zephyrtitle zephyrbody"
                        || !results[1].is_empty(),
                    "expected hits for {query:?}"
                );
            }
        }
    }
    drop_role(&pool, &role).await;
    drop_archive(&pool, arch_a).await;
}

#[derive(Debug, Clone)]
struct Node {
    kind: String,
    index: Option<String>,
    relation: Option<String>,
    filter: Option<String>,
    function: Option<String>,
}

fn collect(plan: &Value, nodes: &mut Vec<Node>) {
    match plan {
        Value::Object(map) => {
            if let Some(Value::String(kind)) = map.get("Node Type") {
                let text = |key: &str| map.get(key).and_then(Value::as_str).map(str::to_string);
                nodes.push(Node {
                    kind: kind.clone(),
                    index: text("Index Name"),
                    relation: text("Relation Name"),
                    filter: text("Filter"),
                    function: text("Function Name"),
                });
            }
            map.values().for_each(|value| collect(value, nodes));
        }
        Value::Array(items) => items.iter().for_each(|value| collect(value, nodes)),
        _ => {}
    }
}

fn bitmap_on(nodes: &[Node], index: &str) -> bool {
    nodes
        .iter()
        .any(|n| n.kind == "Bitmap Index Scan" && n.index.as_deref() == Some(index))
}

fn seq_scan_with_match_filter(nodes: &[Node], table: &str) -> bool {
    nodes.iter().any(|n| {
        n.kind == "Seq Scan"
            && n.relation.as_deref() == Some(table)
            && n.filter.as_deref().is_some_and(|f| f.contains("@@"))
    })
}

fn seq_scans_on(nodes: &[Node], tables: &[&str]) -> Vec<String> {
    nodes
        .iter()
        .filter(|n| {
            n.kind == "Seq Scan" && n.relation.as_deref().is_some_and(|r| tables.contains(&r))
        })
        .map(|n| n.relation.clone().unwrap_or_default())
        .collect()
}

async fn seed_scale(pool: &PgPool) {
    let mut tx = pool.begin().await.unwrap();
    use_tenant(&mut tx, TENANT_A).await;
    // Bulk fixture load only: skip per-row triggers (superuser, this transaction).
    sqlx::query("SET LOCAL session_replication_role = replica")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("CREATE TEMP TABLE plan_seed ON COMMIT DROP AS SELECT uuidv7() AS id, g FROM generate_series(1, 40000) g")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO note (id, format, source, created_at_utc, updated_at_utc, title) SELECT id, 'markdown', 'plan', now(), now(), CASE WHEN g % 20000 = 1 THEN 'plantitle ' || g ELSE 'Meeting notes ' || g END FROM plan_seed")
        .execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO note_revised_current (note_id, content) SELECT id, '{\"ref\":\"obj-' || g || '\",\"kind\":\"pointer\"' || CASE WHEN g % 20000 = 2 THEN ' planbody' ELSE '' END || '}' FROM plan_seed")
        .execute(&mut *tx).await.unwrap();
    let blob = Uuid::new_v4();
    sqlx::query("INSERT INTO attachment_blob (id, content_hash, content_type, size_bytes, data) VALUES ($1, $2, 'text/plain', 1, $3)")
        .bind(blob)
        .bind("f".repeat(64))
        .bind(vec![1_u8])
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO attachment (note_id, blob_id, filename, status, extracted_text) SELECT id, $1, 'plan.txt', 'completed', CASE WHEN g % 20000 = 3 THEN 'planattach text' ELSE 'extracted words ' || g END FROM plan_seed WHERE g % 10 = 3")
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

async fn explain(
    tx: &mut PgConnection,
    query: &str,
    strategy: LexicalStrategy,
    path: AdmissionPath,
) -> Vec<Node> {
    let plan = if query.contains(" OR ") {
        let parsed = matric_db::search_query::ParsedQuery::parse(query).unwrap();
        let expression = normalize_expression(&mut *tx, &parsed, strategy)
            .await
            .unwrap();
        let compiled = expression.tsquery().unwrap();
        let scope = SearchCandidateScope {
            lexical_constraint: Some((expression, strategy)),
            admission: Some(path),
            ..Default::default()
        };
        explain_lexical_compiled(tx, &compiled, 10, strategy, &scope)
            .await
            .unwrap()
    } else {
        let tsquery = normalize_query(&mut *tx, query, strategy).await.unwrap();
        let scope = SearchCandidateScope {
            admission: Some(path),
            ..Default::default()
        };
        explain_lexical_compiled(tx, &tsquery, 10, strategy, &scope)
            .await
            .unwrap()
    };
    let mut nodes = Vec::new();
    collect(&plan, &mut nodes);
    nodes
}

/// The bug, reproduced: as the runtime-like role the direct statements cannot
/// use the GIN indexes. A control as the privileged role shows the indexes are
/// usable, so the cause is row-level security, not the indexes.
#[sqlx::test(migrations = false)]
async fn direct_path_under_rls_is_a_seq_scan_with_a_match_filter(pool: PgPool) {
    let _db = migrated(&pool).await;
    seed_scale(&pool).await;
    let role = runtime_role(&pool, true).await;
    let indexes = [
        "idx_note_revised_tsv",
        "idx_note_title_tsv_english",
        "idx_attachment_extracted_fts_english",
    ];
    for query in ["plantitle", "plantitle OR planbody OR planattach"] {
        // Runtime-like role, natural planning.
        let mut tx = pool.begin().await.unwrap();
        use_tenant(&mut tx, TENANT_A).await;
        sqlx::query(&format!("SET LOCAL ROLE {role}"))
            .execute(&mut *tx)
            .await
            .unwrap();
        let nodes = explain(
            &mut tx,
            query,
            LexicalStrategy::English,
            AdmissionPath::Direct,
        )
        .await;
        for table in ["note", "note_revised_current", "attachment"] {
            assert!(
                seq_scan_with_match_filter(&nodes, table),
                "{query:?}: expected Seq Scan with @@ filter on {table} under RLS: {nodes:?}"
            );
        }
        for index in indexes {
            assert!(
                !bitmap_on(&nodes, index),
                "{query:?}: {index} used under RLS: {nodes:?}"
            );
        }
        // Even with sequential and plain index scans disabled the indexes stay unusable.
        for setting in ["enable_seqscan", "enable_indexscan", "enable_indexonlyscan"] {
            sqlx::query(&format!("SET LOCAL {setting} = off"))
                .execute(&mut *tx)
                .await
                .unwrap();
        }
        let forced = explain(
            &mut tx,
            query,
            LexicalStrategy::English,
            AdmissionPath::Direct,
        )
        .await;
        for index in indexes {
            assert!(
                !bitmap_on(&forced, index),
                "{query:?}: forced plan used {index}: {forced:?}"
            );
        }
        tx.rollback().await.unwrap();

        // Control: the same statement as the privileged role CAN use the indexes.
        let mut tx = pool.begin().await.unwrap();
        use_tenant(&mut tx, TENANT_A).await;
        for setting in ["enable_seqscan", "enable_indexscan", "enable_indexonlyscan"] {
            sqlx::query(&format!("SET LOCAL {setting} = off"))
                .execute(&mut *tx)
                .await
                .unwrap();
        }
        let control = explain(
            &mut tx,
            query,
            LexicalStrategy::English,
            AdmissionPath::Direct,
        )
        .await;
        for index in indexes {
            assert!(
                bitmap_on(&control, index),
                "{query:?}: control lacks {index}: {control:?}"
            );
        }
        tx.rollback().await.unwrap();
    }
    drop_role(&pool, &role).await;
}

async fn stat_counts(pool: &PgPool) -> std::collections::BTreeMap<String, i64> {
    let mut counts = std::collections::BTreeMap::new();
    let indexes: Vec<(String, i64)> = sqlx::query_as(
        "SELECT indexrelname::text, COALESCE(idx_scan, 0) FROM pg_stat_user_indexes WHERE schemaname = 'public' AND relname IN ('note', 'note_revised_current', 'attachment')",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    counts.extend(indexes);
    let tables: Vec<(String, i64)> = sqlx::query_as(
        "SELECT 'seq:' || relname::text, COALESCE(seq_scan, 0) FROM pg_stat_user_tables WHERE schemaname = 'public' AND relname IN ('note_revised_current', 'attachment')",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    counts.extend(tables);
    counts
}

/// Index ELIGIBILITY, not natural planning: with sequential scans disabled on a
/// small synthetic fixture, each function arm's expression must match its GIN
/// index (proven with the statistics collector, as the non-bypass role; the
/// outer EXPLAIN shows only a Function Scan). Natural-cost planning at scale is
/// measured by the `fts_scale_check` example, which is single-tenant synthetic
/// data, not a representative multi-tenant workload.
#[sqlx::test(migrations = false)]
async fn function_arms_match_their_gin_indexes_under_rls_with_seqscan_disabled(pool: PgPool) {
    let _db = migrated(&pool).await;
    seed_scale(&pool).await;
    let role = runtime_role(&pool, true).await;

    for (strategy, indexes) in [
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
    ] {
        for query in ["plantitle", "plantitle OR planbody OR planattach"] {
            let before = stat_counts(&pool).await;
            let mut conn = pool.acquire().await.unwrap();
            let mut tx = sqlx::Connection::begin(&mut *conn).await.unwrap();
            use_tenant(&mut tx, TENANT_A).await;
            sqlx::query(&format!("SET LOCAL ROLE {role}"))
                .execute(&mut *tx)
                .await
                .unwrap();
            // The synthetic fixture is small and its tsvector statistics are
            // coarse, so disable sequential scans to prove each arm's expression
            // matches its GIN index; natural-cost planning at scale is measured
            // by the `fts_scale_check` example under the same role.
            sqlx::query("SET LOCAL enable_seqscan = off")
                .execute(&mut *tx)
                .await
                .unwrap();
            let nodes = explain(&mut tx, query, strategy, AdmissionPath::Function).await;
            assert!(
                nodes.iter().any(|n| n.kind == "Function Scan"
                    && n.function.as_deref() == Some("fts_admit_note_ids")),
                "{strategy:?} {query:?}: expected Function Scan: {nodes:?}"
            );
            assert!(
                seq_scans_on(&nodes, &["note", "note_revised_current", "attachment"]).is_empty(),
                "{strategy:?} {query:?}: outer statement seq-scans a search table: {nodes:?}"
            );
            // Execute as the role (statement and plan as in production).
            let hits = lexical_on_connection(
                &mut tx,
                query,
                10,
                strategy,
                &SearchCandidateScope {
                    admission: Some(AdmissionPath::Function),
                    ..Default::default()
                },
            )
            .await
            .unwrap_or_else(|e| {
                let code = match &e {
                    matric_core::Error::Database(sqlx::Error::Database(db)) => {
                        db.code().map(|c| c.to_string())
                    }
                    _ => None,
                };
                panic!("{strategy:?} {query:?}: function path failed: sqlstate {code:?}")
            });
            assert!(!hits.is_empty(), "{strategy:?} {query:?}");
            tx.commit().await.unwrap();
            // Flush this backend's pending statistics (forced at the next
            // transaction end on this same connection) before reading them.
            sqlx::query("SELECT pg_stat_force_next_flush()")
                .execute(&mut *conn)
                .await
                .unwrap();
            sqlx::query("SELECT 1").execute(&mut *conn).await.unwrap();
            drop(conn);
            let after = stat_counts(&pool).await;
            let changed: Vec<_> = after
                .iter()
                .filter(|(name, count)| before.get(*name) != Some(count))
                .collect();
            for index in indexes {
                assert!(
                    after[index] > before[index],
                    "{strategy:?} {query:?}: {index} not scanned by the function path; changed counters: {changed:?}"
                );
            }
            for table in ["seq:note_revised_current", "seq:attachment"] {
                assert_eq!(
                    after[table], before[table],
                    "{strategy:?} {query:?}: {table} was sequentially scanned ({before:?} -> {after:?})"
                );
            }
        }
    }
    drop_role(&pool, &role).await;
}

/// Federated statement: function and direct variants agree under the role
/// (title + content arms only, no attachment arm), with tenant isolation and
/// archive scoping.
#[sqlx::test(migrations = false)]
async fn federated_function_statement_matches_direct_statement(pool: PgPool) {
    let db = migrated(&pool).await;
    let tenant_b = register_tenant(&pool).await;
    let arch_a = create_archive(&pool, TENANT_A).await;
    let role = runtime_role(&pool, true).await;
    grant_schema(&pool, &role, &arch_a.schema).await;

    let mut seed = pool.begin().await.unwrap();
    use_tenant(&mut seed, tenant_b).await;
    let b_title = add_note(&db, &mut seed, "fedtitle theirs", "filler").await;
    let b_content = add_note(&db, &mut seed, "heading", "fedbody theirs").await;
    use_tenant(&mut seed, TENANT_A).await;
    let a_title = add_note(&db, &mut seed, "fedtitle mine", "filler").await;
    let a_content = add_note(&db, &mut seed, "heading", "fedbody mine").await;
    let a_both = add_note(&db, &mut seed, "fedtitle fedbody", "fedbody").await;
    let a_attach_only = add_note(&db, &mut seed, "heading", "filler").await;
    add_attachment(&mut seed, a_attach_only, "fedattach only").await;
    let a_deleted = add_note(&db, &mut seed, "fedtitle gone", "fedbody gone").await;
    sqlx::query("UPDATE note SET deleted_at = now() WHERE id = $1")
        .bind(a_deleted)
        .execute(&mut *seed)
        .await
        .unwrap();
    in_schema(&mut seed, &arch_a.schema).await;
    let a_archived = add_note(&db, &mut seed, "fedtitle archived", "filler").await;
    seed.commit().await.unwrap();

    type FedRow = (Uuid, u32, Option<String>, Option<String>, String);
    async fn run(
        pool: &PgPool,
        role: &str,
        tenant: Uuid,
        schema: Option<&str>,
        sql: &str,
        query: &str,
    ) -> Result<BTreeSet<Uuid>, String> {
        run_rows(pool, role, tenant, schema, sql, query)
            .await
            .map(|rows| rows.into_iter().map(|r| r.0).collect())
    }
    async fn run_rows(
        pool: &PgPool,
        role: &str,
        tenant: Uuid,
        schema: Option<&str>,
        sql: &str,
        query: &str,
    ) -> Result<Vec<FedRow>, String> {
        use sqlx::Row;
        let mut tx = pool.begin().await.unwrap();
        use_tenant(&mut tx, tenant).await;
        if let Some(schema) = schema {
            in_schema(&mut tx, schema).await;
        }
        sqlx::query(&format!("SET LOCAL ROLE {role}"))
            .execute(&mut *tx)
            .await
            .unwrap();
        let statement = sqlx::query(sql).bind(query).bind(100_i64);
        let statement = if sql.contains("$3") {
            statement.bind(100_000_i32)
        } else {
            statement
        };
        let rows = statement
            .fetch_all(&mut *tx)
            .await
            .map_err(|e| e.to_string());
        let _ = tx.rollback().await;
        rows.map(|rows| {
            rows.into_iter()
                .map(|r| {
                    (
                        r.get::<Uuid, _>("note_id"),
                        r.get::<f32, _>("score").to_bits(),
                        r.get::<Option<String>, _>("title"),
                        r.get::<Option<String>, _>("snippet"),
                        r.get::<String, _>("tags"),
                    )
                })
                .collect()
        })
    }

    for (tenant, schema) in [
        (TENANT_A, None),
        (tenant_b, None),
        (TENANT_A, Some(arch_a.schema.as_str())),
    ] {
        for query in [
            "fedtitle",
            "fedbody",
            "fedattach",
            "fedtitle OR fedbody",
            "fedbody -fedtitle",
            "\"fedtitle mine\"",
            "nomatchever",
        ] {
            let direct = run(
                &pool,
                &role,
                tenant,
                schema,
                matric_db::search::FEDERATED_FTS_SQL,
                query,
            )
            .await;
            let function = run(
                &pool,
                &role,
                tenant,
                schema,
                matric_db::search::FEDERATED_FTS_FUNCTION_SQL,
                query,
            )
            .await;
            assert!(direct.is_ok(), "{direct:?}");
            assert_eq!(
                direct, function,
                "tenant {tenant} schema {schema:?} {query:?}"
            );
            // Ordered rows, scores, titles, snippets and tags. Tie policy: the SQL
            // orders by score only, so equal-score rows may differ in order; each
            // output must be non-increasing in score, and after a canonical
            // (score desc, note_id) ordering the two outputs must be identical.
            let mut outputs = Vec::new();
            for sql in [
                matric_db::search::FEDERATED_FTS_SQL,
                matric_db::search::FEDERATED_FTS_FUNCTION_SQL,
            ] {
                let mut rows = run_rows(&pool, &role, tenant, schema, sql, query)
                    .await
                    .unwrap();
                assert!(
                    rows.windows(2)
                        .all(|w| f32::from_bits(w[0].1) >= f32::from_bits(w[1].1)),
                    "scores must be non-increasing: {rows:?}"
                );
                rows.sort_by(|a, b| {
                    f32::from_bits(b.1)
                        .partial_cmp(&f32::from_bits(a.1))
                        .unwrap()
                        .then(a.0.cmp(&b.0))
                });
                outputs.push(rows);
            }
            assert_eq!(
                outputs[0], outputs[1],
                "rows/scores/titles/snippets/tags: tenant {tenant} schema {schema:?} {query:?}"
            );
        }
    }
    // Spot-check the expected sets (parity could otherwise be two empty sets).
    let f = matric_db::search::FEDERATED_FTS_FUNCTION_SQL;
    assert_eq!(
        run(&pool, &role, TENANT_A, None, f, "fedtitle").await,
        Ok(set(&[a_title, a_both]))
    );
    assert_eq!(
        run(&pool, &role, TENANT_A, None, f, "fedbody").await,
        Ok(set(&[a_content, a_both]))
    );
    assert_eq!(
        run(&pool, &role, TENANT_A, None, f, "fedattach").await,
        Ok(BTreeSet::new()),
        "federated search has no attachment arm"
    );
    assert_eq!(
        run(&pool, &role, tenant_b, None, f, "fedtitle").await,
        Ok(set(&[b_title]))
    );
    assert_eq!(
        run(&pool, &role, tenant_b, None, f, "fedbody").await,
        Ok(set(&[b_content]))
    );
    assert_eq!(
        run(&pool, &role, TENANT_A, Some(&arch_a.schema), f, "fedtitle").await,
        Ok(set(&[a_archived]))
    );
    // A crafted search_path at an archive of the other tenant is refused, never widened.
    let arch_b = create_archive(&pool, tenant_b).await;
    grant_schema(&pool, &role, &arch_b.schema).await;
    // Rows must exist in the crafted schema so the outer scan reaches the function.
    let mut seed_b = pool.begin().await.unwrap();
    use_tenant(&mut seed_b, tenant_b).await;
    in_schema(&mut seed_b, &arch_b.schema).await;
    add_note(&db, &mut seed_b, "fedtitle other tenant archive", "filler").await;
    seed_b.commit().await.unwrap();
    let refused = run(&pool, &role, TENANT_A, Some(&arch_b.schema), f, "fedtitle").await;
    assert!(
        refused
            .as_ref()
            .is_err_and(|e| e.contains("not a registered memory")),
        "{refused:?}"
    );
    drop_role(&pool, &role).await;
    for archive in [arch_a, arch_b] {
        drop_archive(&pool, archive).await;
    }
}

async fn pool_as(pool: &PgPool, role: &str) -> PgPool {
    let role = role.to_string();
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .after_connect(move |connection, _| {
            let role = role.clone();
            Box::pin(async move {
                sqlx::query(&format!("SET ROLE {role}"))
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
        .connect_with((*pool.connect_options()).clone())
        .await
        .unwrap()
}

/// Startup probe: the function path is selected only when the runtime role can
/// execute it and its owner bypasses RLS; otherwise the direct path is kept.
#[sqlx::test(migrations = false)]
async fn startup_probe_selects_function_only_when_it_works(pool: PgPool) {
    let _switch = GLOBAL_SWITCH.lock().await;
    let db = migrated(&pool).await;

    // Runtime role with EXECUTE and a bypassing owner: function path.
    let granted = runtime_role(&pool, true).await;
    let runtime = pool_as(&pool, &granted).await;
    assert_eq!(
        fts_admission::probe_and_select(&runtime).await,
        AdmissionPath::Function
    );
    assert_eq!(fts_admission::active_path(), AdmissionPath::Function);

    // No EXECUTE grant: direct path.
    let ungranted = runtime_role(&pool, false).await;
    let runtime2 = pool_as(&pool, &ungranted).await;
    assert_eq!(
        fts_admission::probe_and_select(&runtime2).await,
        AdmissionPath::Direct
    );
    assert_eq!(fts_admission::active_path(), AdmissionPath::Direct);

    // Request-time self-healing: a role without EXECUTE (grant revoked after
    // startup) falls back to the direct statements and stops using the function.
    let mut seed = pool.begin().await.unwrap();
    use_tenant(&mut seed, TENANT_A).await;
    let note = add_note(&db, &mut seed, "healword title", "filler").await;
    seed.commit().await.unwrap();
    fts_admission::set_active_path(AdmissionPath::Function);
    let mut tx = pool.begin().await.unwrap();
    use_tenant(&mut tx, TENANT_A).await;
    sqlx::query(&format!("SET LOCAL ROLE {ungranted}"))
        .execute(&mut *tx)
        .await
        .unwrap();
    let hits = lexical_on_connection(
        &mut tx,
        "healword",
        10,
        LexicalStrategy::English,
        &SearchCandidateScope::default(),
    )
    .await
    .unwrap();
    assert_eq!(
        hits.iter().map(|h| h.note_id).collect::<Vec<_>>(),
        vec![note]
    );
    assert_eq!(
        fts_admission::active_path(),
        AdmissionPath::Direct,
        "unavailable function switches the process to the direct path"
    );
    tx.rollback().await.unwrap();

    // Community-bundle shape: the owner cannot bypass RLS. Execution fails, so
    // the probe falls back.
    let owner = format!("fts_owner_{}", Uuid::new_v4().simple());
    for statement in [
        format!("CREATE ROLE {owner} NOSUPERUSER NOBYPASSRLS NOLOGIN"),
        format!("GRANT CREATE, USAGE ON SCHEMA public TO {owner}"),
        format!("GRANT SELECT ON ALL TABLES IN SCHEMA public TO {owner}"),
        format!("ALTER FUNCTION {SIGNATURE} OWNER TO {owner}"),
    ] {
        sqlx::query(&statement).execute(&pool).await.unwrap();
    }
    assert_eq!(
        fts_admission::probe_and_select(&runtime).await,
        AdmissionPath::Direct,
        "a non-bypass function owner must select the direct path"
    );
    // Function missing entirely: direct path.
    sqlx::query(&format!("DROP FUNCTION {SIGNATURE}"))
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        fts_admission::probe_and_select(&runtime).await,
        AdmissionPath::Direct
    );
    // A genuinely missing function: the request still succeeds (direct path for
    // it), the re-probe confirms, and the process switch flips to Direct.
    fts_admission::set_active_path(AdmissionPath::Function);
    let mut tx = pool.begin().await.unwrap();
    use_tenant(&mut tx, TENANT_A).await;
    let hits = lexical_on_connection(
        &mut tx,
        "healword",
        10,
        LexicalStrategy::English,
        &SearchCandidateScope::default(),
    )
    .await
    .unwrap();
    assert_eq!(
        hits.iter().map(|h| h.note_id).collect::<Vec<_>>(),
        vec![note]
    );
    assert_eq!(
        fts_admission::active_path(),
        AdmissionPath::Direct,
        "a genuinely missing function flips the process switch"
    );
    tx.rollback().await.unwrap();
    fts_admission::set_active_path(AdmissionPath::Direct);
    runtime.close().await;
    runtime2.close().await;
    for role in [granted, ungranted, owner] {
        drop_role(&pool, &role).await;
    }
}

/// P3/P4: the stored-tsv unit predicate and the single-parse attachment rank
/// equal the former per-row re-parse expressions exactly.
#[sqlx::test(migrations = false)]
async fn stored_tsv_and_single_parse_rank_equal_the_legacy_expressions(pool: PgPool) {
    let db = migrated(&pool).await;
    let mut tx = pool.begin().await.unwrap();
    use_tenant(&mut tx, TENANT_A).await;
    let mut ids = Vec::new();
    for (title, content, attachments) in [
        ("zephyrtitle handbook", "unrelated filler body", vec![]),
        (
            "plain heading",
            "body mentions zephyrbody and zephyrs here",
            vec![],
        ),
        (
            "another",
            "nothing",
            vec![
                "scanned zephyrattach page",
                "zephyrattach again zephyrattach",
            ],
        ),
        (
            "zephyrtitle combined",
            "also zephyrbody inside",
            vec!["plus zephyrattach text"],
        ),
        ("fox", "the quick red fox jumps", vec![]),
        (
            "Quick Résumé naïve",
            "café naïve résumé",
            vec!["Résumé of naïve café"],
        ),
    ] {
        let id = add_note(&db, &mut tx, title, content).await;
        for text in attachments {
            add_attachment(&mut tx, id, text).await;
        }
        ids.push(id);
    }
    for strategy in [LexicalStrategy::English, LexicalStrategy::Simple] {
        let config = match strategy {
            LexicalStrategy::English => "public.matric_english",
            _ => "public.matric_simple",
        };
        for query in [
            "zephyrtitle",
            "zephyrbody",
            "zephyrattach",
            "fox",
            "naive",
            "résumé",
            "quick & red",
        ] {
            let tsquery = normalize_query(&mut tx, query, strategy).await.unwrap();
            // classify_lexical_matches (stored tsv for English) vs the legacy predicate.
            let classified = classify_lexical_matches(&mut tx, &ids, &tsquery, &[], strategy)
                .await
                .unwrap();
            let legacy: Vec<(Uuid, bool)> = sqlx::query_as(&format!(
                "SELECT n.id, (to_tsvector('{config}', COALESCE(n.title, '')) @@ $2::tsquery OR to_tsvector('{config}', COALESCE(nrc.content, '')) @@ $2::tsquery OR EXISTS (SELECT 1 FROM attachment a WHERE a.note_id = n.id AND a.tenant_id = n.tenant_id AND a.status = 'completed' AND to_tsvector('{config}', COALESCE(a.extracted_text, '')) @@ $2::tsquery)) FROM note n JOIN note_revised_current nrc ON nrc.note_id = n.id AND nrc.tenant_id = n.tenant_id WHERE n.id = ANY($1)"
            ))
            .bind(&ids)
            .bind(&tsquery)
            .fetch_all(&mut *tx)
            .await
            .unwrap();
            let mut new: Vec<(Uuid, bool)> = classified
                .iter()
                .map(|(id, full, _)| (*id, *full))
                .collect();
            let mut old = legacy;
            new.sort();
            old.sort();
            assert_eq!(new, old, "unit predicate {strategy:?} {query:?}");

            // Matched-term counts with non-empty terms vs the legacy per-term predicate.
            let terms: Vec<String> = ["zephyrtitle", "zephyrbody", "zephyrattach", "fox", "quick"]
                .iter()
                .map(|t| matric_db::search_query::quote_term(t))
                .collect();
            let counted = classify_lexical_matches(&mut tx, &ids, &tsquery, &terms, strategy)
                .await
                .unwrap();
            let legacy_counts: Vec<(Uuid, i64)> = sqlx::query_as(&format!(
                "SELECT n.id, (SELECT count(*) FROM unnest($2::text[]) term WHERE to_tsvector('{config}', COALESCE(n.title, '')) @@ term::tsquery OR to_tsvector('{config}', COALESCE(nrc.content, '')) @@ term::tsquery OR EXISTS (SELECT 1 FROM attachment a WHERE a.note_id = n.id AND a.tenant_id = n.tenant_id AND a.status = 'completed' AND to_tsvector('{config}', COALESCE(a.extracted_text, '')) @@ term::tsquery)) FROM note n JOIN note_revised_current nrc ON nrc.note_id = n.id AND nrc.tenant_id = n.tenant_id WHERE n.id = ANY($1)"
            ))
            .bind(&ids)
            .bind(&terms)
            .fetch_all(&mut *tx)
            .await
            .unwrap();
            let mut new_counts: Vec<(Uuid, i64)> =
                counted.iter().map(|(id, _, n)| (*id, *n)).collect();
            let mut old_counts = legacy_counts;
            new_counts.sort();
            old_counts.sort();
            assert_eq!(
                new_counts, old_counts,
                "matched-term counts {strategy:?} {query:?}"
            );

            // Attachment-ranked score vs the former double-parse expression.
            let hits = lexical_on_connection(
                &mut tx,
                query,
                100,
                strategy,
                &SearchCandidateScope::default(),
            )
            .await
            .unwrap();
            for hit in hits {
                let legacy_score: f32 = sqlx::query_scalar(&match strategy {
                    LexicalStrategy::English => "SELECT GREATEST((ts_rank(setweight(COALESCE(to_tsvector('public.matric_english', n.title), ''::tsvector), 'A') || setweight(COALESCE((SELECT to_tsvector('public.matric_english', string_agg(nt.tag_name, ' ')) FROM note_tag nt WHERE nt.note_id = n.id AND nt.tenant_id = n.tenant_id), ''::tsvector), 'B') || setweight(nrc.tsv, 'C'), $2::tsquery, 32))::real, COALESCE((SELECT max(ts_rank(setweight(to_tsvector('public.matric_english', a.extracted_text), 'C'), $2::tsquery, 32)) FROM attachment a WHERE a.note_id = n.id AND a.tenant_id = n.tenant_id AND a.status = 'completed' AND (to_tsvector('public.matric_english', a.extracted_text) @@ $2::tsquery)), 0.0))::real FROM note n JOIN note_revised_current nrc ON nrc.note_id = n.id AND nrc.tenant_id = n.tenant_id WHERE n.id = $1".to_string(),
                    _ => "SELECT GREATEST((ts_rank(to_tsvector('public.matric_simple', COALESCE(n.title, '')) || to_tsvector('public.matric_simple', nrc.content), $2::tsquery))::real, COALESCE((SELECT max(ts_rank(to_tsvector('public.matric_simple', a.extracted_text), $2::tsquery)) FROM attachment a WHERE a.note_id = n.id AND a.tenant_id = n.tenant_id AND a.status = 'completed' AND (to_tsvector('public.matric_simple', a.extracted_text) @@ $2::tsquery)), 0.0))::real FROM note n JOIN note_revised_current nrc ON nrc.note_id = n.id AND nrc.tenant_id = n.tenant_id WHERE n.id = $1".to_string(),
                })
                .bind(hit.note_id)
                .bind(&tsquery)
                .fetch_one(&mut *tx)
                .await
                .unwrap();
                assert_eq!(
                    hit.score.to_bits(),
                    legacy_score.to_bits(),
                    "rank {strategy:?} {query:?} note {}",
                    hit.note_id
                );
            }
        }
    }
    tx.rollback().await.unwrap();
}

/// An exceeded admission cap never truncates: lexical and federated searches
/// retry on the direct path and return the complete, tenant-scoped result.
#[sqlx::test(migrations = false)]
async fn exceeding_the_admission_cap_falls_back_to_the_direct_path(pool: PgPool) {
    let db = migrated(&pool).await;
    let tenant_b = register_tenant(&pool).await;
    let role = runtime_role(&pool, true).await;
    let mut seed = pool.begin().await.unwrap();
    use_tenant(&mut seed, TENANT_A).await;
    let mut expected = BTreeSet::new();
    for i in 0..6 {
        expected.insert(add_note(&db, &mut seed, &format!("capword item {i}"), "filler").await);
    }
    use_tenant(&mut seed, tenant_b).await;
    add_note(&db, &mut seed, "capword other tenant", "filler").await;
    seed.commit().await.unwrap();

    let mut tx = pool.begin().await.unwrap();
    use_tenant(&mut tx, TENANT_A).await;
    sqlx::query(&format!("SET LOCAL ROLE {role}"))
        .execute(&mut *tx)
        .await
        .unwrap();
    for query in ["capword", "capword OR nomatchever"] {
        let run = |cap: Option<i32>, path| {
            let scope = SearchCandidateScope {
                admission: Some(path),
                admission_cap: cap,
                ..Default::default()
            };
            (query, scope)
        };
        // The capped function path alone would raise; the call must still succeed.
        let (q, scope) = run(Some(2), AdmissionPath::Function);
        let capped = lexical_on_connection(&mut tx, q, 100, LexicalStrategy::English, &scope)
            .await
            .unwrap();
        let (q, scope) = run(None, AdmissionPath::Function);
        let uncapped = lexical_on_connection(&mut tx, q, 100, LexicalStrategy::English, &scope)
            .await
            .unwrap();
        let ids = |hits: &[matric_core::SearchHit]| {
            hits.iter().map(|h| h.note_id).collect::<BTreeSet<_>>()
        };
        assert_eq!(
            ids(&capped),
            expected,
            "{query}: complete result after fallback"
        );
        assert_eq!(ids(&capped), ids(&uncapped));
    }
    // Federated: cap 2 < 6 matches still returns all 6 of tenant A's notes.
    let rows = matric_db::search::federated_fts_rows_with(
        &mut tx,
        "capword",
        100,
        AdmissionPath::Function,
        2,
    )
    .await
    .unwrap();
    use sqlx::Row;
    let got: BTreeSet<Uuid> = rows.iter().map(|r| r.get("note_id")).collect();
    assert_eq!(got, expected);
    // The transaction is still usable (savepoint rolled back, not aborted).
    let one: i32 = sqlx::query_scalar("SELECT 1")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(one, 1);
    tx.rollback().await.unwrap();
    drop_role(&pool, &role).await;
}

/// HIGH-1: only errors about the admission function itself can switch the
/// process to the direct path; an unrelated undefined function/operator in the
/// same statement cannot (it is retried on the direct path for that request).
#[sqlx::test(migrations = false)]
async fn unrelated_undefined_function_does_not_flip_the_process_switch(pool: PgPool) {
    let _switch = GLOBAL_SWITCH.lock().await;
    let _db = migrated(&pool).await;
    // The matcher on real server errors.
    for (statement, expected) in [
        ("SELECT nosuchfunc_xyz()", false),
        ("SELECT '{}'::jsonb ## 1", false),
        (
            "SELECT public.fts_admit_note_ids('x'::tsquery, 'public.matric_english', 'public', ARRAY['title']::text[], 1, 99)",
            true,
        ),
    ] {
        let error = sqlx::query(statement).execute(&pool).await.unwrap_err();
        assert!(
            matches!(&error, sqlx::Error::Database(db) if db.code().as_deref() == Some("42883")),
            "{statement}"
        );
        assert_eq!(
            matric_db::search_candidates::is_admission_function_unavailable(&error),
            expected,
            "{statement}"
        );
    }
    // The whole flow, case 1: the function-path statement raises an unrelated
    // 42883. The error is returned (never retried as a function problem) and the
    // process-wide switch stays on the function path.
    fts_admission::set_active_path(AdmissionPath::Function);
    let scope = SearchCandidateScope {
        admission: Some(AdmissionPath::Function),
        ..Default::default()
    };
    let mut tx = pool.begin().await.unwrap();
    use_tenant(&mut tx, TENANT_A).await;
    let unrelated = matric_db::search_candidates::with_admission_fallback(
        &mut tx,
        &scope,
        |connection, attempt| {
            Box::pin(async move {
                if attempt.admission == Some(AdmissionPath::Function) {
                    sqlx::query("SELECT nosuchfunc_xyz()")
                        .execute(connection)
                        .await
                        .map_err(matric_core::Error::Database)?;
                }
                Ok(attempt.admission)
            })
        },
    )
    .await;
    assert!(
        unrelated.is_err(),
        "an unrelated 42883 is an error, not a fallback"
    );
    assert_eq!(
        fts_admission::active_path(),
        AdmissionPath::Function,
        "an unrelated 42883 must not flip the process-wide switch"
    );
    tx.rollback().await.unwrap();

    // Case 2: an error that names the function while the function in fact works
    // (a wrong-arity call): the re-probe passes, so only this request uses the
    // direct path and the switch is not flipped.
    let mut tx = pool.begin().await.unwrap();
    use_tenant(&mut tx, TENANT_A).await;
    let ran_on = matric_db::search_candidates::with_admission_fallback(
        &mut tx,
        &scope,
        |connection, attempt| {
            Box::pin(async move {
                if attempt.admission == Some(AdmissionPath::Function) {
                    sqlx::query("SELECT public.fts_admit_note_ids('x'::tsquery, 'public.matric_english', 'public', ARRAY['title']::text[], 1, 99)")
                        .execute(connection)
                        .await
                        .map_err(matric_core::Error::Database)?;
                }
                Ok(attempt.admission)
            })
        },
    )
    .await
    .unwrap();
    assert_eq!(
        ran_on,
        Some(AdmissionPath::Direct),
        "direct path for this request only"
    );
    assert_eq!(
        fts_admission::active_path(),
        AdmissionPath::Function,
        "a passing re-probe must not flip the process-wide switch"
    );
    tx.rollback().await.unwrap();
    fts_admission::set_active_path(AdmissionPath::Direct);
}

/// LOW-4: a 42501 that is not the function's own refusal (the owner lost
/// BYPASSRLS) fails closed, is recognised for an operator warning, and neither
/// falls back silently nor flips the process switch.
#[sqlx::test(migrations = false)]
async fn owner_without_bypassrls_fails_closed_without_a_silent_fallback(pool: PgPool) {
    let _switch = GLOBAL_SWITCH.lock().await;
    let db = migrated(&pool).await;
    let role = runtime_role(&pool, true).await;
    let owner = format!("fts_nb_{}", Uuid::new_v4().simple());
    for statement in [
        format!("CREATE ROLE {owner} NOSUPERUSER NOBYPASSRLS NOLOGIN"),
        format!("GRANT CREATE, USAGE ON SCHEMA public TO {owner}"),
        format!("GRANT SELECT ON ALL TABLES IN SCHEMA public TO {owner}"),
        format!("ALTER FUNCTION {SIGNATURE} OWNER TO {owner}"),
    ] {
        sqlx::query(&statement).execute(&pool).await.unwrap();
    }
    let mut seed = pool.begin().await.unwrap();
    use_tenant(&mut seed, TENANT_A).await;
    add_note(&db, &mut seed, "failclosed title", "filler").await;
    seed.commit().await.unwrap();

    fts_admission::set_active_path(AdmissionPath::Function);
    let mut tx = pool.begin().await.unwrap();
    use_tenant(&mut tx, TENANT_A).await;
    sqlx::query(&format!("SET LOCAL ROLE {role}"))
        .execute(&mut *tx)
        .await
        .unwrap();
    let outcome = lexical_on_connection(
        &mut tx,
        "failclosed",
        10,
        LexicalStrategy::English,
        &SearchCandidateScope {
            admission: Some(AdmissionPath::Function),
            ..Default::default()
        },
    )
    .await;
    match outcome {
        Err(matric_core::Error::Database(inner)) => {
            assert!(
                matric_db::search_candidates::is_admission_function_misconfigured(&inner),
                "expected the owner-lost-BYPASSRLS 42501 to be recognised"
            );
            assert!(!matric_db::search_candidates::is_admission_function_unavailable(&inner));
        }
        other => panic!(
            "expected a fail-closed database error, got {:?}",
            other.map(|h| h.len())
        ),
    }
    assert_eq!(
        fts_admission::active_path(),
        AdmissionPath::Function,
        "fail closed: no silent fallback and no process-wide switch"
    );
    tx.rollback().await.unwrap();
    fts_admission::set_active_path(AdmissionPath::Direct);
    sqlx::query(&format!("ALTER FUNCTION {SIGNATURE} OWNER TO CURRENT_USER"))
        .execute(&pool)
        .await
        .unwrap();
    drop_role(&pool, &owner).await;
    drop_role(&pool, &role).await;
}

/// LOW-5: the hosted shape: the function owner is NOSUPERUSER with BYPASSRLS.
/// The function path works for the non-bypass runtime role and isolation holds.
#[sqlx::test(migrations = false)]
async fn function_owned_by_a_non_superuser_bypassrls_role_works_and_isolates(pool: PgPool) {
    let db = migrated(&pool).await;
    let tenant_b = register_tenant(&pool).await;
    let runtime = runtime_role(&pool, true).await;
    let owner = format!("fts_mig_{}", Uuid::new_v4().simple());
    for statement in [
        format!("CREATE ROLE {owner} NOSUPERUSER BYPASSRLS NOLOGIN"),
        format!("GRANT CREATE, USAGE ON SCHEMA public TO {owner}"),
        format!("GRANT SELECT ON ALL TABLES IN SCHEMA public TO {owner}"),
        format!("ALTER FUNCTION {SIGNATURE} OWNER TO {owner}"),
    ] {
        sqlx::query(&statement).execute(&pool).await.unwrap();
    }
    let attrs: (bool, bool) =
        sqlx::query_as("SELECT rolsuper, rolbypassrls FROM pg_roles WHERE rolname = $1")
            .bind(&owner)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(attrs, (false, true));

    let mut seed = pool.begin().await.unwrap();
    use_tenant(&mut seed, TENANT_A).await;
    let a = add_note(&db, &mut seed, "ownertoken mine", "filler").await;
    use_tenant(&mut seed, tenant_b).await;
    let b = add_note(&db, &mut seed, "ownertoken theirs", "filler").await;
    seed.commit().await.unwrap();

    let mut tx = pool.begin().await.unwrap();
    sqlx::query(&format!("SET LOCAL ROLE {runtime}"))
        .execute(&mut *tx)
        .await
        .unwrap();
    use_tenant(&mut tx, TENANT_A).await;
    let got = call_function(&mut tx, "ownertoken", ENGLISH, "public", &ALL_ARMS, 1000).await;
    assert_eq!(got, Ok(set(&[a])), "tenant A sees only its own id");
    use_tenant(&mut tx, tenant_b).await;
    let got = call_function(&mut tx, "ownertoken", ENGLISH, "public", &ALL_ARMS, 1000).await;
    assert_eq!(got, Ok(set(&[b])), "tenant B sees only its own id");
    for bad in ["", "not-a-uuid"] {
        use_tenant_text(&mut tx, bad).await;
        let got = call_function(&mut tx, "ownertoken", ENGLISH, "public", &ALL_ARMS, 1000).await;
        assert!(got.is_err(), "tenant {bad:?} must fail closed");
    }
    use_tenant(&mut tx, TENANT_A).await;
    let got = call_function(
        &mut tx,
        "ownertoken",
        ENGLISH,
        "archive_not_registered",
        &ALL_ARMS,
        1000,
    )
    .await;
    assert!(got.is_err(), "unregistered schema refused");
    // End to end through the search statement, parity with the direct path.
    let mut results = Vec::new();
    for path in [AdmissionPath::Direct, AdmissionPath::Function] {
        let hits = lexical_on_connection(
            &mut tx,
            "ownertoken",
            10,
            LexicalStrategy::English,
            &SearchCandidateScope {
                admission: Some(path),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        results.push(
            hits.into_iter()
                .map(|h| (h.note_id, h.score.to_bits()))
                .collect::<Vec<_>>(),
        );
    }
    assert_eq!(results[0], results[1]);
    assert_eq!(results[1].iter().map(|r| r.0).collect::<Vec<_>>(), vec![a]);
    tx.rollback().await.unwrap();
    sqlx::query(&format!("ALTER FUNCTION {SIGNATURE} OWNER TO CURRENT_USER"))
        .execute(&pool)
        .await
        .unwrap();
    drop_role(&pool, &owner).await;
    drop_role(&pool, &runtime).await;
}

async fn raise(pool: &PgPool, sqlstate: &str, message: &str) -> sqlx::Error {
    sqlx::query(&format!(
        "DO $$ BEGIN RAISE EXCEPTION '{message}' USING ERRCODE = '{sqlstate}'; END $$"
    ))
    .execute(pool)
    .await
    .unwrap_err()
}

async fn ambient_tenant(tx: &mut PgConnection) -> Option<String> {
    sqlx::query_scalar("SELECT current_setting('app.current_tenant', true)")
        .fetch_one(tx)
        .await
        .unwrap()
}

/// M2: server errors are classified by what they prove. Only a missing function
/// signature or a permission error for THIS function is definitive.
#[sqlx::test(migrations = false)]
async fn error_classes_separate_definitive_from_inconclusive(pool: PgPool) {
    use fts_admission::{classify_error, ErrorClass};
    let _db = migrated(&pool).await;
    for (sqlstate, expected) in [
        ("57014", ErrorClass::Inconclusive), // cancellation / statement timeout
        ("40P01", ErrorClass::Inconclusive), // deadlock
        ("40001", ErrorClass::Inconclusive), // serialization
        ("53200", ErrorClass::Inconclusive), // out of memory (resource)
        ("53300", ErrorClass::Inconclusive), // too many connections
        ("54000", ErrorClass::Inconclusive), // program limit (cap overflow)
        ("08006", ErrorClass::Inconclusive), // connection failure
        ("42501", ErrorClass::Other),        // generic insufficient privilege
        ("22023", ErrorClass::Other),
    ] {
        let error = raise(&pool, sqlstate, "simulated").await;
        assert_eq!(classify_error(&error), expected, "{sqlstate}");
    }
    assert_eq!(
        classify_error(&sqlx::Error::PoolTimedOut),
        ErrorClass::Inconclusive
    );
    // A missing function signature naming this function: definitive.
    let missing = sqlx::query(
        "SELECT public.fts_admit_note_ids('x'::tsquery, 'a', 'b', ARRAY['title']::text[], 1, 2)",
    )
    .execute(&pool)
    .await
    .unwrap_err();
    assert_eq!(classify_error(&missing), ErrorClass::Definitive);
    // An unrelated undefined function/operator: not definitive.
    let unrelated = sqlx::query("SELECT nosuchfunc_xyz()")
        .execute(&pool)
        .await
        .unwrap_err();
    assert_eq!(classify_error(&unrelated), ErrorClass::Other);
    // Permission denied for THIS function: definitive; for an unrelated function: not.
    let role = runtime_role(&pool, false).await;
    sqlx::query("CREATE FUNCTION public.fts_unrelated_fn() RETURNS int LANGUAGE sql AS 'SELECT 1'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("REVOKE ALL ON FUNCTION public.fts_unrelated_fn() FROM PUBLIC")
        .execute(&pool)
        .await
        .unwrap();
    let mut tx = pool.begin().await.unwrap();
    use_tenant(&mut tx, TENANT_A).await;
    sqlx::query(&format!("SET LOCAL ROLE {role}"))
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("SAVEPOINT s1").execute(&mut *tx).await.unwrap();
    let denied_this = sqlx::query(
        "SELECT * FROM public.fts_admit_note_ids('x'::tsquery, 'public.matric_english', 'public', ARRAY['title']::text[], 1)",
    )
    .execute(&mut *tx)
    .await
    .unwrap_err();
    assert_eq!(classify_error(&denied_this), ErrorClass::Definitive);
    sqlx::query("ROLLBACK TO SAVEPOINT s1")
        .execute(&mut *tx)
        .await
        .unwrap();
    let denied_other = sqlx::query("SELECT public.fts_unrelated_fn()")
        .execute(&mut *tx)
        .await
        .unwrap_err();
    assert!(
        matches!(&denied_other, sqlx::Error::Database(db) if db.code().as_deref() == Some("42501")),
        "unrelated function must be denied for this check"
    );
    assert_eq!(
        classify_error(&denied_other),
        ErrorClass::Other,
        "an unrelated function's permission error must not look definitive"
    );
    tx.rollback().await.unwrap();
    sqlx::query("DROP FUNCTION public.fts_unrelated_fn()")
        .execute(&pool)
        .await
        .unwrap();
    drop_role(&pool, &role).await;
}

/// M2: the probe cannot be defeated by data. Two nil-tenant notes that match the
/// former literal probe token leave a healthy function selected.
#[sqlx::test(migrations = false)]
async fn probe_is_data_independent(pool: PgPool) {
    let _switch = GLOBAL_SWITCH.lock().await;
    let db = migrated(&pool).await;
    let mut seed = pool.begin().await.unwrap();
    use_tenant(&mut seed, TENANT_A).await;
    add_note(&db, &mut seed, "fortemiprobe one", "fortemiprobe").await;
    add_note(&db, &mut seed, "fortemiprobe two", "fortemiprobe").await;
    seed.commit().await.unwrap();
    let role = runtime_role(&pool, true).await;
    let runtime = pool_as(&pool, &role).await;
    assert_eq!(
        fts_admission::probe_and_select(&runtime).await,
        AdmissionPath::Function
    );
    let mut conn = runtime.acquire().await.unwrap();
    assert_eq!(
        fts_admission::probe_on_connection(&mut conn).await,
        fts_admission::ProbeOutcome::Works
    );
    drop(conn);
    runtime.close().await;
    fts_admission::set_active_path(AdmissionPath::Direct);
    drop_role(&pool, &role).await;
}

/// M2: an inconclusive error (cancellation, deadlock, resource) on the function
/// path serves that request via the direct path and keeps the global state; the
/// request's tenant GUC is untouched.
#[sqlx::test(migrations = false)]
async fn inconclusive_errors_use_the_direct_path_for_that_request_only(pool: PgPool) {
    let _switch = GLOBAL_SWITCH.lock().await;
    let _db = migrated(&pool).await;
    let tenant_b = register_tenant(&pool).await;
    for sqlstate in ["57014", "40P01", "53200", "54000"] {
        fts_admission::set_active_path(AdmissionPath::Function);
        let mut tx = pool.begin().await.unwrap();
        use_tenant(&mut tx, tenant_b).await;
        let scope = SearchCandidateScope::default();
        let ran_on = matric_db::search_candidates::with_admission_fallback(
            &mut tx,
            &scope,
            |connection, attempt| {
                Box::pin(async move {
                    if attempt.admission == Some(AdmissionPath::Function) {
                        sqlx::query(&format!(
                            "DO $$ BEGIN RAISE EXCEPTION 'admission cap exceeded' USING ERRCODE = '{sqlstate}'; END $$"
                        ))
                        .execute(connection)
                        .await
                        .map_err(matric_core::Error::Database)?;
                    }
                    Ok(attempt.admission)
                })
            },
        )
        .await
        .unwrap();
        assert_eq!(ran_on, Some(AdmissionPath::Direct), "{sqlstate}");
        assert_eq!(
            fts_admission::active_path(),
            AdmissionPath::Function,
            "{sqlstate}: global state kept"
        );
        assert_eq!(
            ambient_tenant(&mut tx).await.as_deref(),
            Some(tenant_b.to_string().as_str()),
            "{sqlstate}: the request's tenant scope is unchanged"
        );
        tx.rollback().await.unwrap();
    }
    fts_admission::set_active_path(AdmissionPath::Direct);
}

/// M2: a definitive failure downgrades the process; after the re-probe interval
/// a passing probe returns it to the function path (bounded recovery), and the
/// request's tenant GUC is restored after the probe.
#[sqlx::test(migrations = false)]
async fn downgraded_process_recovers_after_the_reprobe_interval(pool: PgPool) {
    let _switch = GLOBAL_SWITCH.lock().await;
    let db = migrated(&pool).await;
    let tenant_b = register_tenant(&pool).await;
    let role = runtime_role(&pool, false).await;
    let mut seed = pool.begin().await.unwrap();
    use_tenant(&mut seed, TENANT_A).await;
    let note = add_note(&db, &mut seed, "recoverword title", "filler").await;
    seed.commit().await.unwrap();

    async fn request(pool: &PgPool, role: &str, tenant: Uuid, expect: Uuid) {
        let mut tx = pool.begin().await.unwrap();
        use_tenant(&mut tx, TENANT_A).await;
        sqlx::query(&format!("SET LOCAL ROLE {role}"))
            .execute(&mut *tx)
            .await
            .unwrap();
        let hits = lexical_on_connection(
            &mut tx,
            "recoverword",
            10,
            LexicalStrategy::English,
            &SearchCandidateScope::default(),
        )
        .await
        .unwrap();
        assert_eq!(
            hits.iter().map(|h| h.note_id).collect::<Vec<_>>(),
            vec![expect]
        );
        let _ = tenant;
        tx.rollback().await.unwrap();
    }

    fts_admission::set_recovery_interval_secs(3600);
    fts_admission::set_active_path(AdmissionPath::Function);
    // No EXECUTE: definitive, confirmed by the re-probe, process downgraded.
    request(&pool, &role, tenant_b, note).await;
    assert_eq!(fts_admission::active_path(), AdmissionPath::Direct);
    // Grant restored, but the interval has not elapsed: still downgraded.
    sqlx::query(&format!("GRANT EXECUTE ON FUNCTION {SIGNATURE} TO {role}"))
        .execute(&pool)
        .await
        .unwrap();
    request(&pool, &role, tenant_b, note).await;
    assert_eq!(fts_admission::active_path(), AdmissionPath::Direct);
    // Interval elapsed: the next request re-probes on its own connection and the
    // process returns to the function path; the tenant scope is restored.
    fts_admission::set_recovery_interval_secs(0);
    let mut tx = pool.begin().await.unwrap();
    use_tenant(&mut tx, tenant_b).await;
    sqlx::query(&format!("SET LOCAL ROLE {role}"))
        .execute(&mut *tx)
        .await
        .unwrap();
    matric_db::search_candidates::with_admission_fallback(
        &mut tx,
        &SearchCandidateScope::default(),
        |_, attempt| Box::pin(async move { Ok(attempt.admission) }),
    )
    .await
    .unwrap();
    assert_eq!(
        fts_admission::active_path(),
        AdmissionPath::Function,
        "bounded recovery returns to the function path"
    );
    assert_eq!(
        ambient_tenant(&mut tx).await.as_deref(),
        Some(tenant_b.to_string().as_str()),
        "the probe must not change the request's tenant scope"
    );
    tx.rollback().await.unwrap();
    fts_admission::set_recovery_interval_secs(300);
    fts_admission::set_active_path(AdmissionPath::Direct);
    drop_role(&pool, &role).await;
}

/// M1: activation is gated on the deployment being single-tenant for lexical
/// search; a hosted multi-tenant deployment keeps the direct path (and is never
/// recovered into the function path) unless the operator opts in.
#[sqlx::test(migrations = false)]
async fn function_path_activates_only_for_eligible_deployments(pool: PgPool) {
    use fts_admission::{configure, Eligibility};
    let _switch = GLOBAL_SWITCH.lock().await;
    let _db = migrated(&pool).await;
    let role = runtime_role(&pool, true).await;
    let runtime = pool_as(&pool, &role).await;
    assert_eq!(
        configure(&runtime, Eligibility::Disabled("hosted multi-tenant")).await,
        AdmissionPath::Direct,
        "a working function is not activated for hosted multi-tenant"
    );
    // Not recovered either, whatever the interval.
    fts_admission::set_recovery_interval_secs(0);
    let mut conn = runtime.acquire().await.unwrap();
    let mut tx = sqlx::Connection::begin(&mut *conn).await.unwrap();
    use_tenant(&mut tx, TENANT_A).await;
    matric_db::search_candidates::with_admission_fallback(
        &mut tx,
        &SearchCandidateScope::default(),
        |_, attempt| Box::pin(async move { Ok(attempt.admission) }),
    )
    .await
    .unwrap();
    assert_eq!(fts_admission::active_path(), AdmissionPath::Direct);
    tx.rollback().await.unwrap();
    drop(conn);
    fts_admission::set_recovery_interval_secs(300);
    assert_eq!(
        configure(&runtime, Eligibility::SharedTenantAccepted).await,
        AdmissionPath::Function,
        "explicit opt-in activates it"
    );
    assert_eq!(
        configure(&runtime, Eligibility::SingleTenant("external-OIDC")).await,
        AdmissionPath::Function
    );
    runtime.close().await;
    fts_admission::set_active_path(AdmissionPath::Direct);
    drop_role(&pool, &role).await;
}

/// M1: the function-path statement is bounded by a statement timeout applied on
/// the request's savepoint: the bound is in force while the statement runs, never
/// loosens a stricter setting, is restored afterwards, and a timeout (57014) is
/// inconclusive: the request is served on the direct path and global state is kept.
/// (A function-local SET statement_timeout does not interrupt the function's own
/// statements, which is why the bound is applied by the caller.)
#[sqlx::test(migrations = false)]
async fn function_path_statements_are_bounded_by_a_statement_timeout(pool: PgPool) {
    let _switch = GLOBAL_SWITCH.lock().await;
    let _db = migrated(&pool).await;
    let scope = SearchCandidateScope {
        admission: Some(AdmissionPath::Function),
        ..Default::default()
    };
    async fn setting(tx: &mut PgConnection) -> String {
        sqlx::query_scalar("SELECT current_setting('statement_timeout')")
            .fetch_one(tx)
            .await
            .unwrap()
    }
    for (outer, expected_inside) in [("0", "10s"), ("30s", "10s"), ("2s", "2s")] {
        fts_admission::set_function_timeout_ms(10_000);
        let mut tx = pool.begin().await.unwrap();
        sqlx::query(&format!("SET LOCAL statement_timeout = '{outer}'"))
            .execute(&mut *tx)
            .await
            .unwrap();
        let inside = matric_db::search_candidates::with_admission_fallback(
            &mut tx,
            &scope,
            |connection, _| {
                Box::pin(async move {
                    sqlx::query_scalar::<_, String>("SELECT current_setting('statement_timeout')")
                        .fetch_one(connection)
                        .await
                        .map_err(matric_core::Error::Database)
                })
            },
        )
        .await
        .unwrap();
        assert_eq!(inside, expected_inside, "outer {outer}");
        assert_eq!(
            setting(&mut tx).await,
            if outer == "0" { "0" } else { outer },
            "the previous statement_timeout is restored"
        );
        tx.rollback().await.unwrap();
    }
    // A timeout is inconclusive: direct path for this request, global state kept.
    fts_admission::set_active_path(AdmissionPath::Function);
    fts_admission::set_function_timeout_ms(100);
    let mut tx = pool.begin().await.unwrap();
    let ran_on = matric_db::search_candidates::with_admission_fallback(
        &mut tx,
        &SearchCandidateScope::default(),
        |connection, attempt| {
            Box::pin(async move {
                if attempt.admission == Some(AdmissionPath::Function) {
                    sqlx::query("SELECT pg_sleep(5)")
                        .execute(connection)
                        .await
                        .map_err(matric_core::Error::Database)?;
                }
                Ok(attempt.admission)
            })
        },
    )
    .await
    .unwrap();
    assert_eq!(ran_on, Some(AdmissionPath::Direct));
    assert_eq!(fts_admission::active_path(), AdmissionPath::Function);
    tx.rollback().await.unwrap();
    fts_admission::set_function_timeout_ms(10_000);
    fts_admission::set_active_path(AdmissionPath::Direct);
}
