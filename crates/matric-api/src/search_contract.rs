use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::LazyLock;

const ROOT: &str = "https://fortemi.com/contracts/metadata-search/candidate/1.0.0/";
const REST: &str =
    include_str!("../../../contracts/metadata-search/candidate/1.0.0/search-rest.schema.json");
const SCHEMAS: &[(&str, &str, &str)] = &[
    ("SearchRestContract", "search-rest.schema.json", REST),
    (
        "SearchEvidenceResolutionContract",
        "evidence-resolution.schema.json",
        include_str!(
            "../../../contracts/metadata-search/candidate/1.0.0/evidence-resolution.schema.json"
        ),
    ),
    (
        "SearchEvidenceLocator",
        "evidence-locator.schema.json",
        include_str!(
            "../../../contracts/metadata-search/candidate/1.0.0/evidence-locator.schema.json"
        ),
    ),
    (
        "SearchEvidenceSet",
        "evidence-set.schema.json",
        include_str!("../../../contracts/metadata-search/candidate/1.0.0/evidence-set.schema.json"),
    ),
    (
        "MetadataPredicates",
        "predicates.schema.json",
        include_str!("../../../contracts/metadata-search/candidate/1.0.0/predicates.schema.json"),
    ),
];

// Relocate references without round-tripping JSON Schema through utoipa's
// smaller schema model, which would discard conditional constraints.
fn relocate(value: &mut Value, current: &str) {
    match value {
        Value::Object(object) => {
            object.remove("$id");
            if let Some(reference) = object.get_mut("$ref") {
                let raw = reference.as_str().expect("schema reference string");
                let (resource, fragment) = raw.split_once('#').unwrap_or((raw, ""));
                let target = if resource.is_empty() {
                    current
                } else {
                    let file = resource.strip_prefix(ROOT).unwrap_or(resource);
                    SCHEMAS
                        .iter()
                        .find(|(_, name, _)| *name == file)
                        .map(|(component, _, _)| *component)
                        .expect("reference must be in local search registry")
                };
                *reference = json!(format!("#/components/schemas/{target}{fragment}"));
            }
            for child in object.values_mut() {
                relocate(child, current);
            }
        }
        Value::Array(values) => {
            for child in values {
                relocate(child, current);
            }
        }
        _ => {}
    }
}

pub(super) fn components() -> BTreeMap<String, Value> {
    let mut components = BTreeMap::new();
    for (name, _, bytes) in SCHEMAS {
        let mut schema: Value = serde_json::from_str(bytes).expect("embedded search schema");
        relocate(&mut schema, name);
        components.insert((*name).to_owned(), schema);
    }
    for name in [
        "SearchHit",
        "EnhancedSearchHit",
        "ChainSearchInfo",
        "SearchDegradation",
        "SearchRestRequest",
        "SearchRestResponse",
    ] {
        components.insert(
            name.to_owned(),
            json!({"$ref":format!("#/components/schemas/SearchRestContract/$defs/{name}")}),
        );
    }
    for name in [
        "SearchEvidenceResolveRequest",
        "SearchEvidenceResolveResponse",
    ] {
        components.insert(name.to_owned(), json!({"$ref":format!("#/components/schemas/SearchEvidenceResolutionContract/$defs/{name}")}));
    }
    components
}

fn sorted_yaml(mut value: Value) -> serde_yaml::Value {
    fn sort(value: &mut Value) {
        match value {
            Value::Object(object) => {
                for child in object.values_mut() {
                    sort(child);
                }
                object.sort_keys();
            }
            Value::Array(values) => {
                for child in values {
                    sort(child);
                }
            }
            _ => {}
        }
    }
    // serde_json/preserve_order can be enabled by dev dependencies. The CLI
    // and test builds must emit the same artifact under either feature set.
    sort(&mut value);
    serde_yaml::to_value(value).expect("OpenAPI search fragment")
}

pub(super) fn apply_openapi(document: &mut serde_yaml::Value) {
    let schemas = document["components"]["schemas"]
        .as_mapping_mut()
        .expect("OpenAPI schemas");
    for (name, schema) in components() {
        schemas.insert(serde_yaml::Value::String(name), sorted_yaml(schema));
    }
    let request: Value = serde_json::from_str(REST).expect("search request schema");
    let definition = &request["$defs"]["SearchRestRequest"];
    let mut parameters = Vec::new();
    for (name, schema) in definition["properties"]
        .as_object()
        .expect("request properties")
    {
        let mut schema = schema.clone();
        let content_type = schema
            .as_object_mut()
            .unwrap()
            .remove("x-fortemi-query-content");
        relocate(&mut schema, "SearchRestContract");
        let mut parameter = json!({"name":name,"in":"query","required":name == "q"});
        if let Some(media_type) = content_type {
            parameter["content"] = json!({media_type.as_str().unwrap():{"schema":schema}});
        } else {
            parameter["schema"] = schema;
        }
        parameters.push(parameter);
    }
    parameters.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    parameters.push(json!({"name":"X-Fortemi-Memory","in":"header","required":false,
        "description":"Select an accessible memory; middleware verifies tenant membership. This header is not an authorization grant.","schema":{"type":"string"}}));
    let operation = &mut document["paths"]["/api/v1/search"]["get"];
    assert!(operation.is_mapping(), "search operation must exist");
    operation["parameters"] = sorted_yaml(json!(parameters));
    operation["responses"]["200"]["content"] = sorted_yaml(
        json!({"application/json":{"schema":{"$ref":"#/components/schemas/SearchRestResponse"}}}),
    );
    operation["x-fortemi-search-contract"] = sorted_yaml(
        json!({"authority":"contracts/metadata-search/candidate/1.0.0/search-rest.schema.json",
        "status":"candidate-unpublished","consumer_issues":["Fortemi/fortemi-react#405"],
        "scope_identity":["verified_tenant","active_memory"],"total_semantics":"returned-hits",
        "evidence_complete_capability":false}),
    );
    let operation = &mut document["paths"]["/api/v1/search/evidence/resolve"]["post"];
    assert!(
        operation.is_mapping(),
        "evidence resolution operation must exist"
    );
    operation["requestBody"] = sorted_yaml(
        json!({"required":true,"content":{"application/json":{"schema":{"$ref":"#/components/schemas/SearchEvidenceResolveRequest"}}}}),
    );
    operation["responses"]["200"]["content"] = sorted_yaml(
        json!({"application/json":{"schema":{"$ref":"#/components/schemas/SearchEvidenceResolveResponse"}}}),
    );
    operation["parameters"] = sorted_yaml(
        json!([{"name":"X-Fortemi-Memory","in":"header","required":false,"schema":{"type":"string"}}]),
    );
    operation["x-fortemi-evidence-resolution"] = sorted_yaml(
        json!({"status":"candidate-unpublished","max_request_bytes":65536,"max_unit_utf8_bytes":16777216,
        "required_scope":"read","snapshot":"current-authorized-statement","cache":"no-store","evidence_complete_capability":false}),
    );
}

pub(super) fn validate_request(query: &super::SearchQuery) -> Result<i64, super::ApiError> {
    let limit = query
        .limit
        .unwrap_or(matric_core::defaults::PAGE_LIMIT_SEARCH);
    if !(0..=1000).contains(&limit) {
        return Err(super::ApiError::BadRequest(
            "search limit must be between 0 and 1000".into(),
        ));
    }
    if query.diversity.is_some_and(|value| !value.is_finite()) {
        return Err(super::ApiError::BadRequest("SEARCH_REQUEST_INVALID".into()));
    }
    Ok(limit)
}

pub(super) fn validate_response(response: &super::SearchResponse) -> Result<(), &'static str> {
    static CODE: LazyLock<regex::Regex> = LazyLock::new(|| {
        let schema: Value = serde_json::from_str(REST).expect("search response schema");
        regex::Regex::new(
            schema["$defs"]["SearchDegradation"]["properties"]["code"]["pattern"]
                .as_str()
                .unwrap(),
        )
        .unwrap()
    });
    if response.total != response.results.len()
        || response.total > 1000
        || response.degraded != response.degradation.is_some()
        || response
            .degradation
            .as_ref()
            .is_some_and(|d| d.effective_mode != "fts" || !CODE.is_match(&d.code))
        || response.results.iter().any(|hit| {
            hit.chain_info
                .as_ref()
                .is_some_and(|chain| chain.chain_id != hit.hit.note_id)
        })
    {
        return Err("SEARCH_RESULT_INVALID");
    }
    Ok(())
}

#[cfg(test)]
mod tests;
