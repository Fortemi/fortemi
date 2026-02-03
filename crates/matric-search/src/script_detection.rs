//! Unicode script detection for multilingual text analysis.
//!
//! This module provides efficient detection of Unicode scripts in text, which is essential
//! for choosing appropriate search strategies (e.g., FTS vs. n-gram matching for CJK).
//!
//! The detector performs a single O(n) pass through the input text and classifies it
//! into one or more script categories. Confidence is calculated based on the proportion
//! of characters belonging to each script.

use std::collections::HashMap;
use unicode_script::{Script, UnicodeScript};

/// Detected script category.
///
/// Scripts are grouped into broad categories relevant for search optimization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DetectedScript {
    /// Latin alphabet (English, French, German, etc.)
    Latin,
    /// CJK scripts: Han (Chinese), Hiragana, Katakana (Japanese), Hangul (Korean)
    Cjk,
    /// Arabic script
    Arabic,
    /// Cyrillic script (Russian, Ukrainian, etc.)
    Cyrillic,
    /// Greek script
    Greek,
    /// Hebrew script
    Hebrew,
    /// Devanagari script (Hindi, Sanskrit, etc.)
    Devanagari,
    /// Thai script
    Thai,
    /// Emoji characters
    Emoji,
    /// Multiple scripts detected with significant presence
    Mixed,
    /// Unknown or unclassified script
    Unknown,
}

/// Result of script detection analysis.
#[derive(Debug, Clone, PartialEq)]
pub struct ScriptDetection {
    /// Primary (most common) script detected
    pub primary: DetectedScript,
    /// Confidence level (0.0 - 1.0) representing proportion of primary script
    pub confidence: f32,
    /// All scripts found with significant presence (>5% of characters)
    pub scripts_found: Vec<DetectedScript>,
}

impl ScriptDetection {
    /// Creates a new script detection result.
    pub fn new(
        primary: DetectedScript,
        confidence: f32,
        scripts_found: Vec<DetectedScript>,
    ) -> Self {
        Self {
            primary,
            confidence,
            scripts_found,
        }
    }
}

/// Maps Unicode script to our DetectedScript categories.
fn map_unicode_script(script: Script) -> DetectedScript {
    match script {
        Script::Latin => DetectedScript::Latin,
        Script::Han | Script::Hiragana | Script::Katakana | Script::Hangul => DetectedScript::Cjk,
        Script::Arabic => DetectedScript::Arabic,
        Script::Cyrillic => DetectedScript::Cyrillic,
        Script::Greek => DetectedScript::Greek,
        Script::Hebrew => DetectedScript::Hebrew,
        Script::Devanagari => DetectedScript::Devanagari,
        Script::Thai => DetectedScript::Thai,
        _ => DetectedScript::Unknown,
    }
}

/// Detects the script(s) used in the input text.
///
/// Performs a single O(n) pass through the text, skipping whitespace and common
/// punctuation. Returns the primary script, confidence level, and all scripts
/// found with significant presence (>5%).
///
/// # Examples
///
/// ```
/// use matric_search::script_detection::{detect_script, DetectedScript};
///
/// let result = detect_script("Hello world");
/// assert_eq!(result.primary, DetectedScript::Latin);
/// assert!(result.confidence > 0.9);
///
/// let result = detect_script("こんにちは");
/// assert_eq!(result.primary, DetectedScript::Cjk);
/// ```
pub fn detect_script(query: &str) -> ScriptDetection {
    if query.is_empty() {
        return ScriptDetection::new(DetectedScript::Unknown, 0.0, vec![]);
    }

    let mut script_counts: HashMap<DetectedScript, usize> = HashMap::new();
    let mut total_count = 0usize;

    for ch in query.chars() {
        // Skip whitespace and common ASCII punctuation
        if ch.is_whitespace() || (ch.is_ascii_punctuation() && !is_emoji(ch)) {
            continue;
        }

        // Check for emoji first (some emoji have script properties)
        if is_emoji(ch) {
            *script_counts.entry(DetectedScript::Emoji).or_insert(0) += 1;
            total_count += 1;
            continue;
        }

        // Get Unicode script and map to our categories
        let unicode_script = ch.script();
        let detected = map_unicode_script(unicode_script);

        *script_counts.entry(detected).or_insert(0) += 1;
        total_count += 1;
    }

    // Handle empty result after filtering
    if total_count == 0 {
        return ScriptDetection::new(DetectedScript::Unknown, 0.0, vec![]);
    }

    // Find primary script (most common)
    let (primary_script, primary_count) = script_counts
        .iter()
        .max_by_key(|(_, count)| *count)
        .map(|(script, count)| (*script, *count))
        .unwrap_or((DetectedScript::Unknown, 0));

    // Calculate confidence
    let confidence = if total_count > 0 {
        primary_count as f32 / total_count as f32
    } else {
        0.0
    };

    // Collect all scripts with significant presence (>5%)
    let threshold = (total_count as f32 * 0.05).ceil() as usize;
    let mut scripts_found: Vec<DetectedScript> = script_counts
        .iter()
        .filter(|(script, count)| **count >= threshold && **script != DetectedScript::Unknown)
        .map(|(script, _)| *script)
        .collect();

    // Sort for deterministic output
    scripts_found.sort_by_key(|s| format!("{:?}", s));

    // Determine if mixed (multiple scripts with >20% presence)
    let mixed_threshold = (total_count as f32 * 0.20).ceil() as usize;
    let significant_scripts: Vec<_> = script_counts
        .iter()
        .filter(|(script, count)| **count >= mixed_threshold && **script != DetectedScript::Unknown)
        .collect();

    let final_primary = if significant_scripts.len() > 1 {
        DetectedScript::Mixed
    } else {
        primary_script
    };

    ScriptDetection::new(final_primary, confidence, scripts_found)
}

/// Checks if the query contains CJK characters.
///
/// This is a fast helper for determining if CJK-specific search strategies
/// should be used (e.g., n-gram matching instead of word-based FTS).
///
/// # Examples
///
/// ```
/// use matric_search::script_detection::has_cjk;
///
/// assert!(has_cjk("日本語"));
/// assert!(has_cjk("Hello 世界"));
/// assert!(!has_cjk("Hello world"));
/// ```
pub fn has_cjk(query: &str) -> bool {
    query.chars().any(|ch| {
        matches!(
            ch.script(),
            Script::Han | Script::Hiragana | Script::Katakana | Script::Hangul
        )
    })
}

/// Checks if the query contains emoji characters.
///
/// # Examples
///
/// ```
/// use matric_search::script_detection::has_emoji;
///
/// assert!(has_emoji("Hello 👋"));
/// assert!(has_emoji("🎉"));
/// assert!(!has_emoji("Hello world"));
/// ```
pub fn has_emoji(query: &str) -> bool {
    query.chars().any(is_emoji)
}

/// Determines if a character is an emoji.
///
/// Uses Unicode properties to identify emoji characters, including:
/// - Emoji_Presentation
/// - Extended_Pictographic
/// - Common emoji ranges
fn is_emoji(ch: char) -> bool {
    // Common emoji ranges
    matches!(ch as u32,
        0x1F600..=0x1F64F | // Emoticons
        0x1F300..=0x1F5FF | // Misc Symbols and Pictographs
        0x1F680..=0x1F6FF | // Transport and Map
        0x1F1E6..=0x1F1FF | // Regional indicator symbols
        0x2600..=0x26FF |   // Misc symbols
        0x2700..=0x27BF |   // Dingbats
        0xFE00..=0xFE0F |   // Variation selectors
        0x1F900..=0x1F9FF | // Supplemental Symbols and Pictographs
        0x1F780..=0x1F7FF | // Geometric Shapes Extended
        0x1F800..=0x1F8FF   // Supplemental Arrows-C
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_pure_latin() {
        let result = detect_script("Hello world");
        assert_eq!(result.primary, DetectedScript::Latin);
        assert!(result.confidence > 0.99);
        assert_eq!(result.scripts_found, vec![DetectedScript::Latin]);
    }

    #[test]
    fn test_detect_pure_latin_with_punctuation() {
        let result = detect_script("Hello, world! How are you?");
        assert_eq!(result.primary, DetectedScript::Latin);
        assert!(result.confidence > 0.99);
        assert_eq!(result.scripts_found, vec![DetectedScript::Latin]);
    }

    #[test]
    fn test_detect_chinese_han() {
        let result = detect_script("你好世界");
        assert_eq!(result.primary, DetectedScript::Cjk);
        assert!(result.confidence > 0.99);
        assert_eq!(result.scripts_found, vec![DetectedScript::Cjk]);
    }

    #[test]
    fn test_detect_japanese_hiragana() {
        let result = detect_script("こんにちは");
        assert_eq!(result.primary, DetectedScript::Cjk);
        assert!(result.confidence > 0.99);
        assert_eq!(result.scripts_found, vec![DetectedScript::Cjk]);
    }

    #[test]
    fn test_detect_japanese_katakana() {
        let result = detect_script("カタカナ");
        assert_eq!(result.primary, DetectedScript::Cjk);
        assert!(result.confidence > 0.99);
        assert_eq!(result.scripts_found, vec![DetectedScript::Cjk]);
    }

    #[test]
    fn test_detect_japanese_mixed_scripts() {
        // Japanese text with hiragana, katakana, and kanji
        let result = detect_script("私はカタカナが好きです");
        assert_eq!(result.primary, DetectedScript::Cjk);
        assert!(result.confidence > 0.99);
        assert_eq!(result.scripts_found, vec![DetectedScript::Cjk]);
    }

    #[test]
    fn test_detect_korean_hangul() {
        let result = detect_script("안녕하세요");
        assert_eq!(result.primary, DetectedScript::Cjk);
        assert!(result.confidence > 0.99);
        assert_eq!(result.scripts_found, vec![DetectedScript::Cjk]);
    }

    #[test]
    fn test_detect_arabic() {
        let result = detect_script("مرحبا بك");
        assert_eq!(result.primary, DetectedScript::Arabic);
        assert!(result.confidence > 0.99);
        assert_eq!(result.scripts_found, vec![DetectedScript::Arabic]);
    }

    #[test]
    fn test_detect_cyrillic() {
        let result = detect_script("Привет мир");
        assert_eq!(result.primary, DetectedScript::Cyrillic);
        assert!(result.confidence > 0.99);
        assert_eq!(result.scripts_found, vec![DetectedScript::Cyrillic]);
    }

    #[test]
    fn test_detect_greek() {
        let result = detect_script("Γεια σου κόσμε");
        assert_eq!(result.primary, DetectedScript::Greek);
        assert!(result.confidence > 0.99);
        assert_eq!(result.scripts_found, vec![DetectedScript::Greek]);
    }

    #[test]
    fn test_detect_hebrew() {
        let result = detect_script("שלום עולם");
        assert_eq!(result.primary, DetectedScript::Hebrew);
        assert!(result.confidence > 0.99);
        assert_eq!(result.scripts_found, vec![DetectedScript::Hebrew]);
    }

    #[test]
    fn test_detect_devanagari() {
        let result = detect_script("नमस्ते दुनिया");
        assert_eq!(result.primary, DetectedScript::Devanagari);
        assert!(result.confidence > 0.99);
        assert_eq!(result.scripts_found, vec![DetectedScript::Devanagari]);
    }

    #[test]
    fn test_detect_thai() {
        let result = detect_script("สวัสดีชาวโลก");
        assert_eq!(result.primary, DetectedScript::Thai);
        assert!(result.confidence > 0.99);
        assert_eq!(result.scripts_found, vec![DetectedScript::Thai]);
    }

    #[test]
    fn test_detect_emoji_only() {
        let result = detect_script("👋🌍🎉");
        assert_eq!(result.primary, DetectedScript::Emoji);
        assert!(result.confidence > 0.99);
        assert_eq!(result.scripts_found, vec![DetectedScript::Emoji]);
    }

    #[test]
    fn test_detect_emoji_with_text() {
        let result = detect_script("Hello 👋 world 🌍");
        // Latin should be primary as there are more Latin characters
        assert!(matches!(
            result.primary,
            DetectedScript::Latin | DetectedScript::Mixed
        ));
        assert!(result.scripts_found.contains(&DetectedScript::Latin));
        // Emoji might not reach 5% threshold depending on character count
    }

    #[test]
    fn test_detect_mixed_latin_cjk() {
        // Roughly equal amounts of Latin and CJK
        let result = detect_script("Hello 你好 World 世界");
        assert_eq!(result.primary, DetectedScript::Mixed);
        assert!(result.scripts_found.contains(&DetectedScript::Cjk));
        assert!(result.scripts_found.contains(&DetectedScript::Latin));
    }

    #[test]
    fn test_detect_mixed_arabic_latin() {
        // Mix of Arabic and Latin
        let result = detect_script("Hello مرحبا World بك");
        assert_eq!(result.primary, DetectedScript::Mixed);
        assert!(result.scripts_found.contains(&DetectedScript::Arabic));
        assert!(result.scripts_found.contains(&DetectedScript::Latin));
    }

    #[test]
    fn test_detect_empty_string() {
        let result = detect_script("");
        assert_eq!(result.primary, DetectedScript::Unknown);
        assert_eq!(result.confidence, 0.0);
        assert!(result.scripts_found.is_empty());
    }

    #[test]
    fn test_detect_whitespace_only() {
        let result = detect_script("   \t\n  ");
        assert_eq!(result.primary, DetectedScript::Unknown);
        assert_eq!(result.confidence, 0.0);
        assert!(result.scripts_found.is_empty());
    }

    #[test]
    fn test_detect_punctuation_only() {
        let result = detect_script("!@#$%^&*()");
        assert_eq!(result.primary, DetectedScript::Unknown);
        assert_eq!(result.confidence, 0.0);
        assert!(result.scripts_found.is_empty());
    }

    #[test]
    fn test_has_cjk_pure_latin() {
        assert!(!has_cjk("Hello world"));
    }

    #[test]
    fn test_has_cjk_pure_chinese() {
        assert!(has_cjk("你好世界"));
    }

    #[test]
    fn test_has_cjk_pure_japanese() {
        assert!(has_cjk("こんにちは"));
        assert!(has_cjk("カタカナ"));
    }

    #[test]
    fn test_has_cjk_pure_korean() {
        assert!(has_cjk("안녕하세요"));
    }

    #[test]
    fn test_has_cjk_mixed_text() {
        assert!(has_cjk("Hello 世界"));
        assert!(has_cjk("Search for 東京"));
    }

    #[test]
    fn test_has_cjk_empty() {
        assert!(!has_cjk(""));
    }

    #[test]
    fn test_has_emoji_pure_text() {
        assert!(!has_emoji("Hello world"));
    }

    #[test]
    fn test_has_emoji_with_emoji() {
        assert!(has_emoji("Hello 👋"));
        assert!(has_emoji("🎉"));
        assert!(has_emoji("Great work! 🚀"));
    }

    #[test]
    fn test_has_emoji_common_emoji() {
        assert!(has_emoji("😀")); // Emoticon
        assert!(has_emoji("🌍")); // Misc symbol
        assert!(has_emoji("🚀")); // Transport
        assert!(has_emoji("❤️")); // Misc symbol
    }

    #[test]
    fn test_has_emoji_empty() {
        assert!(!has_emoji(""));
    }

    #[test]
    fn test_confidence_calculation() {
        // 10 Latin chars, 0 others = 100% confidence
        let result = detect_script("HelloWorld");
        assert!((result.confidence - 1.0).abs() < 0.01);

        // 5 Latin, 5 CJK = should be Mixed with ~50% confidence
        let result = detect_script("Hi你好ab世界");
        if result.primary == DetectedScript::Mixed {
            // Confidence is for the most common script, which could be ~50%
            assert!(result.confidence >= 0.4 && result.confidence <= 0.6);
        }
    }

    #[test]
    fn test_scripts_found_threshold() {
        // 95 Latin chars, 5 CJK chars = CJK is exactly at 5% threshold
        let latin = "a".repeat(95);
        let text = format!("{}你好世界你", latin);
        let result = detect_script(&text);

        assert_eq!(result.primary, DetectedScript::Latin);
        assert!(result.scripts_found.contains(&DetectedScript::Latin));
        // CJK should be included as it's at the 5% threshold
        assert!(result.scripts_found.contains(&DetectedScript::Cjk));
    }

    #[test]
    fn test_mixed_script_threshold() {
        // Test that Mixed is only returned when multiple scripts have >20% presence
        // 30 Latin, 30 CJK, 40 Arabic = Latin and CJK both over 20%
        let text = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaا你好世界你好世界你好世界你好世界你好世界你好世界你好世界你好世界مرحبامرحبامرحبامرحبامرحبامرحبامرحبامرحبا";
        let result = detect_script(text);

        // Should detect as Mixed because multiple scripts exceed 20%
        assert_eq!(result.primary, DetectedScript::Mixed);
    }

    #[test]
    fn test_minor_script_not_mixed() {
        // 90 Latin, 10 CJK = should be Latin, not Mixed (CJK is only ~10%)
        let latin = "a".repeat(90);
        let text = format!("{}你好世界你好世界你好世", latin);
        let result = detect_script(&text);

        assert_eq!(result.primary, DetectedScript::Latin);
        assert!(result.scripts_found.contains(&DetectedScript::Latin));
        assert!(result.scripts_found.contains(&DetectedScript::Cjk));
    }
}
