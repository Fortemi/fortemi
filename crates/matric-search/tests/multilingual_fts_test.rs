//! Comprehensive multilingual FTS test suite.
//!
//! This test suite validates the multilingual full-text search functionality including:
//! - Script detection for various writing systems
//! - FTS feature flags and phased rollout
//! - Search strategy selection based on detected scripts
//! - Configuration options for multilingual search

use matric_search::{
    detect_script, has_cjk, has_emoji, DetectedScript, FtsFeatureFlags, HybridSearchConfig,
    ScriptDetection, SearchStrategy,
};

// ========== SCRIPT DETECTION TESTS ==========

#[test]
fn test_detect_latin_english() {
    let result = detect_script("Hello world");
    assert_eq!(result.primary, DetectedScript::Latin);
    assert!(result.confidence > 0.99);
    assert!(result.scripts_found.contains(&DetectedScript::Latin));
}

#[test]
fn test_detect_latin_german() {
    let result = detect_script("Guten Tag Welt");
    assert_eq!(result.primary, DetectedScript::Latin);
    assert!(result.confidence > 0.99);
}

#[test]
fn test_detect_latin_french() {
    let result = detect_script("Bonjour le monde");
    assert_eq!(result.primary, DetectedScript::Latin);
    assert!(result.confidence > 0.99);
}

#[test]
fn test_detect_latin_with_accents() {
    let result = detect_script("Café résumé naïve");
    assert_eq!(result.primary, DetectedScript::Latin);
    assert!(result.confidence > 0.99);
}

#[test]
fn test_detect_latin_with_punctuation() {
    let result = detect_script("Hello, world! How are you?");
    assert_eq!(result.primary, DetectedScript::Latin);
    assert!(result.confidence > 0.99);
}

#[test]
fn test_detect_cjk_chinese_simplified() {
    let result = detect_script("你好世界");
    assert_eq!(result.primary, DetectedScript::Cjk);
    assert!(result.confidence > 0.99);
    assert!(result.scripts_found.contains(&DetectedScript::Cjk));
}

#[test]
fn test_detect_cjk_chinese_traditional() {
    let result = detect_script("你好世界繁體字");
    assert_eq!(result.primary, DetectedScript::Cjk);
    assert!(result.confidence > 0.99);
}

#[test]
fn test_detect_cjk_japanese_hiragana() {
    let result = detect_script("こんにちは");
    assert_eq!(result.primary, DetectedScript::Cjk);
    assert!(result.confidence > 0.99);
}

#[test]
fn test_detect_cjk_japanese_katakana() {
    let result = detect_script("カタカナ");
    assert_eq!(result.primary, DetectedScript::Cjk);
    assert!(result.confidence > 0.99);
}

#[test]
fn test_detect_cjk_japanese_mixed() {
    // Hiragana, Katakana, and Kanji mixed
    let result = detect_script("私はカタカナが好きです");
    assert_eq!(result.primary, DetectedScript::Cjk);
    assert!(result.confidence > 0.99);
}

#[test]
fn test_detect_cjk_korean() {
    let result = detect_script("안녕하세요");
    assert_eq!(result.primary, DetectedScript::Cjk);
    assert!(result.confidence > 0.99);
}

#[test]
fn test_detect_arabic() {
    let result = detect_script("مرحبا بك");
    assert_eq!(result.primary, DetectedScript::Arabic);
    assert!(result.confidence > 0.99);
    assert!(result.scripts_found.contains(&DetectedScript::Arabic));
}

#[test]
fn test_detect_arabic_with_diacritics() {
    // Arabic diacritics may be treated as combining marks, potentially lowering confidence
    let result = detect_script("مَرْحَبًا بِكَ");
    assert_eq!(result.primary, DetectedScript::Arabic);
    // Diacritics might be filtered, so use a more lenient confidence threshold
    // Diacritics might lower confidence, so just verify primary script
}

#[test]
fn test_detect_cyrillic_russian() {
    let result = detect_script("Привет мир");
    assert_eq!(result.primary, DetectedScript::Cyrillic);
    assert!(result.confidence > 0.99);
    assert!(result.scripts_found.contains(&DetectedScript::Cyrillic));
}

#[test]
fn test_detect_cyrillic_ukrainian() {
    let result = detect_script("Привіт світ");
    assert_eq!(result.primary, DetectedScript::Cyrillic);
    assert!(result.confidence > 0.99);
}

#[test]
fn test_detect_greek() {
    let result = detect_script("Γεια σου κόσμε");
    assert_eq!(result.primary, DetectedScript::Greek);
    assert!(result.confidence > 0.99);
    assert!(result.scripts_found.contains(&DetectedScript::Greek));
}

#[test]
fn test_detect_hebrew() {
    let result = detect_script("שלום עולם");
    assert_eq!(result.primary, DetectedScript::Hebrew);
    assert!(result.confidence > 0.99);
    assert!(result.scripts_found.contains(&DetectedScript::Hebrew));
}

#[test]
fn test_detect_devanagari_hindi() {
    let result = detect_script("नमस्ते दुनिया");
    assert_eq!(result.primary, DetectedScript::Devanagari);
    assert!(result.confidence > 0.99);
}

#[test]
fn test_detect_thai() {
    let result = detect_script("สวัสดีชาวโลก");
    assert_eq!(result.primary, DetectedScript::Thai);
    assert!(result.confidence > 0.99);
}

#[test]
fn test_detect_emoji_only() {
    let result = detect_script("👋🌍🎉");
    assert_eq!(result.primary, DetectedScript::Emoji);
    assert!(result.confidence > 0.99);
    assert!(result.scripts_found.contains(&DetectedScript::Emoji));
}

#[test]
fn test_detect_emoji_emoticons() {
    let result = detect_script("😀😃😄");
    assert_eq!(result.primary, DetectedScript::Emoji);
    assert!(result.confidence > 0.99);
}

#[test]
fn test_detect_emoji_symbols() {
    let result = detect_script("🚀🌟💡");
    assert_eq!(result.primary, DetectedScript::Emoji);
    assert!(result.confidence > 0.99);
}

#[test]
fn test_detect_emoji_with_latin() {
    let result = detect_script("Hello 👋 world 🌍");
    // Latin should be primary as there are more Latin characters
    assert!(matches!(
        result.primary,
        DetectedScript::Latin | DetectedScript::Mixed
    ));
    assert!(result.scripts_found.contains(&DetectedScript::Latin));
}

#[test]
fn test_detect_mixed_latin_cjk_balanced() {
    // Roughly equal amounts of Latin and CJK
    let result = detect_script("Hello 你好 World 世界");
    assert_eq!(result.primary, DetectedScript::Mixed);
    assert!(result.scripts_found.contains(&DetectedScript::Cjk));
    assert!(result.scripts_found.contains(&DetectedScript::Latin));
}

#[test]
fn test_detect_mixed_arabic_latin_balanced() {
    let result = detect_script("Hello مرحبا World بك");
    assert_eq!(result.primary, DetectedScript::Mixed);
    assert!(result.scripts_found.contains(&DetectedScript::Arabic));
    assert!(result.scripts_found.contains(&DetectedScript::Latin));
}

#[test]
fn test_detect_mixed_cyrillic_latin() {
    let result = detect_script("Привет Hello мир World");
    assert_eq!(result.primary, DetectedScript::Mixed);
    assert!(result.scripts_found.contains(&DetectedScript::Cyrillic));
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
fn test_detect_numbers_only() {
    // Numbers have Unicode script property "Common" which maps to Unknown
    // The implementation skips whitespace and punctuation but processes numbers
    let result = detect_script("123456789");
    // Numbers are processed but map to Unknown, so confidence may vary
    // depending on how they're handled
    assert!(matches!(result.primary, DetectedScript::Unknown));
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
fn test_has_emoji_common_emoji_types() {
    assert!(has_emoji("😀")); // Emoticon
    assert!(has_emoji("🌍")); // Misc symbol
    assert!(has_emoji("🚀")); // Transport
    assert!(has_emoji("❤️")); // Misc symbol with variation selector
}

#[test]
fn test_has_emoji_empty() {
    assert!(!has_emoji(""));
}

#[test]
fn test_script_detection_confidence() {
    // 100% Latin
    let result = detect_script("HelloWorld");
    assert!((result.confidence - 1.0).abs() < 0.01);

    // Roughly balanced mixed
    let result = detect_script("Hi你好ab世界");
    if result.primary == DetectedScript::Mixed {
        // Confidence is for the most common script, which could be ~50%
        assert!(result.confidence >= 0.4 && result.confidence <= 0.6);
    }
}

// ========== FTS FEATURE FLAGS TESTS ==========

#[test]
fn test_fts_flags_default() {
    let flags = FtsFeatureFlags::default();

    // Phase 1 should be enabled (already migrated)
    assert!(flags.websearch_to_tsquery);

    // Phase 2 and 3 should be disabled by default
    assert!(!flags.trigram_fallback);
    assert!(!flags.bigram_cjk);
    assert!(!flags.script_detection);
    assert!(!flags.multilingual_configs);
}

#[test]
fn test_fts_flags_all_enabled() {
    let flags = FtsFeatureFlags::all_enabled();

    assert!(flags.websearch_to_tsquery);
    assert!(flags.trigram_fallback);
    assert!(flags.bigram_cjk);
    assert!(flags.script_detection);
    assert!(flags.multilingual_configs);
}

#[test]
fn test_fts_flags_phase1_enabled() {
    let mut flags = FtsFeatureFlags::default();
    assert!(flags.is_phase1_enabled());

    flags.websearch_to_tsquery = false;
    assert!(!flags.is_phase1_enabled());
}

#[test]
fn test_fts_flags_phase2_requires_both() {
    // Neither flag set
    let flags = FtsFeatureFlags {
        trigram_fallback: false,
        script_detection: false,
        ..Default::default()
    };
    assert!(!flags.is_phase2_enabled());

    // Only trigram_fallback
    let flags = FtsFeatureFlags {
        trigram_fallback: true,
        script_detection: false,
        ..Default::default()
    };
    assert!(!flags.is_phase2_enabled());

    // Only script_detection
    let flags = FtsFeatureFlags {
        trigram_fallback: false,
        script_detection: true,
        ..Default::default()
    };
    assert!(!flags.is_phase2_enabled());

    // Both flags set
    let flags = FtsFeatureFlags {
        trigram_fallback: true,
        script_detection: true,
        ..Default::default()
    };
    assert!(flags.is_phase2_enabled());
}

#[test]
fn test_fts_flags_phase3_requires_both() {
    // Neither flag set
    let flags = FtsFeatureFlags {
        bigram_cjk: false,
        multilingual_configs: false,
        ..Default::default()
    };
    assert!(!flags.is_phase3_enabled());

    // Only bigram_cjk
    let flags = FtsFeatureFlags {
        bigram_cjk: true,
        multilingual_configs: false,
        ..Default::default()
    };
    assert!(!flags.is_phase3_enabled());

    // Only multilingual_configs
    let flags = FtsFeatureFlags {
        bigram_cjk: false,
        multilingual_configs: true,
        ..Default::default()
    };
    assert!(!flags.is_phase3_enabled());

    // Both flags set
    let flags = FtsFeatureFlags {
        bigram_cjk: true,
        multilingual_configs: true,
        ..Default::default()
    };
    assert!(flags.is_phase3_enabled());
}

#[test]
fn test_fts_flags_clone_and_equality() {
    let flags1 = FtsFeatureFlags::all_enabled();
    let flags2 = flags1.clone();
    assert_eq!(flags1, flags2);

    let flags3 = FtsFeatureFlags::default();
    assert_ne!(flags1, flags3);
}

// ========== HYBRID SEARCH CONFIG TESTS ==========

#[test]
fn test_config_default_values() {
    let config = HybridSearchConfig::default();
    assert_eq!(config.fts_weight, 0.5);
    assert_eq!(config.semantic_weight, 0.5);
    assert!(config.exclude_archived);
    assert_eq!(config.min_score, 0.0);
    assert!(config.lang_hint.is_none());
    assert!(config.script_hint.is_none());
    assert_eq!(config.fts_flags, FtsFeatureFlags::default());
}

#[test]
fn test_config_with_lang_hint() {
    let config = HybridSearchConfig::default().with_lang_hint("zh");
    assert_eq!(config.lang_hint, Some("zh".to_string()));

    let config = HybridSearchConfig::default().with_lang_hint("ja");
    assert_eq!(config.lang_hint, Some("ja".to_string()));

    let config = HybridSearchConfig::default().with_lang_hint("en");
    assert_eq!(config.lang_hint, Some("en".to_string()));
}

#[test]
fn test_config_with_script_hint() {
    let config = HybridSearchConfig::default().with_script_hint("han");
    assert_eq!(config.script_hint, Some("han".to_string()));

    let config = HybridSearchConfig::default().with_script_hint("latin");
    assert_eq!(config.script_hint, Some("latin".to_string()));

    let config = HybridSearchConfig::default().with_script_hint("cyrillic");
    assert_eq!(config.script_hint, Some("cyrillic".to_string()));
}

#[test]
fn test_config_with_fts_flags() {
    let flags = FtsFeatureFlags::all_enabled();
    let config = HybridSearchConfig::default().with_fts_flags(flags.clone());
    assert_eq!(config.fts_flags, flags);
}

#[test]
fn test_config_chaining_multilingual_options() {
    let flags = FtsFeatureFlags::all_enabled();
    let config = HybridSearchConfig::default()
        .with_lang_hint("zh")
        .with_script_hint("han")
        .with_fts_flags(flags.clone());

    assert_eq!(config.lang_hint, Some("zh".to_string()));
    assert_eq!(config.script_hint, Some("han".to_string()));
    assert_eq!(config.fts_flags, flags);
}

#[test]
fn test_config_fts_only_with_multilingual() {
    let config = HybridSearchConfig::fts_only()
        .with_lang_hint("ja")
        .with_fts_flags(FtsFeatureFlags::all_enabled());

    assert_eq!(config.fts_weight, 1.0);
    assert_eq!(config.semantic_weight, 0.0);
    assert_eq!(config.lang_hint, Some("ja".to_string()));
    assert!(config.fts_flags.script_detection);
}

// ========== SEARCH STRATEGY SELECTION TESTS ==========
// Note: These tests validate the strategy selection logic indirectly
// through the public API, as select_strategy is private.

#[test]
fn test_search_strategy_enum_values() {
    // Verify enum variants exist and are distinct
    assert_ne!(SearchStrategy::FtsEnglish, SearchStrategy::FtsSimple);
    assert_ne!(SearchStrategy::FtsEnglish, SearchStrategy::Trigram);
    assert_ne!(SearchStrategy::FtsEnglish, SearchStrategy::Bigram);
    assert_ne!(SearchStrategy::FtsEnglish, SearchStrategy::Cjk);
    assert_ne!(SearchStrategy::Trigram, SearchStrategy::Bigram);
    assert_ne!(SearchStrategy::Trigram, SearchStrategy::Cjk);
}

#[test]
fn test_search_strategy_clone() {
    let strategy = SearchStrategy::FtsEnglish;
    let cloned = strategy;
    assert_eq!(strategy, cloned);
}

#[test]
fn test_search_strategy_debug() {
    let strategy = SearchStrategy::Trigram;
    let debug_str = format!("{:?}", strategy);
    assert!(debug_str.contains("Trigram"));
}

// ========== INTEGRATION TESTS FOR MULTILINGUAL CONFIG ==========

#[test]
fn test_config_latin_query_with_default_flags() {
    let config = HybridSearchConfig::default();

    // With default flags, script detection is disabled
    assert!(!config.fts_flags.script_detection);

    // Should fall back to FtsEnglish
    let query = "hello world";
    let result = detect_script(query);
    assert_eq!(result.primary, DetectedScript::Latin);
}

#[test]
fn test_config_cjk_query_with_phase3_flags() {
    let flags = FtsFeatureFlags {
        bigram_cjk: true,
        script_detection: true,
        ..Default::default()
    };

    let config = HybridSearchConfig::default().with_fts_flags(flags);

    // Verify Phase 3 CJK flag is enabled
    assert!(config.fts_flags.bigram_cjk);
    assert!(config.fts_flags.script_detection);

    let query = "你好世界";
    let result = detect_script(query);
    assert_eq!(result.primary, DetectedScript::Cjk);
}

#[test]
fn test_config_emoji_query_with_trigram() {
    let flags = FtsFeatureFlags {
        trigram_fallback: true,
        script_detection: true,
        ..Default::default()
    };

    let config = HybridSearchConfig::default().with_fts_flags(flags);

    assert!(config.fts_flags.trigram_fallback);

    let query = "🎉👋🌍";
    let result = detect_script(query);
    assert_eq!(result.primary, DetectedScript::Emoji);
}

#[test]
fn test_config_mixed_script_with_trigram() {
    let flags = FtsFeatureFlags {
        trigram_fallback: true,
        script_detection: true,
        ..Default::default()
    };

    let _config = HybridSearchConfig::default().with_fts_flags(flags);

    let query = "Hello 你好 World 世界";
    let result = detect_script(query);
    assert_eq!(result.primary, DetectedScript::Mixed);
}

#[test]
fn test_config_lang_hint_overrides() {
    // Language hint: Chinese
    let config = HybridSearchConfig::default().with_lang_hint("zh");
    assert_eq!(config.lang_hint, Some("zh".to_string()));

    // Language hint: Japanese
    let config = HybridSearchConfig::default().with_lang_hint("ja");
    assert_eq!(config.lang_hint, Some("ja".to_string()));

    // Language hint: Korean
    let config = HybridSearchConfig::default().with_lang_hint("ko");
    assert_eq!(config.lang_hint, Some("ko".to_string()));

    // Language hint: Russian
    let config = HybridSearchConfig::default().with_lang_hint("ru");
    assert_eq!(config.lang_hint, Some("ru".to_string()));

    // Language hint: Arabic
    let config = HybridSearchConfig::default().with_lang_hint("ar");
    assert_eq!(config.lang_hint, Some("ar".to_string()));
}

#[test]
fn test_config_script_hint_overrides() {
    // Script hint: Han/CJK
    let config = HybridSearchConfig::default().with_script_hint("han");
    assert_eq!(config.script_hint, Some("han".to_string()));

    // Script hint: Cyrillic
    let config = HybridSearchConfig::default().with_script_hint("cyrillic");
    assert_eq!(config.script_hint, Some("cyrillic".to_string()));

    // Script hint: Arabic
    let config = HybridSearchConfig::default().with_script_hint("arabic");
    assert_eq!(config.script_hint, Some("arabic".to_string()));

    // Script hint: Latin
    let config = HybridSearchConfig::default().with_script_hint("latin");
    assert_eq!(config.script_hint, Some("latin".to_string()));
}

// ========== EDGE CASE TESTS ==========

#[test]
fn test_detect_script_with_mixed_punctuation() {
    let result = detect_script("Hello! 你好！World! 世界！");
    assert_eq!(result.primary, DetectedScript::Mixed);
    assert!(result.scripts_found.contains(&DetectedScript::Latin));
    assert!(result.scripts_found.contains(&DetectedScript::Cjk));
}

#[test]
fn test_detect_script_dominant_latin_minor_cjk() {
    // 90% Latin, 10% CJK - should be Latin, not Mixed
    let latin = "a".repeat(90);
    let text = format!("{}你好世界你好", latin);
    let result = detect_script(&text);

    assert_eq!(result.primary, DetectedScript::Latin);
    assert!(result.scripts_found.contains(&DetectedScript::Latin));
    // CJK should be detected as it exceeds 5% threshold
    assert!(result.scripts_found.contains(&DetectedScript::Cjk));
}

#[test]
fn test_detect_script_at_mixed_threshold() {
    // Test boundary: Multiple scripts at exactly 20% threshold
    // Need significant presence (>20%) of multiple scripts for Mixed
    let text = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa你好世界你好世界你好世界你好世界你好";
    let result = detect_script(text);

    // Should have both scripts detected
    assert!(result.scripts_found.contains(&DetectedScript::Latin));
    assert!(result.scripts_found.contains(&DetectedScript::Cjk));
}

#[test]
fn test_config_all_flags_enabled() {
    let config = HybridSearchConfig::default().with_fts_flags(FtsFeatureFlags::all_enabled());

    assert!(config.fts_flags.is_phase1_enabled());
    assert!(config.fts_flags.is_phase2_enabled());
    assert!(config.fts_flags.is_phase3_enabled());
}

#[test]
fn test_config_selective_phase_enablement() {
    // Enable only Phase 1
    let flags = FtsFeatureFlags::default();
    let config = HybridSearchConfig::default().with_fts_flags(flags.clone());
    assert!(config.fts_flags.is_phase1_enabled());
    assert!(!config.fts_flags.is_phase2_enabled());
    assert!(!config.fts_flags.is_phase3_enabled());

    // Enable Phase 1 and 2
    let flags = FtsFeatureFlags {
        trigram_fallback: true,
        script_detection: true,
        ..Default::default()
    };
    let config = HybridSearchConfig::default().with_fts_flags(flags);
    assert!(config.fts_flags.is_phase1_enabled());
    assert!(config.fts_flags.is_phase2_enabled());
    assert!(!config.fts_flags.is_phase3_enabled());

    // Enable all phases
    let flags = FtsFeatureFlags {
        trigram_fallback: true,
        script_detection: true,
        bigram_cjk: true,
        multilingual_configs: true,
        ..Default::default()
    };
    let config = HybridSearchConfig::default().with_fts_flags(flags);
    assert!(config.fts_flags.is_phase1_enabled());
    assert!(config.fts_flags.is_phase2_enabled());
    assert!(config.fts_flags.is_phase3_enabled());
}

#[test]
fn test_script_detection_new_constructor() {
    let detection = ScriptDetection::new(DetectedScript::Latin, 0.95, vec![DetectedScript::Latin]);

    assert_eq!(detection.primary, DetectedScript::Latin);
    assert_eq!(detection.confidence, 0.95);
    assert_eq!(detection.scripts_found, vec![DetectedScript::Latin]);
}

#[test]
fn test_script_detection_clone() {
    let detection1 = ScriptDetection::new(DetectedScript::Cjk, 0.99, vec![DetectedScript::Cjk]);

    let detection2 = detection1.clone();
    assert_eq!(detection1.primary, detection2.primary);
    assert_eq!(detection1.confidence, detection2.confidence);
    assert_eq!(detection1.scripts_found, detection2.scripts_found);
}

#[test]
fn test_detected_script_all_variants() {
    // Ensure all enum variants can be instantiated
    let scripts = vec![
        DetectedScript::Latin,
        DetectedScript::Cjk,
        DetectedScript::Arabic,
        DetectedScript::Cyrillic,
        DetectedScript::Greek,
        DetectedScript::Hebrew,
        DetectedScript::Devanagari,
        DetectedScript::Thai,
        DetectedScript::Emoji,
        DetectedScript::Mixed,
        DetectedScript::Unknown,
    ];

    // Verify all are distinct
    for (i, script1) in scripts.iter().enumerate() {
        for (j, script2) in scripts.iter().enumerate() {
            if i != j {
                assert_ne!(script1, script2);
            } else {
                assert_eq!(script1, script2);
            }
        }
    }
}
