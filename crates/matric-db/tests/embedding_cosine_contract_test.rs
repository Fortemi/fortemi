use std::{fs, path::Path};

// PostgreSQL also uses <-> for tsquery phrase adjacency. These exact lines
// construct/test lexical phrases; they never compare embedding vectors. Keep
// the exception scoped to this file and these statements, not all query code.
const TSQUERY_PHRASE_LINES: [&str; 2] = [
    r#".join(" <-> ")"#,
    r#"("\"alpha the beta\"", "('alpha' <-> 'the' <-> 'beta')", true),"#,
];

fn forbidden_operator(path: &Path, line: &str) -> Option<&'static str> {
    [
        "vector_ip_ops",
        "halfvec_ip_ops",
        "vector_l2_ops",
        "halfvec_l2_ops",
        "<#>",
        "<->",
    ]
    .into_iter()
    .find(|&forbidden| {
        line.contains(forbidden)
            && !(forbidden == "<->"
                && path.ends_with("src/search_query.rs")
                && TSQUERY_PHRASE_LINES.contains(&line.trim()))
    })
}

fn scan(path: &Path, extension: &str, checked: &mut usize) {
    for entry in fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            scan(&path, extension, checked);
        } else if path.extension().is_some_and(|ext| ext == extension) {
            let source = fs::read_to_string(&path).unwrap();
            for (line, text) in source.lines().enumerate() {
                if let Some(forbidden) = forbidden_operator(&path, text) {
                    panic!(
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
fn only_documented_tsquery_phrase_statements_are_allowlisted() {
    let path = Path::new("src/search_query.rs");
    for line in TSQUERY_PHRASE_LINES {
        assert_eq!(forbidden_operator(path, line), None);
        // Identical text in another source file is not exempt.
        assert_eq!(
            forbidden_operator(Path::new("src/embedding.rs"), line),
            Some("<->")
        );
        // Extra vector SQL on an allowlisted line must still be rejected.
        assert_eq!(
            forbidden_operator(path, &format!("{line} e.vector <-> $1")),
            Some("<->")
        );
    }
}

#[test]
fn non_cosine_vector_queries_and_indexes_are_rejected_even_in_phrase_parser() {
    for path in [
        "src/search_query.rs",
        "src/embedding.rs",
        "migrations/test.sql",
    ] {
        for forbidden in [
            "vector_ip_ops",
            "halfvec_ip_ops",
            "vector_l2_ops",
            "halfvec_l2_ops",
            "<#>",
            "<->",
        ] {
            assert_eq!(
                forbidden_operator(Path::new(path), &format!("e.vector {forbidden} $1")),
                Some(forbidden)
            );
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
