use std::{fs, path::Path};

fn scan(path: &Path, extension: &str, checked: &mut usize) {
    for entry in fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            scan(&path, extension, checked);
        } else if path.extension().is_some_and(|ext| ext == extension) {
            let source = fs::read_to_string(&path).unwrap();
            // All current distance operators here are embedding searches. There
            // are no non-embedding exceptions; add a documented allowlist if needed.
            for forbidden in [
                "vector_ip_ops",
                "halfvec_ip_ops",
                "vector_l2_ops",
                "halfvec_l2_ops",
                "<#>",
                "<->",
            ] {
                for (line, text) in source.lines().enumerate() {
                    assert!(
                        !text.contains(forbidden),
                        "{}:{}: non-cosine embedding search: {forbidden}",
                        path.display(),
                        line + 1
                    );
                }
            }
            *checked += 1;
        }
    }
}

#[test]
fn embedding_indexes_and_queries_remain_cosine_for_normalization_compatibility() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut migrations = 0;
    let mut queries = 0;
    scan(&root.join("../../migrations"), "sql", &mut migrations);
    scan(&root.join("src"), "rs", &mut queries);
    assert!(migrations > 0 && queries > 0);
}
