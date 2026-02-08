//! Template repository implementation.

use async_trait::async_trait;
use chrono::Utc;
use sqlx::{Pool, Postgres, Row, Transaction};
use uuid::Uuid;

use matric_core::{
    new_v7, CreateTemplateRequest, Error, NoteTemplate, Result, TemplateRepository,
    UpdateTemplateRequest,
};

/// PostgreSQL implementation of TemplateRepository.
pub struct PgTemplateRepository {
    pool: Pool<Postgres>,
}

impl PgTemplateRepository {
    /// Create a new PgTemplateRepository with the given connection pool.
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TemplateRepository for PgTemplateRepository {
    async fn create(&self, req: CreateTemplateRequest) -> Result<Uuid> {
        let id = new_v7();
        let now = Utc::now();
        let format = req.format.unwrap_or_else(|| "markdown".to_string());
        let default_tags: Vec<String> = req.default_tags.unwrap_or_default();

        sqlx::query(
            r#"
            INSERT INTO note_template (id, name, description, content, format, default_tags, collection_id, created_at_utc, updated_at_utc)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
        )
        .bind(id)
        .bind(&req.name)
        .bind(&req.description)
        .bind(&req.content)
        .bind(&format)
        .bind(&default_tags)
        .bind(req.collection_id)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(id)
    }

    async fn get(&self, id: Uuid) -> Result<Option<NoteTemplate>> {
        let row = sqlx::query(
            r#"
            SELECT id, name, description, content, format, default_tags, collection_id, created_at_utc, updated_at_utc
            FROM note_template
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(row.map(|r| NoteTemplate {
            id: r.get("id"),
            name: r.get("name"),
            description: r.get("description"),
            content: r.get("content"),
            format: r.get("format"),
            default_tags: r.get::<Vec<String>, _>("default_tags"),
            collection_id: r.get("collection_id"),
            created_at_utc: r.get("created_at_utc"),
            updated_at_utc: r.get("updated_at_utc"),
        }))
    }

    async fn get_by_name(&self, name: &str) -> Result<Option<NoteTemplate>> {
        let row = sqlx::query(
            r#"
            SELECT id, name, description, content, format, default_tags, collection_id, created_at_utc, updated_at_utc
            FROM note_template
            WHERE name = $1
            "#,
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(row.map(|r| NoteTemplate {
            id: r.get("id"),
            name: r.get("name"),
            description: r.get("description"),
            content: r.get("content"),
            format: r.get("format"),
            default_tags: r.get::<Vec<String>, _>("default_tags"),
            collection_id: r.get("collection_id"),
            created_at_utc: r.get("created_at_utc"),
            updated_at_utc: r.get("updated_at_utc"),
        }))
    }

    async fn list(&self) -> Result<Vec<NoteTemplate>> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, description, content, format, default_tags, collection_id, created_at_utc, updated_at_utc
            FROM note_template
            ORDER BY name
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(rows
            .into_iter()
            .map(|r| NoteTemplate {
                id: r.get("id"),
                name: r.get("name"),
                description: r.get("description"),
                content: r.get("content"),
                format: r.get("format"),
                default_tags: r.get::<Vec<String>, _>("default_tags"),
                collection_id: r.get("collection_id"),
                created_at_utc: r.get("created_at_utc"),
                updated_at_utc: r.get("updated_at_utc"),
            })
            .collect())
    }

    async fn update(&self, id: Uuid, req: UpdateTemplateRequest) -> Result<()> {
        let now = Utc::now();

        // Build dynamic update query
        let mut updates = vec!["updated_at_utc = $1".to_string()];
        let mut param_count = 2;

        if req.name.is_some() {
            updates.push(format!("name = ${}", param_count));
            param_count += 1;
        }
        if req.description.is_some() {
            updates.push(format!("description = ${}", param_count));
            param_count += 1;
        }
        if req.content.is_some() {
            updates.push(format!("content = ${}", param_count));
            param_count += 1;
        }
        if req.default_tags.is_some() {
            updates.push(format!("default_tags = ${}", param_count));
            param_count += 1;
        }
        if req.collection_id.is_some() {
            updates.push(format!("collection_id = ${}", param_count));
            param_count += 1;
        }

        let query = format!(
            "UPDATE note_template SET {} WHERE id = ${}",
            updates.join(", "),
            param_count
        );

        let mut q = sqlx::query(&query).bind(now);

        if let Some(name) = &req.name {
            q = q.bind(name);
        }
        if let Some(description) = &req.description {
            q = q.bind(description);
        }
        if let Some(content) = &req.content {
            q = q.bind(content);
        }
        if let Some(default_tags) = &req.default_tags {
            q = q.bind(default_tags);
        }
        if let Some(collection_id) = &req.collection_id {
            q = q.bind(*collection_id);
        }

        q.bind(id)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;

        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM note_template WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;
        Ok(())
    }
}

/// Transaction-aware variants for archive-scoped operations.
impl PgTemplateRepository {
    /// Create a template within an existing transaction.
    pub async fn create_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        req: CreateTemplateRequest,
    ) -> Result<Uuid> {
        let id = new_v7();
        let now = Utc::now();
        let format = req.format.unwrap_or_else(|| "markdown".to_string());
        let default_tags: Vec<String> = req.default_tags.unwrap_or_default();

        sqlx::query(
            r#"
            INSERT INTO note_template (id, name, description, content, format, default_tags, collection_id, created_at_utc, updated_at_utc)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
        )
        .bind(id)
        .bind(&req.name)
        .bind(&req.description)
        .bind(&req.content)
        .bind(&format)
        .bind(&default_tags)
        .bind(req.collection_id)
        .bind(now)
        .bind(now)
        .execute(&mut **tx)
        .await
        .map_err(Error::Database)?;

        Ok(id)
    }

    /// Get a template by ID within an existing transaction.
    pub async fn get_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        id: Uuid,
    ) -> Result<Option<NoteTemplate>> {
        let row = sqlx::query(
            r#"
            SELECT id, name, description, content, format, default_tags, collection_id, created_at_utc, updated_at_utc
            FROM note_template
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(Error::Database)?;

        Ok(row.map(|r| NoteTemplate {
            id: r.get("id"),
            name: r.get("name"),
            description: r.get("description"),
            content: r.get("content"),
            format: r.get("format"),
            default_tags: r.get::<Vec<String>, _>("default_tags"),
            collection_id: r.get("collection_id"),
            created_at_utc: r.get("created_at_utc"),
            updated_at_utc: r.get("updated_at_utc"),
        }))
    }

    /// List templates within an existing transaction.
    pub async fn list_tx(&self, tx: &mut Transaction<'_, Postgres>) -> Result<Vec<NoteTemplate>> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, description, content, format, default_tags, collection_id, created_at_utc, updated_at_utc
            FROM note_template
            ORDER BY name
            "#,
        )
        .fetch_all(&mut **tx)
        .await
        .map_err(Error::Database)?;

        Ok(rows
            .into_iter()
            .map(|r| NoteTemplate {
                id: r.get("id"),
                name: r.get("name"),
                description: r.get("description"),
                content: r.get("content"),
                format: r.get("format"),
                default_tags: r.get::<Vec<String>, _>("default_tags"),
                collection_id: r.get("collection_id"),
                created_at_utc: r.get("created_at_utc"),
                updated_at_utc: r.get("updated_at_utc"),
            })
            .collect())
    }

    /// Update a template within an existing transaction.
    pub async fn update_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        id: Uuid,
        req: UpdateTemplateRequest,
    ) -> Result<()> {
        let now = Utc::now();

        let mut updates = vec!["updated_at_utc = $1".to_string()];
        let mut param_count = 2;

        if req.name.is_some() {
            updates.push(format!("name = ${}", param_count));
            param_count += 1;
        }
        if req.description.is_some() {
            updates.push(format!("description = ${}", param_count));
            param_count += 1;
        }
        if req.content.is_some() {
            updates.push(format!("content = ${}", param_count));
            param_count += 1;
        }
        if req.default_tags.is_some() {
            updates.push(format!("default_tags = ${}", param_count));
            param_count += 1;
        }
        if req.collection_id.is_some() {
            updates.push(format!("collection_id = ${}", param_count));
            param_count += 1;
        }

        let query = format!(
            "UPDATE note_template SET {} WHERE id = ${}",
            updates.join(", "),
            param_count
        );

        let mut q = sqlx::query(&query).bind(now);

        if let Some(name) = &req.name {
            q = q.bind(name);
        }
        if let Some(description) = &req.description {
            q = q.bind(description);
        }
        if let Some(content) = &req.content {
            q = q.bind(content);
        }
        if let Some(default_tags) = &req.default_tags {
            q = q.bind(default_tags);
        }
        if let Some(collection_id) = &req.collection_id {
            q = q.bind(*collection_id);
        }

        q.bind(id)
            .execute(&mut **tx)
            .await
            .map_err(Error::Database)?;

        Ok(())
    }

    /// Delete a template within an existing transaction.
    pub async fn delete_tx(&self, tx: &mut Transaction<'_, Postgres>, id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM note_template WHERE id = $1")
            .bind(id)
            .execute(&mut **tx)
            .await
            .map_err(Error::Database)?;
        Ok(())
    }
}
