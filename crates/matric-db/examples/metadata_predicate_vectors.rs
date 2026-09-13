//! Emit the actual Rust compiler's candidate queries for isolated SQL conformance.

use matric_core::metadata_search::MetadataPredicates;
use matric_db::{metadata_predicates::MetadataPredicateQueryBuilder, strict_filter::QueryParam};
use serde_json::{json, Value};

fn compile_cases(cases: &[Value]) -> Vec<Value> {
    cases
        .iter()
        .map(|case| {
            let mut output = case.clone();
            match MetadataPredicates::try_from(case["predicates"].clone()) {
                Ok(predicates) => {
                    assert_eq!(case["valid"], true, "{}", case["id"]);
                    let (sql, params) = MetadataPredicateQueryBuilder::new(&predicates, 0).build();
                    output["sql"] = json!(sql);
                    output["params"] = json!(params
                        .into_iter()
                        .map(|param| match param {
                            QueryParam::String(value) => value,
                            _ => panic!("unexpected metadata parameter type"),
                        })
                        .collect::<Vec<_>>());
                }
                Err(error) => {
                    assert_eq!(case["valid"], false, "{}", case["id"]);
                    assert_eq!(case["code"], error.to_string(), "{}", case["id"]);
                }
            }
            output
        })
        .collect()
}

fn main() {
    let corpus: Value = serde_json::from_str(include_str!(
        "../../../contracts/metadata-search/candidate/1.0.0/predicate-vectors.json"
    ))
    .expect("shared metadata predicate corpus");
    let scopes: Value = serde_json::from_str(include_str!(
        "../../../contracts/metadata-search/candidate/1.0.0/sql-scope-vectors.json"
    ))
    .expect("shared SQL scope corpus");
    let cases = compile_cases(corpus["cases"].as_array().unwrap());
    let sql_cases = compile_cases(scopes["cases"].as_array().unwrap());
    let plans: Vec<Value> = scopes["planPredicates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|plan| {
            let mut plan = plan.clone();
            plan["valid"] = json!(true);
            plan
        })
        .collect();
    let mut output = corpus;
    output["cases"] = json!(cases);
    output["sqlCases"] = json!(sql_cases);
    output["plans"] = json!(compile_cases(&plans));
    println!("{}", serde_json::to_string(&output).unwrap());
}
