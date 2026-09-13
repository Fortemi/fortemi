//! Parameterized candidate predicates for #1091, evaluated against note alias `n`.
//!
//! The caller supplies its authorized schema/tenant context and applies this
//! clause before ranking and LIMIT. Source identities stay in that same schema.

use matric_core::metadata_search::{MetadataPath, MetadataPredicate, MetadataPredicates};
use serde_json::Value;

use crate::strict_filter::QueryParam;

pub struct MetadataPredicateQueryBuilder<'a> {
    predicates: &'a MetadataPredicates,
    param_offset: usize,
}

impl<'a> MetadataPredicateQueryBuilder<'a> {
    pub fn new(predicates: &'a MetadataPredicates, param_offset: usize) -> Self {
        Self {
            predicates,
            param_offset,
        }
    }

    pub fn build(self) -> (String, Vec<QueryParam>) {
        let mut params = Vec::new();
        let mut clauses = Vec::new();
        let mut source_clauses = Vec::new();
        let mut source_absent = false;
        for predicate in self.predicates.as_slice() {
            let path = match predicate {
                MetadataPredicate::Eq { path, .. }
                | MetadataPredicate::In { path, .. }
                | MetadataPredicate::Range { path, .. }
                | MetadataPredicate::Exists { path, .. } => *path,
            };
            if path == MetadataPath::ImportRunId {
                if matches!(predicate, MetadataPredicate::Exists { value: false, .. }) {
                    source_absent = true;
                } else {
                    source_clauses.push(compile_source(predicate, self.param_offset, &mut params));
                }
            } else {
                let expr = format!("(n.metadata -> '{}')", path.as_str());
                clauses.push(compile_metadata(
                    predicate,
                    &expr,
                    self.param_offset,
                    &mut params,
                ));
            }
        }

        // All positive source predicates must hold for one identity. Independent
        // EXISTS clauses could incorrectly satisfy a conjunction across identities.
        const IDENTITY_SCOPE: &str =
            "si.note_id = n.id AND si.tenant_id = n.tenant_id AND si.import_run_id IS NOT NULL";
        if !source_clauses.is_empty() {
            clauses.push(format!(
                "EXISTS (SELECT 1 FROM source_identity si WHERE {IDENTITY_SCOPE} AND {})",
                source_clauses.join(" AND ")
            ));
        }
        if source_absent {
            clauses.push(format!(
                "NOT EXISTS (SELECT 1 FROM source_identity si WHERE {IDENTITY_SCOPE})"
            ));
        }
        if clauses.is_empty() {
            ("TRUE".into(), params)
        } else {
            (format!("({})", clauses.join(" AND ")), params)
        }
    }

    /// Restrict projected source tuples to the same identity conjunction used
    /// for note eligibility, while keeping all request values parameterized.
    pub(crate) fn source_projection(self) -> (String, Vec<QueryParam>) {
        let mut params = Vec::new();
        let mut clauses = vec![
            "si.note_id = n.id AND si.tenant_id = n.tenant_id AND si.import_run_id IS NOT NULL"
                .to_string(),
        ];
        for predicate in self.predicates.as_slice() {
            let path = match predicate {
                MetadataPredicate::Eq { path, .. }
                | MetadataPredicate::In { path, .. }
                | MetadataPredicate::Range { path, .. }
                | MetadataPredicate::Exists { path, .. } => *path,
            };
            if path == MetadataPath::ImportRunId {
                if matches!(predicate, MetadataPredicate::Exists { value: false, .. }) {
                    clauses.push("FALSE".into());
                } else {
                    clauses.push(compile_source(predicate, self.param_offset, &mut params));
                }
            }
        }
        (clauses.join(" AND "), params)
    }
}

fn bind(value: &Value, offset: usize, params: &mut Vec<QueryParam>) -> String {
    params.push(QueryParam::String(value.to_string()));
    format!("${}::jsonb", offset + params.len())
}

fn comparison(
    expr: &str,
    value: &Value,
    op: &str,
    offset: usize,
    params: &mut Vec<QueryParam>,
) -> String {
    if value.is_null() {
        return format!("jsonb_typeof({expr}) = 'null'");
    }
    let parameter = bind(value, offset, params);
    if value.is_string() {
        format!(
            "(jsonb_typeof({expr}) = 'string' \
             AND public.metadata_search_order_key_v1({expr}) IS NULL \
             AND public.metadata_search_text_key_v1({expr}) COLLATE \"C\" {op} \
                 public.metadata_search_text_key_v1({parameter}) COLLATE \"C\" \
             AND ({expr} #>> '{{}}') COLLATE \"C\" {op} ({parameter} #>> '{{}}') COLLATE \"C\")"
        )
    } else {
        let kind = if value.is_boolean() {
            "boolean"
        } else {
            "number"
        };
        format!(
            "(jsonb_typeof({expr}) = '{kind}' \
             AND public.metadata_search_order_key_v1({expr}) {op} \
                 public.metadata_search_order_key_v1({parameter}) \
             AND {expr} {op} {parameter})"
        )
    }
}

fn compile_metadata(
    predicate: &MetadataPredicate,
    expr: &str,
    offset: usize,
    params: &mut Vec<QueryParam>,
) -> String {
    match predicate {
        MetadataPredicate::Eq { value, .. } => comparison(expr, value, "=", offset, params),
        MetadataPredicate::In { value, .. } => {
            if value.is_empty() {
                "FALSE".into()
            } else {
                format!(
                    "({})",
                    value
                        .iter()
                        .map(|v| comparison(expr, v, "=", offset, params))
                        .collect::<Vec<_>>()
                        .join(" OR ")
                )
            }
        }
        MetadataPredicate::Range { gte, lte, .. } => {
            let mut clauses = Vec::new();
            if let Some(value) = gte {
                clauses.push(comparison(expr, value, ">=", offset, params));
            }
            if let Some(value) = lte {
                clauses.push(comparison(expr, value, "<=", offset, params));
            }
            format!("({})", clauses.join(" AND "))
        }
        MetadataPredicate::Exists { value, .. } => {
            format!(
                "jsonb_typeof({expr}) IS {}NULL",
                if *value { "NOT " } else { "" }
            )
        }
    }
}

fn compile_source(
    predicate: &MetadataPredicate,
    offset: usize,
    params: &mut Vec<QueryParam>,
) -> String {
    let compare = |value: &Value, op: &str, params: &mut Vec<QueryParam>| {
        let parameter = bind(value, offset, params);
        format!("si.import_run_id COLLATE \"C\" {op} ({parameter} #>> '{{}}') COLLATE \"C\"")
    };
    match predicate {
        MetadataPredicate::Eq { value, .. } => compare(value, "=", params),
        MetadataPredicate::In { value, .. } => {
            if value.is_empty() {
                "FALSE".into()
            } else {
                format!(
                    "({})",
                    value
                        .iter()
                        .map(|v| compare(v, "=", params))
                        .collect::<Vec<_>>()
                        .join(" OR ")
                )
            }
        }
        MetadataPredicate::Range { gte, lte, .. } => {
            let mut clauses = Vec::new();
            if let Some(value) = gte {
                clauses.push(compare(value, ">=", params));
            }
            if let Some(value) = lte {
                clauses.push(compare(value, "<=", params));
            }
            format!("({})", clauses.join(" AND "))
        }
        MetadataPredicate::Exists { .. } => "TRUE".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn binds_values_without_sql_interpolation_or_debug_disclosure() {
        let predicates = MetadataPredicates::try_from(json!([
            {"path": "model", "op": "eq", "value": "private' OR TRUE --"}
        ]))
        .unwrap();
        let (sql, params) = MetadataPredicateQueryBuilder::new(&predicates, 3).build();
        assert!(sql.contains("$4::jsonb"));
        assert!(!sql.contains("private"));
        assert!(!format!("{params:?}").contains("private"));
        assert!(sql.contains("COLLATE \"C\""));
    }

    #[test]
    fn exact_numeric_recheck_follows_bounded_index_key() {
        let predicates = MetadataPredicates::try_from(json!([
            {"path": "model", "op": "range", "gte": 9}
        ]))
        .unwrap();
        let (sql, params) = MetadataPredicateQueryBuilder::new(&predicates, 0).build();
        assert!(sql.contains("jsonb_typeof((n.metadata -> 'model')) = 'number'"));
        assert!(sql.contains("metadata_search_order_key_v1"));
        assert!(sql.contains("(n.metadata -> 'model') >= $1::jsonb"));
        assert_eq!(params.len(), 1);
    }

    #[test]
    fn source_conjunction_uses_one_tenant_correlated_identity() {
        let predicates = MetadataPredicates::try_from(json!([
            {"path": "import_run_id", "op": "eq", "value": "run-1"},
            {"path": "import_run_id", "op": "eq", "value": "run-2"}
        ]))
        .unwrap();
        let (sql, params) = MetadataPredicateQueryBuilder::new(&predicates, 0).build();
        assert_eq!(sql.matches("EXISTS").count(), 1);
        assert!(sql.contains("si.tenant_id = n.tenant_id"));
        assert!(!sql.contains("JOIN"));
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn missing_null_and_empty_membership_are_distinct() {
        for (predicate, expected) in [
            (
                json!({"path": "model", "op": "eq", "value": null}),
                "= 'null'",
            ),
            (
                json!({"path": "model", "op": "exists", "value": false}),
                "IS NULL",
            ),
            (json!({"path": "model", "op": "in", "value": []}), "FALSE"),
            (
                json!({"path": "import_run_id", "op": "exists", "value": false}),
                "NOT EXISTS",
            ),
        ] {
            let predicates = MetadataPredicates::try_from(json!([predicate])).unwrap();
            let (sql, params) = MetadataPredicateQueryBuilder::new(&predicates, 0).build();
            assert!(sql.contains(expected));
            assert!(params.is_empty());
        }
    }
}
