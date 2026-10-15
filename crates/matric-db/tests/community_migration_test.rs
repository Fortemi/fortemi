//! Exercise bundle startup privileges, rather than the testdb superuser's.
use matric_db::{create_pool_with_config, Database, PoolConfig};
use sqlx::{ConnectOptions, PgPool};
use uuid::Uuid;

#[sqlx::test(migrations = false)]
async fn full_migration_set_as_non_superuser_nobypassrls_owner(admin: PgPool) {
    // bundle-entrypoint.sh provisions these untrusted extensions as postgres,
    // then the API migrates as the ordinary CREATEDB database owner.
    sqlx::raw_sql("CREATE EXTENSION IF NOT EXISTS vector; CREATE EXTENSION IF NOT EXISTS postgis")
        .execute(&admin)
        .await
        .unwrap();
    let owner = format!("community_owner_{}", Uuid::new_v4().simple());
    let options = admin.connect_options();
    let database = options.get_database().unwrap();
    sqlx::query(&format!(
        "CREATE ROLE {owner} LOGIN PASSWORD '{owner}' NOSUPERUSER NOBYPASSRLS CREATEDB NOCREATEROLE NOREPLICATION"
    ))
    .execute(&admin)
    .await
    .unwrap();
    sqlx::query(&format!("ALTER DATABASE \"{database}\" OWNER TO {owner}"))
        .execute(&admin)
        .await
        .unwrap();
    let owner_url = options
        .as_ref()
        .clone()
        .username(&owner)
        .password(&owner)
        .to_url_lossy();
    let pool = create_pool_with_config(owner_url.as_str(), PoolConfig::new().max_connections(1))
        .await
        .unwrap();

    let result = exercise_community_migrations(&pool).await;
    pool.close().await;
    // Roles are cluster-wide: release their ownership even when migration fails.
    sqlx::raw_sql(&format!(
        "ALTER DATABASE \"{database}\" OWNER TO CURRENT_USER; DROP OWNED BY {owner} CASCADE; DROP ROLE {owner}"
    ))
    .execute(&admin)
    .await
    .unwrap();
    result.unwrap();
}

async fn exercise_community_migrations(pool: &PgPool) -> anyhow::Result<()> {
    let privileges: (bool, bool, bool) = sqlx::query_as(
        "SELECT r.rolsuper, r.rolbypassrls, d.datdba = r.oid
         FROM pg_roles r JOIN pg_database d ON d.datname = current_database()
         WHERE r.rolname = current_user",
    )
    .fetch_one(pool)
    .await?;
    assert_eq!(privileges, (false, false, true));

    let db = Database::new(pool.clone());
    db.migrate().await?;
    let applied: i64 =
        sqlx::query_scalar("SELECT count(*) FROM public._sqlx_migrations WHERE success")
            .fetch_one(pool)
            .await?;
    assert_eq!(
        applied as usize,
        sqlx::migrate!("../../migrations").iter().count()
    );
    let owned_and_forced: bool = sqlx::query_scalar(
        "SELECT c.relowner = r.oid AND c.relrowsecurity AND c.relforcerowsecurity
            AND p.proowner = r.oid
         FROM pg_class c CROSS JOIN pg_roles r CROSS JOIN pg_proc p
         WHERE c.oid = 'public.personal_access_token'::regclass
            AND r.rolname = current_user
            AND p.oid = 'public.resolve_pat_tenant(text)'::regprocedure",
    )
    .fetch_one(pool)
    .await?;
    assert!(owned_and_forced);

    // Creation must succeed without BYPASSRLS; execution must still fail closed
    // under forced RLS, even for an unknown HMAC, rather than filtering rows.
    let error = sqlx::query("SELECT * FROM public.resolve_pat_tenant($1)")
        .bind("0".repeat(64))
        .fetch_all(pool)
        .await
        .expect_err("a function owner without BYPASSRLS must not silently filter PATs");
    let error = error.as_database_error().unwrap();
    assert_eq!(error.code().as_deref(), Some("42501"));
    assert!(error.message().contains("row-level security policy"));
    // Hosted startup's administrative admission must still reject this owner.
    let error = Database::new(pool.clone())
        .with_administrative_pool(pool.clone())
        .await
        .err()
        .expect("hosted administrative access still requires BYPASSRLS");
    assert!(matches!(error, matric_core::Error::Config(message) if message.contains("BYPASSRLS")));
    db.migrate().await?;
    Ok(())
}

#[sqlx::test(migrations = false)]
async fn pat_checksum_repairs_accept_only_the_known_successful_original(admin: PgPool) {
    sqlx::raw_sql("CREATE EXTENSION IF NOT EXISTS vector; CREATE EXTENSION IF NOT EXISTS postgis")
        .execute(&admin)
        .await
        .unwrap();
    let db = Database::new(admin.clone());
    db.migrate().await.unwrap();
    let migrator = sqlx::migrate!("../../migrations");
    let canonical = migrator
        .iter()
        .find(|migration| migration.version == 20261014050000)
        .unwrap()
        .checksum
        .as_ref();
    let original = hex::decode("e95e56f3cc4829c7deaec70c908e8128ee720b9503650479a91253ce87d47461538d6adf8218caa4cddc6cc3d282b0d5").unwrap();
    // Execute the shipped bundle repair SQL, so both startup paths must agree
    // with sqlx's embedded checksum and preserve unknown/failed ledger entries.
    let bundle_repair = include_str!("../../../docker/bundle-entrypoint.sh")
        .split_once("echo \">>> Checking migration checksum repair...\"")
        .unwrap()
        .1
        .split_once("<<'SQL'\n")
        .unwrap()
        .1
        .split_once("\nSQL\n")
        .unwrap()
        .0;
    for bundle in [false, true] {
        for (checksum, success, repairable) in [
            (original.as_slice(), true, true),
            (&[0_u8][..], true, false),
            (original.as_slice(), false, false),
        ] {
            sqlx::query("UPDATE public._sqlx_migrations SET checksum=$1, success=$2 WHERE version=20261014050000")
                .bind(checksum).bind(success).execute(&admin).await.unwrap();
            if bundle {
                sqlx::raw_sql(bundle_repair).execute(&admin).await.unwrap();
            } else {
                assert_eq!(db.migrate().await.is_ok(), repairable);
            }
            let stored: (Vec<u8>, bool) = sqlx::query_as(
                "SELECT checksum, success FROM public._sqlx_migrations WHERE version=20261014050000",
            )
            .fetch_one(&admin)
            .await
            .unwrap();
            assert_eq!(stored.0, if repairable { canonical } else { checksum });
            assert_eq!(stored.1, success);
        }
    }
    sqlx::query(
        "UPDATE public._sqlx_migrations SET checksum=$1, success=true WHERE version=20261014050000",
    )
    .bind(canonical)
    .execute(&admin)
    .await
    .unwrap();
    db.migrate().await.unwrap();
}
