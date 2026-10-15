//! Populated legacy and mixed-owner deployment ownership upgrades.
use matric_core::{ArchiveRepository, CreateNoteRequest, NoteRepository};
use matric_db::{external_owner, Database, PoolConfig};
use sqlx::{ConnectOptions, PgPool, Row};
use uuid::Uuid;

async fn fixture(admin: &PgPool, mixed: bool) -> (PgPool, Uuid, Uuid, String, Vec<Uuid>) {
    sqlx::query("CREATE EXTENSION IF NOT EXISTS postgis")
        .execute(admin)
        .await
        .unwrap();
    // Build the release schema first; populate it before running new migrations.
    let release = sqlx::migrate!("../../migrations");
    let release = sqlx::migrate::Migrator {
        migrations: std::borrow::Cow::Owned(
            release
                .iter()
                .filter(|migration| migration.version < 20261014000000)
                .cloned()
                .collect(),
        ),
        ..sqlx::migrate::Migrator::DEFAULT
    };
    release.run(admin).await.unwrap();
    let pool = matric_db::create_pool(admin.connect_options().to_url_lossy().as_str())
        .await
        .unwrap();
    let db = Database::new(pool.clone());
    let owner = Uuid::new_v4();
    let foreign = Uuid::new_v4();
    for id in [owner, foreign] {
        sqlx::query("INSERT INTO tenant_registry(id,slug,display_name,status) VALUES($1,$2,'Synthetic upgrade','active')")
            .bind(id).bind(id.to_string()).execute(&pool).await.unwrap();
    }
    let user = Uuid::new_v4();
    sqlx::query("INSERT INTO app_user(id,tenant_id,iss,sub,kind,status) VALUES($1,$2,'https://synthetic.example','disabled-legacy','user','disabled')")
        .bind(user).bind(Uuid::nil()).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO personal_access_token(id,user_id,token_hash,token_prefix,token_last4,name,status,expires_at) VALUES(uuidv7(),$1,repeat('a',64),'mm_pat_synthetic','1234','Upgrade fixture','suspended','2030-01-01')")
        .bind(user).execute(&pool).await.unwrap();
    let external_user = Uuid::new_v4();
    sqlx::query("INSERT INTO app_user(id,tenant_id,iss,sub,kind) VALUES($1,$2,'https://synthetic.example','jit-external','user')")
        .bind(external_user).bind(owner).execute(&pool).await.unwrap();
    let archive = db
        .archives
        .create_archive_schema("legacy", None)
        .await
        .unwrap();
    let mut ids = Vec::new();
    for schema in ["public", archive.schema_name.as_str()] {
        let notes = matric_db::PgNoteRepository::new(pool.clone());
        let id = db
            .for_schema(schema)
            .unwrap()
            .execute(move |tx| {
                Box::pin(async move {
                    notes
                        .insert_tx(
                            tx,
                            CreateNoteRequest {
                                content: "Synthetic legacy vector attachment research".into(),
                                format: "markdown".into(),
                                source: "upgrade".into(),
                                collection_id: None,
                                tags: Some(vec!["upgrade".into()]),
                                metadata: None,
                                document_type_id: None,
                                title: Some("Legacy".into()),
                            },
                        )
                        .await
                })
            })
            .await
            .unwrap();
        sqlx::query(&format!(
            "UPDATE {schema}.note SET created_by_user_id=$2,updated_by_user_id=$2 WHERE id=$1"
        ))
        .bind(id)
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();
        // Database-backed blob proves bytes survive without any filesystem rewrite.
        let blob = Uuid::new_v4();
        sqlx::query(&format!("INSERT INTO {schema}.attachment_blob(id,content_hash,content_type,size_bytes,storage_type,object_key,data) VALUES($1,$2,'text/plain',7,'database',$3,$4)"))
            .bind(blob).bind(format!("synthetic-{id}")).bind(blob.to_string()).bind(b"payload".as_slice()).execute(&pool).await.unwrap();
        sqlx::query(&format!("INSERT INTO {schema}.attachment(id,note_id,blob_id,filename) VALUES(uuidv7(),$1,$2,'legacy.txt')"))
            .bind(id).bind(blob).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO public.job_queue(id,note_id,job_type,status,payload) VALUES(uuidv7(),$1,'embedding','pending',$2)")
            .bind(id).bind(serde_json::json!({"schema":schema,"tenant_id":Uuid::nil(),"nested":{"tenant_id":Uuid::nil()},"literal":Uuid::nil().to_string()})).execute(&pool).await.unwrap();
        let embeddings = matric_db::PgEmbeddingRepository::new(pool.clone());
        db.for_schema(schema)
            .unwrap()
            .execute(move |tx| {
                Box::pin(async move {
                    embeddings
                        .store_tx(
                            tx,
                            id,
                            vec![(
                                "Synthetic legacy vector".into(),
                                matric_core::Vector::from(vec![0.25; 768]),
                            )],
                            "nomic-embed-text",
                        )
                        .await
                })
            })
            .await
            .unwrap();
        sqlx::query(&format!("INSERT INTO {schema}.note_revision(id,note_id,revision_number,content,created_at_utc) VALUES(uuidv7(),$1,1,'Synthetic legacy revision',now())"))
            .bind(id).execute(&pool).await.unwrap();
        ids.push(id);
    }
    // Keep the actual old columns absent at adoption, not just a stale version.
    sqlx::raw_sql(&format!("ALTER TABLE {}.note DROP COLUMN created_by_user_id CASCADE; ALTER TABLE {}.note DROP COLUMN updated_by_user_id CASCADE; UPDATE archive_registry SET schema_version=0 WHERE name='legacy'",archive.schema_name,archive.schema_name))
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO document_type(id,name,display_name,category,is_system,chunk_size_default,agentic_config) VALUES(uuidv7(),'custom-upgrade','Custom Upgrade','custom',false,777,'{\"revision_prompt\":\"Synthetic custom prompt\"}')")
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO embedding_config(id,name,model,dimension,chunk_size,chunk_overlap,is_default) VALUES(uuidv7(),'custom-upgrade','nomic-embed-text',768,777,17,false)")
        .execute(&pool).await.unwrap();
    sqlx::query("UPDATE document_type SET recommended_config_id=(SELECT id FROM embedding_config WHERE tenant_id=$1 AND name='custom-upgrade') WHERE tenant_id=$1 AND name='custom-upgrade'")
        .bind(Uuid::nil()).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO note SELECT * FROM jsonb_populate_record(NULL::note,(SELECT to_jsonb(n) FROM note n LIMIT 1)||jsonb_build_object('id',uuidv7(),'tenant_id',$1::uuid,'title','Foreign unchanged','created_by_user_id',null,'updated_by_user_id',null,'document_type_id',null))")
        .bind(foreign)
        .execute(&pool)
        .await
        .unwrap();
    Database::new(pool.clone()).migrate().await.unwrap();
    // Keep one archive genuinely older at first adoption, including absent guards.
    sqlx::raw_sql(&format!("ALTER TABLE {}.note DROP COLUMN created_by_user_id CASCADE; ALTER TABLE {}.note DROP COLUMN updated_by_user_id CASCADE; UPDATE archive_registry SET schema_version=0 WHERE name='legacy'",archive.schema_name,archive.schema_name)).execute(&pool).await.unwrap();
    if mixed {
        sqlx::query("INSERT INTO document_type SELECT (jsonb_populate_record(NULL::document_type,to_jsonb(d)||jsonb_build_object('id',uuidv7(),'tenant_id',$1::uuid))).* FROM document_type d WHERE tenant_id=$2 AND name='markdown'")
            .bind(owner).bind(Uuid::nil()).execute(&pool).await.unwrap();
        // W01-shaped note already owned by E, with E identity and E type.
        sqlx::query("INSERT INTO note SELECT * FROM jsonb_populate_record(NULL::note,(SELECT to_jsonb(n) FROM note n LIMIT 1)||jsonb_build_object('id',uuidv7(),'tenant_id',$1::uuid,'title','Mixed owner','document_type_id',(SELECT id FROM document_type WHERE tenant_id=$1 AND name='markdown'),'created_by_user_id',$2::uuid,'updated_by_user_id',$2::uuid))")
            .bind(owner).bind(external_user).execute(&pool).await.unwrap();
    }
    (pool, owner, foreign, archive.schema_name, ids)
}

async fn snapshot(pool: &PgPool) -> serde_json::Value {
    let tables:Vec<(String,String)>=sqlx::query_as("SELECT n.nspname::text,c.relname::text FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace JOIN pg_attribute a ON a.attrelid=c.oid AND a.attname='tenant_id' WHERE c.relkind='r' AND (n.nspname='public' OR n.nspname LIKE 'archive_%') ORDER BY 1,2").fetch_all(pool).await.unwrap();
    let mut result = serde_json::Map::new();
    for (schema, table) in tables {
        let rows:serde_json::Value=sqlx::query_scalar(&format!("SELECT COALESCE(jsonb_agg(v ORDER BY v::text),'[]') FROM (SELECT to_jsonb(t) v FROM {schema}.{table} t) s")).fetch_one(pool).await.unwrap();
        result.insert(format!("{schema}.{table}"), rows);
    }
    serde_json::Value::Object(result)
}

#[sqlx::test(migrations = false)]
async fn populated_legacy_and_mixed_upgrade_preserves_graph_and_is_idempotent(admin: PgPool) {
    let (pool, owner, foreign, schema, ids) = fixture(&admin, true).await;
    let before = snapshot(&pool).await;
    assert!(external_owner::adopt(&pool, owner).await.unwrap());
    let after = snapshot(&pool).await;
    for (table, rows) in before.as_object().unwrap() {
        for row in rows.as_array().unwrap() {
            if row["tenant_id"] == Uuid::nil().to_string() {
                let candidate = after[table]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|candidate| {
                        if let Some(id) = row.get("id") {
                            candidate.get("id") == Some(id)
                        } else {
                            row.as_object().unwrap().iter().all(|(key, value)| {
                                key == "tenant_id" || candidate.get(key) == Some(value)
                            })
                        }
                    })
                    .unwrap_or_else(|| panic!("Legacy UUID/relationship lost in {table}: {row}"));
                assert_eq!(candidate["tenant_id"], owner.to_string());
                for (key, value) in row.as_object().unwrap() {
                    if key == "tenant_id"
                        || key == "payload"
                        || (table.ends_with(".embedding_set")
                            && row["slug"] == "default"
                            && matches!(
                                key.as_str(),
                                "document_count"
                                    | "embedding_count"
                                    | "embeddings_current"
                                    | "updated_at"
                            ))
                        || (table == "public.archive_registry" && key == "schema_version")
                    {
                        continue;
                    }
                    assert_eq!(
                        candidate.get(key),
                        Some(value),
                        "Legacy field {table}.{key}"
                    );
                }
            }
        }

        for row in rows.as_array().unwrap() {
            if row["tenant_id"] == foreign.to_string() {
                assert!(
                    after[table].as_array().unwrap().contains(row),
                    "foreign row changed in {table}"
                );
            }
        }
    }
    for (index, archive) in ["public", schema.as_str()].iter().enumerate() {
        let tenant: Uuid =
            sqlx::query_scalar(&format!("SELECT tenant_id FROM {archive}.note WHERE id=$1"))
                .bind(ids[index])
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(tenant, owner);
        let bytes: Vec<u8> = sqlx::query_scalar(&format!(
            "SELECT data FROM {archive}.attachment_blob LIMIT 1"
        ))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(bytes, b"payload");
        let baseline:i64=sqlx::query_scalar(&format!("SELECT count(*) FROM {archive}.embedding_set s JOIN public.embedding_config c ON c.id=s.embedding_config_id AND c.tenant_id=s.tenant_id WHERE s.tenant_id=$1 AND s.slug='default'")).bind(owner).fetch_one(&pool).await.unwrap();
        assert_eq!(baseline, 1);
        // Adoption adds the previously E-owned note to the canonical default set.
        // Its membership trigger refreshes these derived fields; prove they agree
        // with preserved rows rather than requiring the old cached values.
        let stats: (i32, i32, bool, i64, i64) = sqlx::query_as(&format!(
            "SELECT s.document_count,s.embedding_count,s.embeddings_current,
                (SELECT count(DISTINCT m.note_id) FROM {archive}.embedding_set_member m
                 WHERE m.tenant_id=s.tenant_id AND m.embedding_set_id=s.id),
                (SELECT count(*) FROM {archive}.embedding e
                 WHERE e.tenant_id=s.tenant_id AND e.embedding_set_id=s.id)
             FROM {archive}.embedding_set s WHERE s.tenant_id=$1 AND s.slug='default'"
        ))
        .bind(owner)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(i64::from(stats.0), stats.3, "{archive} membership count");
        assert_eq!(i64::from(stats.1), stats.4, "{archive} embedding count");
        assert_eq!(stats.2, stats.4 >= stats.3, "{archive} embedding coverage");
        assert_eq!(stats.3, if index == 0 { 2 } else { 1 });
        assert_eq!(stats.4, 1, "{archive} legacy embedding must remain");
        let missing: i64 = sqlx::query_scalar(&format!(
            "SELECT count(*) FROM {archive}.note n
             WHERE n.tenant_id=$1 AND n.deleted_at IS NULL AND NOT n.archived
             AND NOT EXISTS(SELECT 1 FROM {archive}.embedding_set_member m
                 JOIN {archive}.embedding_set s ON s.id=m.embedding_set_id
                     AND s.tenant_id=m.tenant_id
                 WHERE m.note_id=n.id AND m.tenant_id=n.tenant_id AND s.slug='default')"
        ))
        .bind(owner)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            missing, 0,
            "{archive} eligible note lost default membership"
        );
        let fks:i64=sqlx::query_scalar("SELECT count(*) FROM pg_constraint WHERE conrelid=to_regclass($1) AND confrelid='public.app_user'::regclass AND convalidated").bind(format!("{archive}.note")).fetch_one(&pool).await.unwrap();
        assert_eq!(fks, 2);
    }
    let pat = sqlx::query(
        "SELECT tenant_id,token_hash,status,expires_at::text FROM personal_access_token",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(pat.get::<Uuid, _>("tenant_id"), owner);
    assert_eq!(pat.get::<String, _>("token_hash"), "a".repeat(64));
    assert_eq!(pat.get::<String, _>("status"), "suspended");
    let disabled: String =
        sqlx::query_scalar("SELECT status FROM app_user WHERE sub='disabled-legacy'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(disabled, "disabled");
    let jobs:i64=sqlx::query_scalar("SELECT count(*) FROM job_queue WHERE tenant_id=$1 AND payload->>'tenant_id'=$2 AND payload->'nested'->>'tenant_id'=$2 AND payload->>'literal'=$3")
        .bind(owner).bind(owner.to_string()).bind(Uuid::nil().to_string()).fetch_one(&pool).await.unwrap();
    assert_eq!(jobs, 2);
    assert!(!external_owner::adopt(&pool, owner).await.unwrap());
    assert_eq!(snapshot(&pool).await, after);
    assert!(external_owner::adopt(&pool, foreign).await.is_err());
    let runtime = matric_db::create_pool_with_config(
        pool.connect_options().to_url_lossy().as_str(),
        PoolConfig::new().single_tenant(owner).max_connections(1),
    )
    .await
    .unwrap();
    let db = Database::new(runtime.clone());
    assert!(db.notes.fetch(ids[0]).await.is_ok());
    let archives = db.archives.list_archive_schemas().await.unwrap();
    assert!(archives.iter().any(|a| a.name == "legacy"));
    runtime.close().await;
}

#[sqlx::test(migrations = false)]
async fn late_failure_rolls_back_ownership_schema_guards_and_receipt(admin: PgPool) {
    let (pool, owner, _, schema, _) = fixture(&admin, false).await;
    let before = snapshot(&pool).await;
    sqlx::query(&format!("ALTER TABLE external_oidc_ownership ADD CONSTRAINT synthetic_fail CHECK(owner<>'{owner}'::uuid)"))
        .execute(&pool).await.unwrap();
    assert!(external_owner::adopt(&pool, owner).await.is_err());
    assert_eq!(snapshot(&pool).await, before);
    let columns:i64=sqlx::query_scalar("SELECT count(*) FROM information_schema.columns WHERE table_schema=$1 AND table_name='note' AND column_name='created_by_user_id'").bind(&schema).fetch_one(&pool).await.unwrap();
    assert_eq!(columns, 0);
    let receipt: i64 = sqlx::query_scalar("SELECT count(*) FROM external_oidc_ownership")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(receipt, 0);
    sqlx::query("ALTER TABLE external_oidc_ownership DROP CONSTRAINT synthetic_fail")
        .execute(&pool)
        .await
        .unwrap();
    assert!(external_owner::adopt(&pool, owner).await.unwrap());
}

#[sqlx::test(migrations = false)]
async fn customized_collision_and_identity_collision_abort_without_writes(admin: PgPool) {
    let (pool, owner, _, _, _) = fixture(&admin, true).await;
    sqlx::query(
        "UPDATE document_type SET chunk_size_default=123 WHERE tenant_id=$1 AND name='markdown'",
    )
    .bind(owner)
    .execute(&pool)
    .await
    .unwrap();
    let before = snapshot(&pool).await;
    assert!(external_owner::adopt(&pool, owner).await.is_err());
    assert_eq!(snapshot(&pool).await, before);
    sqlx::query("UPDATE document_type SET chunk_size_default=(SELECT chunk_size_default FROM document_type WHERE tenant_id=$2 AND name='markdown') WHERE tenant_id=$1 AND name='markdown'").bind(owner).bind(Uuid::nil()).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO app_user(tenant_id,iss,sub,kind) VALUES($1,'https://synthetic.example','disabled-legacy','user')").bind(owner).execute(&pool).await.unwrap();
    let before = snapshot(&pool).await;
    assert!(external_owner::adopt(&pool, owner).await.is_err());
    assert_eq!(snapshot(&pool).await, before);
}

#[sqlx::test(migrations = false)]
async fn interrupted_upgrade_restores_all_rows_and_constraints(admin: PgPool) {
    let (pool, owner, _, _, _) = fixture(&admin, false).await;
    let before = snapshot(&pool).await;
    sqlx::raw_sql("CREATE FUNCTION public.synthetic_upgrade_pause() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(30); RETURN NEW; END $$; CREATE TRIGGER synthetic_upgrade_pause BEFORE INSERT ON public.external_oidc_ownership FOR EACH ROW EXECUTE FUNCTION public.synthetic_upgrade_pause()")
        .execute(&pool).await.unwrap();
    let interrupter = async {
        for _ in 0..200 {
            let pid:Option<i32>=sqlx::query_scalar("SELECT pid FROM pg_stat_activity WHERE datname=current_database() AND pid<>pg_backend_pid() AND wait_event='PgSleep' AND query LIKE 'INSERT INTO public.external_oidc_ownership%' LIMIT 1")
                .fetch_optional(&admin).await.unwrap();
            if let Some(pid) = pid {
                sqlx::query("SELECT pg_terminate_backend($1)")
                    .bind(pid)
                    .execute(&admin)
                    .await
                    .unwrap();
                return true;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        false
    };
    let (result, interrupted) = tokio::join!(external_owner::adopt(&pool, owner), interrupter);
    assert!(
        interrupted,
        "upgrade did not reach the final receipt checkpoint"
    );
    assert!(result.is_err());
    assert_eq!(snapshot(&pool).await, before);
    sqlx::query("DROP TRIGGER synthetic_upgrade_pause ON public.external_oidc_ownership")
        .execute(&pool)
        .await
        .unwrap();
    assert!(external_owner::adopt(&pool, owner).await.unwrap());
}
