//! One candidate scope for transaction-bound lexical and vector retrieval.
//! The caller supplies the authorized tenant/archive connection; filters do not
//! grant access or substitute for RLS.

use matric_core::metadata_search::MetadataPredicates;
use matric_core::{Error, Result, SearchHit, StrictFilter, StrictTagFilter};
use pgvector::Vector;
use sqlx::{postgres::PgArguments, query::Query, PgConnection, Postgres, Row};
use uuid::Uuid;

use crate::{
    embedding_storage_contract::{contract_for_set, default_contract},
    escape_like,
    fts_admission::{active_path, AdmissionPath, ADMISSION_CAP},
    metadata_predicates::MetadataPredicateQueryBuilder,
    strict_filter::{QueryParam, StrictFilterQueryBuilder},
    unified_filter::UnifiedFilterQueryBuilder,
    vector_ef_search::apply_ef_search,
};

#[derive(Clone, Default)]
pub struct SearchCandidateScope {
    pub metadata: Option<MetadataPredicates>,
    pub strict: Option<StrictTagFilter>,
    pub unified: Option<StrictFilter>,
    pub legacy_filters: String,
    pub embedding_set_id: Option<Uuid>,
    pub exclude_archived: bool,
    /// Explicit expression applied before limits in every retrieval mode.
    pub lexical_constraint: Option<(crate::search_query::Expression, LexicalStrategy)>,
    /// Lexical admission path. `None` uses the path selected at startup
    /// ([`crate::fts_admission::active_path`]); tests set it explicitly.
    pub admission: Option<AdmissionPath>,
    /// Function-path id cap (default [`ADMISSION_CAP`]). Exceeding it never
    /// truncates: the request is retried on the direct path.
    pub admission_cap: Option<i32>,
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
        if let Some((expression, strategy)) = &self.lexical_constraint {
            clauses.push(expression_predicate(
                expression,
                *strategy,
                self.admission.unwrap_or_else(active_path),
                self.admission_cap.unwrap_or(ADMISSION_CAP),
                offset,
                &mut params,
            ));
        }
        if self.exclude_archived {
            clauses.push("n.archived IS NOT TRUE".into());
        }
        if let Some(filter) = &self.unified {
            let mut filter = filter.clone();
            filter.tags = None;
            let built = UnifiedFilterQueryBuilder::new(filter, offset + params.len()).build();
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

fn analyzer(strategy: LexicalStrategy) -> &'static str {
    match strategy {
        LexicalStrategy::English => "public.matric_english",
        _ => "public.matric_simple",
    }
}

/// Normalize exactly once, retaining analyzer stopword/phrase/prefix semantics.
pub async fn normalize_query(
    connection: &mut PgConnection,
    query: &str,
    strategy: LexicalStrategy,
) -> Result<String> {
    Ok(
        sqlx::query_scalar("SELECT to_tsquery($1::regconfig, $2)::text")
            .bind(analyzer(strategy))
            .bind(query)
            .fetch_one(connection)
            .await?,
    )
}

/// Normalize leaves and remove analyzer stopwords before applying Boolean
/// constraints across units. An empty expression matches nothing, including NOT.
pub async fn normalize_expression(
    connection: &mut PgConnection,
    parsed: &crate::search_query::ParsedQuery,
    strategy: LexicalStrategy,
) -> Result<crate::search_query::Expression> {
    use crate::search_query::Expression;
    let Some(expression) = &parsed.expression else {
        return Ok(Expression::Term(String::new()));
    };
    let (sql, params) = expression.normalization_query();
    let mut query = sqlx::query_scalar(&sql).bind(analyzer(strategy));
    for param in params {
        query = query.bind(param);
    }
    let normalized: Vec<String> = query.fetch_all(connection).await?;
    Ok(expression
        .normalized(&mut normalized.into_iter())
        .unwrap_or(Expression::Term(String::new())))
}

/// Unique analyzed terms, excluding stopwords; used for minimum-should-match.
pub async fn query_terms(
    connection: &mut PgConnection,
    query: &str,
    strategy: LexicalStrategy,
) -> Result<Vec<String>> {
    Ok(
        sqlx::query_scalar("SELECT tsvector_to_array(to_tsvector($1::regconfig, $2))")
            .bind(analyzer(strategy))
            .bind(query)
            .fetch_one(connection)
            .await?,
    )
}

/// Row-correlated unit predicate. Only for bounded id lists (classification);
/// candidate selection must use the index-backed set builders below.
fn unit_predicate(query: &str, strategy: LexicalStrategy) -> String {
    let config = analyzer(strategy);
    let content = current_unit_match(strategy, "nrc", &format!("({query})"));
    format!("(to_tsvector('{config}', COALESCE(n.title, '')) @@ ({query}) OR {content} OR EXISTS (SELECT 1 FROM attachment a WHERE a.note_id = n.id AND a.tenant_id = n.tenant_id AND a.status = 'completed' AND to_tsvector('{config}', COALESCE(a.extracted_text, '')) @@ ({query})))")
}

/// Content-unit match for a joined `note_revised_current` alias. The English
/// analyzer is exactly the stored generated `tsv`
/// (`to_tsvector('public.matric_english', content)`, migrations/20260215200000),
/// so it is reused instead of re-parsing the content; other analyzers parse.
fn current_unit_match(strategy: LexicalStrategy, alias: &str, tsquery: &str) -> String {
    match strategy {
        LexicalStrategy::English => format!("{alias}.tsv @@ {tsquery}"),
        _ => format!(
            "to_tsvector('public.matric_simple', COALESCE({alias}.content, '')) @@ {tsquery}"
        ),
    }
}

/// Function-backed `(tenant_id, note_id)` leaf set (RLS-safe admission). The
/// function enforces the current tenant and a registered schema itself; see
/// migration `20261120010000`. The schema is the caller's search_path head.
fn admission_function_set(query: &str, strategy: LexicalStrategy, cap: i32) -> String {
    format!(
        "(SELECT f.tenant_id, f.note_id FROM public.fts_admit_note_ids({query}, '{}', current_schema()::text, ARRAY['title','content','attachment']::text[], {cap}) f)",
        analyzer(strategy)
    )
}

/// Index-backed `(tenant_id, note_id)` set for one tsquery leaf. Each arm uses
/// an expression that exactly matches a GIN index:
/// - content: the stored `note_revised_current.tsv` (`public.matric_english`,
///   `idx_note_revised_tsv`) or `idx_note_revised_tsv_simple` (`public.matric_simple`)
/// - title: `idx_note_title_tsv_english` / `idx_note_title_tsv_simple`
/// - attachment: `idx_attachment_extracted_fts_english` / `_simple`
fn unit_id_set(query: &str, strategy: LexicalStrategy, path: AdmissionPath, cap: i32) -> String {
    if path == AdmissionPath::Function {
        return admission_function_set(query, strategy, cap);
    }
    let (content, title, attachment) = match strategy {
        LexicalStrategy::English => (
            format!("r.tsv @@ ({query})"),
            format!("to_tsvector('public.matric_english', COALESCE(t.title, '')) @@ ({query})"),
            format!(
                "to_tsvector('public.matric_english', COALESCE(a.extracted_text, '')) @@ ({query})"
            ),
        ),
        _ => (
            format!("to_tsvector('public.matric_simple', r.content) @@ ({query})"),
            format!("to_tsvector('public.matric_simple', COALESCE(t.title, '')) @@ ({query})"),
            format!(
                "to_tsvector('public.matric_simple', COALESCE(a.extracted_text, '')) @@ ({query})"
            ),
        ),
    };
    format!("(SELECT t.tenant_id, t.id FROM note t WHERE {title} UNION SELECT r.tenant_id, r.note_id FROM note_revised_current r WHERE {content} UNION SELECT a.tenant_id, a.note_id FROM attachment a WHERE a.status = 'completed' AND {attachment})")
}

/// Compile a Boolean expression to a `(tenant_id, note_id)` set using set
/// algebra over index-backed leaf sets (AND = INTERSECT, OR = UNION,
/// NOT = all notes EXCEPT). Leaf semantics match the former per-note predicate:
/// a leaf matches when any unit (title, current content, attachment) matches.
fn expression_id_set(
    expression: &crate::search_query::Expression,
    strategy: LexicalStrategy,
    path: AdmissionPath,
    cap: i32,
    offset: usize,
    params: &mut Vec<QueryParam>,
) -> String {
    use crate::search_query::Expression;
    match expression {
        Expression::Term(term) => {
            params.push(QueryParam::String(term.clone()));
            unit_id_set(
                &format!("${}::tsquery", offset + params.len()),
                strategy,
                path,
                cap,
            )
        }
        Expression::Phrase(phrase) => {
            params.push(QueryParam::String(phrase.clone()));
            unit_id_set(
                &format!(
                    "phraseto_tsquery('{}', ${})",
                    analyzer(strategy),
                    offset + params.len()
                ),
                strategy,
                path,
                cap,
            )
        }
        Expression::And(a, b) => {
            let left = expression_id_set(a, strategy, path, cap, offset, params);
            let right = expression_id_set(b, strategy, path, cap, offset, params);
            format!("({left} INTERSECT {right})")
        }
        Expression::Or(a, b) => {
            let left = expression_id_set(a, strategy, path, cap, offset, params);
            let right = expression_id_set(b, strategy, path, cap, offset, params);
            format!("({left} UNION {right})")
        }
        Expression::Not(a) => {
            let inner = expression_id_set(a, strategy, path, cap, offset, params);
            format!("((SELECT u.tenant_id, u.id FROM note u) EXCEPT {inner})")
        }
    }
}

fn expression_predicate(
    expression: &crate::search_query::Expression,
    strategy: LexicalStrategy,
    path: AdmissionPath,
    cap: i32,
    offset: usize,
    params: &mut Vec<QueryParam>,
) -> String {
    let set = expression_id_set(expression, strategy, path, cap, offset, params);
    match path {
        // The id list is an uncorrelated InitPlan, so the outer statement uses
        // the note primary key (uuid_eq is leakproof) while RLS stays on as a
        // second tenant check.
        AdmissionPath::Function => {
            format!("n.id = ANY(ARRAY(SELECT s.note_id FROM {set} AS s(tenant_id, note_id)))")
        }
        AdmissionPath::Direct => format!("(n.tenant_id, n.id) IN {set}"),
    }
}

/// Classify a bounded candidate union using the same unit-local admission as FTS.
/// Returns full-match IDs and counts of distinct analyzed terms for each note.
pub async fn classify_lexical_matches(
    connection: &mut PgConnection,
    ids: &[Uuid],
    strict: &str,
    terms: &[String],
    strategy: LexicalStrategy,
) -> Result<Vec<(Uuid, bool, i64)>> {
    let full = unit_predicate("$2::tsquery", strategy);
    let term = unit_predicate("term::tsquery", strategy);
    let sql = format!("SELECT n.id, ({full}) AS full_match, (SELECT count(*) FROM unnest($3::text[]) term WHERE {term}) AS matched FROM note n JOIN note_revised_current nrc ON nrc.note_id = n.id AND nrc.tenant_id = n.tenant_id WHERE n.id = ANY($1)");
    Ok(sqlx::query_as(&sql)
        .bind(ids)
        .bind(strict)
        .bind(terms)
        .fetch_all(connection)
        .await?)
}

pub async fn lexical_on_connection(
    connection: &mut PgConnection,
    query: &str,
    limit: i64,
    strategy: LexicalStrategy,
    scope: &SearchCandidateScope,
) -> Result<Vec<SearchHit>> {
    let parsed = crate::search_query::ParsedQuery::parse(query)?;
    let mut scope = scope.clone();
    let strategy = if parsed.explicit
        && !matches!(strategy, LexicalStrategy::English | LexicalStrategy::Simple)
    {
        LexicalStrategy::Simple
    } else {
        strategy
    };
    let compiled = if parsed.explicit {
        let expression = normalize_expression(connection, &parsed, strategy).await?;
        let compiled = expression.tsquery()?;
        scope.lexical_constraint = Some((expression, strategy));
        compiled
    } else if matches!(strategy, LexicalStrategy::English | LexicalStrategy::Simple) {
        normalize_query(connection, &parsed.tsquery()?, strategy).await?
    } else {
        query.to_string()
    };
    lexical_compiled_on_connection(connection, &compiled, limit, strategy, &scope).await
}

/// Build the lexical candidate statement: `(sql, scope params, trigram)`.
async fn prepare_lexical(
    connection: &mut PgConnection,
    mut strategy: LexicalStrategy,
    scope: &SearchCandidateScope,
) -> Result<(String, Vec<QueryParam>, bool)> {
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
    let score = match strategy {
        LexicalStrategy::English => "ts_rank(setweight(COALESCE(to_tsvector('public.matric_english', n.title), ''::tsvector), 'A') || setweight(COALESCE((SELECT to_tsvector('public.matric_english', string_agg(nt.tag_name, ' ')) FROM note_tag nt WHERE nt.note_id = n.id AND nt.tenant_id = n.tenant_id), ''::tsvector), 'B') || setweight(nrc.tsv, 'C'), $1::tsquery, 32)",
        LexicalStrategy::Simple => "ts_rank(to_tsvector('public.matric_simple', COALESCE(n.title, '')) || to_tsvector('public.matric_simple', nrc.content), $1::tsquery)",
        LexicalStrategy::Trigram => "GREATEST(similarity(nrc.content, $1), similarity(COALESCE(n.title, ''), $1))",
        LexicalStrategy::Bigram => "GREATEST(bigm_similarity(nrc.content, $1), bigm_similarity(COALESCE(n.title, ''), $1))",
    };
    let trigram = matches!(strategy, LexicalStrategy::Trigram);
    let offset = if trigram { 3 } else { 2 };
    let (cte, candidate, mut params) = scope.build(offset);
    let matches_unit = |text: &str| match strategy {
        LexicalStrategy::English => {
            format!("to_tsvector('public.matric_english', {text}) @@ $1::tsquery")
        }
        LexicalStrategy::Simple => {
            format!("to_tsvector('public.matric_simple', {text}) @@ $1::tsquery")
        }
        LexicalStrategy::Trigram => {
            format!("{text} % $1 OR {text} ILIKE '%' || $3 || '%' ESCAPE '\\'")
        }
        LexicalStrategy::Bigram => format!("{text} LIKE likequery($1)"),
    };
    // The English content unit reuses the stored `nrc.tsv` instead of re-parsing.
    let current_match = match strategy {
        LexicalStrategy::English => current_unit_match(strategy, "nrc", "$1::tsquery"),
        _ => matches_unit("nrc.content"),
    };
    let units = format!("SELECT 1, 'title'::text, n.id::text, 0, n.title WHERE {} UNION ALL SELECT 2, 'current', n.id::text, 0, nrc.content WHERE {} UNION ALL SELECT 3, 'attachment', a.id::text, 0, a.extracted_text FROM attachment a WHERE a.note_id = n.id AND a.tenant_id = n.tenant_id AND a.status = 'completed' AND ({})", matches_unit("n.title"), current_match, matches_unit("a.extracted_text"));
    let attachment_match = matches_unit("a.extracted_text");
    let attachment_scope =
        "a.note_id = n.id AND a.tenant_id = n.tenant_id AND a.status = 'completed'";
    // Candidate admission: union of per-table index-backed id sets keyed by
    // (tenant_id, note_id); only that set is joined, ranked and projected.
    let path = scope.admission.unwrap_or_else(active_path);
    let matching = if scope.lexical_constraint.is_some() {
        "TRUE".to_string()
    } else if path == AdmissionPath::Function
        && matches!(strategy, LexicalStrategy::English | LexicalStrategy::Simple)
    {
        // RLS-safe admission: ids come from the SECURITY DEFINER function
        // (tenant and schema enforced inside); the outer statement keeps RLS on
        // and filters on the leakproof `uuid = ANY(...)`.
        let set = admission_function_set(
            "$1::tsquery",
            strategy,
            scope.admission_cap.unwrap_or(ADMISSION_CAP),
        );
        format!("n.id = ANY(ARRAY(SELECT s.note_id FROM {set} AS s(tenant_id, note_id)))")
    } else {
        let (title_arm, content_arm, attachment_arm) = match strategy {
            LexicalStrategy::English => (
                matches_unit("COALESCE(t.title, '')"),
                "r.tsv @@ $1::tsquery".to_string(),
                matches_unit("COALESCE(a.extracted_text, '')"),
            ),
            LexicalStrategy::Simple => (
                matches_unit("COALESCE(t.title, '')"),
                matches_unit("r.content"),
                matches_unit("COALESCE(a.extracted_text, '')"),
            ),
            LexicalStrategy::Trigram | LexicalStrategy::Bigram => (
                matches_unit("t.title"),
                matches_unit("r.content"),
                matches_unit("a.extracted_text"),
            ),
        };
        format!("(n.tenant_id, n.id) IN (SELECT t.tenant_id, t.id FROM note t WHERE {title_arm} UNION SELECT r.tenant_id, r.note_id FROM note_revised_current r WHERE {content_arm} UNION SELECT a.tenant_id, a.note_id FROM attachment a WHERE a.status = 'completed' AND ({attachment_arm}))")
    };
    let attachment_score = match strategy {
        LexicalStrategy::English => "ts_rank(setweight(to_tsvector('public.matric_english', a.extracted_text), 'C'), $1::tsquery, 32)",
        LexicalStrategy::Simple => "ts_rank(to_tsvector('public.matric_simple', a.extracted_text), $1::tsquery)",
        LexicalStrategy::Trigram => "similarity(a.extracted_text, $1)",
        LexicalStrategy::Bigram => "bigm_similarity(a.extracted_text, $1)",
    };
    // Parse each attachment once per row (LATERAL, OFFSET 0 blocks pull-up so
    // the filter and the rank share one tsvector) for the tsvector analyzers.
    let attachment_rank = match strategy {
        LexicalStrategy::English => format!("SELECT max(ts_rank(setweight(v.tsv, 'C'), $1::tsquery, 32)) FROM attachment a CROSS JOIN LATERAL (SELECT to_tsvector('public.matric_english', a.extracted_text) AS tsv OFFSET 0) v WHERE {attachment_scope} AND v.tsv @@ $1::tsquery"),
        LexicalStrategy::Simple => format!("SELECT max(ts_rank(v.tsv, $1::tsquery)) FROM attachment a CROSS JOIN LATERAL (SELECT to_tsvector('public.matric_simple', a.extracted_text) AS tsv OFFSET 0) v WHERE {attachment_scope} AND v.tsv @@ $1::tsquery"),
        _ => format!("SELECT max({attachment_score}) FROM attachment a WHERE {attachment_scope} AND ({attachment_match})"),
    };
    let score = format!("GREATEST(({score})::real, COALESCE(({attachment_rank}), 0.0))");
    let (evidence, evidence_params) = crate::search_evidence_projection::projection(
        &units,
        scope.metadata.as_ref(),
        offset + params.len(),
    );
    params.extend(evidence_params);
    let sql = format!("{cte} SELECT n.id AS note_id, ({score})::real AS score, substring(nrc.content for 200) AS snippet, n.title, {TAGS} AS tags, {evidence} AS evidence FROM note n JOIN note_revised_current nrc ON nrc.note_id = n.id AND nrc.tenant_id = n.tenant_id WHERE ({candidate}) AND ({matching}) ORDER BY score DESC, n.id LIMIT $2");
    Ok((sql, params, trigram))
}

fn bind_lexical<'q>(
    sql: &'q str,
    query: &str,
    limit: i64,
    trigram: bool,
    params: Vec<QueryParam>,
) -> Query<'q, Postgres, PgArguments> {
    let mut statement = sqlx::query(sql).bind(query.to_string()).bind(limit);
    if trigram {
        statement = statement.bind(escape_like(query));
    }
    bind_params(statement, params)
}

/// True for the function's "admission cap exceeded" error (SQLSTATE 54000).
pub(crate) fn is_admission_cap_exceeded(error: &sqlx::Error) -> bool {
    matches!(error, sqlx::Error::Database(db)
        if db.code().as_deref() == Some("54000") && db.message().contains("admission cap exceeded"))
}

/// True when the admission function itself is missing or not executable by this
/// role: "function public.fts_admit_note_ids(...) does not exist" (42883) or
/// "permission denied for function fts_admit_note_ids" (42501). An unrelated
/// undefined function/operator or an unrelated function's permission error, and
/// the function's own tenant/schema refusals, are deliberately not matched.
#[doc(hidden)]
pub fn is_admission_function_unavailable(error: &sqlx::Error) -> bool {
    crate::fts_admission::classify_error(error) == crate::fts_admission::ErrorClass::Definitive
}

/// Cancellation, deadlock, resource or connection errors (cap overflow has its
/// own handling): they say nothing about the function, so the request is served
/// on the direct path and global state is kept.
#[doc(hidden)]
pub fn is_admission_inconclusive(error: &sqlx::Error) -> bool {
    crate::fts_admission::classify_error(error) == crate::fts_admission::ErrorClass::Inconclusive
}

/// An insufficient-privilege error that did not come from the function's own
/// checks (those start with `fts_admit_note_ids:`) and is not a definitive
/// missing-EXECUTE error, e.g. the owner lost BYPASSRLS ("query would be
/// affected by row-level security policy").
#[doc(hidden)]
pub fn is_admission_function_misconfigured(error: &sqlx::Error) -> bool {
    matches!(error, sqlx::Error::Database(db)
        if db.code().as_deref() == Some("42501")
            && !db.message().starts_with("fts_admit_note_ids:")
            && !is_admission_function_unavailable(error))
}

pub(crate) fn warn_admission_function_misconfigured(error: &sqlx::Error) {
    tracing::warn!(
        sqlstate = "42501",
        "an insufficient_privilege error occurred on the public.fts_admit_note_ids path; \
         if its owner lost BYPASSRLS or cannot read a searched schema, searches on the \
         function path fail closed until this is repaired and the service restarted: {}",
        match error {
            sqlx::Error::Database(db) => db.message().chars().take(160).collect::<String>(),
            other => other.to_string(),
        }
    );
}

/// After the function path reported "unavailable", re-probe on this request's
/// connection. Only a definitive probe failure switches the process to the
/// direct statements (recoverable after the re-probe interval); a passing or
/// inconclusive probe keeps the global state and only this request uses the
/// direct path.
pub(crate) async fn confirm_function_unavailable(connection: &mut PgConnection) {
    use crate::fts_admission::ProbeOutcome;
    match crate::fts_admission::probe_on_connection(connection).await {
        ProbeOutcome::Definitive => {
            crate::fts_admission::note_downgrade();
            tracing::warn!(
                "admission function unavailable (confirmed by re-probe); switching to direct \
                 statements, re-probing periodically"
            );
        }
        ProbeOutcome::Works | ProbeOutcome::Inconclusive => tracing::warn!(
            "admission function reported unavailable but the re-probe did not confirm it; \
             using direct statements for this request only"
        ),
    }
}

/// `calls_function` says whether the statement `run` generates can invoke
/// `fts_admit_note_ids` (a lexical English/Simple admission, or a vector search
/// with a lexical constraint); otherwise `run` executes once, untouched.
///
/// Run `run` on the function admission path inside a savepoint; if the function
/// reports that its cap was exceeded, roll the savepoint back and run it again
/// on the direct path so the result is never silently truncated.
#[doc(hidden)]
pub async fn with_admission_fallback<T, F>(
    connection: &mut PgConnection,
    scope: &SearchCandidateScope,
    calls_function: bool,
    mut run: F,
) -> Result<T>
where
    F: for<'c> FnMut(
        &'c mut PgConnection,
        SearchCandidateScope,
    ) -> futures::future::BoxFuture<'c, Result<T>>,
{
    if !calls_function {
        // The generated SQL cannot call the admission function (semantic-only,
        // trigram/bigram): no timeout, savepoint, recovery probe or retry.
        let mut direct = scope.clone();
        direct.admission = Some(AdmissionPath::Direct);
        return run(connection, direct).await;
    }
    if scope.admission.is_none() {
        crate::fts_admission::maybe_recover(connection).await;
    }
    let uses_function = scope.admission.unwrap_or_else(active_path) == AdmissionPath::Function;
    if uses_function {
        let mut attempt = scope.clone();
        attempt.admission = Some(AdmissionPath::Function);
        let mut savepoint = sqlx::Connection::begin(&mut *connection)
            .await
            .map_err(Error::Database)?;
        let previous_timeout = crate::fts_admission::bound_statement_timeout(&mut savepoint).await;
        match run(&mut savepoint, attempt).await {
            Ok(value) => {
                crate::fts_admission::restore_statement_timeout(&mut savepoint, previous_timeout)
                    .await;
                savepoint.commit().await.map_err(Error::Database)?;
                return Ok(value);
            }
            Err(Error::Database(error)) if is_admission_cap_exceeded(&error) => {
                savepoint.rollback().await.map_err(Error::Database)?;
                tracing::warn!(
                    "lexical admission cap exceeded; retrying this request on the direct path"
                );
            }
            Err(Error::Database(error)) if is_admission_function_unavailable(&error) => {
                savepoint.rollback().await.map_err(Error::Database)?;
                confirm_function_unavailable(connection).await;
            }
            Err(Error::Database(error)) if is_admission_inconclusive(&error) => {
                savepoint.rollback().await.map_err(Error::Database)?;
                tracing::warn!(
                    "admission function path failed inconclusively; using direct statements \
                     for this request only"
                );
            }
            Err(Error::Database(error)) if is_admission_function_misconfigured(&error) => {
                warn_admission_function_misconfigured(&error);
                return Err(Error::Database(error));
            }
            Err(error) => return Err(error),
        }
    }
    let mut direct = scope.clone();
    direct.admission = Some(AdmissionPath::Direct);
    run(connection, direct).await
}

/// Execute an analyzer-normalized tsquery, or raw trigram/bigram text.
pub async fn lexical_compiled_on_connection(
    connection: &mut PgConnection,
    query: &str,
    limit: i64,
    strategy: LexicalStrategy,
    scope: &SearchCandidateScope,
) -> Result<Vec<SearchHit>> {
    if limit <= 0 {
        return Ok(Vec::new());
    }
    let query = query.to_string();
    // Only English/Simple admission can call the function; trigram/bigram never do.
    let calls_function = matches!(strategy, LexicalStrategy::English | LexicalStrategy::Simple);
    with_admission_fallback(
        connection,
        scope,
        calls_function,
        move |connection, scope| {
            let query = query.clone();
            Box::pin(async move {
                let (sql, params, trigram) = prepare_lexical(connection, strategy, &scope).await?;
                hits(
                    bind_lexical(&sql, &query, limit, trigram, params)
                        .fetch_all(connection)
                        .await
                        .map_err(Error::Database)?,
                )
            })
        },
    )
    .await
}

/// The exact SQL text (placeholders intact) that
/// [`lexical_compiled_on_connection`] issues for `strategy` and `scope`:
/// `$1` tsquery text, `$2` limit (plus `$3` LIKE text for trigram).
/// Diagnostic use, e.g. to run `EXPLAIN` against a production-shaped database.
pub async fn lexical_candidate_sql(
    connection: &mut PgConnection,
    strategy: LexicalStrategy,
    scope: &SearchCandidateScope,
) -> Result<String> {
    Ok(prepare_lexical(connection, strategy, scope).await?.0)
}

/// `EXPLAIN (FORMAT JSON)` of the exact lexical candidate statement that
/// [`lexical_compiled_on_connection`] executes. Diagnostic and plan-regression
/// use only; it plans but does not run the statement.
pub async fn explain_lexical_compiled(
    connection: &mut PgConnection,
    query: &str,
    limit: i64,
    strategy: LexicalStrategy,
    scope: &SearchCandidateScope,
) -> Result<serde_json::Value> {
    let (sql, params, trigram) = prepare_lexical(connection, strategy, scope).await?;
    let explain = format!("EXPLAIN (FORMAT JSON) {sql}");
    let row = bind_lexical(&explain, query, limit, trigram, params)
        .fetch_one(connection)
        .await
        .map_err(Error::Database)?;
    Ok(row.try_get::<serde_json::Value, _>(0)?)
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
    let contract = match scope.embedding_set_id {
        Some(id) => contract_for_set(&mut *connection, id).await?,
        None => default_contract(&mut *connection, vector.as_slice().len()).await?,
    };
    contract.validate_vector(vector)?;
    let scoped_set_id = scope.embedding_set_id.unwrap_or(contract.embedding_set_id);
    apply_ef_search(connection, Some(scoped_set_id)).await?;
    let vector = vector.clone();
    // A vector search calls the function only through a lexical constraint.
    let calls_function = scope.lexical_constraint.is_some();
    with_admission_fallback(connection, scope, calls_function, move |connection, scope| {
        let vector = vector.clone();
        Box::pin(async move {
        let distance = contract.distance_expr("e.vector", "$1");
        // External sets store their searchable vectors as imported entity profiles.
        let (cte, candidate, mut params) = scope.build(2);
        params.push(QueryParam::Uuid(scoped_set_id));
        let set_clause = format!(
            "AND {}",
            contract.set_predicate("e", &format!("${}", 2 + params.len()))
        );
        let (evidence, evidence_params) = crate::search_evidence_projection::projection(
            "SELECT 0, 'embedding'::text, e.id::text, e.chunk_index, NULLIF(e.text, '')",
            scope.metadata.as_ref(),
            2 + params.len(),
        );
        params.extend(evidence_params);
        let sql = format!(
            "{cte} SELECT ranked.*, {evidence} AS evidence FROM (SELECT DISTINCT ON (n.id) n.id AS note_id, e.id AS evidence_embedding_id, (1.0 - ({distance}))::real AS score, substring(COALESCE(noc.content, nrc.content) for 200) AS snippet, n.title, {TAGS} AS tags FROM note n JOIN embedding e ON e.note_id = n.id AND e.tenant_id = n.tenant_id LEFT JOIN note_original noc ON noc.note_id = n.id AND noc.tenant_id = n.tenant_id LEFT JOIN note_revised_current nrc ON nrc.note_id = n.id AND nrc.tenant_id = n.tenant_id WHERE ({candidate}) AND e.vector IS NOT NULL {set_clause} ORDER BY n.id, {distance}, e.id) ranked JOIN note n ON n.id = ranked.note_id JOIN embedding e ON e.id = ranked.evidence_embedding_id AND e.note_id = n.id AND e.tenant_id = n.tenant_id ORDER BY ranked.score DESC, ranked.note_id LIMIT $2"
        );
        hits(
            bind_params(sqlx::query(&sql).bind(&vector).bind(limit), params)
                .fetch_all(connection)
                .await
                .map_err(Error::Database)?,
        )
        })
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_explicit_unified_filter_offsets(offset: usize) {
        use matric_core::{MetadataFilter, StrictCollectionFilter, StrictTemporalFilter};
        use std::collections::BTreeMap;

        let collection = StrictFilter::new()
            .with_collections(StrictCollectionFilter::new().in_collection(Uuid::from_u128(1)));
        let recursive = StrictFilter::new().with_collections(
            StrictCollectionFilter::new()
                .in_collection(Uuid::from_u128(1))
                .with_descendants(true),
        );
        let temporal = StrictFilter::new().with_temporal(
            StrictTemporalFilter::new().updated_after("2026-01-01T00:00:00Z".parse().unwrap()),
        );
        let metadata = StrictFilter::new()
            .with_metadata(MetadataFilter::new().with_format("markdown").starred_only());
        let combined = StrictFilter {
            collections: recursive.collections.clone(),
            temporal: temporal.temporal.clone(),
            metadata: metadata.metadata.clone(),
            ..Default::default()
        };
        let placeholders = regex::Regex::new(r"\$(\d+)").unwrap();
        for (filter, filter_params) in [
            (collection, 1),
            (recursive, 1),
            (temporal, 1),
            (metadata, 2),
            (combined, 4),
        ] {
            let expression = crate::search_query::ParsedQuery::parse("alpha AND beta")
                .unwrap()
                .expression
                .unwrap();
            let (cte, sql, params) = SearchCandidateScope {
                lexical_constraint: Some((expression, LexicalStrategy::English)),
                unified: Some(filter),
                ..Default::default()
            }
            .build(offset);
            assert_eq!(params.len(), 2 + filter_params);
            let mut counts = BTreeMap::new();
            for capture in placeholders.captures_iter(&format!("{cte}{sql}")) {
                *counts
                    .entry(capture[1].parse::<usize>().unwrap())
                    .or_insert(0) += 1;
            }
            assert_eq!(
                counts.keys().copied().collect::<Vec<_>>(),
                (offset + 1..=offset + params.len()).collect::<Vec<_>>()
            );
            // Each expression parameter is intentionally reused across the three
            // searchable units. Unified filters must use their own unique slots.
            assert_eq!(counts[&(offset + 1)], 3);
            assert_eq!(counts[&(offset + 2)], 3);
            for index in offset + 3..=offset + params.len() {
                assert_eq!(counts[&index], 1, "filter placeholder ${index} reused");
            }
        }
    }

    #[test]
    fn lexical_explicit_syntax_and_unified_filters_have_unique_contiguous_params() {
        // Lexical retrieval reserves query and limit (plus escaped LIKE text for trigram).
        assert_explicit_unified_filter_offsets(2);
        assert_explicit_unified_filter_offsets(3);
    }

    #[test]
    fn semantic_explicit_syntax_and_unified_filters_have_unique_contiguous_params() {
        // Semantic retrieval reserves vector and limit.
        assert_explicit_unified_filter_offsets(2);
    }

    #[test]
    fn hybrid_explicit_syntax_and_unified_filters_have_unique_contiguous_params() {
        // Both queries in hybrid retrieval reserve query/vector and limit.
        assert_explicit_unified_filter_offsets(2);
    }

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
