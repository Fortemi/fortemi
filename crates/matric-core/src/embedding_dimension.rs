use serde::{Deserialize, Serialize};
use std::{error::Error, fmt, str::FromStr};

pub const MAX_HNSW_VECTOR_DIM: usize = 2000;
pub const MAX_HNSW_HALFVEC_DIM: usize = 4000;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum EmbeddingVectorType {
    #[default]
    Vector,
    Halfvec,
}

impl EmbeddingVectorType {
    pub fn max_hnsw_dimension(self) -> usize {
        match self {
            Self::Vector => MAX_HNSW_VECTOR_DIM,
            Self::Halfvec => MAX_HNSW_HALFVEC_DIM,
        }
    }
}

impl fmt::Display for EmbeddingVectorType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Vector => f.write_str("vector"),
            Self::Halfvec => f.write_str("halfvec"),
        }
    }
}

impl FromStr for EmbeddingVectorType {
    type Err = DimensionError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "vector" => Ok(Self::Vector),
            "halfvec" => Ok(Self::Halfvec),
            _ => Err(DimensionError::InvalidVectorType(value.to_string())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DimensionError {
    InvalidVectorType(String),
    DimensionOutOfRange {
        dimension: usize,
        vector_type: EmbeddingVectorType,
        max: usize,
    },
    ValueDimensionMismatch {
        actual: usize,
        expected: usize,
    },
    NonFiniteValue {
        index: usize,
    },
}

impl fmt::Display for DimensionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidVectorType(value) => {
                write!(
                    f,
                    "embedding vector_type must be 'vector' or 'halfvec', got '{value}'"
                )
            }
            Self::DimensionOutOfRange {
                dimension,
                vector_type,
                max,
            } => write!(
                f,
                "embedding dimension {dimension} is invalid for {vector_type}; expected 1..={max}"
            ),
            Self::ValueDimensionMismatch { actual, expected } => write!(
                f,
                "embedding vector has {actual} values; expected {expected}"
            ),
            Self::NonFiniteValue { index } => {
                write!(f, "embedding vector value at index {index} is not finite")
            }
        }
    }
}

impl Error for DimensionError {}

pub fn validate_embedding_dimension(
    dimension: usize,
    vector_type: EmbeddingVectorType,
) -> Result<(), DimensionError> {
    let max = vector_type.max_hnsw_dimension();
    if (1..=max).contains(&dimension) {
        Ok(())
    } else {
        Err(DimensionError::DimensionOutOfRange {
            dimension,
            vector_type,
            max,
        })
    }
}

pub fn validate_embedding_values(
    values: &[f32],
    expected_dimension: usize,
) -> Result<(), DimensionError> {
    if values.len() != expected_dimension {
        return Err(DimensionError::ValueDimensionMismatch {
            actual: values.len(),
            expected: expected_dimension,
        });
    }
    if let Some((index, _)) = values
        .iter()
        .enumerate()
        .find(|(_, value)| !value.is_finite())
    {
        return Err(DimensionError::NonFiniteValue { index });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_dimension_limits_by_vector_type() {
        assert!(validate_embedding_dimension(1, EmbeddingVectorType::Vector).is_ok());
        assert!(
            validate_embedding_dimension(MAX_HNSW_VECTOR_DIM, EmbeddingVectorType::Vector).is_ok()
        );
        assert!(
            validate_embedding_dimension(MAX_HNSW_VECTOR_DIM + 1, EmbeddingVectorType::Vector)
                .is_err()
        );

        assert!(
            validate_embedding_dimension(MAX_HNSW_HALFVEC_DIM, EmbeddingVectorType::Halfvec)
                .is_ok()
        );
        assert!(validate_embedding_dimension(
            MAX_HNSW_HALFVEC_DIM + 1,
            EmbeddingVectorType::Halfvec
        )
        .is_err());
    }

    #[test]
    fn rejects_zero_dimensions() {
        let error = validate_embedding_dimension(0, EmbeddingVectorType::Vector).unwrap_err();
        assert_eq!(
            error.to_string(),
            "embedding dimension 0 is invalid for vector; expected 1..=2000"
        );
    }

    #[test]
    fn validates_embedding_values() {
        assert!(validate_embedding_values(&[0.1, 0.2, 0.3], 3).is_ok());

        let wrong_len = validate_embedding_values(&[0.1, 0.2], 3).unwrap_err();
        assert_eq!(
            wrong_len.to_string(),
            "embedding vector has 2 values; expected 3"
        );

        let non_finite = validate_embedding_values(&[0.1, f32::NAN], 2).unwrap_err();
        assert_eq!(
            non_finite.to_string(),
            "embedding vector value at index 1 is not finite"
        );
    }
}
