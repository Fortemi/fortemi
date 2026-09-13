use matric_core::{
    search_evidence::{
        EvidenceKind, EvidenceText, EvidenceUnit, SearchEvidenceLocator, SearchEvidenceSet,
    },
    SearchHit,
};
use matric_search::{
    deduplication::{deduplicate_search_results, DeduplicationConfig, EnhancedSearchHit},
    mmr_rerank, rrf_fuse, rsf_fuse,
};
use pgvector::Vector;
use serde_json::{json, Value};
use std::collections::HashMap;
use uuid::Uuid;

fn hit(kind: EvidenceKind, index: u32) -> SearchHit {
    let note_id = Uuid::from_u128(1);
    let id = note_id.to_string();
    let unit = EvidenceUnit {
        kind,
        id: if kind == EvidenceKind::Embedding {
            Uuid::from_u128(index as u128 + 2).to_string()
        } else {
            id.clone()
        },
        index,
    };
    let text = "\u{feff}raw \u{1f642}\r\n";
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
    SearchHit {
        note_id,
        score: if kind == EvidenceKind::Embedding {
            0.9
        } else {
            0.5
        },
        snippet: Some("display only".into()),
        title: None,
        tags: vec![],
        embedding_status: None,
        evidence: Some(SearchEvidenceSet::new(&id, vec![locator], vec![]).unwrap()),
    }
}

fn outputs(hits: Vec<SearchHit>) -> Vec<SearchHit> {
    let lists = hits
        .iter()
        .cloned()
        .map(|hit| vec![hit])
        .collect::<Vec<_>>();
    let weights = vec![1.0 / lists.len() as f32; lists.len()];
    vec![
        rrf_fuse(lists.clone(), 10).remove(0),
        rsf_fuse(lists, &weights, 10).remove(0),
        deduplicate_search_results(hits, &DeduplicationConfig::default())
            .remove(0)
            .hit,
    ]
}

#[test]
fn actual_fusion_and_dedup_keep_both_matched_units_without_rebinding() {
    let title = hit(EvidenceKind::Title, 0);
    let embedding = hit(EvidenceKind::Embedding, 7);
    let expected = SearchEvidenceSet::merge(
        &title.note_id.to_string(),
        &[
            title.evidence.clone().unwrap(),
            embedding.evidence.clone().unwrap(),
        ],
    )
    .unwrap();
    for result in outputs(vec![title, embedding.clone(), embedding]) {
        assert_eq!(
            serde_json::to_value(result.evidence).unwrap(),
            serde_json::to_value(&expected).unwrap()
        );
    }
}

#[test]
fn fusion_marks_missing_legs_and_preserves_absent_support() {
    let mut missing = hit(EvidenceKind::Current, 0);
    missing.evidence = None;
    for result in outputs(vec![missing.clone(), missing.clone()]) {
        assert!(result.evidence.is_none());
    }
    for result in outputs(vec![missing, hit(EvidenceKind::Embedding, 7)]) {
        let value = serde_json::to_value(result.evidence).unwrap();
        assert_eq!(value["omissions"], json!(["unavailable-unit"]));
        assert_eq!(value["locators"].as_array().unwrap().len(), 1);
    }
}

#[test]
fn internal_foreign_identity_never_leaks_through_fusion() {
    let mut foreign = hit(EvidenceKind::Embedding, 7);
    foreign.note_id = Uuid::from_u128(8);
    for result in outputs(vec![foreign]) {
        let value = serde_json::to_value(result.evidence).unwrap();
        assert_eq!(value["locators"], json!([]));
        assert_eq!(value["omissions"], json!(["unavailable-unit"]));
    }
}

#[test]
fn actual_fusion_reports_locator_limit() {
    for result in outputs((0..65).map(|n| hit(EvidenceKind::Embedding, n)).collect()) {
        let value = serde_json::to_value(result.evidence).unwrap();
        assert_eq!(value["locators"].as_array().unwrap().len(), 64);
        assert_eq!(value["omissions"], json!(["locator-limit"]));
    }
}

#[test]
fn mmr_preserves_evidence_with_and_without_vectors() {
    let candidate = hit(EvidenceKind::Embedding, 7);
    let expected = serde_json::to_value(&candidate.evidence).unwrap();
    for diversity in [0.0, 0.5, 1.0] {
        for vectors in [
            HashMap::new(),
            HashMap::from([(candidate.note_id, Vector::from(vec![1.0, 0.0]))]),
        ] {
            let result = mmr_rerank(
                vec![candidate.clone()],
                &vectors,
                &Vector::from(vec![1.0, 0.0]),
                diversity,
                1,
            );
            assert_eq!(serde_json::to_value(&result[0].evidence).unwrap(), expected);
        }
    }
}

#[test]
fn flattened_enhanced_hit_round_trips_without_stripping_or_accepting_invalid_evidence() {
    let result = deduplicate_search_results(
        vec![hit(EvidenceKind::Embedding, 7)],
        &DeduplicationConfig::default(),
    )
    .remove(0);
    let value = serde_json::to_value(&result).unwrap();
    let decoded: EnhancedSearchHit = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), value);
    let mut invalid = value;
    invalid["evidence"] = Value::Null;
    assert!(serde_json::from_value::<EnhancedSearchHit>(invalid).is_err());
}
