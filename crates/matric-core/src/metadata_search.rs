//! Candidate metadata-search authority for #1091. Not yet an API capability.

use std::{cmp::Ordering, fmt, str::FromStr, sync::LazyLock};

use bigdecimal::BigDecimal;
use serde::Deserialize;
use serde_json::Value;

pub const PREDICATES_SCHEMA: &str =
    include_str!("../../../contracts/metadata-search/candidate/1.0.0/predicates.schema.json");

static VALIDATOR: LazyLock<jsonschema::Validator> = LazyLock::new(|| {
    let schema: Value = serde_json::from_str(PREDICATES_SCHEMA).expect("embedded predicate schema");
    jsonschema::validator_for(&schema).expect("valid embedded predicate schema")
});

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MetadataPath {
    Provider,
    Model,
    Role,
    EventKind,
    Sensitivity,
    ImportRunId,
}

impl MetadataPath {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Provider => "provider",
            Self::Model => "model",
            Self::Role => "role",
            Self::EventKind => "event_kind",
            Self::Sensitivity => "sensitivity",
            Self::ImportRunId => "import_run_id",
        }
    }
}

// Values deliberately have no Debug implementation: predicates can contain private data.
#[derive(Clone, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum MetadataPredicate {
    Eq {
        path: MetadataPath,
        value: Value,
    },
    In {
        path: MetadataPath,
        value: Vec<Value>,
    },
    Range {
        path: MetadataPath,
        gte: Option<Value>,
        lte: Option<Value>,
    },
    Exists {
        path: MetadataPath,
        #[serde(default = "default_exists")]
        value: bool,
    },
}

fn default_exists() -> bool {
    true
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum MetadataPredicateError {
    #[error("METADATA_PREDICATES_INVALID")]
    Invalid,
    #[error("METADATA_RANGE_INVALID")]
    InvalidRange,
}

/// Only schema-validated, semantically ordered predicates enter this wrapper.
#[derive(Clone)]
pub struct MetadataPredicates(Vec<MetadataPredicate>);

impl fmt::Debug for MetadataPredicates {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MetadataPredicates")
            .field("count", &self.0.len())
            .finish()
    }
}

impl TryFrom<Value> for MetadataPredicates {
    type Error = MetadataPredicateError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        if !VALIDATOR.is_valid(&value) {
            return Err(MetadataPredicateError::Invalid);
        }
        let predicates: Vec<MetadataPredicate> =
            serde_json::from_value(value).map_err(|_| MetadataPredicateError::Invalid)?;
        for predicate in &predicates {
            if let MetadataPredicate::Range {
                gte: Some(lower),
                lte: Some(upper),
                ..
            } = predicate
            {
                if scalar_order(lower, upper) == Some(Ordering::Greater) {
                    return Err(MetadataPredicateError::InvalidRange);
                }
            }
        }
        Ok(Self(predicates))
    }
}

impl MetadataPredicates {
    pub fn as_slice(&self) -> &[MetadataPredicate] {
        &self.0
    }
}

fn scalar_order(left: &Value, right: &Value) -> Option<Ordering> {
    match (left, right) {
        (Value::String(a), Value::String(b)) => Some(a.cmp(b)),
        (Value::Number(a), Value::Number(b)) => {
            let a = BigDecimal::from_str(&a.to_string()).ok()?;
            let b = BigDecimal::from_str(&b.to_string()).ok()?;
            Some(a.cmp(&b))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const VECTORS: &str =
        include_str!("../../../contracts/metadata-search/candidate/1.0.0/predicate-vectors.json");

    // Reference semantics only; production retrieval must filter before ranking in SQL.
    fn matches(predicate: &MetadataPredicate, row: &Value) -> bool {
        let path = match predicate {
            MetadataPredicate::Eq { path, .. }
            | MetadataPredicate::In { path, .. }
            | MetadataPredicate::Range { path, .. }
            | MetadataPredicate::Exists { path, .. } => *path,
        };
        let actual = if path == MetadataPath::ImportRunId {
            row.get("importRunId")
        } else {
            row["metadata"].get(path.as_str())
        };
        let equal = |expected: &Value| {
            actual.is_some_and(|value| {
                value == expected || scalar_order(value, expected) == Some(Ordering::Equal)
            })
        };
        match predicate {
            MetadataPredicate::Eq { value, .. } => equal(value),
            MetadataPredicate::In { value, .. } => value.iter().any(equal),
            MetadataPredicate::Exists { value, .. } => actual.is_some() == *value,
            MetadataPredicate::Range { gte, lte, .. } => actual.is_some_and(|actual| {
                gte.as_ref().is_none_or(|bound| {
                    scalar_order(actual, bound).is_some_and(|order| order != Ordering::Less)
                }) && lte.as_ref().is_none_or(|bound| {
                    scalar_order(actual, bound).is_some_and(|order| order != Ordering::Greater)
                })
            }),
        }
    }

    #[test]
    fn shared_predicate_vectors() {
        let corpus: Value = serde_json::from_str(VECTORS).unwrap();
        for case in corpus["cases"].as_array().unwrap() {
            let id = case["id"].as_str().unwrap();
            let parsed = MetadataPredicates::try_from(case["predicates"].clone());
            if case["valid"] == true {
                let parsed = parsed.unwrap_or_else(|error| panic!("{id}: {error}"));
                let actual: Vec<&Value> = corpus["rows"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|row| parsed.as_slice().iter().all(|p| matches(p, row)))
                    .map(|row| &row["id"])
                    .collect();
                assert_eq!(
                    serde_json::to_value(actual).unwrap(),
                    case["expectedIds"],
                    "{id}"
                );
            } else {
                assert_eq!(
                    parsed.unwrap_err().to_string(),
                    case["code"].as_str().unwrap(),
                    "{id}"
                );
            }
        }
    }

    #[test]
    fn diagnostics_are_content_free() {
        let input = json!([{"path": "model", "op": "eq", "value": "private-search-term"}]);
        let parsed = MetadataPredicates::try_from(input).unwrap();
        assert_eq!(format!("{parsed:?}"), "MetadataPredicates { count: 1 }");
        let input = json!([{"path": "private-path", "op": "eq", "value": "private-search-term"}]);
        let error = MetadataPredicates::try_from(input).unwrap_err();
        assert_eq!(error.to_string(), "METADATA_PREDICATES_INVALID");
        assert_eq!(format!("{error:?}"), "Invalid");
    }

    #[test]
    fn numeric_representations_compare_without_text_coercion() {
        assert_eq!(scalar_order(&json!(1), &json!(1.0)), Some(Ordering::Equal));
        assert_eq!(
            scalar_order(&json!(0.01), &json!(0.1)),
            Some(Ordering::Less)
        );
        assert_eq!(scalar_order(&json!(1), &json!("1")), None);
        assert_eq!(
            scalar_order(&json!(9007199254740990_i64), &json!(9007199254740991_i64)),
            Some(Ordering::Less)
        );
    }

    #[test]
    fn import_run_id_uses_source_identity_not_metadata() {
        let parsed = MetadataPredicates::try_from(json!([
            {"path": "import_run_id", "op": "eq", "value": "run-1"}
        ]))
        .unwrap();
        assert!(!matches(
            &parsed.as_slice()[0],
            &json!({"metadata": {"import_run_id": "run-1"}})
        ));
        assert!(matches(
            &parsed.as_slice()[0],
            &json!({"metadata": {}, "importRunId": "run-1"})
        ));
    }
}
