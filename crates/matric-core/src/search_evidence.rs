//! Candidate citation byte binding, not authentication or database resolution.

use std::{fmt, sync::LazyLock};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub const MAX_EVIDENCE_BYTES: usize = 16 * 1024 * 1024;
pub const EVIDENCE_SCHEMA: &str =
    include_str!("../../../contracts/metadata-search/candidate/1.0.0/evidence-locator.schema.json");
static VALIDATOR: LazyLock<jsonschema::Validator> = LazyLock::new(|| {
    let schema: Value = serde_json::from_str(EVIDENCE_SCHEMA).expect("embedded evidence schema");
    jsonschema::validator_for(&schema).expect("valid embedded evidence schema")
});

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    Current,
    Title,
    Embedding,
    Attachment,
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EvidenceUnit {
    pub kind: EvidenceKind,
    pub id: String,
    pub index: u32,
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EvidenceSource {
    pub namespace: String,
    pub external_id_hash: String,
    pub import_run_id: String,
    pub schema_version: String,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct EvidenceSpan {
    unit: String,
    start: usize,
    end: usize,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LocatorFields {
    version: String,
    note_id: String,
    unit: EvidenceUnit,
    content_digest: String,
    span: EvidenceSpan,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<EvidenceSource>,
}

/// The fields cannot be changed after schema/semantic validation.
#[derive(Clone, Serialize)]
#[serde(transparent)]
pub struct SearchEvidenceLocator(LocatorFields);

impl fmt::Debug for SearchEvidenceLocator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SearchEvidenceLocator")
            .field("kind", &self.0.unit.kind)
            .field("span_bytes", &(self.0.span.end - self.0.span.start))
            .field("source_set", &self.0.source.is_some())
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SearchEvidenceError {
    #[error("SEARCH_EVIDENCE_INVALID")]
    Invalid,
    #[error("SEARCH_EVIDENCE_UNAVAILABLE")]
    Unavailable,
}

impl TryFrom<Value> for SearchEvidenceLocator {
    type Error = SearchEvidenceError;

    fn try_from(mut value: Value) -> Result<Self, Self::Error> {
        if !VALIDATOR.is_valid(&value) {
            return Err(SearchEvidenceError::Invalid);
        }
        // JSON Schema integers include 1.0. Normalize only schema-bounded integer
        // fields before serde's stricter integer deserializer, without rounding.
        for path in ["/unit/index", "/span/start", "/span/end"] {
            let field = value
                .pointer_mut(path)
                .ok_or(SearchEvidenceError::Invalid)?;
            let number = field.as_f64().ok_or(SearchEvidenceError::Invalid)?;
            *field = Value::from(number as u64);
        }
        let fields: LocatorFields =
            serde_json::from_value(value).map_err(|_| SearchEvidenceError::Invalid)?;
        if fields.span.start > fields.span.end
            || (matches!(
                fields.unit.kind,
                EvidenceKind::Current | EvidenceKind::Title
            ) && fields.unit.id != fields.note_id)
        {
            return Err(SearchEvidenceError::Invalid);
        }
        Ok(Self(fields))
    }
}

/// Only supply a snapshot after the caller's tenant/archive/auth/deletion/purge
/// checks. This pure value is not proof of authorization and performs no I/O.
pub struct EvidenceText<'a> {
    pub note_id: &'a str,
    pub unit: &'a EvidenceUnit,
    pub content: &'a str,
    pub source: Option<&'a EvidenceSource>,
}

impl SearchEvidenceLocator {
    pub fn note_id(&self) -> &str {
        &self.0.note_id
    }

    pub fn source(&self) -> Option<&EvidenceSource> {
        self.0.source.as_ref()
    }

    pub fn unit(&self) -> &EvidenceUnit {
        &self.0.unit
    }

    /// Bind the exact raw UTF-8 text, without normalization or HTML processing.
    pub fn bind(
        text: EvidenceText<'_>,
        start: usize,
        end: usize,
    ) -> Result<Self, SearchEvidenceError> {
        if text.content.len() > MAX_EVIDENCE_BYTES || text.content.get(start..end).is_none() {
            return Err(SearchEvidenceError::Invalid);
        }
        let mut value = serde_json::json!({
            "version": "1.0.0", "note_id": text.note_id, "unit": text.unit,
            "content_digest": format!("sha256:{}", hex::encode(Sha256::digest(text.content.as_bytes()))),
            "span": { "unit": "utf8-bytes", "start": start, "end": end }
        });
        if let Some(source) = text.source {
            value["source"] =
                serde_json::to_value(source).map_err(|_| SearchEvidenceError::Invalid)?;
        }
        Self::try_from(value)
    }

    /// A missing, changed or mismatched source has one non-disclosing outcome.
    /// A digest does not authorize access or guarantee historical retention.
    pub fn resolve<'a>(
        &self,
        text: Option<EvidenceText<'a>>,
    ) -> Result<&'a str, SearchEvidenceError> {
        let text = text.ok_or(SearchEvidenceError::Unavailable)?;
        if text.content.len() > MAX_EVIDENCE_BYTES
            || self.0.note_id != text.note_id
            || &self.0.unit != text.unit
            || self.0.source.as_ref() != text.source
            || self.0.content_digest
                != format!(
                    "sha256:{}",
                    hex::encode(Sha256::digest(text.content.as_bytes()))
                )
        {
            return Err(SearchEvidenceError::Unavailable);
        }
        text.content
            .get(self.0.span.start..self.0.span.end)
            .ok_or(SearchEvidenceError::Unavailable)
    }
}

pub const MAX_SEARCH_LOCATORS: usize = 64;
const SET_SCHEMA: &str =
    include_str!("../../../contracts/metadata-search/candidate/1.0.0/evidence-set.schema.json");

impl utoipa::PartialSchema for SearchEvidenceSet {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        // Keep the candidate authority's conditional constraints intact. Do not
        // substitute a lossy generated object schema for the JSON Schema dialect.
        utoipa::openapi::Ref::new(
            "https://fortemi.com/contracts/metadata-search/candidate/1.0.0/evidence-set.schema.json",
        ).into()
    }
}

impl utoipa::ToSchema for SearchEvidenceSet {}
static SET_VALIDATOR: LazyLock<jsonschema::Validator> = LazyLock::new(|| {
    let locator: Value = serde_json::from_str(EVIDENCE_SCHEMA).expect("embedded locator schema");
    let schema: Value = serde_json::from_str(SET_SCHEMA).expect("embedded evidence set schema");
    let registry = jsonschema::Registry::new()
        .add(
            locator["$id"].as_str().expect("locator schema ID"),
            locator.clone(),
        )
        .expect("locator resource")
        .prepare()
        .expect("local schema registry");
    jsonschema::options()
        .with_registry(&registry)
        .build(&schema)
        .expect("evidence set schema")
});

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum EvidenceOmission {
    UnavailableUnit,
    LocatorLimit,
}

#[derive(Clone, Serialize)]
pub struct SearchEvidenceSet {
    version: &'static str,
    locators: Vec<SearchEvidenceLocator>,
    omissions: Vec<EvidenceOmission>,
}

impl fmt::Debug for SearchEvidenceSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SearchEvidenceSet")
            .field("locator_count", &self.locators.len())
            .field("omissions", &self.omissions)
            .finish()
    }
}

fn compare_locators(a: &SearchEvidenceLocator, b: &SearchEvidenceLocator) -> std::cmp::Ordering {
    let priority = |kind| match kind {
        EvidenceKind::Embedding => 0,
        EvidenceKind::Title => 1,
        EvidenceKind::Current => 2,
        EvidenceKind::Attachment => 3,
    };
    let a = &a.0;
    let b = &b.0;
    priority(a.unit.kind)
        .cmp(&priority(b.unit.kind))
        .then_with(|| a.unit.id.cmp(&b.unit.id))
        .then_with(|| a.unit.index.cmp(&b.unit.index))
        .then_with(|| a.content_digest.cmp(&b.content_digest))
        .then_with(|| a.span.start.cmp(&b.span.start))
        .then_with(|| a.span.end.cmp(&b.span.end))
        .then_with(|| match (&a.source, &b.source) {
            (None, None) => std::cmp::Ordering::Equal,
            (None, Some(_)) => std::cmp::Ordering::Less,
            (Some(_), None) => std::cmp::Ordering::Greater,
            (Some(a), Some(b)) => a
                .namespace
                .cmp(&b.namespace)
                .then_with(|| a.external_id_hash.cmp(&b.external_id_hash))
                .then_with(|| a.import_run_id.cmp(&b.import_run_id))
                .then_with(|| a.schema_version.cmp(&b.schema_version)),
        })
}

impl SearchEvidenceSet {
    pub fn validate_note(&self, note_id: &str) -> Result<(), SearchEvidenceError> {
        if self
            .locators
            .iter()
            .any(|locator| locator.0.note_id != note_id)
        {
            return Err(SearchEvidenceError::Invalid);
        }
        Ok(())
    }

    pub fn locators(&self) -> &[SearchEvidenceLocator] {
        &self.locators
    }

    /// Strict wire validation includes the enclosing hit identity and canonical order.
    pub fn parse(value: Value, note_id: &str) -> Result<Self, SearchEvidenceError> {
        if !SET_VALIDATOR.is_valid(&value) {
            return Err(SearchEvidenceError::Invalid);
        }
        let locators = value["locators"]
            .as_array()
            .ok_or(SearchEvidenceError::Invalid)?
            .iter()
            .cloned()
            .map(SearchEvidenceLocator::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        let omissions: Vec<EvidenceOmission> = serde_json::from_value(value["omissions"].clone())
            .map_err(|_| SearchEvidenceError::Invalid)?;
        if locators.iter().any(|locator| locator.0.note_id != note_id)
            || locators
                .windows(2)
                .any(|pair| !compare_locators(&pair[0], &pair[1]).is_lt())
            || omissions.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(SearchEvidenceError::Invalid);
        }
        Ok(Self {
            version: "1.0.0",
            locators,
            omissions,
        })
    }

    /// Bound and deduplicate existing evidence without manufacturing a text snapshot.
    pub fn new(
        note_id: &str,
        mut locators: Vec<SearchEvidenceLocator>,
        mut omissions: Vec<EvidenceOmission>,
    ) -> Result<Self, SearchEvidenceError> {
        if locators.iter().any(|locator| locator.0.note_id != note_id) {
            return Err(SearchEvidenceError::Invalid);
        }
        locators.sort_by(compare_locators);
        locators.dedup_by(|a, b| compare_locators(a, b).is_eq());
        if locators.len() > MAX_SEARCH_LOCATORS {
            omissions.push(EvidenceOmission::LocatorLimit);
            locators.truncate(MAX_SEARCH_LOCATORS);
        }
        if locators.is_empty() {
            omissions.push(EvidenceOmission::UnavailableUnit);
        }
        omissions.sort();
        omissions.dedup();
        Ok(Self {
            version: "1.0.0",
            locators,
            omissions,
        })
    }

    pub fn merge(note_id: &str, sets: &[Self]) -> Result<Self, SearchEvidenceError> {
        Self::new(
            note_id,
            sets.iter().flat_map(|set| set.locators.clone()).collect(),
            sets.iter().flat_map(|set| set.omissions.clone()).collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_evidence_set_corpus() {
        let corpus: Value = serde_json::from_str(include_str!(
            "../../../contracts/metadata-search/candidate/1.0.0/evidence-set-vectors.json"
        ))
        .unwrap();
        let cases = corpus["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 36);
        for case in cases {
            let note_id = case["note_id"].as_str().unwrap();
            let execute = || -> Result<SearchEvidenceSet, SearchEvidenceError> {
                match case["operation"].as_str().unwrap() {
                    "parse" => SearchEvidenceSet::parse(case["input"].clone(), note_id),
                    "build" => {
                        let locators = case["input"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .cloned()
                            .map(SearchEvidenceLocator::try_from)
                            .collect::<Result<Vec<_>, _>>()?;
                        let omissions = serde_json::from_value(case["omissions"].clone()).unwrap();
                        SearchEvidenceSet::new(note_id, locators, omissions)
                    }
                    "merge" => {
                        let sets = case["input"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .cloned()
                            .map(|value| SearchEvidenceSet::parse(value, note_id))
                            .collect::<Result<Vec<_>, _>>()?;
                        SearchEvidenceSet::merge(note_id, &sets)
                    }
                    _ => panic!("unknown corpus operation"),
                }
            };
            match execute() {
                Ok(value) => {
                    assert!(case.get("error").is_none(), "case {}", case["id"]);
                    assert_eq!(
                        serde_json::to_value(&value).unwrap(),
                        case["expected"],
                        "case {}",
                        case["id"]
                    );
                    let debug = format!("{value:?}");
                    assert!(!debug.contains(note_id));
                    assert!(!debug.contains("sha256:"));
                }
                Err(error) => assert_eq!(
                    case["error"].as_str(),
                    Some(error.to_string().as_str()),
                    "case {}",
                    case["id"]
                ),
            }
        }
    }

    const VECTORS: &str =
        include_str!("../../../contracts/metadata-search/candidate/1.0.0/evidence-vectors.json");

    #[derive(Deserialize)]
    struct Snapshot {
        note_id: String,
        unit: EvidenceUnit,
        content: String,
        source: Option<EvidenceSource>,
    }

    impl Snapshot {
        fn text(&self) -> EvidenceText<'_> {
            EvidenceText {
                note_id: &self.note_id,
                unit: &self.unit,
                content: &self.content,
                source: self.source.as_ref(),
            }
        }
    }

    #[test]
    fn authority_resolution_vectors() {
        let corpus: Value = serde_json::from_str(VECTORS).unwrap();
        for case in corpus["cases"].as_array().unwrap() {
            let parsed = SearchEvidenceLocator::try_from(case["locator"].clone());
            let snapshot: Option<Snapshot> =
                serde_json::from_value(case["snapshot"].clone()).unwrap();
            let actual = parsed.and_then(|locator| {
                locator
                    .resolve(snapshot.as_ref().map(Snapshot::text))
                    .map(str::to_owned)
            });
            if let Some(error) = case["error"].as_str() {
                assert_eq!(actual.unwrap_err().to_string(), error, "{}", case["id"]);
            } else {
                assert_eq!(
                    actual.unwrap(),
                    case["text"].as_str().unwrap(),
                    "{}",
                    case["id"]
                );
                let input = snapshot.as_ref().unwrap();
                let start = case["locator"]["span"]["start"].as_u64().unwrap() as usize;
                let end = case["locator"]["span"]["end"].as_u64().unwrap() as usize;
                let bound = SearchEvidenceLocator::bind(input.text(), start, end).unwrap();
                assert_eq!(
                    serde_json::to_value(bound).unwrap(),
                    case["locator"],
                    "{}",
                    case["id"]
                );
            }
        }
    }

    #[test]
    fn binding_roundtrips_and_debug_does_not_disclose_identifiers() {
        let unit = EvidenceUnit {
            kind: EvidenceKind::Embedding,
            id: "private-unit".into(),
            index: 7,
        };
        let text = "a\u{1f680}e\u{301}";
        let snapshot = || EvidenceText {
            note_id: "private-note",
            unit: &unit,
            content: text,
            source: None,
        };
        let locator = SearchEvidenceLocator::bind(snapshot(), 1, 5).unwrap();
        assert_eq!(locator.resolve(Some(snapshot())).unwrap(), "\u{1f680}");
        assert_eq!(
            SearchEvidenceLocator::bind(snapshot(), 2, 5).unwrap_err(),
            SearchEvidenceError::Invalid
        );
        let reparsed =
            SearchEvidenceLocator::try_from(serde_json::to_value(&locator).unwrap()).unwrap();
        assert_eq!(reparsed.resolve(Some(snapshot())).unwrap(), "\u{1f680}");
        assert!(!format!("{locator:?}").contains("private"));
    }

    #[test]
    fn full_text_budget_is_independent_of_span_and_json_integer_spelling() {
        let unit = EvidenceUnit {
            kind: EvidenceKind::Embedding,
            id: "unit".into(),
            index: 1,
        };
        let large = "\u{1f680}".repeat(MAX_EVIDENCE_BYTES / 4 + 1);
        let text = EvidenceText {
            note_id: "note",
            unit: &unit,
            content: &large,
            source: None,
        };
        assert_eq!(
            SearchEvidenceLocator::bind(text, 0, 0).unwrap_err(),
            SearchEvidenceError::Invalid
        );
        let value: Value = serde_json::from_str(r#"{"version":"1.0.0","note_id":"note","unit":{"kind":"embedding","id":"unit","index":1.0},"content_digest":"sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad","span":{"unit":"utf8-bytes","start":1.0,"end":3.0}}"#).unwrap();
        let locator = SearchEvidenceLocator::try_from(value).unwrap();
        assert_eq!(
            locator
                .resolve(Some(EvidenceText {
                    note_id: "note",
                    unit: &unit,
                    content: "abc",
                    source: None
                }))
                .unwrap(),
            "bc"
        );
        assert_eq!(
            locator
                .resolve(Some(EvidenceText {
                    note_id: "note",
                    unit: &unit,
                    content: &large,
                    source: None
                }))
                .unwrap_err(),
            SearchEvidenceError::Unavailable
        );
    }
}
