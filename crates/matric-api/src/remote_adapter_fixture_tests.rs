use serde_json::Value;

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../contracts/openapi/fixtures/remote-adapter.json"
    ))
    .unwrap()
}

fn body(fixture: &Value, name: &str) -> Value {
    fixture["cases"][name]["response"]["body"].clone()
}

#[test]
fn remote_adapter_capture_uses_producer_models() {
    let fixture = fixture();
    assert_eq!(
        fixture["producer"]["commit"],
        "e91c595a896275f835cb7ed1aef173cb26056206"
    );
    for name in ["list_empty", "list_nonempty"] {
        let notes: matric_core::ListNotesResponse =
            serde_json::from_value(body(&fixture, name)).unwrap();
        assert_eq!(notes.notes.len() as i64, notes.total);
    }
    for name in [
        "detail_first",
        "detail_second",
        "detail_revised",
        "detail_starred",
    ] {
        let note: matric_core::NoteFull = serde_json::from_value(body(&fixture, name)).unwrap();
        assert!(!note.note.id.is_nil());
    }
    for name in ["links_empty", "links_outgoing", "links_incoming"] {
        let response = body(&fixture, name);
        for direction in ["incoming", "outgoing"] {
            let _: Vec<matric_core::Link> =
                serde_json::from_value(response[direction].clone()).unwrap();
        }
    }
    for name in ["search_empty", "search_nonempty", "search_tags"] {
        let response: super::SearchResponse = serde_json::from_value(body(&fixture, name)).unwrap();
        assert_eq!(response.results.len(), response.total);
    }
    let concepts: Vec<(
        matric_core::NoteSkosConceptTag,
        matric_core::SkosConceptWithLabel,
    )> = serde_json::from_value(body(&fixture, "concepts_nonempty")).unwrap();
    assert_eq!(concepts.len(), 2);
    for (tag, concept) in concepts {
        assert_eq!(tag.concept_id, concept.concept.id);
    }
    for name in ["provenance_empty", "provenance_nonempty"] {
        let graph = body(&fixture, name);
        let _: Option<matric_core::ProvenanceChain> =
            serde_json::from_value(graph["current_chain"].clone()).unwrap();
        let derived: Vec<uuid::Uuid> =
            serde_json::from_value(graph["derived_notes"].clone()).unwrap();
        assert_eq!(
            derived.len() as u64,
            graph["derived_count"].as_u64().unwrap()
        );
    }
    let provenance = body(&fixture, "provenance_nonempty");
    let activities: Vec<matric_core::ProvenanceActivity> =
        serde_json::from_value(provenance["all_activities"].clone()).unwrap();
    let edges: Vec<matric_core::ProvenanceEdge> =
        serde_json::from_value(provenance["all_edges"].clone()).unwrap();
    assert_eq!(activities.len(), 1);
    assert_eq!(edges.len(), 1);
    assert_eq!(activities[0].revision_id, Some(edges[0].revision_id));
    assert_eq!(fixture["cleanup"]["remainingVisibleNotes"], 0);
}

#[test]
fn remote_adapter_capture_rejects_malformed_required_fields() {
    let fixture = fixture();
    for field in ["id", "created_at_utc", "updated_at_utc"] {
        let mut note = body(&fixture, "detail_first");
        note["note"].as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<matric_core::NoteFull>(note).is_err());
    }
    let mut list = body(&fixture, "list_nonempty");
    list["notes"] = serde_json::json!({});
    assert!(serde_json::from_value::<matric_core::ListNotesResponse>(list).is_err());
}

#[test]
fn remote_operations_capture_preserves_search_degradation_and_mutation_contracts() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../contracts/openapi/fixtures/remote-operations.json"
    ))
    .unwrap();
    for name in [
        "search_empty",
        "search_nonempty",
        "search_tags",
        "search_limit",
        "search_tags_no_match",
        "search_semantic_degraded",
        "search_hybrid_degraded",
    ] {
        let search: super::SearchResponse = serde_json::from_value(body(&fixture, name)).unwrap();
        assert_eq!(search.results.len(), search.total);
        assert_eq!(search.degraded, name.ends_with("_degraded"));
        if search.degraded {
            assert_eq!(search.degradation.unwrap().effective_mode, "fts");
        }
    }
    for name in ["create_first", "create_second"] {
        let _: super::CreateNoteBody =
            serde_json::from_value(fixture["cases"][name]["request"]["body"].clone()).unwrap();
        let id: uuid::Uuid = serde_json::from_value(body(&fixture, name)["id"].clone()).unwrap();
        assert!(!id.is_nil());
        assert_eq!(fixture["cases"][name]["response"]["status"], 201);
    }
    for name in [
        "update_content",
        "update_tags",
        "star",
        "unstar",
        "archive",
        "unarchive",
    ] {
        let _: super::UpdateNoteBody =
            serde_json::from_value(fixture["cases"][name]["request"]["body"].clone()).unwrap();
        let _: matric_core::NoteFull = serde_json::from_value(body(&fixture, name)).unwrap();
    }
    assert_eq!(
        body(&fixture, "update_content")["revised"]["content"],
        "REMOTE CONTRACT UPDATED"
    );
    assert_eq!(body(&fixture, "archive")["note"]["archived"], true);
    assert_eq!(body(&fixture, "unarchive")["note"]["archived"], false);
    assert_eq!(fixture["cases"]["delete"]["response"]["status"], 204);
    assert!(body(&fixture, "delete").is_null());
    assert_eq!(
        fixture["cases"]["deleted_not_found"]["response"]["status"],
        404
    );
    assert_eq!(body(&fixture, "restore")["restored"], true);
    let restored: matric_core::NoteFull =
        serde_json::from_value(body(&fixture, "detail_restored")).unwrap();
    assert_eq!(
        serde_json::to_value(restored.note.id).unwrap(),
        body(&fixture, "restore")["id"]
    );
    assert_eq!(fixture["cleanup"]["remainingVisibleNotes"], 0);
}
