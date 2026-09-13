//! Current-storage resolution on an already authorized tenant/archive connection.

use matric_core::{
    search_evidence::{
        EvidenceKind, EvidenceText, SearchEvidenceError, SearchEvidenceLocator, MAX_EVIDENCE_BYTES,
    },
    Error, Result,
};
use sqlx::{PgConnection, Row};
use uuid::Uuid;

use crate::{
    metadata_predicates::MetadataPredicateQueryBuilder,
    search_candidates::{bind_params, SearchCandidateScope},
    strict_filter::QueryParam,
};

fn unavailable() -> Error {
    Error::Search(SearchEvidenceError::Unavailable.to_string())
}

fn native_id(value: &str) -> Result<Uuid> {
    let id = Uuid::parse_str(value).map_err(|_| unavailable())?;
    if id.to_string() != value {
        return Err(unavailable());
    }
    Ok(id)
}

/// Scope narrows selection, never grants access. The caller must establish the
/// verified tenant and accessible archive before entering this function.
pub async fn resolve_on_connection(
    connection: &mut PgConnection,
    locator: &SearchEvidenceLocator,
    scope: &SearchCandidateScope,
) -> Result<String> {
    let note = native_id(locator.note_id())?;
    let unit = native_id(&locator.unit().id)?;
    let (cte, scope_sql, values) = scope.build(4);
    let mut params = vec![
        QueryParam::Uuid(note),
        QueryParam::Uuid(unit),
        QueryParam::Int(i32::try_from(locator.unit().index).map_err(|_| unavailable())?),
        QueryParam::Int(MAX_EVIDENCE_BYTES as i32),
    ];
    params.extend(values);
    let (joins, content, identity) = match locator.unit().kind {
        EvidenceKind::Current => (
            "JOIN note_revised_current c ON c.note_id=n.id AND c.tenant_id=n.tenant_id",
            "c.content",
            "n.id=$2 AND $3::integer=0",
        ),
        EvidenceKind::Title => ("", "n.title", "n.id=$2 AND $3::integer=0"),
        EvidenceKind::Embedding => (
            "JOIN embedding e ON e.note_id=n.id AND e.tenant_id=n.tenant_id",
            "e.text",
            "e.id=$2 AND e.chunk_index=$3::integer",
        ),
        EvidenceKind::Attachment => (
            "JOIN attachment a ON a.note_id=n.id AND a.tenant_id=n.tenant_id",
            "a.extracted_text",
            "a.id=$2 AND $3::integer=0 AND a.status='completed'",
        ),
    };
    let mut conditions = vec![
        "n.id=$1".to_owned(),
        identity.to_owned(),
        scope_sql,
        "n.tenant_id = nullif(current_setting('app.current_tenant', true), '')::uuid".to_owned(),
    ];
    if let Some(source) = locator.source() {
        let (source_sql, source_params) = scope
            .metadata
            .as_ref()
            .map(|metadata| {
                MetadataPredicateQueryBuilder::new(metadata, params.len()).source_projection()
            })
            .unwrap_or_else(|| {
                (
                    "si.note_id=n.id AND si.tenant_id=n.tenant_id AND si.import_run_id IS NOT NULL"
                        .into(),
                    vec![],
                )
            });
        params.extend(source_params);
        let first = params.len() + 1;
        params.extend(
            [
                source.namespace.clone(),
                source.external_id_hash.clone(),
                source.import_run_id.clone(),
                source.schema_version.clone(),
            ]
            .map(QueryParam::String),
        );
        conditions.push(format!("EXISTS (SELECT 1 FROM source_identity si WHERE {source_sql}
            AND si.source_namespace=${first} AND {}=${} AND si.import_run_id=${} AND si.source_schema_version=${})",
            crate::search_evidence_projection::SOURCE_IDENTITY_HASH_SQL, first+1, first+2, first+3));
    }
    // The statement snapshot checks current authorization, unit and source before
    // bounded text crosses the database boundary. Never truncate to make it fit.
    let sql = format!("{cte} SELECT CASE WHEN octet_length({content}) <= $4::integer THEN {content} ELSE NULL END AS content
        FROM note n {joins} WHERE {} LIMIT 1", conditions.join(" AND "));
    let row = bind_params(sqlx::query(&sql), params)
        .fetch_optional(connection)
        .await?;
    let content: Option<String> = row.map(|row| row.try_get("content")).transpose()?.flatten();
    locator
        .resolve(content.as_deref().map(|content| EvidenceText {
            note_id: locator.note_id(),
            unit: locator.unit(),
            content,
            source: locator.source(),
        }))
        .map(str::to_owned)
        .map_err(|_| unavailable())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_identity_is_exact_and_errors_are_bounded() {
        assert_eq!(
            native_id("00000000-0000-0000-0000-00000000000a").unwrap(),
            Uuid::from_u128(10)
        );
        for value in [
            "secret-private-key",
            "00000000-0000-0000-0000-00000000000A",
            "0000000000000000000000000000000a",
        ] {
            let error = native_id(value).unwrap_err();
            assert!(
                matches!(error, Error::Search(ref code) if code == "SEARCH_EVIDENCE_UNAVAILABLE")
            );
            assert!(!format!("{error:?}").contains(value));
        }
    }
}
