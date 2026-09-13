//! Bounded matched-unit projection in the authorized ranking statement snapshot.

use crate::{metadata_predicates::MetadataPredicateQueryBuilder, strict_filter::QueryParam};
use matric_core::{
    metadata_search::MetadataPredicates,
    search_evidence::{
        EvidenceOmission, SearchEvidenceLocator, SearchEvidenceSet, MAX_EVIDENCE_BYTES,
        MAX_SEARCH_LOCATORS,
    },
    Error, Result,
};
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

// source_upsert::source_identity_digest: each UTF-8 part is followed by NUL.
pub(crate) const SOURCE_IDENTITY_HASH_SQL: &str =
    "'sha256:' || encode(sha256(convert_to(si.tenant_id::text, 'UTF8') || decode('00', 'hex')
    || convert_to(current_schema()::text, 'UTF8') || decode('00', 'hex')
    || convert_to(si.source_namespace, 'UTF8') || decode('00', 'hex')
    || convert_to(si.external_id, 'UTF8') || decode('00', 'hex')), 'hex')";

pub(crate) fn projection(
    units: &str,
    metadata: Option<&MetadataPredicates>,
    offset: usize,
) -> (String, Vec<QueryParam>) {
    let (source, params) = metadata
        .map(|predicates| {
            MetadataPredicateQueryBuilder::new(predicates, offset).source_projection()
        })
        .unwrap_or_else(|| {
            (
                "si.note_id = n.id AND si.tenant_id = n.tenant_id AND si.import_run_id IS NOT NULL"
                    .into(),
                vec![],
            )
        });
    let extra = MAX_SEARCH_LOCATORS + 1;
    // This byte sequence is source_upsert::source_identity_digest: each UTF-8
    // tenant/schema/namespace/key part followed by NUL, never the raw key on wire.
    let sql = format!(
        r#"(WITH units(priority, kind, id, index, content) AS ({units}),
      sources AS (
        SELECT DISTINCT si.source_namespace COLLATE "C" AS namespace,
          {SOURCE_IDENTITY_HASH_SQL} AS external_id_hash,
          si.import_run_id COLLATE "C" AS import_run_id, si.source_schema_version COLLATE "C" AS source_schema_version
        FROM source_identity si WHERE {source}
        ORDER BY namespace, external_id_hash, import_run_id, source_schema_version LIMIT {extra}
      ), candidates AS MATERIALIZED (
        SELECT u.*, s.namespace, s.external_id_hash, s.import_run_id, s.source_schema_version
        FROM units u LEFT JOIN sources s ON true
        ORDER BY u.priority, u.id COLLATE "C", u.index, s.namespace COLLATE "C", s.external_id_hash,
          s.import_run_id COLLATE "C", s.source_schema_version COLLATE "C" LIMIT {extra}
      ), bound AS (
        SELECT CASE WHEN index BETWEEN 0 AND 2147483647 AND octet_length(content) <= {MAX_EVIDENCE_BYTES}
        THEN jsonb_build_object('version', '1.0.0', 'note_id', n.id,
          'unit', jsonb_build_object('kind', kind, 'id', id, 'index', index),
          'content_digest', 'sha256:' || encode(sha256(convert_to(content, 'UTF8')), 'hex'),
          'span', jsonb_build_object('unit', 'utf8-bytes', 'start', 0, 'end', octet_length(content)))
          || CASE WHEN namespace IS NULL THEN '{{}}'::jsonb ELSE jsonb_build_object('source',
            jsonb_build_object('namespace', namespace, 'external_id_hash', external_id_hash,
              'import_run_id', import_run_id, 'schema_version', source_schema_version)) END
        ELSE NULL END AS locator FROM candidates
      ) SELECT jsonb_build_object('locators', coalesce(jsonb_agg(locator), '[]'::jsonb),
        'limited', count(*) > {MAX_SEARCH_LOCATORS}) FROM bound)"#
    );
    (sql, params)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Projection {
    locators: Vec<Option<Value>>,
    limited: bool,
}

pub(crate) fn decode(note_id: Uuid, value: Value) -> Result<SearchEvidenceSet> {
    let invalid = || Error::Search("SEARCH_EVIDENCE_INVALID".into());
    let projected: Projection = serde_json::from_value(value).map_err(|_| invalid())?;
    if projected.locators.len() > MAX_SEARCH_LOCATORS + 1 {
        return Err(invalid());
    }
    let mut omissions = Vec::new();
    if projected.limited {
        omissions.push(EvidenceOmission::LocatorLimit);
    }
    if projected.locators.iter().any(Option::is_none) {
        omissions.push(EvidenceOmission::UnavailableUnit);
    }
    let locators = projected
        .locators
        .into_iter()
        .flatten()
        .map(SearchEvidenceLocator::try_from)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|_| invalid())?;
    SearchEvidenceSet::new(&note_id.to_string(), locators, omissions).map_err(|_| invalid())
}
