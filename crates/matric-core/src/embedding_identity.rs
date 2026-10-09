//! Stable identity helpers for embedding documents and chunks.

use sha2::{Digest, Sha256};

/// Compute the repository-standard SHA-256 digest for embedding identity fields.
///
/// The result is lowercase hex prefixed with `sha256:`, matching note content
/// and source-addressed digest conventions elsewhere in the codebase.
pub fn embedding_sha256(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("sha256:{}", hex::encode(hasher.finalize()))
}

/// Hash of the exact text passed to the chunker.
pub fn embedding_doc_hash(document_text: &str) -> String {
    embedding_sha256(document_text)
}

/// Hash of one chunk's exact text.
pub fn embedding_chunk_hash(chunk_text: &str) -> String {
    embedding_sha256(chunk_text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedding_hashes_use_prefixed_lowercase_sha256() {
        let hash = embedding_doc_hash("test");
        assert_eq!(
            hash,
            "sha256:9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
        );
        assert_eq!(embedding_chunk_hash("test"), hash);
    }
}
