use matric_core::{Error, Result, ServerEvent};
use sqlx::Row;
use uuid::Uuid;

use super::PgNoteRepository;
use crate::TenantScopedConn;

impl PgNoteRepository {
    /// Read-only metadata for a committed worker completion. The caller admits
    /// the claim's content schema and retains this transaction through settlement.
    /// Bounds reject oversized metadata rather than silently truncating an event.
    pub async fn updated_event_scoped(
        scope: &mut TenantScopedConn<'_>,
        note_id: Uuid,
    ) -> Result<Option<ServerEvent>> {
        let row = sqlx::query(
            r#"
            SELECT CASE WHEN octet_length(n.title) <= 8192 THEN n.title END AS title,
                   COALESCE(octet_length(n.title), 0) > 8192 AS title_oversized,
                   ARRAY(SELECT CASE WHEN octet_length(nt.tag_name) <= 1024
                                     THEN nt.tag_name ELSE NULL END
                           FROM note_tag nt WHERE nt.note_id = n.id
                           ORDER BY nt.tag_name LIMIT 1025) AS tags,
                   EXISTS(SELECT 1 FROM note_revised_current rc
                            JOIN note_revision r ON r.id = rc.last_revision_id
                             AND r.note_id = rc.note_id
                           WHERE rc.note_id = n.id AND r.ai_generated_at IS NOT NULL)
                     AS has_ai_content,
                   EXISTS(SELECT 1 FROM link l WHERE l.from_note_id = n.id) AS has_links
              FROM note n WHERE n.id = $1 AND n.deleted_at IS NULL
              FOR SHARE OF n
            "#,
        )
        .bind(note_id)
        .fetch_optional(scope.executor())
        .await
        .map_err(Error::Database)?;
        let Some(row) = row else { return Ok(None) };
        let tags: Vec<Option<String>> = row.get("tags");
        if row.get::<bool, _>("title_oversized")
            || tags.len() > 1024
            || tags.iter().any(Option::is_none)
            || tags.iter().flatten().map(String::len).sum::<usize>() > 65536
        {
            return Err(Error::InvalidInput(
                "Hosted note event metadata exceeds bounds".into(),
            ));
        }
        Ok(Some(ServerEvent::NoteUpdated {
            note_id,
            title: row.get("title"),
            tags: tags.into_iter().flatten().collect(),
            has_ai_content: row.get("has_ai_content"),
            has_links: row.get("has_links"),
        }))
    }
}
