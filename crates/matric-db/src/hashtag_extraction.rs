//! Inline hashtag extraction from markdown content.
//!
//! This module provides utilities to extract hashtags from note content
//! while properly handling markdown syntax to avoid false positives.

use regex::Regex;
use std::collections::HashSet;

/// Extract hashtags from markdown content.
///
/// Returns lowercase, deduplicated tag names.
///
/// # Rules
///
/// 1. Hashtags must start with `#` followed by a letter
/// 2. Can contain letters, numbers, hyphens, and underscores
/// 3. Numeric-only tags are excluded (e.g., `#123`)
/// 4. Markdown headings are excluded (e.g., `# Heading`, `## Heading`)
/// 5. Code blocks and inline code are excluded
/// 6. URLs with fragments are excluded (e.g., `https://example.com/#anchor`)
/// 7. Markdown links with anchors are excluded (e.g., `[link](#anchor)`)
/// 8. All tags are normalized to lowercase
/// 9. Duplicate tags are removed
///
/// # Examples
///
/// ```
/// use matric_db::extract_inline_hashtags;
///
/// let content = "This is a #test with #multiple-tags";
/// let tags = extract_inline_hashtags(content);
/// assert!(tags.contains(&"test".to_string()));
/// assert!(tags.contains(&"multiple-tags".to_string()));
/// ```
pub fn extract_inline_hashtags(content: &str) -> Vec<String> {
    // Step 1: Remove code blocks (fenced with ```)
    let without_code_blocks = remove_code_blocks(content);

    // Step 2: Remove inline code (backtick-wrapped)
    let without_inline_code = remove_inline_code(&without_code_blocks);

    // Step 3: Remove markdown headings (lines starting with #)
    let without_headings = remove_headings(&without_inline_code);

    // Step 4: Remove markdown links (to avoid extracting anchors)
    let without_markdown_links = remove_markdown_links(&without_headings);

    // Step 5: Remove URLs
    let without_urls = remove_urls(&without_markdown_links);

    // Step 6: Extract hashtags
    let hashtag_pattern = Regex::new(r"(?:^|[^a-zA-Z0-9_-])#([a-zA-Z][a-zA-Z0-9_-]*)").unwrap();

    let mut tags = HashSet::new();

    for cap in hashtag_pattern.captures_iter(&without_urls) {
        if let Some(tag) = cap.get(1) {
            let tag_str = tag.as_str();

            // Skip numeric-only tags (should be caught by regex, but double-check)
            if tag_str.chars().all(|c| c.is_numeric()) {
                continue;
            }

            // Normalize to lowercase and add to set (deduplication)
            tags.insert(tag_str.to_lowercase());
        }
    }

    // Convert to sorted vector for consistent output
    let mut result: Vec<String> = tags.into_iter().collect();
    result.sort();
    result
}

/// Remove fenced code blocks from content.
fn remove_code_blocks(content: &str) -> String {
    let code_block_pattern = Regex::new(r"(?s)```[a-z]*\n.*?```").unwrap();
    code_block_pattern.replace_all(content, "").to_string()
}

/// Remove inline code (backtick-wrapped) from content.
fn remove_inline_code(content: &str) -> String {
    let inline_code_pattern = Regex::new(r"`[^`]+`").unwrap();
    inline_code_pattern.replace_all(content, "").to_string()
}

/// Remove markdown headings (lines starting with one or more #).
fn remove_headings(content: &str) -> String {
    content
        .lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            // Check if line starts with # followed by space (markdown heading)
            if trimmed.starts_with('#') {
                // Count leading #
                let hash_count = trimmed.chars().take_while(|&c| c == '#').count();
                let after_hashes = trimmed.chars().nth(hash_count);

                // If followed by space or end of line, it's a heading
                !(after_hashes.is_none() || after_hashes == Some(' '))
            } else {
                true
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Remove markdown links to avoid extracting anchor fragments.
///
/// Matches patterns like:
/// - `[text](url)` - Standard markdown links
/// - `[text](#anchor)` - Internal anchor links
/// - `[text](url#anchor)` - Links with fragments
fn remove_markdown_links(content: &str) -> String {
    // Match markdown link syntax: [text](url)
    let link_pattern = Regex::new(r"\[([^\]]+)\]\(([^)]+)\)").unwrap();
    link_pattern.replace_all(content, "$1").to_string()
}

/// Remove URLs from content (to avoid extracting fragments).
fn remove_urls(content: &str) -> String {
    // Match common URL patterns
    let url_pattern = Regex::new(
        r"https?://[^\s<>\[\]()]+|www\.[^\s<>\[\]()]+|[a-zA-Z0-9.-]+\.[a-z]{2,}[^\s<>\[\]()]*",
    )
    .unwrap();
    url_pattern.replace_all(content, "").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remove_code_blocks() {
        let content = "Before\n```rust\n#[derive(Debug)]\n```\nAfter";
        let result = remove_code_blocks(content);
        assert!(!result.contains("derive"));
        assert!(result.contains("Before"));
        assert!(result.contains("After"));
    }

    #[test]
    fn test_remove_inline_code() {
        let content = "Use `#include` in C";
        let result = remove_inline_code(content);
        assert!(!result.contains("#include"));
    }

    #[test]
    fn test_remove_headings() {
        let content = "# Heading 1\n## Heading 2\nNormal #tag\n### Heading 3";
        let result = remove_headings(content);
        assert!(!result.contains("# Heading 1"));
        assert!(!result.contains("## Heading 2"));
        assert!(result.contains("Normal #tag"));
    }

    #[test]
    fn test_remove_markdown_links() {
        let content = "Visit [this link](#section) for more info";
        let result = remove_markdown_links(content);
        assert!(!result.contains("#section"));
        assert!(result.contains("this link"));
    }

    #[test]
    fn test_hashtag_at_line_start_not_heading() {
        // #tag without space after should be extracted
        let content = "#nospacetag\n#another-tag";
        let tags = extract_inline_hashtags(content);
        assert!(tags.contains(&"nospacetag".to_string()));
        assert!(tags.contains(&"another-tag".to_string()));
    }
}
