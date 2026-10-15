use super::*;

impl PgDocumentTypeRepository {
    pub async fn list_tx(conn: &mut sqlx::PgConnection) -> Result<Vec<DocumentTypeSummary>> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, display_name, category::TEXT, description,
                   chunking_strategy::TEXT, tree_sitter_language,
                   extraction_strategy::TEXT, requires_attachment,
                   is_system, is_active
            FROM public.document_type
            WHERE tenant_id = current_setting('app.current_tenant')::uuid AND is_active = TRUE
            ORDER BY category, name
            "#,
        )
        .fetch_all(&mut *conn)
        .await
        .map_err(Error::Database)?;

        Ok(rows
            .into_iter()
            .map(|row| DocumentTypeSummary {
                id: row.get("id"),
                name: row.get("name"),
                display_name: row.get("display_name"),
                category: Self::parse_category(row.get("category")),
                description: row.get("description"),
                chunking_strategy: Self::parse_chunking_strategy(row.get("chunking_strategy")),
                tree_sitter_language: row.get("tree_sitter_language"),
                extraction_strategy: Self::parse_extraction_strategy(
                    row.get::<Option<&str>, _>("extraction_strategy"),
                ),
                requires_attachment: row
                    .get::<Option<bool>, _>("requires_attachment")
                    .unwrap_or(false),
                is_system: row.get("is_system"),
                is_active: row.get("is_active"),
            })
            .collect())
    }

    pub async fn list_by_category_tx(
        conn: &mut sqlx::PgConnection,
        category: &str,
    ) -> Result<Vec<DocumentTypeSummary>> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, display_name, category::TEXT, description,
                   chunking_strategy::TEXT, tree_sitter_language,
                   extraction_strategy::TEXT, requires_attachment,
                   is_system, is_active
            FROM public.document_type
            WHERE tenant_id = current_setting('app.current_tenant')::uuid AND is_active = TRUE AND category::TEXT = $1
            ORDER BY name
            "#,
        )
        .bind(category)
        .fetch_all(&mut *conn)
        .await
        .map_err(Error::Database)?;

        Ok(rows
            .into_iter()
            .map(|row| DocumentTypeSummary {
                id: row.get("id"),
                name: row.get("name"),
                display_name: row.get("display_name"),
                category: Self::parse_category(row.get("category")),
                description: row.get("description"),
                chunking_strategy: Self::parse_chunking_strategy(row.get("chunking_strategy")),
                tree_sitter_language: row.get("tree_sitter_language"),
                extraction_strategy: Self::parse_extraction_strategy(
                    row.get::<Option<&str>, _>("extraction_strategy"),
                ),
                requires_attachment: row
                    .get::<Option<bool>, _>("requires_attachment")
                    .unwrap_or(false),
                is_system: row.get("is_system"),
                is_active: row.get("is_active"),
            })
            .collect())
    }

    pub async fn get_tx(conn: &mut sqlx::PgConnection, id: Uuid) -> Result<Option<DocumentType>> {
        let row = sqlx::query(
            r#"
            SELECT id, name, display_name, category::TEXT, description,
                   file_extensions, mime_types, magic_patterns, filename_patterns,
                   chunking_strategy::TEXT, chunk_size_default, chunk_overlap_default,
                   preserve_boundaries, chunking_config, recommended_config_id,
                   content_types, tree_sitter_language,
                   extraction_strategy::TEXT, extraction_config, requires_attachment, attachment_generates_content,
                   is_system, is_active,
                   created_at, updated_at, created_by, agentic_config
            FROM public.document_type
            WHERE tenant_id = current_setting('app.current_tenant')::uuid AND id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(Error::Database)?;

        Ok(row.map(|r| Self::row_to_document_type(&r)))
    }

    pub async fn get_by_name_tx(
        conn: &mut sqlx::PgConnection,
        name: &str,
    ) -> Result<Option<DocumentType>> {
        let row = sqlx::query(
            r#"
            SELECT id, name, display_name, category::TEXT, description,
                   file_extensions, mime_types, magic_patterns, filename_patterns,
                   chunking_strategy::TEXT, chunk_size_default, chunk_overlap_default,
                   preserve_boundaries, chunking_config, recommended_config_id,
                   content_types, tree_sitter_language,
                   extraction_strategy::TEXT, extraction_config, requires_attachment, attachment_generates_content,
                   is_system, is_active,
                   created_at, updated_at, created_by, agentic_config
            FROM public.document_type
            WHERE tenant_id = current_setting('app.current_tenant')::uuid AND name = $1
            "#,
        )
        .bind(name)
        .fetch_optional(&mut *conn)
        .await
        .map_err(Error::Database)?;

        Ok(row.map(|r| Self::row_to_document_type(&r)))
    }

    pub async fn create_tx(
        conn: &mut sqlx::PgConnection,
        req: CreateDocumentTypeRequest,
    ) -> Result<Uuid> {
        let id = new_v7();
        // Auto-generate display_name from name if not provided (kebab-case → Title Case)
        let display_name = req.display_name.unwrap_or_else(|| {
            req.name
                .split(['-', '_'])
                .map(|word| {
                    let mut chars = word.chars();
                    match chars.next() {
                        None => String::new(),
                        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        });

        sqlx::query(
            r#"
            INSERT INTO public.document_type (
                id, name, display_name, category, description,
                file_extensions, mime_types, magic_patterns, filename_patterns,
                chunking_strategy, chunk_size_default, chunk_overlap_default,
                preserve_boundaries, chunking_config, recommended_config_id,
                content_types, tree_sitter_language, is_system
            ) VALUES (
                $1, $2, $3, $4::document_category, $5,
                $6, $7, $8, $9,
                $10::chunking_strategy, $11, $12,
                $13, $14, $15,
                $16, $17, FALSE
            )
            "#,
        )
        .bind(id)
        .bind(&req.name)
        .bind(&display_name)
        .bind(req.category.to_string())
        .bind(&req.description)
        .bind(&req.file_extensions)
        .bind(&req.mime_types)
        .bind(&req.magic_patterns)
        .bind(&req.filename_patterns)
        .bind(req.chunking_strategy.to_string())
        .bind(req.chunk_size_default)
        .bind(req.chunk_overlap_default)
        .bind(req.preserve_boundaries)
        .bind(req.chunking_config.unwrap_or(serde_json::json!({})))
        .bind(req.recommended_config_id)
        .bind(&req.content_types)
        .bind(&req.tree_sitter_language)
        .execute(&mut *conn)
        .await
        .map_err(Error::Database)?;

        Ok(id)
    }

    pub async fn update_tx(
        conn: &mut sqlx::PgConnection,
        name: &str,
        req: UpdateDocumentTypeRequest,
    ) -> Result<()> {
        // First check if it's a system type
        let existing = Self::get_by_name_tx(conn, name).await?;
        if let Some(doc_type) = existing {
            if doc_type.is_system {
                return Err(Error::InvalidInput(
                    "Cannot modify system document type".into(),
                ));
            }
        } else {
            return Err(document_type_not_found_error(name));
        }

        // Use simpler approach with conditional updates
        sqlx::query(
            r#"
            UPDATE public.document_type SET
                display_name = COALESCE($2, display_name),
                description = COALESCE($3, description),
                file_extensions = COALESCE($4, file_extensions),
                chunking_strategy = COALESCE($5::chunking_strategy, chunking_strategy),
                chunk_size_default = COALESCE($6, chunk_size_default),
                chunk_overlap_default = COALESCE($7, chunk_overlap_default),
                is_active = COALESCE($8, is_active),
                updated_at = NOW()
            WHERE tenant_id = current_setting('app.current_tenant')::uuid AND name = $1 AND is_system = FALSE
            "#,
        )
        .bind(name)
        .bind(&req.display_name)
        .bind(&req.description)
        .bind(&req.file_extensions)
        .bind(req.chunking_strategy.map(|s| s.to_string()))
        .bind(req.chunk_size_default)
        .bind(req.chunk_overlap_default)
        .bind(req.is_active)
        .execute(&mut *conn)
        .await
        .map_err(Error::Database)?;

        Ok(())
    }

    pub async fn delete_tx(conn: &mut sqlx::PgConnection, name: &str) -> Result<()> {
        // Check if system type
        let existing = Self::get_by_name_tx(conn, name).await?;
        if let Some(doc_type) = existing {
            if doc_type.is_system {
                return Err(Error::InvalidInput(
                    "Cannot delete system document type".into(),
                ));
            }
        } else {
            return Err(document_type_not_found_error(name));
        }

        // Check if in use by notes
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM note WHERE tenant_id = current_setting('app.current_tenant')::uuid AND document_type_id = (SELECT id FROM public.document_type WHERE tenant_id = current_setting('app.current_tenant')::uuid AND name = $1)"
        )
        .bind(name)
        .fetch_one(&mut *conn)
        .await
        .map_err(Error::Database)?;

        if count > 0 {
            return Err(document_type_in_use_error(name, count));
        }

        sqlx::query("DELETE FROM public.document_type WHERE tenant_id = current_setting('app.current_tenant')::uuid AND name = $1 AND is_system = FALSE")
            .bind(name)
            .execute(&mut *conn)
            .await
            .map_err(Error::Database)?;

        Ok(())
    }
}
