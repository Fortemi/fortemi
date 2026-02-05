//! Token counting and encoding utilities for LLM context management.
//!
//! This module provides tokenization capabilities using the tiktoken library,
//! which is compatible with OpenAI's tokenization schemes. It also provides
//! fast estimation functions for quick token limit checks.

use crate::error::{Error, Result};

/// Trait for tokenization operations.
///
/// Implementations should be thread-safe and support common tokenization
/// operations needed for LLM context management.
pub trait Tokenizer: Send + Sync {
    /// Count the number of tokens in the given text.
    fn count_tokens(&self, text: &str) -> usize;

    /// Encode text into token IDs.
    fn encode(&self, text: &str) -> Vec<u32>;

    /// Decode token IDs back into text.
    fn decode(&self, tokens: &[u32]) -> String;

    /// Get the name/identifier of this tokenizer.
    fn name(&self) -> &str;
}

/// Tiktoken-based tokenizer implementation.
///
/// Uses the tiktoken-rs library to provide accurate token counting
/// compatible with OpenAI's tokenization schemes.
pub struct TiktokenTokenizer {
    bpe: tiktoken_rs::CoreBPE,
    name: String,
}

impl TiktokenTokenizer {
    /// Create a new tokenizer for the specified model.
    ///
    /// # Arguments
    /// * `model` - Model identifier (e.g., "gpt-4", "gpt-3.5-turbo")
    ///
    /// # Errors
    /// Returns an error if the model is not recognized or BPE initialization fails.
    pub fn new(model: &str) -> Result<Self> {
        let bpe = tiktoken_rs::get_bpe_from_model(model)
            .map_err(|e| Error::Internal(format!("Failed to initialize tokenizer: {}", e)))?;

        Ok(Self {
            bpe,
            name: model.to_string(),
        })
    }

    /// Create a tokenizer for embeddings (uses cl100k_base).
    ///
    /// This is the tokenizer used by text-embedding-ada-002 and similar models.
    pub fn for_embeddings() -> Result<Self> {
        let bpe = tiktoken_rs::cl100k_base()
            .map_err(|e| Error::Internal(format!("Failed to initialize cl100k_base: {}", e)))?;

        Ok(Self {
            bpe,
            name: "cl100k_base".to_string(),
        })
    }
}

impl Tokenizer for TiktokenTokenizer {
    fn count_tokens(&self, text: &str) -> usize {
        self.bpe.encode_ordinary(text).len()
    }

    fn encode(&self, text: &str) -> Vec<u32> {
        self.bpe
            .encode_ordinary(text)
            .into_iter()
            .map(|t| t as u32)
            .collect()
    }

    fn decode(&self, tokens: &[u32]) -> String {
        let token_vec: Vec<usize> = tokens.iter().map(|&t| t as usize).collect();
        self.bpe.decode(token_vec).unwrap_or_default()
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// Quickly estimate token count without full tokenization.
///
/// Uses a heuristic ratio of ~3.7 characters per token for English text.
/// This is much faster than full tokenization but less accurate.
///
/// # Arguments
/// * `text` - The text to estimate tokens for
///
/// # Returns
/// Estimated number of tokens (rounded up)
pub fn estimate_tokens(text: &str) -> usize {
    (text.len() as f32 / 3.7).ceil() as usize
}

/// Check if text likely exceeds a token limit using estimation.
///
/// Useful for quick filtering before expensive tokenization.
///
/// # Arguments
/// * `text` - The text to check
/// * `limit` - The token limit to check against
///
/// # Returns
/// `true` if the text likely exceeds the limit
pub fn likely_exceeds_limit(text: &str, limit: usize) -> bool {
    estimate_tokens(text) > limit
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test data constants
    const SIMPLE_ENGLISH: &str = "The quick brown fox jumps over the lazy dog.";
    const LONG_ENGLISH: &str = r#"
        Tokenization is the process of breaking down text into smaller units called tokens.
        These tokens can be words, subwords, or characters depending on the tokenization algorithm.
        Modern language models use byte-pair encoding (BPE) to efficiently represent text.
        This allows models to handle rare words and maintain a reasonable vocabulary size.
    "#;
    const RUST_CODE: &str = r#"
        fn main() {
            let message = "Hello, world!";
            println!("{}", message);
        }
    "#;
    const REPETITIVE_TEXT: &str = "token token token token token token token token";

    #[test]
    fn test_tiktoken_for_embeddings_initialization() {
        let tokenizer = TiktokenTokenizer::for_embeddings();
        assert!(tokenizer.is_ok(), "Should initialize cl100k_base tokenizer");

        let tokenizer = tokenizer.unwrap();
        assert_eq!(tokenizer.name(), "cl100k_base");
    }

    #[test]
    fn test_tiktoken_new_with_model() {
        let tokenizer = TiktokenTokenizer::new("gpt-4");
        assert!(tokenizer.is_ok(), "Should initialize gpt-4 tokenizer");

        let tokenizer = tokenizer.unwrap();
        assert_eq!(tokenizer.name(), "gpt-4");
    }

    #[test]
    fn test_count_tokens_simple_english() {
        let tokenizer = TiktokenTokenizer::for_embeddings().unwrap();
        let count = tokenizer.count_tokens(SIMPLE_ENGLISH);

        // "The quick brown fox jumps over the lazy dog." is typically ~10 tokens
        assert!(
            (8..=12).contains(&count),
            "Expected ~10 tokens, got {}",
            count
        );
    }

    #[test]
    fn test_count_tokens_long_english() {
        let tokenizer = TiktokenTokenizer::for_embeddings().unwrap();
        let count = tokenizer.count_tokens(LONG_ENGLISH);

        // Should be more than the simple case
        assert!(
            count > 20,
            "Long text should have many tokens, got {}",
            count
        );
    }

    #[test]
    fn test_count_tokens_rust_code() {
        let tokenizer = TiktokenTokenizer::for_embeddings().unwrap();
        let count = tokenizer.count_tokens(RUST_CODE);

        // Code should be tokenized reasonably
        assert!(
            count > 10,
            "Code should have multiple tokens, got {}",
            count
        );
    }

    #[test]
    fn test_count_tokens_empty_string() {
        let tokenizer = TiktokenTokenizer::for_embeddings().unwrap();
        let count = tokenizer.count_tokens("");

        assert_eq!(count, 0, "Empty string should have 0 tokens");
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        let tokenizer = TiktokenTokenizer::for_embeddings().unwrap();
        let original = SIMPLE_ENGLISH;

        let tokens = tokenizer.encode(original);
        assert!(!tokens.is_empty(), "Should produce tokens");

        let decoded = tokenizer.decode(&tokens);
        assert_eq!(decoded, original, "Roundtrip encoding should preserve text");
    }

    #[test]
    fn test_encode_produces_valid_tokens() {
        let tokenizer = TiktokenTokenizer::for_embeddings().unwrap();
        let tokens = tokenizer.encode(SIMPLE_ENGLISH);

        // Should have some tokens
        assert!(!tokens.is_empty());

        // All tokens should be valid u32 values
        for token in &tokens {
            assert!(*token > 0, "Tokens should be positive");
        }
    }

    #[test]
    fn test_decode_empty_tokens() {
        let tokenizer = TiktokenTokenizer::for_embeddings().unwrap();
        let decoded = tokenizer.decode(&[]);

        assert_eq!(
            decoded, "",
            "Empty token list should decode to empty string"
        );
    }

    #[test]
    fn test_estimate_tokens_simple() {
        let estimate = estimate_tokens(SIMPLE_ENGLISH);

        // "The quick brown fox jumps over the lazy dog." is 44 chars
        // 44 / 3.7 ≈ 11.89 -> ceil = 12
        assert_eq!(estimate, 12, "Expected 12 estimated tokens");
    }

    #[test]
    fn test_estimate_tokens_vs_actual() {
        let tokenizer = TiktokenTokenizer::for_embeddings().unwrap();

        // Test on various texts
        let test_cases = vec![SIMPLE_ENGLISH, LONG_ENGLISH, RUST_CODE, REPETITIVE_TEXT];

        for text in test_cases {
            let actual = tokenizer.count_tokens(text);
            let estimate = estimate_tokens(text);

            // Estimation should be within 50% of actual for most text
            // (it's a rough heuristic, not meant to be precise)
            let ratio = estimate as f32 / actual as f32;
            assert!(
                (0.5..=2.0).contains(&ratio),
                "Estimate {} should be within 50%-200% of actual {} for text: {:?}",
                estimate,
                actual,
                &text[..text.len().min(50)]
            );
        }
    }

    #[test]
    fn test_estimate_tokens_empty_string() {
        let estimate = estimate_tokens("");
        assert_eq!(estimate, 0, "Empty string should estimate to 0 tokens");
    }

    #[test]
    fn test_estimate_tokens_single_char() {
        let estimate = estimate_tokens("a");
        // 1 / 3.7 = 0.27 -> ceil = 1
        assert_eq!(estimate, 1, "Single char should estimate to 1 token");
    }

    #[test]
    fn test_likely_exceeds_limit_under() {
        let text = "Short text";
        assert!(
            !likely_exceeds_limit(text, 100),
            "Short text should not exceed limit of 100"
        );
    }

    #[test]
    fn test_likely_exceeds_limit_over() {
        let long_text = "word ".repeat(1000); // 5000 chars -> ~1351 tokens
        assert!(
            likely_exceeds_limit(&long_text, 100),
            "Long text should exceed limit of 100"
        );
    }

    #[test]
    fn test_likely_exceeds_limit_edge_case() {
        let text = "x".repeat(370); // 370 / 3.7 = 100 tokens exactly
        assert!(
            !likely_exceeds_limit(&text, 100),
            "Text at limit should not exceed"
        );

        let text_over = "x".repeat(371); // 371 / 3.7 = 100.27 -> 101 tokens
        assert!(
            likely_exceeds_limit(&text_over, 100),
            "Text just over limit should exceed"
        );
    }

    #[test]
    fn test_tokenizer_thread_safety() {
        use std::sync::Arc;
        use std::thread;

        let tokenizer = Arc::new(TiktokenTokenizer::for_embeddings().unwrap());
        let mut handles = vec![];

        for i in 0..5 {
            let tokenizer_clone = Arc::clone(&tokenizer);
            let handle = thread::spawn(move || {
                let text = format!("Thread {} is tokenizing this text", i);
                tokenizer_clone.count_tokens(&text)
            });
            handles.push(handle);
        }

        for handle in handles {
            let count = handle.join().unwrap();
            assert!(count > 0, "Should tokenize in thread");
        }
    }

    #[test]
    fn test_estimation_accuracy_on_code() {
        let tokenizer = TiktokenTokenizer::for_embeddings().unwrap();
        let actual = tokenizer.count_tokens(RUST_CODE);
        let estimate = estimate_tokens(RUST_CODE);

        // Code tends to have more tokens per character due to special chars
        // So estimation might be less accurate but should still be in range
        let ratio = estimate as f32 / actual as f32;
        assert!(
            (0.3..=3.0).contains(&ratio),
            "Code estimation {} should be reasonably close to actual {}",
            estimate,
            actual
        );
    }

    #[test]
    fn test_special_characters_tokenization() {
        let tokenizer = TiktokenTokenizer::for_embeddings().unwrap();
        let special_text = "Hello! 你好 🌍 @user #hashtag $100";

        let count = tokenizer.count_tokens(special_text);
        assert!(count > 0, "Should handle special characters");

        // Test roundtrip
        let tokens = tokenizer.encode(special_text);
        let decoded = tokenizer.decode(&tokens);
        assert_eq!(decoded, special_text, "Should preserve special characters");
    }

    #[test]
    fn test_whitespace_tokenization() {
        let tokenizer = TiktokenTokenizer::for_embeddings().unwrap();

        let single_space = tokenizer.count_tokens("a b c");
        let multi_space = tokenizer.count_tokens("a  b  c");
        let tabs = tokenizer.count_tokens("a\tb\tc");
        let newlines = tokenizer.count_tokens("a\nb\nc");

        // All should produce tokens (exact counts may vary)
        assert!(single_space > 0);
        assert!(multi_space > 0);
        assert!(tabs > 0);
        assert!(newlines > 0);
    }

    #[test]
    fn test_repetitive_text_efficiency() {
        let tokenizer = TiktokenTokenizer::for_embeddings().unwrap();
        let count = tokenizer.count_tokens(REPETITIVE_TEXT);

        // "token " repeated 8 times = ~8-16 tokens depending on encoding
        assert!(
            (8..=20).contains(&count),
            "Repetitive text tokenization seems off: {}",
            count
        );
    }

    #[test]
    fn test_numeric_text() {
        let tokenizer = TiktokenTokenizer::for_embeddings().unwrap();
        let numbers = "123456789 987654321 42 3.14159";

        let count = tokenizer.count_tokens(numbers);
        assert!(count > 0, "Should tokenize numbers");

        // Test roundtrip
        let tokens = tokenizer.encode(numbers);
        let decoded = tokenizer.decode(&tokens);
        assert_eq!(decoded, numbers, "Should preserve numbers exactly");
    }
}
