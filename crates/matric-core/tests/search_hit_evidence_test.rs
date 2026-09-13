use matric_core::{
    search_evidence::{
        EvidenceKind, EvidenceText, EvidenceUnit, SearchEvidenceLocator, SearchEvidenceSet,
    },
    SearchHit,
};
use serde_json::{json, Value};
use uuid::Uuid;

fn wire() -> Value {
    let id = Uuid::from_u128(1).to_string();
    let unit = EvidenceUnit {
        kind: EvidenceKind::Embedding,
        id: Uuid::from_u128(2).to_string(),
        index: 7,
    };
    let text = "\u{feff}raw\r\n\u{1f642}";
    let locator = SearchEvidenceLocator::bind(
        EvidenceText {
            note_id: &id,
            unit: &unit,
            content: text,
            source: None,
        },
        0,
        text.len(),
    )
    .unwrap();
    let evidence = SearchEvidenceSet::new(&id, vec![locator], vec![]).unwrap();
    json!({"note_id":id,"score":0.5,"snippet":"rendered, not evidence","evidence":evidence})
}

#[test]
fn hit_round_trip_preserves_exact_validated_evidence() {
    let value = wire();
    let hit: SearchHit = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(&hit).unwrap(), value);
    assert_eq!(hit.evidence.as_ref().unwrap().locators()[0].unit().index, 7);
    let debug = format!("{hit:?}");
    for secret in [
        "sha256:",
        "rendered",
        "00000000-0000-0000-0000-000000000002",
    ] {
        assert!(!debug.contains(secret));
    }
}

#[test]
fn missing_evidence_retains_legacy_wire_shape() {
    let mut value = wire();
    value.as_object_mut().unwrap().remove("evidence");
    let hit: SearchHit = serde_json::from_value(value.clone()).unwrap();
    assert!(hit.evidence.is_none());
    assert_eq!(serde_json::to_value(hit).unwrap(), value);
}

#[test]
fn malformed_or_foreign_evidence_is_rejected_without_values() {
    for mutation in 0..7 {
        let mut value = wire();
        match mutation {
            0 => value["evidence"] = Value::Null,
            1 => value["note_id"] = json!(Uuid::from_u128(3)),
            2 => value["evidence"]["raw_external_key"] = json!("PRIVATE"),
            3 => value["evidence"]["locators"][0]["span"]["start"] = json!(100),
            4 => value["evidence"]["version"] = json!("9.0.0"),
            5 => value["evidence"]["locators"] = json!([]),
            _ => {
                let duplicate = value["evidence"]["locators"][0].clone();
                value["evidence"]["locators"]
                    .as_array_mut()
                    .unwrap()
                    .push(duplicate);
            }
        }
        let error = serde_json::from_value::<SearchHit>(value)
            .unwrap_err()
            .to_string();
        assert!(error.contains("SEARCH_EVIDENCE_INVALID"));
        assert!(!error.contains("PRIVATE"));
    }
}

#[test]
fn mutated_hit_identity_is_rejected_at_serialization() {
    let mut hit: SearchHit = serde_json::from_value(wire()).unwrap();
    hit.note_id = Uuid::from_u128(3);
    assert_eq!(
        serde_json::to_value(hit).unwrap_err().to_string(),
        "SEARCH_EVIDENCE_INVALID"
    );
}

#[test]
fn non_finite_scores_fail_serialization_without_silently_becoming_null() {
    let mut hit: SearchHit = serde_json::from_value(wire()).unwrap();
    for score in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        hit.score = score;
        assert_eq!(
            serde_json::to_value(&hit).unwrap_err().to_string(),
            "SEARCH_RESULT_INVALID"
        );
    }
    for score in [f32::MIN, -0.5, 0.0, 0.5, f32::MAX] {
        hit.score = score;
        assert!(serde_json::to_value(&hit).unwrap()["score"].is_number());
    }
}

#[test]
fn candidate_schema_references_exact_authority_not_an_unconstrained_object() {
    use utoipa::PartialSchema;
    let schema = serde_json::to_value(SearchEvidenceSet::schema()).unwrap();
    assert_eq!(
        schema,
        json!({"$ref":"https://fortemi.com/contracts/metadata-search/candidate/1.0.0/evidence-set.schema.json"})
    );
    let hit_schema = serde_json::to_value(SearchHit::schema()).unwrap();
    assert!(hit_schema["properties"]["evidence"]
        .to_string()
        .contains("SearchEvidenceSet"));
    assert!(!hit_schema["required"]
        .as_array()
        .unwrap()
        .contains(&json!("evidence")));
}
