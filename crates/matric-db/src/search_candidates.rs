//! One candidate scope for transaction-bound lexical and vector retrieval.
//! The caller supplies the authorized tenant/archive connection; filters do not
//! grant access or substitute for RLS.

use matric_core::metadata_search::MetadataPredicates;
use matric_core::{Error, Result, SearchHit, StrictFilter, StrictTagFilter};
use pgvector::Vector;
use sqlx::{postgres::PgArguments, query::Query, PgConnection, Postgres, Row};
use uuid::Uuid;

use crate::{
    escape_like,
    metadata_predicates::MetadataPredicateQueryBuilder,
    strict_filter::{QueryParam, StrictFilterQueryBuilder},
    unified_filter::UnifiedFilterQueryBuilder,
};

#[derive(Clone, Default)]
pub struct SearchCandidateScope {
    pub metadata: Option<MetadataPredicates>,
    pub strict: Option<StrictTagFilter>,
    pub unified: Option<StrictFilter>,
    pub legacy_filters: String,
    pub embedding_set_id: Option<Uuid>,
    pub exclude_archived: bool,
}

#[derive(Clone, Copy, Debug)]
pub enum LexicalStrategy {
    English,
    Simple,
    Trigram,
    Bigram,
}

impl SearchCandidateScope {
    pub(crate) fn build(&self, offset: usize) -> (String, String, Vec<QueryParam>) {
        let mut clauses = vec!["n.deleted_at IS NULL".to_string()];
        let mut params = Vec::new();
        let mut cte = String::new();
        let strict = if let Some(unified) = &self.unified {
            unified.tags.as_ref()
        } else {
            self.strict.as_ref()
        };
        if self.exclude_archived {
            clauses.push("n.archived IS NOT TRUE".into());
        }
        if let Some(filter) = &self.unified {
            let mut filter = filter.clone();
            filter.tags = None;
            let built = UnifiedFilterQueryBuilder::new(filter, offset).build();
            if let Some(value) = built.cte_clause {
                cte = format!("WITH RECURSIVE {value} ");
            }
            clauses.push(built.where_clause);
            params.extend(built.params);
        }
        if let Some(filter) = strict {
            if filter.match_none {
                clauses.push("FALSE".into());
            } else {
                let (sql, values) =
                    StrictFilterQueryBuilder::new(filter.clone(), offset + params.len()).build();
                clauses.push(sql);
                params.extend(values);
            }
        }
        if let Some(metadata) = &self.metadata {
            let (sql, values) =
                MetadataPredicateQueryBuilder::new(metadata, offset + params.len()).build();
            clauses.push(sql);
            params.extend(values);
        }
        if let Some(id) = self.embedding_set_id {
            params.push(QueryParam::Uuid(id));
            clauses.push(format!("EXISTS (SELECT 1 FROM embedding_set_member esm WHERE esm.note_id = n.id AND esm.tenant_id = n.tenant_id AND esm.embedding_set_id = ${})", offset + params.len()));
        }
        // Preserve the established legacy filter grammar, but use it in both
        // retrieval modes before candidate limits instead of post-filtering.
        for token in self.legacy_filters.split_whitespace() {
            if let Some(tag) = token.strip_prefix("tag:") {
                params.push(QueryParam::String(tag.to_string()));
                let exact = offset + params.len();
                params.push(QueryParam::String(escape_like(tag)));
                clauses.push(format!("EXISTS (SELECT 1 FROM note_tag nt WHERE nt.note_id = n.id AND nt.tenant_id = n.tenant_id AND (LOWER(nt.tag_name) = LOWER(${exact}::text) OR LOWER(nt.tag_name) LIKE LOWER(${}::text) || '/%' ESCAPE '\\'))", offset + params.len()));
            } else if let Some(id) = token
                .strip_prefix("collection:")
                .and_then(|value| Uuid::parse_str(value).ok())
            {
                params.push(QueryParam::Uuid(id));
                clauses.push(format!("n.collection_id = ${}", offset + params.len()));
            } else {
                for (prefix, column, op) in [
                    ("created_after:", "created_at_utc", ">="),
                    ("created_before:", "created_at_utc", "<="),
                    ("updated_after:", "updated_at_utc", ">="),
                    ("updated_before:", "updated_at_utc", "<="),
                ] {
                    if let Some(ts) = token
                        .strip_prefix(prefix)
                        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
                    {
                        params.push(QueryParam::Timestamp(ts.with_timezone(&chrono::Utc)));
                        clauses.push(format!("n.{column} {op} ${}", offset + params.len()));
                    }
                }
            }
        }
        (
            cte,
            clauses
                .into_iter()
                .map(|c| format!("({c})"))
                .collect::<Vec<_>>()
                .join(" AND "),
            params,
        )
    }
}

pub(crate) fn bind_params<'q>(
    mut query: Query<'q, Postgres, PgArguments>,
    params: Vec<QueryParam>,
) -> Query<'q, Postgres, PgArguments> {
    for param in params {
        query = match param {
            QueryParam::Uuid(v) => query.bind(v),
            QueryParam::UuidArray(v) => query.bind(v),
            QueryParam::Int(v) => query.bind(v),
            QueryParam::Timestamp(v) => query.bind(v),
            QueryParam::Bool(v) => query.bind(v),
            QueryParam::String(v) => query.bind(v),
            QueryParam::StringArray(v) => query.bind(v),
        };
    }
    query
}

fn hits(rows: Vec<sqlx::postgres::PgRow>) -> Result<Vec<SearchHit>> {
    rows.into_iter()
        .map(|row| {
            let note_id = row.try_get("note_id")?;
            Ok(SearchHit {
                evidence: Some(crate::search_evidence_projection::decode(
                    note_id,
                    row.try_get("evidence")?,
                )?),
                note_id,
                score: row.try_get::<Option<f32>, _>("score")?.unwrap_or(0.0),
                snippet: row.try_get("snippet")?,
                title: row.try_get("title")?,
                tags: row.try_get("tags")?,
                embedding_status: None,
            })
        })
        .collect()
}

const TAGS: &str = "ARRAY(SELECT nt.tag_name FROM note_tag nt WHERE nt.note_id = n.id AND nt.tenant_id = n.tenant_id ORDER BY nt.tag_name)";

pub async fn lexical_on_connection(
    connection: &mut PgConnection,
    query: &str,
    limit: i64,
    mut strategy: LexicalStrategy,
    scope: &SearchCandidateScope,
) -> Result<Vec<SearchHit>> {
    if limit <= 0 {
        return Ok(Vec::new());
    }
    if matches!(strategy, LexicalStrategy::Bigram) {
        let available: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'pg_bigm')",
        )
        .fetch_one(&mut *connection)
        .await?;
        if !available {
            strategy = LexicalStrategy::Trigram;
        }
    }
    let (score, matching) = match strategy {
        LexicalStrategy::English => (
            "ts_rank(setweight(COALESCE(to_tsvector('public.matric_english', n.title), ''::tsvector), 'A') || setweight(COALESCE((SELECT to_tsvector('public.matric_english', string_agg(nt.tag_name, ' ')) FROM note_tag nt WHERE nt.note_id = n.id AND nt.tenant_id = n.tenant_id), ''::tsvector), 'B') || setweight(nrc.tsv, 'C'), websearch_to_tsquery('public.matric_english', $1), 32)",
            "nrc.tsv @@ websearch_to_tsquery('public.matric_english', $1) OR to_tsvector('public.matric_english', COALESCE(n.title, '')) @@ websearch_to_tsquery('public.matric_english', $1)"),
        LexicalStrategy::Simple => (
            "ts_rank(to_tsvector('public.matric_simple', COALESCE(n.title, '')) || to_tsvector('public.matric_simple', nrc.content), websearch_to_tsquery('public.matric_simple', $1))",
            "to_tsvector('public.matric_simple', nrc.content) @@ websearch_to_tsquery('public.matric_simple', $1) OR to_tsvector('public.matric_simple', COALESCE(n.title, '')) @@ websearch_to_tsquery('public.matric_simple', $1)"),
        LexicalStrategy::Trigram => (
            "GREATEST(similarity(nrc.content, $1), similarity(COALESCE(n.title, ''), $1))",
            "nrc.content % $1 OR nrc.content ILIKE '%' || $3 || '%' ESCAPE '\\' OR n.title ILIKE '%' || $3 || '%' ESCAPE '\\'"),
        LexicalStrategy::Bigram => (
            "GREATEST(bigm_similarity(nrc.content, $1), bigm_similarity(COALESCE(n.title, ''), $1))",
            "nrc.content LIKE likequery($1) OR n.title LIKE likequery($1)"),
    };
    let trigram = matches!(strategy, LexicalStrategy::Trigram);
    let offset = if trigram { 3 } else { 2 };
    let (cte, candidate, mut params) = scope.build(offset);
    let matches_unit = |text: &str| {
        match strategy {
        LexicalStrategy::English => format!("to_tsvector('public.matric_english', {text}) @@ websearch_to_tsquery('public.matric_english', $1)"),
        LexicalStrategy::Simple => format!("to_tsvector('public.matric_simple', {text}) @@ websearch_to_tsquery('public.matric_simple', $1)"),
        LexicalStrategy::Trigram => format!("{text} % $1 OR {text} ILIKE '%' || $3 || '%' ESCAPE '\\'"),
        LexicalStrategy::Bigram => format!("{text} LIKE likequery($1)"),
    }
    };
    let units = format!("SELECT 1, 'title'::text, n.id::text, 0, n.title WHERE {} UNION ALL SELECT 2, 'current', n.id::text, 0, nrc.content WHERE {} UNION ALL SELECT 3, 'attachment', a.id::text, 0, a.extracted_text FROM attachment a WHERE a.note_id = n.id AND a.tenant_id = n.tenant_id AND a.status = 'completed' AND ({})", matches_unit("n.title"), matches_unit("nrc.content"), matches_unit("a.extracted_text"));
    let attachment_match = matches_unit("a.extracted_text");
    let attachment_scope =
        "a.note_id = n.id AND a.tenant_id = n.tenant_id AND a.status = 'completed'";
    let matching = format!("({matching}) OR EXISTS (SELECT 1 FROM attachment a WHERE {attachment_scope} AND ({attachment_match}))");
    let attachment_score = match strategy {
        LexicalStrategy::English => "ts_rank(setweight(to_tsvector('public.matric_english', a.extracted_text), 'C'), websearch_to_tsquery('public.matric_english', $1), 32)",
        LexicalStrategy::Simple => "ts_rank(to_tsvector('public.matric_simple', a.extracted_text), websearch_to_tsquery('public.matric_simple', $1))",
        LexicalStrategy::Trigram => "similarity(a.extracted_text, $1)",
        LexicalStrategy::Bigram => "bigm_similarity(a.extracted_text, $1)",
    };
    let score = format!("GREATEST(({score})::real, COALESCE((SELECT max({attachment_score}) FROM attachment a WHERE {attachment_scope} AND ({attachment_match})), 0.0))");
    let (evidence, evidence_params) = crate::search_evidence_projection::projection(
        &units,
        scope.metadata.as_ref(),
        offset + params.len(),
    );
    params.extend(evidence_params);
    let sql = format!("{cte} SELECT n.id AS note_id, ({score})::real AS score, substring(nrc.content for 200) AS snippet, n.title, {TAGS} AS tags, {evidence} AS evidence FROM note n JOIN note_revised_current nrc ON nrc.note_id = n.id AND nrc.tenant_id = n.tenant_id WHERE ({candidate}) AND ({matching}) ORDER BY score DESC, n.id LIMIT $2");
    let mut statement = sqlx::query(&sql).bind(query).bind(limit);
    if trigram {
        statement = statement.bind(escape_like(query));
    }
    hits(
        bind_params(statement, params)
            .fetch_all(connection)
            .await
            .map_err(Error::Database)?,
    )
}

pub async fn vector_on_connection(
    connection: &mut PgConnection,
    vector: &Vector,
    limit: i64,
    scope: &SearchCandidateScope,
) -> Result<Vec<SearchHit>> {
    if limit <= 0 {
        return Ok(Vec::new());
    }
    let (cte, candidate, mut params) = scope.build(2);
    let set_clause = if let Some(id) = scope.embedding_set_id {
        params.push(QueryParam::Uuid(id));
        format!("AND e.embedding_set_id = ${}", 2 + params.len())
    } else {
        String::new()
    };
    let (evidence, evidence_params) = crate::search_evidence_projection::projection(
        "SELECT 0, 'embedding'::text, e.id::text, e.chunk_index, NULLIF(e.text, '')",
        scope.metadata.as_ref(),
        2 + params.len(),
    );
    params.extend(evidence_params);
    let sql = format!("{cte} SELECT ranked.*, {evidence} AS evidence FROM (SELECT DISTINCT ON (n.id) n.id AS note_id, e.id AS evidence_embedding_id, (1.0 - (e.vector <=> $1::vector))::real AS score, substring(COALESCE(noc.content, nrc.content) for 200) AS snippet, n.title, {TAGS} AS tags FROM note n JOIN embedding e ON e.note_id = n.id AND e.tenant_id = n.tenant_id LEFT JOIN note_original noc ON noc.note_id = n.id AND noc.tenant_id = n.tenant_id LEFT JOIN note_revised_current nrc ON nrc.note_id = n.id AND nrc.tenant_id = n.tenant_id WHERE ({candidate}) AND e.vector IS NOT NULL {set_clause} ORDER BY n.id, e.vector <=> $1::vector, e.id) ranked JOIN note n ON n.id = ranked.note_id JOIN embedding e ON e.id = ranked.evidence_embedding_id AND e.note_id = n.id AND e.tenant_id = n.tenant_id ORDER BY ranked.score DESC, ranked.note_id LIMIT $2");
    hits(
        bind_params(sqlx::query(&sql).bind(vector).bind(limit), params)
            .fetch_all(connection)
            .await
            .map_err(Error::Database)?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn combines_scope_and_binds_offsets_without_values_in_sql() {
        let scope = SearchCandidateScope {
            metadata: Some(
                MetadataPredicates::try_from(
                    serde_json::json!([{"path":"model","op":"eq","value":42}]),
                )
                .unwrap(),
            ),
            strict: Some(StrictTagFilter::new().require_concept(Uuid::new_v4())),
            legacy_filters: "tag:private% created_after:2026-01-01T00:00:00Z".into(),
            embedding_set_id: Some(Uuid::new_v4()),
            exclude_archived: true,
            ..Default::default()
        };
        let (_, sql, params) = scope.build(3);
        assert!(sql.contains("concept_id = $4"));
        assert!(sql.contains("n.metadata -> 'model'"));
        assert!(sql.contains("embedding_set_member"));
        assert!(sql.contains("deleted_at IS NULL"));
        assert!(sql.contains("archived IS NOT TRUE"));
        assert!(sql.contains("created_at_utc >= $9"));
        assert_eq!(params.len(), 6);
        assert!(!sql.contains("private%"));
        assert!(!sql.contains("LIMIT"));
    }
    #[test]
    fn unsatisfiable_strict_tags_do_not_drop_other_dimensions() {
        let mut strict = StrictTagFilter::new();
        strict.match_none = true;
        let (_, sql, _) = SearchCandidateScope {
            strict: Some(strict),
            embedding_set_id: Some(Uuid::new_v4()),
            ..Default::default()
        }
        .build(0);
        assert!(sql.contains("(FALSE)"));
        assert!(sql.contains("embedding_set_member"));
    }
}
