/// Test to verify Cargo.toml metadata reflects Fortémi branding
use std::path::PathBuf;
use std::process::Command;

#[test]
fn test_workspace_metadata() {
    // Find workspace root by walking up from CARGO_MANIFEST_DIR
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .ancestors()
        .find(|p| p.join("Cargo.toml").exists() && p.join("crates").exists())
        .expect("Could not find workspace root");

    // Use CARGO env var set by cargo during test execution (guaranteed correct path)
    // Falls back to "cargo" in PATH if CARGO env not set
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());

    let output = Command::new(&cargo)
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .current_dir(workspace_root)
        .output()
        .unwrap_or_else(|e| panic!("Failed to execute cargo at '{}': {}", cargo, e));

    assert!(output.status.success(), "cargo metadata command failed");

    let metadata: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("Failed to parse cargo metadata JSON");

    // Test workspace metadata
    let workspace = metadata["packages"]
        .as_array()
        .expect("packages should be an array");

    for package in workspace {
        let name = package["name"].as_str().unwrap();
        let repository = package["repository"].as_str();
        let authors = package["authors"].as_array();

        // All packages should have correct repository
        assert_eq!(
            repository,
            Some("https://github.com/fortemi/fortemi"),
            "Package {} has incorrect repository",
            name
        );

        // All packages should have Fortémi Contributors as authors
        assert!(authors.is_some(), "Package {} missing authors", name);
        let authors = authors.unwrap();
        assert_eq!(
            authors.len(),
            1,
            "Package {} should have exactly one author entry",
            name
        );
        assert_eq!(
            authors[0].as_str().unwrap(),
            "Fortémi Contributors",
            "Package {} has incorrect authors",
            name
        );

        // All packages should have homepage
        let homepage = package["homepage"].as_str();
        assert_eq!(
            homepage,
            Some("https://github.com/fortemi/fortemi"),
            "Package {} missing or incorrect homepage",
            name
        );
    }
}
