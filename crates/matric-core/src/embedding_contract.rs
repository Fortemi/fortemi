//! Canonical, non-secret embedding-space identity.

use std::fmt;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Current client-side vector normalization policy.
///
/// Providers return their native vectors; Fortemi does not normalize them
/// before storage or search.
pub const EMBEDDING_NORMALIZATION_PROVIDER_NATIVE: &str = "provider-native";

/// The inputs that define whether stored and query vectors share an embedding space.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmbeddingContract {
    provider_id: String,
    model: String,
    dimension: usize,
    normalization: String,
    embedding_set_id: Option<Uuid>,
}

impl EmbeddingContract {
    pub fn new(
        provider_id: impl Into<String>,
        model: impl Into<String>,
        dimension: usize,
        embedding_set_id: Option<Uuid>,
    ) -> Result<Self, String> {
        let provider_id = provider_id.into();
        let model = model.into();
        if provider_id.trim().is_empty() {
            return Err("Embedding provider must not be empty".to_string());
        }
        if model.trim().is_empty() {
            return Err("Embedding model must not be empty".to_string());
        }
        if dimension == 0 {
            return Err("Embedding dimension must be greater than zero".to_string());
        }
        Ok(Self {
            provider_id,
            model,
            dimension,
            normalization: EMBEDDING_NORMALIZATION_PROVIDER_NATIVE.to_string(),
            embedding_set_id,
        })
    }

    pub fn provider_id(&self) -> &str {
        &self.provider_id
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn dimension(&self) -> usize {
        self.dimension
    }

    pub fn normalization(&self) -> &str {
        &self.normalization
    }

    pub fn embedding_set_id(&self) -> Option<Uuid> {
        self.embedding_set_id
    }

    /// Stable SHA-256 identity for persistence, cache lineage, and freshness checks.
    pub fn fingerprint(&self) -> String {
        fn hash_field(hasher: &mut Sha256, value: &[u8]) {
            hasher.update((value.len() as u64).to_be_bytes());
            hasher.update(value);
        }

        let mut hasher = Sha256::new();
        hasher.update(b"fortemi.embedding-contract.v1");
        hash_field(&mut hasher, self.provider_id.as_bytes());
        hash_field(&mut hasher, self.model.as_bytes());
        hash_field(&mut hasher, &(self.dimension as u64).to_be_bytes());
        hash_field(&mut hasher, self.normalization.as_bytes());
        match self.embedding_set_id {
            Some(id) => {
                hasher.update([1]);
                hash_field(&mut hasher, id.as_bytes());
            }
            None => hasher.update([0]),
        }
        hex::encode(hasher.finalize())
    }
}

impl fmt::Debug for EmbeddingContract {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EmbeddingContract")
            .field("provider_id_len", &self.provider_id.chars().count())
            .field("model_len", &self.model.chars().count())
            .field("dimension", &self.dimension)
            .field("normalization", &self.normalization)
            .field("embedding_set_scoped", &self.embedding_set_id.is_some())
            .field("fingerprint", &self.fingerprint())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_is_stable_and_sensitive_to_each_contract_field() {
        let set_id = Uuid::parse_str("018f4f52-4ff2-7a4e-8b5d-6d9b4f4d7022").unwrap();
        let base =
            EmbeddingContract::new("openai", "text-embedding-3-small", 1536, Some(set_id)).unwrap();
        assert_eq!(base.fingerprint(), base.fingerprint());
        assert_eq!(base.fingerprint().len(), 64);

        let variants = [
            EmbeddingContract::new("openrouter", "text-embedding-3-small", 1536, Some(set_id))
                .unwrap(),
            EmbeddingContract::new("openai", "other-model", 1536, Some(set_id)).unwrap(),
            EmbeddingContract::new("openai", "text-embedding-3-small", 768, Some(set_id)).unwrap(),
            EmbeddingContract::new("openai", "text-embedding-3-small", 1536, None).unwrap(),
        ];
        for variant in variants {
            assert_ne!(base.fingerprint(), variant.fingerprint());
        }
    }

    #[test]
    fn debug_redacts_provider_and_model() {
        let contract =
            EmbeddingContract::new("private-provider", "private-model", 3, None).unwrap();
        let rendered = format!("{contract:?}");
        assert!(!rendered.contains("private-provider"));
        assert!(!rendered.contains("private-model"));
        assert!(rendered.contains("fingerprint"));
    }
}
