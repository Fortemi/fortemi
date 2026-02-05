//! Integration tests for shard downgrade/upgrade analysis (Issue #428).

use matric_core::shard::{
    analyze_downgrade_impact, format_downgrade_message, format_upgrade_message,
    generate_upgrade_guidance, DataLossOutcome,
};
use serde_json::json;

#[test]
fn test_downgrade_impact_analysis_integration() {
    let manifest = json!({
        "version": "1.2.0",
        "embeddings": [
            {"id": "1", "vector": [0.1, 0.2], "truncate_dim": 128},
            {"id": "2", "vector": [0.3, 0.4], "truncate_dim": 256}
        ]
    });

    let impact = analyze_downgrade_impact("1.2.0", "1.0.0", &manifest);

    assert_eq!(impact.shard_version, "1.2.0");
    assert_eq!(impact.current_version, "1.0.0");
    assert_eq!(impact.data_loss.len(), 1);

    let loss = &impact.data_loss[0];
    assert_eq!(loss.component, "embeddings");
    assert_eq!(loss.affected_count, 2);
    assert_eq!(loss.outcome, DataLossOutcome::Discarded);
}

#[test]
fn test_downgrade_message_formatting_integration() {
    let manifest = json!({
        "version": "1.1.0",
        "embeddings": [
            {"id": "1", "vector": [0.1], "truncate_dim": 128}
        ]
    });

    let impact = analyze_downgrade_impact("1.1.0", "1.0.0", &manifest);
    let message = format_downgrade_message(&impact);

    assert!(message.contains("1.1.0"));
    assert!(message.contains("1.0.0"));
    // Check for warning indicator
    assert!(message.contains("⚠️") || message.contains("Importing shard"));
}

#[test]
fn test_upgrade_guidance_generation_integration() {
    let guidance = generate_upgrade_guidance("1.0.0", "2.0.0");

    assert_eq!(guidance.from_version, "1.0.0");
    assert_eq!(guidance.to_version, "2.0.0");
    assert!(!guidance.steps.is_empty());

    // Major version upgrade should be complex
    assert_eq!(
        guidance.difficulty,
        matric_core::shard::UpgradeDifficulty::Complex
    );
}

#[test]
fn test_upgrade_message_formatting_integration() {
    let guidance = generate_upgrade_guidance("1.0.0", "1.5.0");
    let message = format_upgrade_message(&guidance);

    assert!(message.contains("1.0.0"));
    assert!(message.contains("1.5.0"));
    assert!(message.contains("Difficulty"));
}

#[test]
fn test_end_to_end_downgrade_workflow() {
    // Simulate importing a shard from version 1.5.0 into a 1.0.0 system
    let shard_manifest = json!({
        "version": "1.5.0",
        "notes": [
            {"id": "note1", "content": "Hello"},
            {"id": "note2", "content": "World"}
        ],
        "embeddings": [
            {"id": "emb1", "vector": [0.1, 0.2, 0.3], "truncate_dim": 128},
            {"id": "emb2", "vector": [0.4, 0.5, 0.6]}
        ]
    });

    // Step 1: Analyze the impact
    let impact = analyze_downgrade_impact("1.5.0", "1.0.0", &shard_manifest);

    // Step 2: Check if we can proceed (only 1 embedding affected, should be OK)
    assert!(
        impact.can_proceed,
        "Should be able to proceed with minor data loss"
    );

    // Step 3: Generate user-friendly message
    let message = format_downgrade_message(&impact);

    // Step 4: Verify message quality
    assert!(message.contains("1.5.0"));
    assert!(message.contains("1.0.0"));
}

#[test]
fn test_end_to_end_upgrade_workflow() {
    // Simulate a user trying to import a shard that requires a newer version
    let current_version = "1.0.0";
    let required_version = "2.0.0";

    // Step 1: Generate upgrade guidance
    let guidance = generate_upgrade_guidance(current_version, required_version);

    // Step 2: Format as user message
    let message = format_upgrade_message(&guidance);

    // Step 3: Verify guidance is actionable
    assert!(message.contains(required_version));
    assert!(message.contains("Difficulty"));

    // Step 4: Verify steps are present
    assert!(!guidance.steps.is_empty());
}

#[test]
fn test_large_data_loss_blocks_import() {
    // Create a manifest with >100 items that would be discarded
    let mut embeddings = Vec::new();
    for i in 0..150 {
        embeddings.push(json!({
            "id": format!("emb{}", i),
            "vector": [0.1, 0.2, 0.3],
            "truncate_dim": 128
        }));
    }

    let manifest = json!({
        "version": "2.0.0",
        "embeddings": embeddings
    });

    let impact = analyze_downgrade_impact("2.0.0", "1.0.0", &manifest);

    // Should NOT proceed when significant data loss would occur
    assert!(!impact.can_proceed);
    assert!(impact.data_loss[0].affected_count >= 100);
}
