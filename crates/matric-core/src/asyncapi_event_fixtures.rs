use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use chrono::{TimeZone, Utc};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::events::{EventActor, EventEnvelope, ServerEvent};

const FIXTURE_SCHEMA_VERSION: &str = "fortemi.asyncapi.event-fixtures.v1";
const ENVELOPE_CONTRACT_REVISION: &str = "1";
const PAYLOAD_REVISION: u32 = 1;
const FIXTURE_DIR: &str = "contracts/asyncapi/fixtures/events";
const MANIFEST_PATH: &str = "contracts/asyncapi/fixtures/manifest.json";
const RECEIPT_PATH: &str = "contracts/asyncapi/producer-event-fixture-receipt.json";
const ASYNCAPI_PATH: &str = "contracts/asyncapi/asyncapi.yaml";

#[derive(Debug)]
pub struct FixtureSummary {
    pub event_count: usize,
    pub corpus_sha256: String,
}

#[derive(Clone)]
struct FixtureCase {
    event: ServerEvent,
}

impl FixtureCase {
    fn event_type(&self) -> &'static str {
        self.event.namespaced_event_type()
    }

    fn variant_name(&self) -> &'static str {
        self.event.event_type()
    }

    fn relative_path(&self) -> String {
        format!(
            "{}/{}.json",
            FIXTURE_DIR,
            self.event_type().replace('.', "_")
        )
    }

    fn envelope(&self, index: usize) -> EventEnvelope {
        EventEnvelope {
            event_id: fixture_uuid(index, 1),
            event_type: self.event_type().to_string(),
            occurred_at: Utc
                .with_ymd_and_hms(2026, 8, 17, 12, u32::try_from(index % 60).unwrap(), 0)
                .unwrap(),
            memory: Some("fixture-memory-alpha".to_string()),
            tenant_id: Some("tenant-fixture-001".to_string()),
            actor: EventActor::user(
                "fixture-user-001",
                Some("Fortemi event fixture operator".to_string()),
            ),
            entity_type: self.event.entity_type().map(str::to_string),
            entity_id: self.event.entity_id().map(|id| id.to_string()),
            correlation_id: Some(fixture_uuid(index, 2)),
            causation_id: Some(fixture_uuid(index, 3)),
            payload_version: PAYLOAD_REVISION,
            payload: self.event.clone(),
        }
    }
}

#[derive(Serialize)]
struct Manifest {
    schema_version: &'static str,
    producer_repository: &'static str,
    producer_commit: String,
    asyncapi_path: &'static str,
    asyncapi_sha256: String,
    envelope_contract_revision: &'static str,
    payload_revision: u32,
    event_count: usize,
    schema_count: usize,
    corpus: CorpusDigest,
    fixtures: Vec<ManifestFixture>,
}

#[derive(Serialize)]
struct CorpusDigest {
    root: &'static str,
    aggregate_algorithm: &'static str,
    sha256: String,
}

#[derive(Serialize)]
struct ManifestFixture {
    event_type: String,
    variant_name: String,
    path: String,
    payload_schema_component: String,
    payload_revision: u32,
    envelope_contract_revision: &'static str,
    sha256: String,
}

#[derive(Serialize)]
struct Receipt {
    schema_version: &'static str,
    producer_repository: &'static str,
    producer_commit: String,
    manifest_path: &'static str,
    manifest_sha256: String,
    asyncapi_path: &'static str,
    asyncapi_sha256: String,
    corpus_root: &'static str,
    corpus_sha256: String,
    event_count: usize,
    consumer_pin: ConsumerPin,
}

#[derive(Serialize)]
struct ConsumerPin {
    fortemi_commit: String,
    manifest_sha256: String,
    corpus_sha256: String,
}

pub fn generate_fixture_corpus(root: &Path) -> Result<FixtureSummary, String> {
    let rendered = render_corpus(root)?;
    let fixture_root = root.join(FIXTURE_DIR);
    if fixture_root.exists() {
        fs::remove_dir_all(&fixture_root).map_err(|err| {
            format!(
                "failed to remove existing fixture directory {}: {err}",
                fixture_root.display()
            )
        })?;
    }
    fs::create_dir_all(&fixture_root).map_err(|err| {
        format!(
            "failed to create fixture directory {}: {err}",
            fixture_root.display()
        )
    })?;
    for (relative, bytes) in &rendered.files {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
        }
        fs::write(&path, bytes)
            .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
    }
    Ok(rendered.summary)
}

pub fn check_fixture_corpus(root: &Path) -> Result<FixtureSummary, String> {
    let rendered = render_corpus(root)?;
    for (relative, expected) in &rendered.files {
        let path = root.join(relative);
        let actual =
            fs::read(&path).map_err(|err| format!("failed to read {}: {err}", path.display()))?;
        if actual != *expected {
            return Err(format!(
                "{} drifted; run scripts/ci/asyncapi-event-fixtures.sh generate",
                relative
            ));
        }
    }

    let expected_paths: BTreeSet<_> = rendered.files.keys().cloned().collect();
    let actual_paths = committed_fixture_paths(root)?;
    if actual_paths != expected_paths {
        return Err(format!(
            "fixture path set drifted; expected {:?}, got {:?}",
            expected_paths, actual_paths
        ));
    }

    Ok(rendered.summary)
}

fn render_corpus(root: &Path) -> Result<RenderedCorpus, String> {
    let asyncapi_path = root.join(ASYNCAPI_PATH);
    let asyncapi_bytes = fs::read(&asyncapi_path)
        .map_err(|err| format!("failed to read {}: {err}", asyncapi_path.display()))?;
    let asyncapi_sha256 = sha256_hex(&asyncapi_bytes);
    let spec: Value = serde_yaml::from_slice(&asyncapi_bytes)
        .map_err(|err| format!("failed to parse {}: {err}", asyncapi_path.display()))?;
    let schema_count = spec
        .pointer("/components/schemas")
        .and_then(Value::as_object)
        .ok_or("AsyncAPI components.schemas is missing or not an object")?
        .len();

    let cases = fixture_cases();
    validate_asyncapi_event_mappings(&spec, &cases)?;

    let schema = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$ref": "#/components/schemas/EventEnvelope",
        "components": spec["components"].clone()
    });
    let validator = jsonschema::validator_for(&schema)
        .map_err(|err| format!("AsyncAPI EventEnvelope schema failed to compile: {err}"))?;

    let mut fixture_files = BTreeMap::new();
    let mut manifest_fixtures = Vec::new();
    for (index, case) in cases.iter().enumerate() {
        let envelope = case.envelope(index);
        let value = serde_json::to_value(&envelope)
            .map_err(|err| format!("failed to serialize {}: {err}", case.event_type()))?;
        validator.validate(&value).map_err(|err| {
            format!(
                "{} does not validate against committed AsyncAPI EventEnvelope schema: {err}",
                case.event_type()
            )
        })?;
        let round_trip: EventEnvelope = serde_json::from_value(value.clone()).map_err(|err| {
            format!(
                "{} failed runtime EventEnvelope deserialization: {err}",
                case.event_type()
            )
        })?;
        if round_trip != envelope {
            return Err(format!(
                "{} failed runtime serialization/deserialization semantic equality",
                case.event_type()
            ));
        }

        let relative_path = case.relative_path();
        let bytes = pretty_json_bytes(&value)?;
        let sha256 = sha256_hex(&bytes);
        fixture_files.insert(relative_path.clone(), bytes);
        manifest_fixtures.push(ManifestFixture {
            event_type: case.event_type().to_string(),
            variant_name: case.variant_name().to_string(),
            path: relative_path,
            payload_schema_component: format!("ServerEvent.oneOf[type={}]", case.variant_name()),
            payload_revision: PAYLOAD_REVISION,
            envelope_contract_revision: ENVELOPE_CONTRACT_REVISION,
            sha256,
        });
    }
    validate_unknown_events_stay_unknown()?;

    let corpus_sha256 = aggregate_sha256(fixture_files.values());
    let producer_commit = producer_commit();
    let manifest = Manifest {
        schema_version: FIXTURE_SCHEMA_VERSION,
        producer_repository: "Fortemi/fortemi",
        producer_commit: producer_commit.clone(),
        asyncapi_path: ASYNCAPI_PATH,
        asyncapi_sha256: asyncapi_sha256.clone(),
        envelope_contract_revision: ENVELOPE_CONTRACT_REVISION,
        payload_revision: PAYLOAD_REVISION,
        event_count: cases.len(),
        schema_count,
        corpus: CorpusDigest {
            root: FIXTURE_DIR,
            aggregate_algorithm:
                "sha256(raw fixture file bytes concatenated in lexicographic path order)",
            sha256: corpus_sha256.clone(),
        },
        fixtures: manifest_fixtures,
    };
    let manifest_bytes = pretty_json_bytes(&manifest)?;
    let manifest_sha256 = sha256_hex(&manifest_bytes);
    let receipt = Receipt {
        schema_version: "fortemi.asyncapi.event-fixture-receipt.v1",
        producer_repository: "Fortemi/fortemi",
        producer_commit: producer_commit.clone(),
        manifest_path: MANIFEST_PATH,
        manifest_sha256: manifest_sha256.clone(),
        asyncapi_path: ASYNCAPI_PATH,
        asyncapi_sha256,
        corpus_root: FIXTURE_DIR,
        corpus_sha256: corpus_sha256.clone(),
        event_count: cases.len(),
        consumer_pin: ConsumerPin {
            fortemi_commit: producer_commit,
            manifest_sha256,
            corpus_sha256: corpus_sha256.clone(),
        },
    };
    let receipt_bytes = pretty_json_bytes(&receipt)?;

    let mut files = fixture_files;
    files.insert(MANIFEST_PATH.to_string(), manifest_bytes);
    files.insert(RECEIPT_PATH.to_string(), receipt_bytes);

    Ok(RenderedCorpus {
        files,
        summary: FixtureSummary {
            event_count: cases.len(),
            corpus_sha256,
        },
    })
}

struct RenderedCorpus {
    files: BTreeMap<String, Vec<u8>>,
    summary: FixtureSummary,
}

fn validate_asyncapi_event_mappings(spec: &Value, cases: &[FixtureCase]) -> Result<(), String> {
    let messages = spec
        .pointer("/channels/events/messages")
        .and_then(Value::as_object)
        .ok_or("AsyncAPI channels.events.messages is missing or not an object")?;
    let case_by_variant: BTreeMap<_, _> = cases
        .iter()
        .map(|case| (case.variant_name(), case.event_type()))
        .collect();
    let message_keys: BTreeSet<_> = messages.keys().map(String::as_str).collect();
    let case_keys: BTreeSet<_> = case_by_variant.keys().copied().collect();
    if message_keys != case_keys {
        return Err(format!(
            "AsyncAPI message/event fixture mapping mismatch; messages={message_keys:?} fixtures={case_keys:?}"
        ));
    }
    for (variant, event_type) in &case_by_variant {
        let message = &messages[*variant];
        if message.pointer("/payload/$ref").and_then(Value::as_str)
            != Some("#/components/schemas/EventEnvelope")
        {
            return Err(format!(
                "{variant} does not reference EventEnvelope payload schema"
            ));
        }
        if message.get("x-event-type").and_then(Value::as_str) != Some(*event_type) {
            return Err(format!(
                "{variant} x-event-type does not match fixture event type"
            ));
        }
    }

    let variants = spec
        .pointer("/components/schemas/ServerEvent/oneOf")
        .and_then(Value::as_array)
        .ok_or("AsyncAPI ServerEvent.oneOf is missing or not an array")?;
    let mut schema_variants = BTreeSet::new();
    for variant_schema in variants {
        let Some(name) = variant_schema
            .pointer("/properties/type/enum/0")
            .and_then(Value::as_str)
        else {
            return Err(
                "AsyncAPI ServerEvent.oneOf entry is missing type enum discriminator".into(),
            );
        };
        if !schema_variants.insert(name) {
            return Err(format!(
                "AsyncAPI ServerEvent.oneOf has duplicate discriminator {name}"
            ));
        }
    }
    if schema_variants != case_keys {
        return Err(format!(
            "AsyncAPI ServerEvent schema/event fixture mapping mismatch; schema={schema_variants:?} fixtures={case_keys:?}"
        ));
    }
    Ok(())
}

fn validate_unknown_events_stay_unknown() -> Result<(), String> {
    let unknown = json!({
        "type": "FutureProducerEvent",
        "future_field": "must-not-map-to-known-fixture"
    });
    if serde_json::from_value::<ServerEvent>(unknown).is_ok() {
        return Err("unknown ServerEvent discriminator unexpectedly deserialized as known".into());
    }
    Ok(())
}

fn committed_fixture_paths(root: &Path) -> Result<BTreeSet<String>, String> {
    let mut paths = BTreeSet::new();
    collect_json_paths(root, &root.join("contracts/asyncapi/fixtures"), &mut paths)?;
    paths.insert(RECEIPT_PATH.to_string());
    Ok(paths)
}

fn collect_json_paths(root: &Path, dir: &Path, paths: &mut BTreeSet<String>) -> Result<(), String> {
    for entry in
        fs::read_dir(dir).map_err(|err| format!("failed to read {}: {err}", dir.display()))?
    {
        let entry =
            entry.map_err(|err| format!("failed to read {} entry: {err}", dir.display()))?;
        let path = entry.path();
        if path.is_dir() {
            collect_json_paths(root, &path, paths)?;
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
            let relative = path
                .strip_prefix(root)
                .map_err(|err| format!("failed to relativize {}: {err}", path.display()))?
                .to_string_lossy()
                .replace('\\', "/");
            paths.insert(relative);
        }
    }
    Ok(())
}

fn pretty_json_bytes(value: &impl Serialize) -> Result<Vec<u8>, String> {
    let mut bytes =
        serde_json::to_vec_pretty(value).map_err(|err| format!("failed to render JSON: {err}"))?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn aggregate_sha256<'a>(files: impl IntoIterator<Item = &'a Vec<u8>>) -> String {
    let mut hasher = Sha256::new();
    for bytes in files {
        hasher.update(bytes);
    }
    hex::encode(hasher.finalize())
}

fn producer_commit() -> String {
    if let Ok(value) = std::env::var("FORTEMI_EVENT_FIXTURE_PRODUCER_COMMIT") {
        if is_git_sha(&value) {
            return value;
        }
    }
    "external-delivery-pin".to_string()
}

fn is_git_sha(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|b| b.is_ascii_hexdigit())
}

fn fixture_uuid(index: usize, salt: u16) -> Uuid {
    let mut bytes = [0u8; 16];
    bytes[0..4].copy_from_slice(&0x018f_7d2du32.to_be_bytes());
    bytes[4..6].copy_from_slice(&u16::try_from(index).unwrap().to_be_bytes());
    bytes[6..8].copy_from_slice(&0x7000u16.to_be_bytes());
    bytes[8..10].copy_from_slice(&salt.to_be_bytes());
    bytes[10..16].copy_from_slice(&[0xf1, 0x47, 0xd6, 0xa2, 0xe7, 0x0a]);
    Uuid::from_bytes(bytes)
}

fn fixture_cases() -> Vec<FixtureCase> {
    let job_id = fixture_uuid(100, 1);
    let note_id = fixture_uuid(101, 1);
    let attachment_id = fixture_uuid(102, 1);
    let collection_id = fixture_uuid(103, 1);
    let archive_id = fixture_uuid(104, 1);
    let scheme_id = fixture_uuid(105, 1);
    let concept_id = fixture_uuid(106, 1);
    vec![
        case(ServerEvent::QueueStatus {
            total_jobs: 42,
            running: 2,
            pending: 7,
        }),
        case(ServerEvent::JobQueued {
            job_id,
            job_type: "Embedding".to_string(),
            note_id: Some(note_id),
        }),
        case(ServerEvent::JobStarted {
            job_id,
            job_type: "Linking".to_string(),
            note_id: None,
        }),
        case(ServerEvent::JobProgress {
            job_id,
            note_id: Some(note_id),
            progress: 65,
            message: Some("fixture progress update".to_string()),
        }),
        case(ServerEvent::JobCompleted {
            job_id,
            job_type: "Embedding".to_string(),
            note_id: Some(note_id),
            duration_ms: Some(1234),
        }),
        case(ServerEvent::JobFailed {
            job_id,
            job_type: "Extraction".to_string(),
            note_id: None,
            error: "fixture validation failure".to_string(),
        }),
        case(ServerEvent::NoteUpdated {
            note_id,
            title: Some("Fixture note".to_string()),
            tags: vec!["fixture".to_string(), "contract".to_string()],
            has_ai_content: true,
            has_links: true,
        }),
        case(ServerEvent::NoteCreated {
            note_id,
            title: None,
            tags: vec!["new".to_string()],
        }),
        case(ServerEvent::NoteDeleted { note_id }),
        case(ServerEvent::NoteArchived { note_id }),
        case(ServerEvent::NoteRestored { note_id }),
        case(ServerEvent::NoteTagsUpdated {
            note_id,
            tags: vec!["alpha".to_string(), "beta".to_string()],
        }),
        case(ServerEvent::NoteLinksUpdated { note_id }),
        case(ServerEvent::NoteRevisionCreated { note_id }),
        case(ServerEvent::AttachmentCreated {
            attachment_id,
            note_id,
            filename: Some("fixture.pdf".to_string()),
        }),
        case(ServerEvent::AttachmentDeleted {
            attachment_id,
            note_id: None,
        }),
        case(ServerEvent::AttachmentExtractionUpdated {
            attachment_id,
            note_id,
        }),
        case(ServerEvent::CollectionCreated {
            collection_id,
            name: "Fixture collection".to_string(),
        }),
        case(ServerEvent::CollectionUpdated {
            collection_id,
            name: "Updated fixture collection".to_string(),
        }),
        case(ServerEvent::CollectionDeleted { collection_id }),
        case(ServerEvent::CollectionMembershipChanged {
            collection_id: Some(collection_id),
            note_id,
        }),
        case(ServerEvent::ArchiveCreated {
            name: "fixture-memory-alpha".to_string(),
            archive_id: Some(archive_id),
        }),
        case(ServerEvent::ArchiveUpdated {
            name: "fixture-memory-alpha".to_string(),
        }),
        case(ServerEvent::ArchiveDeleted {
            name: "fixture-memory-alpha".to_string(),
        }),
        case(ServerEvent::ArchiveDefaultChanged {
            name: "fixture-memory-alpha".to_string(),
        }),
        case(ServerEvent::ConceptSchemeCreated { scheme_id }),
        case(ServerEvent::ConceptSchemeUpdated { scheme_id }),
        case(ServerEvent::ConceptSchemeDeleted { scheme_id }),
        case(ServerEvent::ConceptCreated {
            concept_id,
            scheme_id: Some(scheme_id),
        }),
        case(ServerEvent::ConceptUpdated { concept_id }),
        case(ServerEvent::ConceptDeleted { concept_id }),
        case(ServerEvent::ConceptRelationsUpdated {
            concept_id,
            relation_type: "broader".to_string(),
        }),
        case(ServerEvent::ConceptSchemeChanged {
            concept_id,
            scheme_id: None,
        }),
        case(ServerEvent::ConceptCollectionMembershipChanged {
            concept_id,
            collection_id: Some(collection_id),
        }),
        case(ServerEvent::TagCreated {
            tag: "fixture".to_string(),
        }),
        case(ServerEvent::TagRenamed {
            old_name: "fixture-old".to_string(),
            new_name: "fixture-new".to_string(),
        }),
        case(ServerEvent::TagDeleted {
            tag: "fixture-old".to_string(),
        }),
        case(ServerEvent::TagMerged {
            source_tag: "source-fixture".to_string(),
            target_tag: "target-fixture".to_string(),
            affected_count: Some(3),
        }),
        case(ServerEvent::TagStatsUpdated),
        case(ServerEvent::JobsPaused {
            scope: "global".to_string(),
        }),
        case(ServerEvent::JobsResumed {
            scope: "fixture-memory-alpha".to_string(),
        }),
        case(ServerEvent::IndexEmbeddingUpdated {
            note_id,
            job_id: Some(job_id),
        }),
        case(ServerEvent::IndexLinkingUpdated {
            note_id,
            job_id: None,
        }),
        case(ServerEvent::IndexFtsUpdated {
            note_id,
            job_id: Some(job_id),
        }),
        case(ServerEvent::ReadmodelGraphUpdated {
            note_id: Some(note_id),
        }),
        case(ServerEvent::ReadmodelSearchReady { note_id }),
        case(ServerEvent::InferenceAvailabilityChanged { available: true }),
        case(ServerEvent::InferenceConfigChanged {
            default_backend: "ollama".to_string(),
            embedding_backend: Some("openrouter".to_string()),
            changed_fields: vec![
                "default_backend".to_string(),
                "openrouter.generation_model".to_string(),
            ],
        }),
    ]
}

fn case(event: ServerEvent) -> FixtureCase {
    FixtureCase { event }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_cases_cover_current_server_event_metadata() {
        let fixture_types: BTreeSet<_> = fixture_cases()
            .iter()
            .map(|case| case.event_type())
            .collect();
        let metadata_types: BTreeSet<_> = ServerEvent::all_variants_metadata()
            .iter()
            .map(|meta| meta.namespaced_type)
            .collect();
        assert_eq!(fixture_types, metadata_types);
    }
}
