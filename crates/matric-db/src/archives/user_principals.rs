//! Author guards for archives created before shared application principals.
use matric_core::Result;
use sqlx::PgConnection;

pub(super) async fn repair(conn: &mut PgConnection, schema: &str) -> Result<()> {
    for (table, column, constraint) in [
        ("note", "created_by_user_id", "note_created_by_user_fk"),
        ("note", "updated_by_user_id", "note_updated_by_user_fk"),
        (
            "embedding_import_run",
            "initiated_by_user_id",
            "embedding_import_run_initiated_by_user_fk",
        ),
    ] {
        let relation = format!("{schema}.{table}");
        let exists: bool = sqlx::query_scalar("SELECT to_regclass($1) IS NOT NULL")
            .bind(&relation)
            .fetch_one(&mut *conn)
            .await?;
        if !exists {
            continue;
        }
        let has_column: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_attribute WHERE attrelid=to_regclass($1) AND attname=$2 AND NOT attisdropped)")
            .bind(&relation).bind(column).fetch_one(&mut *conn).await?;
        if !has_column {
            sqlx::query(&format!("ALTER TABLE {relation} ADD COLUMN {column} UUID"))
                .execute(&mut *conn)
                .await?;
        }
        let correct: Option<bool> = sqlx::query_scalar("SELECT confrelid='public.app_user'::regclass AND convalidated FROM pg_constraint WHERE conrelid=to_regclass($1) AND conname=$2")
            .bind(&relation).bind(constraint).fetch_optional(&mut *conn).await?;
        if correct == Some(true) {
            continue;
        }
        if correct.is_some() {
            sqlx::query(&format!(
                "ALTER TABLE {relation} DROP CONSTRAINT {constraint}"
            ))
            .execute(&mut *conn)
            .await?;
        }
        sqlx::query(&format!("ALTER TABLE {relation} ADD CONSTRAINT {constraint} FOREIGN KEY(tenant_id,{column}) REFERENCES public.app_user(tenant_id,id) ON UPDATE RESTRICT ON DELETE RESTRICT"))
            .execute(&mut *conn).await?;
    }
    Ok(())
}
