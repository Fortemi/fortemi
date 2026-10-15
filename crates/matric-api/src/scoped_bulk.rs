//! Bulk creation shares the atomic note/author/tag/queue write path.
use super::*;

pub(super) async fn create(
    state: &AppState,
    scope: TenantRequestScope,
    archive: ArchiveContext,
    body: BulkCreateNotesBody,
    #[cfg(feature = "hosted-auth")] principal: Option<RequestPrincipal>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let mut ids = Vec::with_capacity(body.notes.len());
    for item in body.notes {
        let revision_mode = match item.revision_mode.as_deref() {
            Some("full") => Some("contextual".into()),
            _ => item.revision_mode,
        };
        let (_, Json(created)) = create_note_hosted(
            state,
            scope.clone(),
            archive.clone(),
            CreateNoteBody {
                content: item.content,
                format: item.format,
                source: item.source.or_else(|| Some("api_bulk".into())),
                collection_id: item.collection_id,
                tags: item.tags,
                title: item.title,
                revision_mode,
                metadata: item.metadata,
                document_type_id: item.document_type_id,
                document_type: item.document_type,
                model: None,
                chunk_max_chars: item.chunk_max_chars,
                chunk_overlap: item.chunk_overlap,
                pipeline: None,
            },
            #[cfg(feature = "hosted-auth")]
            principal.clone(),
        )
        .await?;
        ids.push(created["id"].clone());
    }
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({"count":ids.len(),"ids":ids})),
    ))
}
