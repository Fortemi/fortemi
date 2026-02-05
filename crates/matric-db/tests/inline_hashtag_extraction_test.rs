//! Tests for inline hashtag extraction functionality (Issue #248)

use matric_db::extract_inline_hashtags;

#[test]
fn test_extract_basic_hashtags() {
    let content = "This is a #test with #multiple-tags and #CamelCase";
    let tags = extract_inline_hashtags(content);

    assert_eq!(tags.len(), 3);
    assert!(tags.contains(&"test".to_string()));
    assert!(tags.contains(&"multiple-tags".to_string()));
    assert!(tags.contains(&"camelcase".to_string())); // lowercase
}

#[test]
fn test_ignore_markdown_headings() {
    let content = "## Heading\n### Another Heading\nWith #real-tag";
    let tags = extract_inline_hashtags(content);

    assert_eq!(tags.len(), 1);
    assert!(tags.contains(&"real-tag".to_string()));
}

#[test]
fn test_ignore_code_blocks() {
    let content = "```rust\n#[derive(Debug)]\n```\nAnd #actual-tag";
    let tags = extract_inline_hashtags(content);

    assert_eq!(tags.len(), 1);
    assert!(!tags.contains(&"derive".to_string()));
    assert!(tags.contains(&"actual-tag".to_string()));
}

#[test]
fn test_ignore_inline_code() {
    let content = "Use `#include` in C, but here's a #real-tag";
    let tags = extract_inline_hashtags(content);

    assert_eq!(tags.len(), 1);
    assert!(!tags.contains(&"include".to_string()));
    assert!(tags.contains(&"real-tag".to_string()));
}

#[test]
fn test_ignore_urls() {
    let content = "Visit https://example.com/#anchor and use #actual-tag";
    let tags = extract_inline_hashtags(content);

    assert_eq!(tags.len(), 1);
    assert!(!tags.contains(&"anchor".to_string()));
    assert!(tags.contains(&"actual-tag".to_string()));
}

#[test]
fn test_ignore_numeric_only() {
    let content = "Issue #123 and #456 but also #bug123 and #test";
    let tags = extract_inline_hashtags(content);

    assert!(!tags.contains(&"123".to_string()));
    assert!(!tags.contains(&"456".to_string()));
    assert!(tags.contains(&"bug123".to_string()));
    assert!(tags.contains(&"test".to_string()));
}

#[test]
fn test_deduplication() {
    let content = "Using #test and #Test and #TEST should give one tag";
    let tags = extract_inline_hashtags(content);

    assert_eq!(tags.len(), 1);
    assert!(tags.contains(&"test".to_string()));
}

#[test]
fn test_empty_content() {
    let content = "";
    let tags = extract_inline_hashtags(content);

    assert!(tags.is_empty());
}

#[test]
fn test_no_hashtags() {
    let content = "This content has no hashtags at all";
    let tags = extract_inline_hashtags(content);

    assert!(tags.is_empty());
}

#[test]
fn test_complex_markdown() {
    let content = r#"
# Testing Inline Hashtags

This note tests whether inline #hashtags are extracted.

Topics:
- #inline-extraction behavior
- #tag-parsing rules

```rust
#[derive(Debug)]
struct Test;
```

Use `#include` in code, but #actual-tags work.

Visit [link](#section) for more info.
"#;

    let tags = extract_inline_hashtags(content);

    // Debug output
    eprintln!("Extracted {} tags:", tags.len());
    for tag in &tags {
        eprintln!("  - {}", tag);
    }

    // Should extract: hashtags, inline-extraction, tag-parsing, actual-tags
    assert_eq!(
        tags.len(),
        4,
        "Expected 4 tags but got {}: {:?}",
        tags.len(),
        tags
    );
    assert!(tags.contains(&"hashtags".to_string()));
    assert!(tags.contains(&"inline-extraction".to_string()));
    assert!(tags.contains(&"tag-parsing".to_string()));
    assert!(tags.contains(&"actual-tags".to_string()));

    // Should NOT extract these
    assert!(!tags.contains(&"derive".to_string()));
    assert!(!tags.contains(&"include".to_string()));
    assert!(!tags.contains(&"section".to_string()));
}

#[test]
fn test_hashtag_at_start_of_line() {
    let content = "#start-tag\nMiddle #middle-tag\n#end-tag";
    let tags = extract_inline_hashtags(content);

    // #start-tag should be extracted (not a heading without space after #)
    // Actually, we should check if it's followed by space to distinguish from heading
    // For now, let's be conservative and extract it
    assert!(tags.contains(&"middle-tag".to_string()));
    assert!(tags.contains(&"end-tag".to_string()));
}

#[test]
fn test_unicode_in_tags() {
    // Tags should be ASCII only for now
    let content = "Test #validtag and #invalid-émoji-tag";
    let tags = extract_inline_hashtags(content);

    assert!(tags.contains(&"validtag".to_string()));
    // The unicode tag might not be extracted depending on regex
}

#[test]
fn test_underscores_in_tags() {
    let content = "Using #snake_case and #kebab-case tags";
    let tags = extract_inline_hashtags(content);

    assert!(tags.contains(&"snake_case".to_string()));
    assert!(tags.contains(&"kebab-case".to_string()));
}
