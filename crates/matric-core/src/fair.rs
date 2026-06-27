//! FAIR metadata export types.
//!
//! Implements FAIR (Findable, Accessible, Interoperable, Reusable) principles
//! for metadata export, enabling notes to be discovered and used beyond matric-memory.
//!
//! Supports:
//! - **Dublin Core**: All 15 core elements (ISO 15836)
//! - **JSON-LD**: Linked data with context (schema.org, DC, SKOS, PROV)
//!
//! Reference: REF-056 - Wilkinson et al. (2016) "The FAIR Guiding Principles
//! for scientific data management and stewardship." Scientific Data.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

fn debug_len(value: &str) -> usize {
    value.chars().count()
}

fn optional_debug_len(value: Option<&String>) -> Option<usize> {
    value.map(|value| debug_len(value))
}

// =============================================================================
// DUBLIN CORE EXPORT (ISO 15836)
// =============================================================================

/// Dublin Core metadata export following ISO 15836.
///
/// All 15 core Dublin Core elements mapped from matric-memory note fields.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DublinCoreExport {
    /// DC.identifier - Globally unique persistent identifier (URN:UUID format)
    pub identifier: String,
    /// DC.title - Generated or user-provided title
    pub title: String,
    /// DC.creator - User/author attribution
    pub creator: Option<String>,
    /// DC.subject - Tags and SKOS concepts
    pub subject: Vec<String>,
    /// DC.description - Summary or first paragraph
    pub description: Option<String>,
    /// DC.publisher - Always "matric-memory"
    pub publisher: String,
    /// DC.contributor - AI revision attribution
    pub contributor: Vec<String>,
    /// DC.date - ISO 8601 creation date
    pub date: DateTime<Utc>,
    /// DC.type - Resource type (always "Text")
    #[serde(rename = "type")]
    pub dc_type: String,
    /// DC.format - MIME type (always "text/markdown")
    pub format: String,
    /// DC.source - Source note ID if this is a revision
    pub source: Option<String>,
    /// DC.language - Detected or specified language (ISO 639-1)
    pub language: Option<String>,
    /// DC.relation - Linked note IDs
    pub relation: Vec<String>,
    /// DC.coverage - Collection path for spatial/temporal scope
    pub coverage: Option<String>,
    /// DC.rights - License or access rights
    pub rights: Option<String>,
}

impl fmt::Debug for DublinCoreExport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DublinCoreExport")
            .field("identifier_len", &debug_len(&self.identifier))
            .field("title_len", &debug_len(&self.title))
            .field("creator_len", &optional_debug_len(self.creator.as_ref()))
            .field("subject_count", &self.subject.len())
            .field(
                "subject_lens",
                &self
                    .subject
                    .iter()
                    .map(|value| debug_len(value))
                    .collect::<Vec<_>>(),
            )
            .field(
                "description_len",
                &optional_debug_len(self.description.as_ref()),
            )
            .field("publisher_len", &debug_len(&self.publisher))
            .field("contributor_count", &self.contributor.len())
            .field(
                "contributor_lens",
                &self
                    .contributor
                    .iter()
                    .map(|value| debug_len(value))
                    .collect::<Vec<_>>(),
            )
            .field("date", &self.date)
            .field("dc_type_len", &debug_len(&self.dc_type))
            .field("format_len", &debug_len(&self.format))
            .field("source_len", &optional_debug_len(self.source.as_ref()))
            .field("language_len", &optional_debug_len(self.language.as_ref()))
            .field("relation_count", &self.relation.len())
            .field(
                "relation_lens",
                &self
                    .relation
                    .iter()
                    .map(|value| debug_len(value))
                    .collect::<Vec<_>>(),
            )
            .field("coverage_len", &optional_debug_len(self.coverage.as_ref()))
            .field("rights_len", &optional_debug_len(self.rights.as_ref()))
            .finish()
    }
}

impl DublinCoreExport {
    /// Create a Dublin Core export from note fields.
    pub fn from_note(
        note_id: Uuid,
        title: Option<&str>,
        tags: &[String],
        created_at: DateTime<Utc>,
        collection_path: Option<&str>,
        linked_note_ids: &[Uuid],
        has_ai_revision: bool,
    ) -> Self {
        let mut contributors = Vec::new();
        if has_ai_revision {
            contributors.push("AI Revision System (matric-memory)".to_string());
        }

        Self {
            identifier: format!("urn:uuid:{}", note_id),
            title: title.unwrap_or("Untitled Note").to_string(),
            creator: None, // Set by caller if user info available
            subject: tags.to_vec(),
            description: None, // Set by caller from content
            publisher: "matric-memory".to_string(),
            contributor: contributors,
            date: created_at,
            dc_type: "Text".to_string(),
            format: "text/markdown".to_string(),
            source: None,
            language: None,
            relation: linked_note_ids
                .iter()
                .map(|id| format!("urn:uuid:{}", id))
                .collect(),
            coverage: collection_path.map(String::from),
            rights: None,
        }
    }

    /// Set the description from note content (first paragraph).
    pub fn with_description(mut self, content: &str) -> Self {
        self.description = content
            .split("\n\n")
            .next()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        self
    }

    /// Set the creator field.
    pub fn with_creator(mut self, creator: String) -> Self {
        self.creator = Some(creator);
        self
    }

    /// Set the language field.
    pub fn with_language(mut self, language: String) -> Self {
        self.language = Some(language);
        self
    }

    /// Set the rights/license field.
    pub fn with_rights(mut self, rights: String) -> Self {
        self.rights = Some(rights);
        self
    }

    /// Set the source note (if this is a revision).
    pub fn with_source(mut self, source_note_id: Uuid) -> Self {
        self.source = Some(format!("urn:uuid:{}", source_note_id));
        self
    }
}

// =============================================================================
// JSON-LD EXPORT
// =============================================================================

/// JSON-LD context for linked data export.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct JsonLdContext {
    /// Dublin Core namespace
    pub dc: String,
    /// SKOS namespace
    pub skos: String,
    /// W3C PROV namespace
    pub prov: String,
    /// Schema.org namespace
    pub schema: String,
}

impl fmt::Debug for JsonLdContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JsonLdContext")
            .field("dc_len", &debug_len(&self.dc))
            .field("skos_len", &debug_len(&self.skos))
            .field("prov_len", &debug_len(&self.prov))
            .field("schema_len", &debug_len(&self.schema))
            .finish()
    }
}

impl Default for JsonLdContext {
    fn default() -> Self {
        Self {
            dc: "http://purl.org/dc/elements/1.1/".to_string(),
            skos: "http://www.w3.org/2004/02/skos/core#".to_string(),
            prov: "http://www.w3.org/ns/prov#".to_string(),
            schema: "https://schema.org/".to_string(),
        }
    }
}

/// JSON-LD metadata export with linked data context.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct JsonLdExport {
    /// JSON-LD context declaring namespaces
    #[serde(rename = "@context")]
    pub context: JsonLdContext,
    /// Resource identifier (URN:UUID)
    #[serde(rename = "@id")]
    pub id: String,
    /// Resource type
    #[serde(rename = "@type")]
    pub ld_type: String,
    /// Dublin Core elements
    #[serde(flatten)]
    pub dublin_core: DublinCoreExport,
    /// SKOS concept tags
    #[serde(rename = "skos:concept")]
    pub skos_concepts: Vec<String>,
    /// W3C PROV derivation chain
    #[serde(rename = "prov:wasDerivedFrom")]
    pub prov_derived_from: Vec<String>,
    /// W3C PROV generation activity
    #[serde(rename = "prov:wasGeneratedBy")]
    pub prov_generated_by: Option<String>,
}

impl fmt::Debug for JsonLdExport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JsonLdExport")
            .field("context_namespace_count", &4)
            .field("id_len", &debug_len(&self.id))
            .field("ld_type_len", &debug_len(&self.ld_type))
            .field("dublin_core", &self.dublin_core)
            .field("skos_concepts_count", &self.skos_concepts.len())
            .field(
                "skos_concept_lens",
                &self
                    .skos_concepts
                    .iter()
                    .map(|value| debug_len(value))
                    .collect::<Vec<_>>(),
            )
            .field("prov_derived_from_count", &self.prov_derived_from.len())
            .field(
                "prov_derived_from_lens",
                &self
                    .prov_derived_from
                    .iter()
                    .map(|value| debug_len(value))
                    .collect::<Vec<_>>(),
            )
            .field(
                "prov_generated_by_len",
                &optional_debug_len(self.prov_generated_by.as_ref()),
            )
            .finish()
    }
}

impl JsonLdExport {
    /// Create a JSON-LD export from a Dublin Core export.
    pub fn from_dublin_core(
        dc: DublinCoreExport,
        skos_concepts: Vec<String>,
        prov_derived_from: Vec<String>,
        prov_generated_by: Option<String>,
    ) -> Self {
        let id = dc.identifier.clone();
        Self {
            context: JsonLdContext::default(),
            id,
            ld_type: "schema:DigitalDocument".to_string(),
            dublin_core: dc,
            skos_concepts,
            prov_derived_from,
            prov_generated_by,
        }
    }
}

// =============================================================================
// FAIR COMPLIANCE SCORE
// =============================================================================

/// FAIR compliance assessment for a note's metadata.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct FairScore {
    /// Findable score (0.0-1.0)
    pub findable: f32,
    /// Accessible score (0.0-1.0)
    pub accessible: f32,
    /// Interoperable score (0.0-1.0)
    pub interoperable: f32,
    /// Reusable score (0.0-1.0)
    pub reusable: f32,
    /// Overall FAIR score (average)
    pub overall: f32,
    /// Specific issues found
    pub issues: Vec<String>,
}

impl fmt::Debug for FairScore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FairScore")
            .field("findable", &self.findable)
            .field("accessible", &self.accessible)
            .field("interoperable", &self.interoperable)
            .field("reusable", &self.reusable)
            .field("overall", &self.overall)
            .field("issues_count", &self.issues.len())
            .field(
                "issue_lens",
                &self
                    .issues
                    .iter()
                    .map(|issue| debug_len(issue))
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl FairScore {
    /// Assess FAIR compliance from a Dublin Core export.
    pub fn assess(dc: &DublinCoreExport) -> Self {
        let mut issues = Vec::new();
        let mut findable = 0.0_f32;
        let mut accessible = 0.0_f32;
        let mut interoperable = 0.0_f32;
        let mut reusable = 0.0_f32;

        // F1: Globally unique persistent identifier
        if !dc.identifier.is_empty() {
            findable += 0.4;
        } else {
            issues.push("F1: Missing persistent identifier".to_string());
        }

        // F2: Rich metadata (title + subject + description)
        if !dc.title.is_empty() && dc.title != "Untitled Note" {
            findable += 0.2;
        } else {
            issues.push("F2: Missing or generic title".to_string());
        }
        if !dc.subject.is_empty() {
            findable += 0.2;
        } else {
            issues.push("F2: No subject tags assigned".to_string());
        }
        if dc.description.is_some() {
            findable += 0.2;
        } else {
            issues.push("F2: No description available".to_string());
        }

        // A1: Retrievable by identifier (always true for API-served notes)
        accessible += 0.5;
        // A2: Metadata accessible even if data removed (always true - metadata is separate)
        accessible += 0.5;

        // I1: Knowledge representation (JSON-LD/DC format)
        interoperable += 0.4; // We provide Dublin Core + JSON-LD
                              // I2: Uses FAIR vocabularies (DC, SKOS, PROV)
        interoperable += 0.3;
        // I3: Qualified references to other resources
        if !dc.relation.is_empty() {
            interoperable += 0.3;
        } else {
            issues.push("I3: No linked relations".to_string());
        }

        // R1: Rich provenance
        if !dc.contributor.is_empty() {
            reusable += 0.25;
        } else {
            issues.push("R1: No contributor attribution".to_string());
        }
        if dc.rights.is_some() {
            reusable += 0.25;
        } else {
            issues.push("R1.1: No license/rights information".to_string());
        }
        if dc.language.is_some() {
            reusable += 0.25;
        } else {
            issues.push("R1.2: No language specified".to_string());
        }
        // R1.3: Meets domain-relevant community standards
        reusable += 0.25; // We follow Dublin Core standard

        let overall = (findable + accessible + interoperable + reusable) / 4.0;

        Self {
            findable,
            accessible,
            interoperable,
            reusable,
            overall,
            issues,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_debug_excludes(debug: &str, secrets: &[&str]) {
        for secret in secrets {
            assert!(
                !debug.contains(secret),
                "debug output leaked secret `{secret}`: {debug}"
            );
        }
    }

    // =========================================================================
    // Dublin Core Tests
    // =========================================================================

    #[test]
    fn test_dublin_core_from_note() {
        let note_id = Uuid::new_v4();
        let linked_id = Uuid::new_v4();
        let now = Utc::now();

        let dc = DublinCoreExport::from_note(
            note_id,
            Some("Test Note"),
            &["rust".to_string(), "programming".to_string()],
            now,
            Some("Engineering/Notes"),
            &[linked_id],
            true,
        );

        assert_eq!(dc.identifier, format!("urn:uuid:{}", note_id));
        assert_eq!(dc.title, "Test Note");
        assert_eq!(dc.subject, vec!["rust", "programming"]);
        assert_eq!(dc.publisher, "matric-memory");
        assert_eq!(dc.dc_type, "Text");
        assert_eq!(dc.format, "text/markdown");
        assert!(dc
            .contributor
            .contains(&"AI Revision System (matric-memory)".to_string()));
        assert_eq!(dc.relation.len(), 1);
        assert!(dc.relation[0].contains(&linked_id.to_string()));
        assert_eq!(dc.coverage, Some("Engineering/Notes".to_string()));
    }

    #[test]
    fn test_dublin_core_untitled() {
        let dc =
            DublinCoreExport::from_note(Uuid::new_v4(), None, &[], Utc::now(), None, &[], false);
        assert_eq!(dc.title, "Untitled Note");
        assert!(dc.contributor.is_empty());
    }

    #[test]
    fn test_dublin_core_with_description() {
        let dc = DublinCoreExport::from_note(
            Uuid::new_v4(),
            Some("Title"),
            &[],
            Utc::now(),
            None,
            &[],
            false,
        )
        .with_description("First paragraph here.\n\nSecond paragraph.");

        assert_eq!(dc.description, Some("First paragraph here.".to_string()));
    }

    #[test]
    fn test_dublin_core_with_empty_description() {
        let dc = DublinCoreExport::from_note(
            Uuid::new_v4(),
            Some("Title"),
            &[],
            Utc::now(),
            None,
            &[],
            false,
        )
        .with_description("");

        assert!(dc.description.is_none());
    }

    #[test]
    fn test_dublin_core_with_creator() {
        let dc = DublinCoreExport::from_note(
            Uuid::new_v4(),
            Some("Title"),
            &[],
            Utc::now(),
            None,
            &[],
            false,
        )
        .with_creator("John Doe".to_string());

        assert_eq!(dc.creator, Some("John Doe".to_string()));
    }

    #[test]
    fn test_dublin_core_with_language() {
        let dc = DublinCoreExport::from_note(
            Uuid::new_v4(),
            Some("Title"),
            &[],
            Utc::now(),
            None,
            &[],
            false,
        )
        .with_language("en".to_string());

        assert_eq!(dc.language, Some("en".to_string()));
    }

    #[test]
    fn test_dublin_core_with_source() {
        let source_id = Uuid::new_v4();
        let dc = DublinCoreExport::from_note(
            Uuid::new_v4(),
            Some("Title"),
            &[],
            Utc::now(),
            None,
            &[],
            false,
        )
        .with_source(source_id);

        assert_eq!(dc.source, Some(format!("urn:uuid:{}", source_id)));
    }

    #[test]
    fn test_dublin_core_serialization() {
        let dc = DublinCoreExport::from_note(
            Uuid::new_v4(),
            Some("Test"),
            &["tag1".to_string()],
            Utc::now(),
            None,
            &[],
            false,
        );

        let json = serde_json::to_string(&dc).unwrap();
        assert!(json.contains("\"identifier\""));
        assert!(json.contains("\"publisher\":\"matric-memory\""));
        assert!(json.contains("\"type\":\"Text\""));

        let parsed: DublinCoreExport = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.title, "Test");
    }

    // =========================================================================
    // JSON-LD Tests
    // =========================================================================

    #[test]
    fn test_json_ld_context_default() {
        let ctx = JsonLdContext::default();
        assert!(ctx.dc.starts_with("http://purl.org/dc/"));
        assert!(ctx.skos.contains("skos"));
        assert!(ctx.prov.contains("prov"));
        assert!(ctx.schema.contains("schema.org"));
    }

    #[test]
    fn test_json_ld_from_dublin_core() {
        let dc = DublinCoreExport::from_note(
            Uuid::new_v4(),
            Some("Test"),
            &["rust".to_string()],
            Utc::now(),
            None,
            &[],
            true,
        );

        let ld = JsonLdExport::from_dublin_core(
            dc,
            vec!["concept:systems-programming".to_string()],
            vec!["urn:uuid:source1".to_string()],
            Some("revision-activity-123".to_string()),
        );

        assert!(ld.id.starts_with("urn:uuid:"));
        assert_eq!(ld.ld_type, "schema:DigitalDocument");
        assert_eq!(ld.skos_concepts.len(), 1);
        assert_eq!(ld.prov_derived_from.len(), 1);
        assert!(ld.prov_generated_by.is_some());
    }

    #[test]
    fn test_json_ld_serialization() {
        let dc = DublinCoreExport::from_note(
            Uuid::new_v4(),
            Some("Test"),
            &[],
            Utc::now(),
            None,
            &[],
            false,
        );

        let ld = JsonLdExport::from_dublin_core(dc, vec![], vec![], None);
        let json = serde_json::to_string_pretty(&ld).unwrap();
        assert!(json.contains("@context"));
        assert!(json.contains("@id"));
        assert!(json.contains("@type"));
        assert!(json.contains("schema:DigitalDocument"));
    }

    #[test]
    fn fair_export_debug_redacts_note_metadata_and_identifiers() {
        let note_id = Uuid::parse_str("aaaaaaaa-1111-4222-8333-aaaaaaaaaaaa").unwrap();
        let linked_id = Uuid::parse_str("bbbbbbbb-2222-4333-8444-bbbbbbbbbbbb").unwrap();
        let source_id = Uuid::parse_str("cccccccc-3333-4444-8555-cccccccccccc").unwrap();
        let now = Utc::now();

        let dc = DublinCoreExport::from_note(
            note_id,
            Some("Secret roadmap title for owner@example.internal"),
            &[
                "postgres://user:secret@db.internal/fortemi".to_string(),
                "sk-secret-fair-tag".to_string(),
            ],
            now,
            Some("/srv/fortemi/private/collection"),
            &[linked_id],
            true,
        )
        .with_description("Private paragraph with bearer-secret and host internal.example")
        .with_creator("creator-secret@example.internal".to_string())
        .with_language("en-secret-locale".to_string())
        .with_rights("rights-secret-value".to_string())
        .with_source(source_id);

        let dc_debug = format!("{dc:?}");
        assert!(dc_debug.contains("DublinCoreExport"));
        assert!(dc_debug.contains("title_len"));
        assert!(dc_debug.contains("subject_count"));
        assert!(dc_debug.contains("description_len"));
        assert!(dc_debug.contains("relation_count"));
        assert_debug_excludes(
            &dc_debug,
            &[
                "Secret roadmap title",
                "owner@example.internal",
                "postgres://user:secret@db.internal/fortemi",
                "sk-secret-fair-tag",
                "/srv/fortemi/private/collection",
                "bearer-secret",
                "internal.example",
                "creator-secret@example.internal",
                "en-secret-locale",
                "rights-secret-value",
                "aaaaaaaa-1111-4222-8333-aaaaaaaaaaaa",
                "bbbbbbbb-2222-4333-8444-bbbbbbbbbbbb",
                "cccccccc-3333-4444-8555-cccccccccccc",
            ],
        );

        let ld = JsonLdExport::from_dublin_core(
            dc,
            vec![
                "concept-secret@example.internal".to_string(),
                "postgres://skos:secret@db.internal/skos".to_string(),
            ],
            vec!["urn:uuid:dddddddd-4444-4555-8666-dddddddddddd".to_string()],
            Some("activity-secret-token".to_string()),
        );

        let ld_debug = format!("{ld:?}");
        assert!(ld_debug.contains("JsonLdExport"));
        assert!(ld_debug.contains("dublin_core"));
        assert!(ld_debug.contains("skos_concepts_count"));
        assert!(ld_debug.contains("prov_derived_from_count"));
        assert!(ld_debug.contains("context_namespace_count"));
        assert_debug_excludes(
            &ld_debug,
            &[
                "Secret roadmap title",
                "owner@example.internal",
                "postgres://user:secret@db.internal/fortemi",
                "sk-secret-fair-tag",
                "/srv/fortemi/private/collection",
                "bearer-secret",
                "internal.example",
                "creator-secret@example.internal",
                "concept-secret@example.internal",
                "postgres://skos:secret@db.internal/skos",
                "dddddddd-4444-4555-8666-dddddddddddd",
                "activity-secret-token",
                "http://purl.org/dc/elements/1.1/",
                "http://www.w3.org/2004/02/skos/core#",
                "http://www.w3.org/ns/prov#",
                "https://schema.org/",
            ],
        );

        let context_debug = format!("{:?}", JsonLdContext::default());
        assert!(context_debug.contains("JsonLdContext"));
        assert!(context_debug.contains("dc_len"));
        assert_debug_excludes(
            &context_debug,
            &[
                "http://purl.org/dc/elements/1.1/",
                "http://www.w3.org/2004/02/skos/core#",
                "http://www.w3.org/ns/prov#",
                "https://schema.org/",
            ],
        );

        let fair_score = FairScore {
            findable: 0.1,
            accessible: 0.2,
            interoperable: 0.3,
            reusable: 0.4,
            overall: 0.25,
            issues: vec![
                "Missing relation for private@example.internal".to_string(),
                "License URL contains https://rights.example.test/?token=secret".to_string(),
            ],
        };
        let score_debug = format!("{fair_score:?}");
        assert!(score_debug.contains("FairScore"));
        assert!(score_debug.contains("issues_count"));
        assert_debug_excludes(
            &score_debug,
            &[
                "private@example.internal",
                "https://rights.example.test/?token=secret",
                "token=secret",
            ],
        );
    }

    #[test]
    fn fair_export_debug_reports_character_counts_for_unicode_metadata() {
        let now = Utc::now();
        let dc = DublinCoreExport {
            identifier: "id".to_string(),
            title: "éé".to_string(),
            creator: Some("åå".to_string()),
            subject: vec!["ö丼".to_string(), "üü".to_string()],
            description: Some("ññ".to_string()),
            publisher: "øø".to_string(),
            contributor: vec!["çç".to_string()],
            date: now,
            dc_type: "îî".to_string(),
            format: "ßß".to_string(),
            source: Some("áá".to_string()),
            language: Some("èè".to_string()),
            relation: vec!["ôô".to_string()],
            coverage: Some("ùù".to_string()),
            rights: Some("ââ".to_string()),
        };

        let dc_debug = format!("{dc:?}");
        for expected in [
            "identifier_len: 2",
            "title_len: 2",
            "creator_len: Some(2)",
            "subject_lens: [2, 2]",
            "description_len: Some(2)",
            "publisher_len: 2",
            "contributor_lens: [2]",
            "dc_type_len: 2",
            "format_len: 2",
            "source_len: Some(2)",
            "language_len: Some(2)",
            "relation_lens: [2]",
            "coverage_len: Some(2)",
            "rights_len: Some(2)",
        ] {
            assert!(
                dc_debug.contains(expected),
                "expected {expected:?} in {dc_debug}"
            );
        }
        assert_debug_excludes(
            &dc_debug,
            &[
                "éé", "åå", "ö丼", "üü", "ññ", "øø", "çç", "îî", "ßß", "áá", "èè", "ôô", "ùù", "ââ",
            ],
        );

        let context = JsonLdContext {
            dc: "êê".to_string(),
            skos: "óó".to_string(),
            prov: "úú".to_string(),
            schema: "ää".to_string(),
        };
        let context_debug = format!("{context:?}");
        for expected in ["dc_len: 2", "skos_len: 2", "prov_len: 2", "schema_len: 2"] {
            assert!(
                context_debug.contains(expected),
                "expected {expected:?} in {context_debug}"
            );
        }
        assert_debug_excludes(&context_debug, &["êê", "óó", "úú", "ää"]);

        let ld = JsonLdExport {
            context,
            id: "ëë".to_string(),
            ld_type: "íí".to_string(),
            dublin_core: dc,
            skos_concepts: vec!["ìì".to_string()],
            prov_derived_from: vec!["òò".to_string()],
            prov_generated_by: Some("œœ".to_string()),
        };
        let ld_debug = format!("{ld:?}");
        for expected in [
            "id_len: 2",
            "ld_type_len: 2",
            "skos_concept_lens: [2]",
            "prov_derived_from_lens: [2]",
            "prov_generated_by_len: Some(2)",
        ] {
            assert!(
                ld_debug.contains(expected),
                "expected {expected:?} in {ld_debug}"
            );
        }
        assert_debug_excludes(&ld_debug, &["ëë", "íí", "ìì", "òò", "œœ"]);

        let score = FairScore {
            findable: 0.1,
            accessible: 0.2,
            interoperable: 0.3,
            reusable: 0.4,
            overall: 0.25,
            issues: vec!["ýý".to_string(), "þþ".to_string()],
        };
        let score_debug = format!("{score:?}");
        assert!(
            score_debug.contains("issue_lens: [2, 2]"),
            "expected character-count issue_lens in {score_debug}"
        );
        assert_debug_excludes(&score_debug, &["ýý", "þþ"]);
    }

    // =========================================================================
    // FAIR Score Tests
    // =========================================================================

    #[test]
    fn test_fair_score_full_metadata() {
        let dc = DublinCoreExport::from_note(
            Uuid::new_v4(),
            Some("Well-Titled Note"),
            &["tag1".to_string(), "tag2".to_string()],
            Utc::now(),
            Some("Collection/Path"),
            &[Uuid::new_v4()],
            true,
        )
        .with_description("A great first paragraph.\n\nMore content.")
        .with_language("en".to_string())
        .with_rights("CC-BY-4.0".to_string());

        let score = FairScore::assess(&dc);
        assert!(score.findable > 0.9); // Full findable score
        assert_eq!(score.accessible, 1.0);
        assert!(score.interoperable > 0.9);
        assert!(score.reusable > 0.9);
        assert!(score.overall > 0.9);
        assert!(score.issues.is_empty());
    }

    #[test]
    fn test_fair_score_minimal_metadata() {
        let dc = DublinCoreExport::from_note(
            Uuid::new_v4(),
            None, // no title
            &[],  // no tags
            Utc::now(),
            None,  // no collection
            &[],   // no links
            false, // no AI revision
        );

        let score = FairScore::assess(&dc);
        assert!(score.findable < 0.5); // Missing title, subject, description
        assert_eq!(score.accessible, 1.0); // Always accessible via API
        assert!(score.interoperable < 1.0); // Missing relations
        assert!(score.reusable < 1.0); // Missing contributor, rights, language
        assert!(!score.issues.is_empty());
    }

    #[test]
    fn test_fair_score_issues_tracking() {
        let dc =
            DublinCoreExport::from_note(Uuid::new_v4(), None, &[], Utc::now(), None, &[], false);

        let score = FairScore::assess(&dc);
        // Should have issues for: generic title, no subject, no description,
        // no relations, no contributor, no rights, no language
        assert!(score.issues.len() >= 5);
        assert!(score.issues.iter().any(|i| i.contains("F2")));
        assert!(score.issues.iter().any(|i| i.contains("R1")));
    }
}
