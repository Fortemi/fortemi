use matric_core::search_evidence::SearchEvidenceSet;
use uuid::Uuid;

/// Missing legs must not turn partial evidence into an apparently complete set.
/// Invalid in-process identity withholds every locator; wire parsing/serialization
/// rejects it instead. No content or source identifiers enter diagnostics.
pub(crate) fn merge_evidence<'a>(
    note_id: Uuid,
    inputs: impl IntoIterator<Item = &'a Option<SearchEvidenceSet>>,
) -> Option<SearchEvidenceSet> {
    let inputs: Vec<_> = inputs.into_iter().collect();
    if inputs.iter().all(|value| value.is_none()) {
        return None;
    }
    let note_id = note_id.to_string();
    let unavailable = || {
        SearchEvidenceSet::new(&note_id, vec![], vec![])
            .expect("an empty evidence set always has an unavailable omission")
    };
    let sets: Vec<_> = inputs
        .into_iter()
        .map(|value| value.clone().unwrap_or_else(unavailable))
        .collect();
    Some(SearchEvidenceSet::merge(&note_id, &sets).unwrap_or_else(|_| unavailable()))
}
