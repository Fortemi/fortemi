//! Canonical embedding-space contract identity.

use std::{collections::BTreeMap, error::Error as StdError, fmt};

use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};

/// Canonical JSON bytes used for embedding-space identity hashing.
///
/// Objects are serialized with keys sorted recursively and without
/// insignificant whitespace. Arrays retain order, strings use JSON escaping,
/// and numbers are rendered from their parsed JSON representation.
pub fn canonical_space_contract_json(contract: &JsonValue) -> String {
    match contract {
        JsonValue::Null => "null".to_string(),
        JsonValue::Bool(value) => value.to_string(),
        JsonValue::Number(value) => value.to_string(),
        JsonValue::String(value) => {
            serde_json::to_string(value).expect("serializing a JSON string value should not fail")
        }
        JsonValue::Array(values) => {
            let values = values
                .iter()
                .map(canonical_space_contract_json)
                .collect::<Vec<_>>();
            format!("[{}]", values.join(","))
        }
        JsonValue::Object(values) => {
            let sorted = values
                .iter()
                .collect::<BTreeMap<_, _>>()
                .into_iter()
                .map(|(key, value)| {
                    let key = serde_json::to_string(key)
                        .expect("serializing a JSON object key should not fail");
                    format!("{key}:{}", canonical_space_contract_json(value))
                })
                .collect::<Vec<_>>();
            format!("{{{}}}", sorted.join(","))
        }
    }
}

/// SHA-256 lowercase hex identity of the canonical embedding-space contract.
pub fn embedding_space_id(contract: &JsonValue) -> String {
    let mut hasher = Sha256::new();
    hasher.update(canonical_space_contract_json(contract).as_bytes());
    hex::encode(hasher.finalize())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpaceMismatch {
    SetHasNoSpaceContract {
        row_space_id: String,
    },
    RowSpaceMismatch {
        set_space_id: String,
        row_space_id: String,
    },
}

impl fmt::Display for SpaceMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SetHasNoSpaceContract { .. } => {
                write!(f, "set has no space contract")
            }
            Self::RowSpaceMismatch { .. } => {
                write!(f, "row space_id does not match set space_id")
            }
        }
    }
}

impl StdError for SpaceMismatch {}

pub fn check_row_space(
    set_space_id: Option<&str>,
    row_space_id: &str,
) -> Result<(), SpaceMismatch> {
    match set_space_id {
        Some(set_space_id) if set_space_id == row_space_id => Ok(()),
        Some(set_space_id) => Err(SpaceMismatch::RowSpaceMismatch {
            set_space_id: set_space_id.to_string(),
            row_space_id: row_space_id.to_string(),
        }),
        None => Err(SpaceMismatch::SetHasNoSpaceContract {
            row_space_id: row_space_id.to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_space_contract_hash_matches_documented_vector() {
        let contract = serde_json::json!({
            "provider": "ollama",
            "pipeline": { "truncate": 1024 },
            "normalization": "provider-native",
            "model": "mxbai"
        });

        assert_eq!(
            canonical_space_contract_json(&contract),
            "{\"model\":\"mxbai\",\"normalization\":\"provider-native\",\"pipeline\":{\"truncate\":1024},\"provider\":\"ollama\"}"
        );
        assert_eq!(
            embedding_space_id(&contract),
            "4e40c2903c3cef52c43c7a3e58b2dedbee9d5aece1f003d52c4335aa07a755c2"
        );
    }

    #[test]
    fn row_space_check_enforces_set_contract() {
        let expected = "4e40c2903c3cef52c43c7a3e58b2dedbee9d5aece1f003d52c4335aa07a755c2";
        assert!(check_row_space(Some(expected), expected).is_ok());
        assert!(matches!(
            check_row_space(
                Some(expected),
                "0000000000000000000000000000000000000000000000000000000000000000"
            ),
            Err(SpaceMismatch::RowSpaceMismatch { .. })
        ));
        assert!(matches!(
            check_row_space(None, expected),
            Err(SpaceMismatch::SetHasNoSpaceContract { .. })
        ));
    }
}
