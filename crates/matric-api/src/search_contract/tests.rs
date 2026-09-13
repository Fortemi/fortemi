use super::*;
use crate::{SearchDegradation, SearchQuery, SearchResponse};
use matric_core::{
    search_evidence::{
        EvidenceKind, EvidenceText, EvidenceUnit, SearchEvidenceLocator, SearchEvidenceSet,
    },
    SearchHit,
};
use matric_search::deduplication::{ChainSearchInfo, EnhancedSearchHit};
use std::collections::BTreeSet;
use uuid::Uuid;

fn bundle(name: &str) -> Value {
    json!({"$schema":"https://json-schema.org/draft/2020-12/schema",
        "$ref":format!("#/components/schemas/{name}"),"components":{"schemas":components()}})
}

fn validator(name: &str) -> jsonschema::Validator {
    jsonschema::options()
        .should_validate_formats(true)
        .build(&bundle(name))
        .unwrap()
}

fn response() -> SearchResponse {
    let id = Uuid::from_u128(1);
    let unit = EvidenceUnit {
        kind: EvidenceKind::Embedding,
        id: Uuid::from_u128(2).to_string(),
        index: 7,
    };
    let text = "\u{feff}raw\r\n\u{1f642}";
    let locator = SearchEvidenceLocator::bind(
        EvidenceText {
            note_id: &id.to_string(),
            unit: &unit,
            content: text,
            source: None,
        },
        0,
        text.len(),
    )
    .unwrap();
    let evidence = SearchEvidenceSet::new(&id.to_string(), vec![locator], vec![]).unwrap();
    let hit: SearchHit = serde_json::from_value(
        json!({"note_id":id,"score":0.5,"snippet":"display is not evidence","evidence":evidence}),
    )
    .unwrap();
    SearchResponse {
        results: vec![EnhancedSearchHit {
            hit,
            chain_info: Some(ChainSearchInfo {
                chain_id: id,
                original_title: "display".into(),
                chunks_matched: 1,
                best_chunk_sequence: 0,
                total_chunks: 1,
            }),
        }],
        query: "raw".into(),
        total: 1,
        degraded: false,
        degradation: None,
    }
}

fn assert_refs(value: &Value, root: &Value) -> usize {
    match value {
        Value::Object(map) => {
            assert!(
                !map.contains_key("$id"),
                "bundled resource must not reset reference base"
            );
            let own = if let Some(reference) = map.get("$ref") {
                let reference = reference.as_str().unwrap();
                assert!(
                    reference.starts_with("#/components/schemas/"),
                    "nonlocal reference"
                );
                assert!(
                    root.pointer(&reference[1..]).is_some(),
                    "unresolved reference: {reference}"
                );
                1
            } else {
                0
            };
            own + map
                .values()
                .map(|value| assert_refs(value, root))
                .sum::<usize>()
        }
        Value::Array(values) => values.iter().map(|value| assert_refs(value, root)).sum(),
        _ => 0,
    }
}

#[test]
fn resolution_authority_and_operation_are_bundled_offline() {
    let doc: serde_yaml::Value =
        serde_yaml::from_str(&crate::openapi_yaml_with_problem_contract()).unwrap();
    let doc = serde_json::to_value(doc).unwrap();
    let operation = &doc["paths"]["/api/v1/search/evidence/resolve"]["post"];
    assert_eq!(
        operation["requestBody"]["content"]["application/json"]["schema"]["$ref"],
        "#/components/schemas/SearchEvidenceResolveRequest"
    );
    assert_eq!(
        operation["responses"]["200"]["content"]["application/json"]["schema"]["$ref"],
        "#/components/schemas/SearchEvidenceResolveResponse"
    );
    assert_eq!(
        operation["x-fortemi-evidence-resolution"]["required_scope"],
        "read"
    );
    let vectors: Value = serde_json::from_str(include_str!(
        "../../../../contracts/metadata-search/candidate/1.0.0/evidence-resolution-vectors.json"
    ))
    .unwrap();
    for (group, name) in [
        ("requests", "SearchEvidenceResolveRequest"),
        ("responses", "SearchEvidenceResolveResponse"),
    ] {
        let validator = jsonschema::options()
            .should_validate_formats(true)
            .build(
                &json!({"$schema":"https://json-schema.org/draft/2020-12/schema",
            "$ref":format!("#/components/schemas/{name}"),"components":doc["components"]}),
            )
            .unwrap();
        for case in vectors[group].as_array().unwrap() {
            assert_eq!(
                validator.is_valid(&case["value"]),
                case["valid"].as_bool().unwrap(),
                "resolution vector {}",
                case["id"]
            );
        }
    }
}

#[test]
fn bundled_schemas_resolve_offline_and_preserve_conditionals() {
    let root = bundle("SearchRestResponse");
    assert!(assert_refs(&root, &root) > 20);
    for name in [
        "SearchRestRequest",
        "SearchRestResponse",
        "SearchHit",
        "SearchEvidenceSet",
        "MetadataPredicates",
    ] {
        validator(name);
    }
    assert!(
        root["components"]["schemas"]["SearchRestContract"]["$defs"]["SearchRestResponse"]["then"]
            ["required"]
            .is_array()
    );
    assert_eq!(
        root["components"]["schemas"]["SearchRestContract"]["$defs"]["EnhancedSearchHit"]
            ["unevaluatedProperties"],
        false
    );
}

#[test]
fn generated_yaml_bundle_enforces_shared_vectors_offline() {
    let yaml: serde_yaml::Value =
        serde_yaml::from_str(&crate::openapi_yaml_with_problem_contract()).unwrap();
    let document = serde_json::to_value(yaml).unwrap();
    for name in components().keys() {
        assert_refs(&document["components"]["schemas"][name], &document);
    }
    let vectors: Value = serde_json::from_str(include_str!(
        "../../../../contracts/metadata-search/candidate/1.0.0/search-rest-vectors.json"
    ))
    .unwrap();
    for (group, name) in [
        ("requests", "SearchRestRequest"),
        ("responses", "SearchRestResponse"),
    ] {
        let schema = json!({"$schema":"https://json-schema.org/draft/2020-12/schema","$ref":format!("#/components/schemas/{name}"),"components":document["components"]});
        let validator = jsonschema::options()
            .should_validate_formats(true)
            .build(&schema)
            .unwrap();
        for case in vectors[group].as_array().unwrap() {
            assert_eq!(
                validator.is_valid(&case["value"]),
                case["valid"].as_bool().unwrap(),
                "exported vector {}",
                case["id"]
            );
        }
        if group == "responses" {
            assert!(validator.is_valid(&serde_json::to_value(response()).unwrap()));
        }
    }
}

#[test]
fn generated_operation_covers_every_actual_query_field_and_json_encoding() {
    let yaml: serde_yaml::Value =
        serde_yaml::from_str(&crate::openapi_yaml_with_problem_contract()).unwrap();
    let doc = serde_json::to_value(yaml).unwrap();
    let operation = &doc["paths"]["/api/v1/search"]["get"];
    let parameters = operation["parameters"].as_array().unwrap();
    let query: SearchQuery = serde_urlencoded::from_str("q=").unwrap();
    let fields = serde_json::to_value(query)
        .unwrap()
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    let declared = parameters
        .iter()
        .filter(|p| p["in"] == "query")
        .map(|p| p["name"].as_str().unwrap().to_owned())
        .collect::<BTreeSet<_>>();
    assert_eq!(fields, declared);
    assert_eq!(parameters.len(), fields.len() + 1);
    for parameter in parameters {
        let json_encoded =
            ["strict_filter", "metadata_predicates"].contains(&parameter["name"].as_str().unwrap());
        assert_eq!(parameter.get("content").is_some(), json_encoded);
        assert_eq!(parameter.get("schema").is_some(), !json_encoded);
        assert_eq!(parameter["required"], parameter["name"] == "q");
    }
    assert_eq!(
        operation["responses"]["200"]["content"]["application/json"]["schema"]["$ref"],
        "#/components/schemas/SearchRestResponse"
    );
    assert_eq!(
        operation["x-fortemi-search-contract"]["evidence_complete_capability"],
        false
    );
    assert!(assert_refs(&doc["components"]["schemas"]["SearchRestContract"], &doc) > 5);
}

#[test]
fn bundling_does_not_reorder_unrelated_yaml_or_remove_problem_responses() {
    let mut doc: serde_yaml::Value = serde_yaml::from_str("z: first\na: second\ncomponents:\n  schemas: {}\npaths:\n  /api/v1/search:\n    get:\n      responses:\n        '200':\n          description: Success\n        '400':\n          description: Problem\n  /api/v1/search/evidence/resolve:\n    post:\n      responses:\n        '200':\n          description: Resolved\n        '404':\n          description: Unavailable\n").unwrap();
    let before = doc
        .as_mapping()
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    apply_openapi(&mut doc);
    assert_eq!(
        before,
        doc.as_mapping()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>()
    );
    assert_eq!(
        doc["paths"]["/api/v1/search"]["get"]["responses"]["400"]["description"].as_str(),
        Some("Problem")
    );
    assert_eq!(
        doc["paths"]["/api/v1/search/evidence/resolve"]["post"]["responses"]["404"]["description"]
            .as_str(),
        Some("Unavailable")
    );
}

#[test]
fn query_schema_matches_decoded_legacy_and_json_parameters() {
    let check = validator("SearchRestRequest");
    for limit in [0, 20, 1000] {
        let pairs = [
            ("q", "".to_owned()),
            ("limit", limit.to_string()),
            ("mode", "legacy-unknown".into()),
            ("set", "default".into()),
            (
                "strict_filter",
                "{\"min_tag_count\":-1,\"unknown\":true}".into(),
            ),
            ("metadata_predicates", "[]".into()),
            ("diversity", "2".into()),
            ("created_after", "2026-09-12T00:00:00Z".into()),
            ("since", "ignored-invalid-relative-time".into()),
        ];
        let query: SearchQuery =
            serde_urlencoded::from_str(&serde_urlencoded::to_string(pairs).unwrap()).unwrap();
        assert_eq!(validate_request(&query).unwrap(), limit);
        crate::parse_search_metadata_predicates(query.metadata_predicates.as_deref()).unwrap();
        let _: matric_core::StrictTagFilterInput =
            serde_json::from_str(query.strict_filter.as_ref().unwrap()).unwrap();
        let mut value = serde_json::to_value(query).unwrap();
        value.as_object_mut().unwrap().retain(|_, v| !v.is_null());
        for name in ["strict_filter", "metadata_predicates"] {
            value[name] = serde_json::from_str(value[name].as_str().unwrap()).unwrap();
        }
        assert!(check.is_valid(&value));
    }
    assert_eq!(
        validate_request(&serde_urlencoded::from_str("q=").unwrap()).unwrap(),
        20
    );
    assert!(serde_urlencoded::from_str::<SearchQuery>("limit=1").is_err());
}

#[test]
fn invalid_request_limits_and_non_finite_diversity_fail_before_io() {
    for raw in [
        "q=x&limit=-1",
        "q=x&limit=1001",
        "q=x&diversity=NaN",
        "q=x&diversity=inf",
        "q=x&diversity=-inf",
    ] {
        let query: SearchQuery = serde_urlencoded::from_str(raw).unwrap();
        assert!(validate_request(&query).is_err());
    }
    let check = validator("SearchRestRequest");
    for value in [
        json!({}),
        json!({"q":"x","limit":1001}),
        json!({"q":"x","limit":-1}),
        json!({"q":"x","limit":1.5}),
        json!({"q":"x","created_after":"invalid"}),
        json!({"q":"x","strict_filter":{"include_untagged":0}}),
        json!({"q":"x","metadata_predicates":[{"field":"unregistered","op":"eq","value":"x"}]}),
    ] {
        assert!(!check.is_valid(&value));
    }
}

#[test]
fn real_rust_wire_preserves_native_evidence_and_legacy_absence() {
    let check = validator("SearchRestResponse");
    let mut response = response();
    let value = serde_json::to_value(&response).unwrap();
    assert!(
        check.is_valid(&value),
        "schema rejected actual serialized response"
    );
    assert_eq!(
        value["results"][0]["evidence"]["locators"][0]["unit"]["index"],
        7
    );
    assert!(value.get("degradation").is_none());
    response.results[0].hit.evidence = None;
    response.results[0].chain_info = None;
    let value = serde_json::to_value(response).unwrap();
    assert!(check.is_valid(&value));
    assert!(value["results"][0].get("evidence").is_none());
    assert!(value["results"][0].get("chain_info").is_none());
}

#[test]
fn degraded_and_empty_responses_match_actual_wire() {
    let check = validator("SearchRestResponse");
    let mut response = response();
    response.degraded = true;
    response.degradation = Some(SearchDegradation {
        code: "embedding_unavailable".into(),
        effective_mode: "fts".into(),
    });
    assert!(check.is_valid(&serde_json::to_value(&response).unwrap()));
    response.results.clear();
    response.total = 0;
    assert!(check.is_valid(&serde_json::to_value(response).unwrap()));
}

#[test]
fn serialization_rechecks_cross_field_totals_degradation_and_note_binding() {
    for mutation in 0..9 {
        let mut response = response();
        match mutation {
            0 => response.total = 0,
            1 => response.degraded = true,
            2 => {
                response.degradation = Some(SearchDegradation {
                    code: "embedding_unavailable".into(),
                    effective_mode: "fts".into(),
                })
            }
            3 | 4 => {
                response.degraded = true;
                response.degradation = Some(SearchDegradation {
                    code: if mutation == 3 {
                        "PRIVATE invalid"
                    } else {
                        "valid_code"
                    }
                    .into(),
                    effective_mode: if mutation == 4 { "semantic" } else { "fts" }.into(),
                });
            }
            5 => response.results[0].chain_info.as_mut().unwrap().chain_id = Uuid::from_u128(3),
            6 => {
                response.results = vec![response.results[0].clone(); 1001];
                response.total = 1001;
            }
            7 => response.results[0].hit.score = f32::NAN,
            _ => response.results[0].hit.note_id = Uuid::from_u128(3),
        }
        let error = serde_json::to_value(response).unwrap_err().to_string();
        assert_eq!(error, "SEARCH_RESULT_INVALID");
    }
}

#[test]
fn strict_response_schema_rejects_malformed_json_and_unknown_fields() {
    let check = validator("SearchRestResponse");
    for mutation in 0..12 {
        let mut value = serde_json::to_value(response()).unwrap();
        match mutation {
            0 => value["unexpected"] = json!(true),
            1 => value["results"][0]["unexpected"] = json!(true),
            2 => value["results"][0]["evidence"] = Value::Null,
            3 => value["results"][0]["score"] = Value::Null,
            4 => value["degraded"] = json!(true),
            5 => value["degradation"] = Value::Null,
            6 => value["results"][0]["note_id"] = json!("invalid"),
            7 => value["results"][0]["chain_info"]["unexpected"] = json!(true),
            8 => value["results"][0]["evidence"]["version"] = json!("99.0.0"),
            9 => value["total"] = json!(1001),
            10 => value["results"][0]["chain_info"] = Value::Null,
            _ => {
                value.as_object_mut().unwrap().remove("query");
            }
        }
        assert!(
            !check.is_valid(&value),
            "accepted invalid mutation {mutation}"
        );
    }
}

#[test]
fn captured_released_http_search_bodies_remain_compatible() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../contracts/openapi/fixtures/remote-operations.json"
    ))
    .unwrap();
    let check = validator("SearchRestResponse");
    let mut count = 0;
    for (name, case) in fixture["cases"].as_object().unwrap() {
        if case["request"]["path"]
            .as_str()
            .is_some_and(|path| path.starts_with("/api/v1/search?"))
            && case["response"]["status"] == 200
        {
            let body = &case["response"]["body"];
            assert!(check.is_valid(body), "rejected captured fixture {name}");
            let actual: SearchResponse = serde_json::from_value(body.clone()).unwrap();
            // The capture's JavaScript encoder normalizes 1.0 to 1. Compare
            // scores in their declared f32 domain; retain exact other fields.
            let mut expected = body.clone();
            for hit in expected["results"].as_array_mut().unwrap() {
                hit["score"] = json!(hit["score"].as_f64().unwrap() as f32);
            }
            assert!(
                serde_json::to_value(actual).unwrap() == expected,
                "wire mismatch in {name}"
            );
            count += 1;
        }
    }
    assert!(count >= 3, "must validate real captured search cases");
}

#[test]
fn shared_rest_vectors_match_schema_and_actual_rust_serialization() {
    let vectors: Value = serde_json::from_str(include_str!(
        "../../../../contracts/metadata-search/candidate/1.0.0/search-rest-vectors.json"
    ))
    .unwrap();
    let request = validator("SearchRestRequest");
    let response = validator("SearchRestResponse");
    let mut count = 0;
    for (group, check) in [("requests", &request), ("responses", &response)] {
        for case in vectors[group].as_array().unwrap() {
            let valid = case["valid"].as_bool().unwrap();
            assert_eq!(
                check.is_valid(&case["value"]),
                valid,
                "vector {}",
                case["id"]
            );
            if valid && group == "responses" {
                let actual: SearchResponse = serde_json::from_value(case["value"].clone()).unwrap();
                assert!(check.is_valid(&serde_json::to_value(actual).unwrap()));
            } else if valid {
                let pairs = case["value"]
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(key, value)| {
                        (
                            key,
                            value
                                .as_str()
                                .map(str::to_owned)
                                .unwrap_or_else(|| value.to_string()),
                        )
                    })
                    .collect::<Vec<_>>();
                let actual: SearchQuery =
                    serde_urlencoded::from_str(&serde_urlencoded::to_string(pairs).unwrap())
                        .unwrap();
                validate_request(&actual).unwrap();
                crate::parse_search_metadata_predicates(actual.metadata_predicates.as_deref())
                    .unwrap();
                if let Some(raw) = actual.strict_filter {
                    let _: matric_core::StrictTagFilterInput = serde_json::from_str(&raw).unwrap();
                }
            }
            count += 1;
        }
    }
    assert_eq!(count, 20);
}
