use std::collections::HashSet;

use matric_core::ExtractionStrategy;
use sqlx::{postgres::PgPoolOptions, Connection, Executor, PgConnection};
use uuid::Uuid;

const MIGRATION: &str = include_str!(
    "../../../migrations/20260828190000_add_attachment_extraction_strategy_values.sql"
);

const PRE_V2026_7_19_LABELS: &[&str] = &[
    "text_native",
    "pdf_text",
    "pdf_ocr",
    "pandoc",
    "vision",
    "audio_transcribe",
    "video_multimodal",
    "structured_data",
    "code_analysis",
    "none",
    "structured_extract",
    "code_ast",
    "office_convert",
    "glb_3d_model",
];

async fn create_pre_upgrade_enum(connection: &mut PgConnection) {
    let labels = PRE_V2026_7_19_LABELS
        .iter()
        .map(|label| format!("'{label}'"))
        .collect::<Vec<_>>()
        .join(", ");
    connection
        .execute(format!("CREATE TYPE extraction_strategy AS ENUM ({labels})").as_str())
        .await
        .expect("create pre-v2026.7.19 extraction_strategy enum");
}

async fn enum_labels(connection: &mut PgConnection) -> HashSet<String> {
    sqlx::query_scalar::<_, String>(
        "SELECT e.enumlabel::text
           FROM pg_enum e
           JOIN pg_type t ON t.oid = e.enumtypid
           JOIN pg_namespace n ON n.oid = t.typnamespace
          WHERE t.typname = 'extraction_strategy'
            AND n.nspname = current_schema()",
    )
    .fetch_all(connection)
    .await
    .expect("read extraction_strategy enum labels")
    .into_iter()
    .collect()
}

async fn assert_current_variants_are_persistable(connection: &mut PgConnection) {
    let labels = enum_labels(connection).await;
    let expected = ExtractionStrategy::ALL
        .iter()
        .map(ToString::to_string)
        .collect::<HashSet<_>>();
    let missing = expected.difference(&labels).cloned().collect::<Vec<_>>();
    assert!(
        missing.is_empty(),
        "database enum missing Rust labels: {missing:?}"
    );

    connection
        .execute(
            "CREATE TEMP TABLE attachment_strategy_probe (
                filename text NOT NULL,
                strategy extraction_strategy NOT NULL
            )",
        )
        .await
        .expect("create strategy assignment probe");

    for (filename, strategy) in [
        ("fixture.zip", ExtractionStrategy::Archive),
        ("fixture.xlsx", ExtractionStrategy::Spreadsheet),
        ("fixture.eml", ExtractionStrategy::Email),
        ("fixture.docx", ExtractionStrategy::OfficeConvert),
    ] {
        sqlx::query(
            "INSERT INTO attachment_strategy_probe (filename, strategy)
             VALUES ($1, $2::extraction_strategy)",
        )
        .bind(filename)
        .bind(strategy.to_string())
        .execute(&mut *connection)
        .await
        .unwrap_or_else(|error| panic!("persist strategy for {filename}: {error}"));
    }
}

async fn isolated_connection() -> Option<(PgConnection, String)> {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://matric:matric@localhost/matric".to_string());
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await
        .ok()?;
    let schema = format!("extraction_strategy_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&pool)
        .await
        .ok()?;
    pool.close().await;

    let mut connection = PgConnection::connect(&database_url).await.ok()?;
    connection
        .execute(format!("SET search_path TO {schema}").as_str())
        .await
        .ok()?;
    Some((connection, schema))
}

async fn drop_schema(connection: &mut PgConnection, schema: &str) {
    connection
        .execute("SET search_path TO public")
        .await
        .expect("restore public search path");
    connection
        .execute(format!("DROP SCHEMA {schema} CASCADE").as_str())
        .await
        .expect("drop isolated migration schema");
}

#[tokio::test]
async fn v2026_7_19_upgrade_adds_all_current_extraction_strategies() {
    let Some((mut connection, schema)) = isolated_connection().await else {
        eprintln!("skipping PostgreSQL migration test: DATABASE_URL unavailable");
        return;
    };
    create_pre_upgrade_enum(&mut connection).await;
    connection
        .execute(MIGRATION)
        .await
        .expect("apply extraction strategy migration");
    assert_current_variants_are_persistable(&mut connection).await;
    drop_schema(&mut connection, &schema).await;
}

#[tokio::test]
async fn field_workaround_is_compatible_with_idempotent_migration() {
    let Some((mut connection, schema)) = isolated_connection().await else {
        eprintln!("skipping PostgreSQL migration test: DATABASE_URL unavailable");
        return;
    };
    create_pre_upgrade_enum(&mut connection).await;
    connection
        .execute(
            "ALTER TYPE extraction_strategy ADD VALUE IF NOT EXISTS 'archive';
             ALTER TYPE extraction_strategy ADD VALUE IF NOT EXISTS 'spreadsheet';
             ALTER TYPE extraction_strategy ADD VALUE IF NOT EXISTS 'email';",
        )
        .await
        .expect("apply documented field workaround");
    connection
        .execute(MIGRATION)
        .await
        .expect("migration remains safe after workaround");
    assert_current_variants_are_persistable(&mut connection).await;
    drop_schema(&mut connection, &schema).await;
}
