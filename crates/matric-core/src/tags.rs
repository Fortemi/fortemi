//! W3C SKOS-Compliant Hierarchical Tag System Types
//!
//! This module provides a full implementation of the W3C SKOS (Simple Knowledge
//! Organization System) data model with extensions for PMEST facets, validation
//! rules, and anti-pattern detection.
//!
//! # Standards Compliance
//!
//! - W3C SKOS Reference: https://www.w3.org/TR/skos-reference/
//! - SKOS-XL (eXtension for Labels): https://www.w3.org/TR/skos-reference/skos-xl.html
//!
//! # Key Features
//!
//! - Full SKOS concept model with prefLabel, altLabel, hiddenLabel
//! - Hierarchical relations (broader/narrower) with polyhierarchy support
//! - Associative relations (related) for non-hierarchical links
//! - Concept schemes for vocabulary namespaces
//! - PMEST facets for faceted classification
//! - Mapping properties for cross-vocabulary alignment
//! - Validation rules (depth, breadth, polyhierarchy limits)
//! - Anti-pattern detection for governance
//! - Embedding support for semantic operations

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::fmt;
use uuid::Uuid;

fn text_len(value: &str) -> usize {
    value.chars().count()
}

// =============================================================================
// SKOS ENUMS
// =============================================================================

/// SKOS semantic relation types for concept relationships.
///
/// These represent the core hierarchical and associative relationships
/// defined in the SKOS specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SkosSemanticRelation {
    /// `skos:broader` - The subject concept has a more general meaning.
    /// Used to build hierarchies from specific to general.
    Broader,

    /// `skos:narrower` - The subject concept has a more specific meaning.
    /// Inverse of broader; auto-generated when broader is created.
    Narrower,

    /// `skos:related` - The concepts are associatively related.
    /// Symmetric relation for non-hierarchical links.
    Related,
}

impl std::fmt::Display for SkosSemanticRelation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Broader => write!(f, "broader"),
            Self::Narrower => write!(f, "narrower"),
            Self::Related => write!(f, "related"),
        }
    }
}

impl std::str::FromStr for SkosSemanticRelation {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "broader" => Ok(Self::Broader),
            "narrower" => Ok(Self::Narrower),
            "related" => Ok(Self::Related),
            _ => Err(format!(
                "Invalid SKOS semantic relation; value_len={}",
                text_len(s)
            )),
        }
    }
}

/// SKOS mapping relation types for cross-vocabulary links.
///
/// Used to establish equivalence or similarity between concepts
/// in different vocabularies or knowledge organization systems.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SkosMappingRelation {
    /// `skos:exactMatch` - High confidence equivalence.
    /// The concepts can be used interchangeably.
    ExactMatch,

    /// `skos:closeMatch` - Near equivalence.
    /// Similar meaning but not exact; use with caution.
    CloseMatch,

    /// `skos:broadMatch` - Broader concept in external vocabulary.
    BroadMatch,

    /// `skos:narrowMatch` - Narrower concept in external vocabulary.
    NarrowMatch,

    /// `skos:relatedMatch` - Related concept in external vocabulary.
    RelatedMatch,
}

impl std::fmt::Display for SkosMappingRelation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ExactMatch => write!(f, "exact_match"),
            Self::CloseMatch => write!(f, "close_match"),
            Self::BroadMatch => write!(f, "broad_match"),
            Self::NarrowMatch => write!(f, "narrow_match"),
            Self::RelatedMatch => write!(f, "related_match"),
        }
    }
}

impl std::str::FromStr for SkosMappingRelation {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "exact_match" | "exactmatch" => Ok(Self::ExactMatch),
            "close_match" | "closematch" => Ok(Self::CloseMatch),
            "broad_match" | "broadmatch" => Ok(Self::BroadMatch),
            "narrow_match" | "narrowmatch" => Ok(Self::NarrowMatch),
            "related_match" | "relatedmatch" => Ok(Self::RelatedMatch),
            _ => Err(format!(
                "Invalid SKOS mapping relation; value_len={}",
                text_len(s)
            )),
        }
    }
}

/// SKOS label types for lexical labels.
///
/// Each concept can have multiple labels of different types
/// to support various use cases (display, search, aliases).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum SkosLabelType {
    /// `skos:prefLabel` - Preferred label for display.
    /// Maximum one per language per concept.
    #[default]
    PrefLabel,

    /// `skos:altLabel` - Alternative label (synonym, abbreviation).
    /// Multiple allowed per language.
    AltLabel,

    /// `skos:hiddenLabel` - Hidden label for search/indexing.
    /// Not displayed to users but aids discovery (misspellings, codes).
    HiddenLabel,
}

impl std::fmt::Display for SkosLabelType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PrefLabel => write!(f, "pref_label"),
            Self::AltLabel => write!(f, "alt_label"),
            Self::HiddenLabel => write!(f, "hidden_label"),
        }
    }
}

impl std::str::FromStr for SkosLabelType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pref_label" | "preflabel" => Ok(Self::PrefLabel),
            "alt_label" | "altlabel" => Ok(Self::AltLabel),
            "hidden_label" | "hiddenlabel" => Ok(Self::HiddenLabel),
            _ => Err(format!(
                "Invalid SKOS label type; value_len={}",
                text_len(s)
            )),
        }
    }
}

/// SKOS documentation note types.
///
/// Used to provide various kinds of documentation for concepts,
/// supporting different documentation purposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum SkosNoteType {
    /// `skos:definition` - Formal definition of the concept.
    Definition,

    /// `skos:scopeNote` - Guidance on intended usage scope.
    ScopeNote,

    /// `skos:example` - Example usage or instance.
    Example,

    /// `skos:historyNote` - Historical information about the concept.
    HistoryNote,

    /// `skos:editorialNote` - Internal notes for editors/maintainers.
    EditorialNote,

    /// `skos:changeNote` - Documentation of changes.
    ChangeNote,

    /// `skos:note` - General note (catch-all).
    #[default]
    Note,
}

impl std::fmt::Display for SkosNoteType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Definition => write!(f, "definition"),
            Self::ScopeNote => write!(f, "scope_note"),
            Self::Example => write!(f, "example"),
            Self::HistoryNote => write!(f, "history_note"),
            Self::EditorialNote => write!(f, "editorial_note"),
            Self::ChangeNote => write!(f, "change_note"),
            Self::Note => write!(f, "note"),
        }
    }
}

impl std::str::FromStr for SkosNoteType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "definition" => Ok(Self::Definition),
            "scope_note" | "scopenote" => Ok(Self::ScopeNote),
            "example" => Ok(Self::Example),
            "history_note" | "historynote" => Ok(Self::HistoryNote),
            "editorial_note" | "editorialnote" => Ok(Self::EditorialNote),
            "change_note" | "changenote" => Ok(Self::ChangeNote),
            "note" => Ok(Self::Note),
            _ => Err(format!("Invalid SKOS note type; value_len={}", text_len(s))),
        }
    }
}

/// PMEST facet types (Ranganathan's Colon Classification).
///
/// Provides a fundamental framework for classifying any subject
/// into five basic categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PmestFacet {
    /// Personality: The most specific/distinguishing characteristic.
    /// What the subject fundamentally IS.
    Personality,

    /// Matter: The material, substance, or constituent.
    /// What it's MADE OF or ABOUT.
    Matter,

    /// Energy: The process, activity, or operation.
    /// What HAPPENS or is DONE.
    Energy,

    /// Space: Geographic or spatial aspect.
    /// WHERE it occurs or applies.
    Space,

    /// Time: Temporal aspect.
    /// WHEN it occurs or applies.
    Time,
}

impl std::fmt::Display for PmestFacet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Personality => write!(f, "personality"),
            Self::Matter => write!(f, "matter"),
            Self::Energy => write!(f, "energy"),
            Self::Space => write!(f, "space"),
            Self::Time => write!(f, "time"),
        }
    }
}

impl std::str::FromStr for PmestFacet {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "personality" | "p" => Ok(Self::Personality),
            "matter" | "m" => Ok(Self::Matter),
            "energy" | "e" => Ok(Self::Energy),
            "space" | "s" => Ok(Self::Space),
            "time" | "t" => Ok(Self::Time),
            _ => Err(format!("Invalid PMEST facet; value_len={}", text_len(s))),
        }
    }
}

/// Tag/concept status for workflow management.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum TagStatus {
    /// Candidate: Proposed but not yet approved for general use.
    /// Requires literary warrant (3+ notes) for automatic promotion.
    #[default]
    Candidate,

    /// Approved: Validated and available for general tagging.
    Approved,

    /// Deprecated: Marked for removal; should not be used for new tags.
    /// Existing uses remain but new applications are discouraged.
    Deprecated,

    /// Obsolete: No longer valid; kept only for historical reference.
    Obsolete,
}

impl std::fmt::Display for TagStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Candidate => write!(f, "candidate"),
            Self::Approved => write!(f, "approved"),
            Self::Deprecated => write!(f, "deprecated"),
            Self::Obsolete => write!(f, "obsolete"),
        }
    }
}

impl std::str::FromStr for TagStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "candidate" => Ok(Self::Candidate),
            "approved" => Ok(Self::Approved),
            "deprecated" => Ok(Self::Deprecated),
            "obsolete" => Ok(Self::Obsolete),
            _ => Err(format!("Invalid tag status; value_len={}", text_len(s))),
        }
    }
}

/// Anti-pattern types for taxonomy governance.
///
/// These flags identify structural or usage issues that may
/// indicate problems with the taxonomy design.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TagAntipattern {
    /// Orphan: No hierarchical connections (isolated concept).
    Orphan,

    /// Over-tagged: Too many tags applied to a single resource.
    OverTagged,

    /// Under-used: Approved but rarely applied (low warrant).
    UnderUsed,

    /// Too broad: Excessive narrower concepts (>8 children).
    TooBroad,

    /// Too deep: Exceeds recommended depth (>4 levels).
    TooDeep,

    /// Polyhierarchy excess: Too many broader concepts (>2).
    PolyhierarchyExcess,

    /// Missing labels: Lacks required prefLabel.
    MissingLabels,

    /// Circular hierarchy: Detected cycle in broader/narrower chain.
    CircularHierarchy,
}

impl std::fmt::Display for TagAntipattern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Orphan => write!(f, "orphan"),
            Self::OverTagged => write!(f, "over_tagged"),
            Self::UnderUsed => write!(f, "under_used"),
            Self::TooBroad => write!(f, "too_broad"),
            Self::TooDeep => write!(f, "too_deep"),
            Self::PolyhierarchyExcess => write!(f, "polyhierarchy_excess"),
            Self::MissingLabels => write!(f, "missing_labels"),
            Self::CircularHierarchy => write!(f, "circular_hierarchy"),
        }
    }
}

impl std::str::FromStr for TagAntipattern {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "orphan" => Ok(Self::Orphan),
            "over_tagged" | "overtagged" => Ok(Self::OverTagged),
            "under_used" | "underused" => Ok(Self::UnderUsed),
            "too_broad" | "toobroad" => Ok(Self::TooBroad),
            "too_deep" | "toodeep" => Ok(Self::TooDeep),
            "polyhierarchy_excess" | "polyhierarchyexcess" => Ok(Self::PolyhierarchyExcess),
            "missing_labels" | "missinglabels" => Ok(Self::MissingLabels),
            "circular_hierarchy" | "circularhierarchy" => Ok(Self::CircularHierarchy),
            _ => Err(format!(
                "Invalid tag antipattern; value_len={}",
                text_len(s)
            )),
        }
    }
}

// =============================================================================
// SKOS CONCEPT SCHEME
// =============================================================================

/// SKOS Concept Scheme - a vocabulary/namespace container.
///
/// Concept schemes group related concepts into a coherent vocabulary.
/// Examples: "Topics", "Domains", "Project Tags", "Imported Vocabulary".
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SkosConceptScheme {
    pub id: Uuid,

    /// Canonical URI for the scheme (e.g., "https://matric.io/schemes/topics").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,

    /// Short notation/code (e.g., "topics", "domains").
    pub notation: String,

    /// Human-readable title.
    pub title: String,

    /// Description of the scheme's purpose and scope.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Creator/author of the scheme.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator: Option<String>,

    /// Publisher organization.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,

    /// Rights/license information.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rights: Option<String>,

    /// Version string (semver recommended).
    #[serde(default = "default_version")]
    pub version: String,

    /// Whether the scheme is active (visible for tagging).
    pub is_active: bool,

    /// System scheme (protected from deletion).
    pub is_system: bool,

    /// Timestamps.
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,

    /// Official publication date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issued_at: Option<DateTime<Utc>>,

    /// Last content modification.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<DateTime<Utc>>,
}

impl fmt::Debug for SkosConceptScheme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SkosConceptScheme")
            .field("id_set", &true)
            .field("uri_len", &self.uri.as_ref().map(|value| value.len()))
            .field("notation_len", &self.notation.len())
            .field("title_len", &self.title.len())
            .field(
                "description_len",
                &self.description.as_ref().map(|value| value.len()),
            )
            .field(
                "creator_len",
                &self.creator.as_ref().map(|value| value.len()),
            )
            .field(
                "publisher_len",
                &self.publisher.as_ref().map(|value| value.len()),
            )
            .field("rights_len", &self.rights.as_ref().map(|value| value.len()))
            .field("version_len", &self.version.len())
            .field("is_active", &self.is_active)
            .field("is_system", &self.is_system)
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .field("issued_at_set", &self.issued_at.is_some())
            .field("modified_at_set", &self.modified_at.is_some())
            .finish()
    }
}

fn default_version() -> String {
    "1.0.0".to_string()
}

/// Summary view of a concept scheme for listings.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SkosConceptSchemeSummary {
    pub id: Uuid,
    pub notation: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub is_active: bool,
    pub is_system: bool,
    pub concept_count: i64,
    pub updated_at: DateTime<Utc>,
}

impl fmt::Debug for SkosConceptSchemeSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SkosConceptSchemeSummary")
            .field("id_set", &true)
            .field("notation_len", &self.notation.len())
            .field("title_len", &self.title.len())
            .field(
                "description_len",
                &self.description.as_ref().map(|value| value.len()),
            )
            .field("is_active", &self.is_active)
            .field("is_system", &self.is_system)
            .field("concept_count", &self.concept_count)
            .field("updated_at", &self.updated_at)
            .finish()
    }
}

/// Request to create a new concept scheme.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateConceptSchemeRequest {
    pub notation: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rights: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

impl fmt::Debug for CreateConceptSchemeRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CreateConceptSchemeRequest")
            .field("notation_len", &self.notation.len())
            .field("title_len", &self.title.len())
            .field("uri_len", &self.uri.as_ref().map(|value| value.len()))
            .field(
                "description_len",
                &self.description.as_ref().map(|value| value.len()),
            )
            .field(
                "creator_len",
                &self.creator.as_ref().map(|value| value.len()),
            )
            .field(
                "publisher_len",
                &self.publisher.as_ref().map(|value| value.len()),
            )
            .field("rights_len", &self.rights.as_ref().map(|value| value.len()))
            .field(
                "version_len",
                &self.version.as_ref().map(|value| value.len()),
            )
            .finish()
    }
}

/// Request to update a concept scheme.
#[derive(Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateConceptSchemeRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rights: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_active: Option<bool>,
}

impl fmt::Debug for UpdateConceptSchemeRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UpdateConceptSchemeRequest")
            .field("title_len", &self.title.as_ref().map(|value| value.len()))
            .field(
                "description_len",
                &self.description.as_ref().map(|value| value.len()),
            )
            .field(
                "creator_len",
                &self.creator.as_ref().map(|value| value.len()),
            )
            .field(
                "publisher_len",
                &self.publisher.as_ref().map(|value| value.len()),
            )
            .field("rights_len", &self.rights.as_ref().map(|value| value.len()))
            .field(
                "version_len",
                &self.version.as_ref().map(|value| value.len()),
            )
            .field("is_active", &self.is_active)
            .finish()
    }
}

// =============================================================================
// SKOS CONCEPT
// =============================================================================

/// SKOS Concept - the core tag/concept entity.
///
/// Represents a single concept in the knowledge organization system,
/// with full support for SKOS properties and PMEST facets.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SkosConcept {
    pub id: Uuid,

    /// Primary scheme this concept belongs to.
    pub primary_scheme_id: Uuid,

    /// Canonical URI for the concept.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,

    /// Short notation/code within the scheme.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notation: Option<String>,

    // PMEST Facets
    /// Primary facet classification.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub facet_type: Option<PmestFacet>,

    /// Domain/context for the facet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub facet_source: Option<String>,

    /// Subject domain.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub facet_domain: Option<String>,

    /// Scope description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub facet_scope: Option<String>,

    // Status and workflow
    /// Current status in the approval workflow.
    pub status: TagStatus,

    /// When the concept was promoted to approved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub promoted_at: Option<DateTime<Utc>>,

    /// When the concept was deprecated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated_at: Option<DateTime<Utc>>,

    /// Reason for deprecation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecation_reason: Option<String>,

    /// Replacement concept for deprecated concepts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replaced_by_id: Option<Uuid>,

    // Literary warrant tracking
    /// Number of notes tagged with this concept.
    pub note_count: i32,

    /// When the concept was first used to tag a note.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_used_at: Option<DateTime<Utc>>,

    /// When the concept was last used to tag a note.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<DateTime<Utc>>,

    // Hierarchy metadata
    /// Depth in the hierarchy (0 = top concept).
    pub depth: i32,

    /// Number of broader (parent) concepts.
    pub broader_count: i32,

    /// Number of narrower (child) concepts.
    pub narrower_count: i32,

    /// Number of related concepts.
    pub related_count: i32,

    // Governance
    /// Detected anti-patterns.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub antipatterns: Vec<TagAntipattern>,

    /// When anti-patterns were last checked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub antipattern_checked_at: Option<DateTime<Utc>>,

    // Timestamps
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,

    // Embedding metadata (vector stored separately)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding_model: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedded_at: Option<DateTime<Utc>>,
}

impl fmt::Debug for SkosConcept {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SkosConcept")
            .field("id_set", &true)
            .field("primary_scheme_id_set", &true)
            .field("uri_len", &self.uri.as_ref().map(|value| value.len()))
            .field(
                "notation_len",
                &self.notation.as_ref().map(|value| value.len()),
            )
            .field("facet_type", &self.facet_type)
            .field(
                "facet_source_len",
                &self.facet_source.as_ref().map(|value| value.len()),
            )
            .field(
                "facet_domain_len",
                &self.facet_domain.as_ref().map(|value| value.len()),
            )
            .field(
                "facet_scope_len",
                &self.facet_scope.as_ref().map(|value| value.len()),
            )
            .field("status", &self.status)
            .field("promoted_at", &self.promoted_at)
            .field("deprecated_at", &self.deprecated_at)
            .field(
                "deprecation_reason_len",
                &self.deprecation_reason.as_ref().map(|value| value.len()),
            )
            .field("replaced_by_id_set", &self.replaced_by_id.is_some())
            .field("note_count", &self.note_count)
            .field("first_used_at", &self.first_used_at)
            .field("last_used_at", &self.last_used_at)
            .field("depth", &self.depth)
            .field("broader_count", &self.broader_count)
            .field("narrower_count", &self.narrower_count)
            .field("related_count", &self.related_count)
            .field("antipattern_count", &self.antipatterns.len())
            .field("antipattern_checked_at", &self.antipattern_checked_at)
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .field(
                "embedding_model_len",
                &self.embedding_model.as_ref().map(|value| value.len()),
            )
            .field("embedded_at", &self.embedded_at)
            .finish()
    }
}

/// Concept with its preferred label for display.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SkosConceptWithLabel {
    #[serde(flatten)]
    pub concept: SkosConcept,

    /// Preferred label (usually English).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pref_label: Option<String>,

    /// Language of the preferred label.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label_language: Option<String>,

    /// Scheme notation for context.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheme_notation: Option<String>,

    /// Scheme title for display.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheme_title: Option<String>,
}

impl fmt::Debug for SkosConceptWithLabel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SkosConceptWithLabel")
            .field("concept", &self.concept)
            .field(
                "pref_label_len",
                &self.pref_label.as_ref().map(|value| value.len()),
            )
            .field(
                "label_language_len",
                &self.label_language.as_ref().map(|value| value.len()),
            )
            .field(
                "scheme_notation_len",
                &self.scheme_notation.as_ref().map(|value| value.len()),
            )
            .field(
                "scheme_title_len",
                &self.scheme_title.as_ref().map(|value| value.len()),
            )
            .finish()
    }
}

/// Full concept with all labels, notes, and relations.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SkosConceptFull {
    #[serde(flatten)]
    pub concept: SkosConcept,

    /// All labels for this concept.
    pub labels: Vec<SkosConceptLabel>,

    /// All documentation notes.
    pub notes: Vec<SkosConceptNote>,

    /// Broader concepts (parents).
    pub broader: Vec<SkosConceptSummary>,

    /// Narrower concepts (children).
    pub narrower: Vec<SkosConceptSummary>,

    /// Related concepts.
    pub related: Vec<SkosConceptSummary>,

    /// Mapping relations to external vocabularies.
    pub mappings: Vec<SkosMappingRelationEdge>,

    /// Schemes this concept belongs to.
    pub schemes: Vec<SkosConceptSchemeSummary>,
}

impl fmt::Debug for SkosConceptFull {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SkosConceptFull")
            .field("concept", &self.concept)
            .field("label_count", &self.labels.len())
            .field("note_count", &self.notes.len())
            .field("broader_count", &self.broader.len())
            .field("narrower_count", &self.narrower.len())
            .field("related_count", &self.related.len())
            .field("mapping_count", &self.mappings.len())
            .field("scheme_count", &self.schemes.len())
            .finish()
    }
}

/// Summary view of a concept for listings and relations.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SkosConceptSummary {
    pub id: Uuid,
    pub notation: Option<String>,
    pub pref_label: Option<String>,
    pub status: TagStatus,
    pub note_count: i32,
    pub depth: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheme_notation: Option<String>,
}

impl fmt::Debug for SkosConceptSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SkosConceptSummary")
            .field("id_set", &true)
            .field(
                "notation_len",
                &self.notation.as_ref().map(|value| value.len()),
            )
            .field(
                "pref_label_len",
                &self.pref_label.as_ref().map(|value| value.len()),
            )
            .field("status", &self.status)
            .field("note_count", &self.note_count)
            .field("depth", &self.depth)
            .field(
                "scheme_notation_len",
                &self.scheme_notation.as_ref().map(|value| value.len()),
            )
            .finish()
    }
}

/// Concept in hierarchy view with path information.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SkosConceptHierarchy {
    pub id: Uuid,
    pub notation: Option<String>,
    pub label: Option<String>,
    pub level: i32,
    pub path: Vec<Uuid>,
    pub label_path: Vec<String>,
}

impl fmt::Debug for SkosConceptHierarchy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label_path_lens: Vec<usize> = self.label_path.iter().map(String::len).collect();
        f.debug_struct("SkosConceptHierarchy")
            .field("id_set", &true)
            .field(
                "notation_len",
                &self.notation.as_ref().map(|value| value.len()),
            )
            .field("label_len", &self.label.as_ref().map(|value| value.len()))
            .field("level", &self.level)
            .field("path_count", &self.path.len())
            .field("label_path_lens", &label_path_lens)
            .finish()
    }
}

/// Request to create a new concept.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateConceptRequest {
    /// Scheme to create the concept in.
    pub scheme_id: Uuid,

    /// Short notation (optional, auto-generated if not provided).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notation: Option<String>,

    /// Preferred label (required).
    pub pref_label: String,

    /// Language for the preferred label.
    #[serde(default = "default_language")]
    pub language: String,

    /// Initial status.
    #[serde(default)]
    pub status: TagStatus,

    // Optional PMEST facets
    #[serde(skip_serializing_if = "Option::is_none")]
    pub facet_type: Option<PmestFacet>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub facet_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub facet_domain: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub facet_scope: Option<String>,

    // Optional documentation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub definition: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope_note: Option<String>,

    // Optional relations (created after concept)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub broader_ids: Vec<Uuid>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_ids: Vec<Uuid>,

    // Alternative labels
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alt_labels: Vec<String>,
}

fn default_language() -> String {
    "en".to_string()
}

impl fmt::Debug for CreateConceptRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let alt_label_lens: Vec<usize> = self.alt_labels.iter().map(String::len).collect();
        f.debug_struct("CreateConceptRequest")
            .field("scheme_id_set", &true)
            .field(
                "notation_len",
                &self.notation.as_ref().map(|value| value.len()),
            )
            .field("pref_label_len", &self.pref_label.len())
            .field("language_len", &self.language.len())
            .field("status", &self.status)
            .field("facet_type", &self.facet_type)
            .field(
                "facet_source_len",
                &self.facet_source.as_ref().map(|value| value.len()),
            )
            .field(
                "facet_domain_len",
                &self.facet_domain.as_ref().map(|value| value.len()),
            )
            .field(
                "facet_scope_len",
                &self.facet_scope.as_ref().map(|value| value.len()),
            )
            .field(
                "definition_len",
                &self.definition.as_ref().map(|value| value.len()),
            )
            .field(
                "scope_note_len",
                &self.scope_note.as_ref().map(|value| value.len()),
            )
            .field("broader_id_count", &self.broader_ids.len())
            .field("related_id_count", &self.related_ids.len())
            .field("alt_label_lens", &alt_label_lens)
            .finish()
    }
}

/// Request to update a concept.
#[derive(Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateConceptRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<TagStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecation_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replaced_by_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub facet_type: Option<PmestFacet>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub facet_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub facet_domain: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub facet_scope: Option<String>,
}

impl fmt::Debug for UpdateConceptRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UpdateConceptRequest")
            .field(
                "notation_len",
                &self.notation.as_ref().map(|value| value.len()),
            )
            .field("status", &self.status)
            .field(
                "deprecation_reason_len",
                &self.deprecation_reason.as_ref().map(|value| value.len()),
            )
            .field("replaced_by_id_set", &self.replaced_by_id.is_some())
            .field("facet_type", &self.facet_type)
            .field(
                "facet_source_len",
                &self.facet_source.as_ref().map(|value| value.len()),
            )
            .field(
                "facet_domain_len",
                &self.facet_domain.as_ref().map(|value| value.len()),
            )
            .field(
                "facet_scope_len",
                &self.facet_scope.as_ref().map(|value| value.len()),
            )
            .finish()
    }
}

// =============================================================================
// SKOS LABELS
// =============================================================================

/// A lexical label for a concept.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SkosConceptLabel {
    pub id: Uuid,
    pub concept_id: Uuid,
    pub label_type: SkosLabelType,
    pub value: String,
    pub language: String,
    pub created_at: DateTime<Utc>,
}

impl fmt::Debug for SkosConceptLabel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SkosConceptLabel")
            .field("id_set", &true)
            .field("concept_id_set", &true)
            .field("label_type", &self.label_type)
            .field("value_len", &self.value.len())
            .field("language_len", &self.language.len())
            .field("created_at", &self.created_at)
            .finish()
    }
}

/// Request to add a label to a concept.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AddLabelRequest {
    pub concept_id: Uuid,
    #[serde(default)]
    pub label_type: SkosLabelType,
    pub value: String,
    #[serde(default = "default_language")]
    pub language: String,
}

impl fmt::Debug for AddLabelRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AddLabelRequest")
            .field("concept_id_set", &true)
            .field("label_type", &self.label_type)
            .field("value_len", &self.value.len())
            .field("language_len", &self.language.len())
            .finish()
    }
}

// =============================================================================
// SKOS NOTES (Documentation)
// =============================================================================

/// A documentation note for a concept.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SkosConceptNote {
    pub id: Uuid,
    pub concept_id: Uuid,
    pub note_type: SkosNoteType,
    pub value: String,
    pub language: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl fmt::Debug for SkosConceptNote {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SkosConceptNote")
            .field("id_set", &true)
            .field("concept_id_set", &true)
            .field("note_type", &self.note_type)
            .field("value_len", &self.value.len())
            .field("language_len", &self.language.len())
            .field("author_len", &self.author.as_ref().map(|value| value.len()))
            .field("source_len", &self.source.as_ref().map(|value| value.len()))
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .finish()
    }
}

/// Request to add a note to a concept.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AddNoteRequest {
    pub concept_id: Uuid,
    #[serde(default)]
    pub note_type: SkosNoteType,
    pub value: String,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

impl fmt::Debug for AddNoteRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AddNoteRequest")
            .field("concept_id_set", &true)
            .field("note_type", &self.note_type)
            .field("value_len", &self.value.len())
            .field("language_len", &self.language.len())
            .field("author_len", &self.author.as_ref().map(|value| value.len()))
            .field("source_len", &self.source.as_ref().map(|value| value.len()))
            .finish()
    }
}

// =============================================================================
// SKOS RELATIONS
// =============================================================================

/// A semantic relation between two concepts.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SkosSemanticRelationEdge {
    pub id: Uuid,
    pub subject_id: Uuid,
    pub object_id: Uuid,
    pub relation_type: SkosSemanticRelation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inference_score: Option<f32>,
    pub is_inferred: bool,
    pub is_validated: bool,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
}

impl fmt::Debug for SkosSemanticRelationEdge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SkosSemanticRelationEdge")
            .field("id_set", &true)
            .field("subject_id_set", &true)
            .field("object_id_set", &true)
            .field("relation_type", &self.relation_type)
            .field("inference_score", &self.inference_score)
            .field("is_inferred", &self.is_inferred)
            .field("is_validated", &self.is_validated)
            .field("created_at", &self.created_at)
            .field(
                "created_by_len",
                &self.created_by.as_ref().map(|value| value.len()),
            )
            .finish()
    }
}

/// Request to create a semantic relation.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateSemanticRelationRequest {
    pub subject_id: Uuid,
    pub object_id: Uuid,
    pub relation_type: SkosSemanticRelation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inference_score: Option<f32>,
    #[serde(default)]
    pub is_inferred: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
}

impl fmt::Debug for CreateSemanticRelationRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CreateSemanticRelationRequest")
            .field("subject_id_set", &true)
            .field("object_id_set", &true)
            .field("relation_type", &self.relation_type)
            .field("inference_score", &self.inference_score)
            .field("is_inferred", &self.is_inferred)
            .field(
                "created_by_len",
                &self.created_by.as_ref().map(|value| value.len()),
            )
            .finish()
    }
}

/// A mapping relation to an external vocabulary.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SkosMappingRelationEdge {
    pub id: Uuid,
    pub concept_id: Uuid,
    pub target_uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_scheme_uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_label: Option<String>,
    pub relation_type: SkosMappingRelation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    pub is_validated: bool,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validated_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validated_by: Option<String>,
}

impl fmt::Debug for SkosMappingRelationEdge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SkosMappingRelationEdge")
            .field("id_set", &true)
            .field("concept_id_set", &true)
            .field("target_uri_len", &self.target_uri.len())
            .field(
                "target_scheme_uri_len",
                &self.target_scheme_uri.as_ref().map(|value| value.len()),
            )
            .field(
                "target_label_len",
                &self.target_label.as_ref().map(|value| value.len()),
            )
            .field("relation_type", &self.relation_type)
            .field("confidence", &self.confidence)
            .field("is_validated", &self.is_validated)
            .field("created_at", &self.created_at)
            .field("validated_at", &self.validated_at)
            .field(
                "validated_by_len",
                &self.validated_by.as_ref().map(|value| value.len()),
            )
            .finish()
    }
}

/// Request to create a mapping relation.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateMappingRelationRequest {
    pub concept_id: Uuid,
    pub target_uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_scheme_uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_label: Option<String>,
    pub relation_type: SkosMappingRelation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
}

impl fmt::Debug for CreateMappingRelationRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CreateMappingRelationRequest")
            .field("concept_id_set", &true)
            .field("target_uri_len", &self.target_uri.len())
            .field(
                "target_scheme_uri_len",
                &self.target_scheme_uri.as_ref().map(|value| value.len()),
            )
            .field(
                "target_label_len",
                &self.target_label.as_ref().map(|value| value.len()),
            )
            .field("relation_type", &self.relation_type)
            .field("confidence", &self.confidence)
            .finish()
    }
}

// =============================================================================
// NOTE-CONCEPT TAGGING
// =============================================================================

/// A note-to-concept tagging relationship with provenance.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct NoteSkosConceptTag {
    pub note_id: Uuid,
    pub concept_id: Uuid,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    pub relevance_score: f32,
    pub is_primary: bool,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
}

impl fmt::Debug for NoteSkosConceptTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NoteSkosConceptTag")
            .field("note_id_set", &true)
            .field("concept_id_set", &true)
            .field("source_len", &self.source.len())
            .field("confidence", &self.confidence)
            .field("relevance_score", &self.relevance_score)
            .field("is_primary", &self.is_primary)
            .field("created_at", &self.created_at)
            .field(
                "created_by_len",
                &self.created_by.as_ref().map(|value| value.len()),
            )
            .finish()
    }
}

/// Request to tag a note with a concept.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct TagNoteRequest {
    pub note_id: Uuid,
    pub concept_id: Uuid,
    #[serde(default = "default_source")]
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    #[serde(default = "default_relevance")]
    pub relevance_score: f32,
    #[serde(default)]
    pub is_primary: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
}

impl fmt::Debug for TagNoteRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TagNoteRequest")
            .field("note_id_set", &true)
            .field("concept_id_set", &true)
            .field("source_len", &self.source.len())
            .field("confidence", &self.confidence)
            .field("relevance_score", &self.relevance_score)
            .field("is_primary", &self.is_primary)
            .field(
                "created_by_len",
                &self.created_by.as_ref().map(|value| value.len()),
            )
            .finish()
    }
}

fn default_source() -> String {
    "manual".to_string()
}

fn default_relevance() -> f32 {
    1.0
}

/// Batch tag request for tagging a note with multiple concepts.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct BatchTagNoteRequest {
    pub note_id: Uuid,
    pub concept_ids: Vec<Uuid>,
    #[serde(default = "default_source")]
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
}

impl fmt::Debug for BatchTagNoteRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BatchTagNoteRequest")
            .field("note_id_set", &true)
            .field("concept_id_count", &self.concept_ids.len())
            .field("source_len", &self.source.len())
            .field("confidence", &self.confidence)
            .field(
                "created_by_len",
                &self.created_by.as_ref().map(|value| value.len()),
            )
            .finish()
    }
}

// =============================================================================
// GOVERNANCE AND AUDIT
// =============================================================================

/// Audit log entry for taxonomy changes.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SkosAuditLogEntry {
    pub id: Uuid,
    pub entity_type: String,
    pub entity_id: Uuid,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changes: Option<JsonValue>,
    pub actor: String,
    pub actor_type: String,
    pub created_at: DateTime<Utc>,
}

impl fmt::Debug for SkosAuditLogEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SkosAuditLogEntry")
            .field("id_set", &true)
            .field("entity_type_len", &self.entity_type.len())
            .field("entity_id_set", &true)
            .field("action_len", &self.action.len())
            .field(
                "changes_class",
                &self.changes.as_ref().map(json_value_class),
            )
            .field(
                "changes_len",
                &self
                    .changes
                    .as_ref()
                    .map(|value| serde_json::to_string(value).map_or(0, |json| json.len())),
            )
            .field("actor_len", &self.actor.len())
            .field("actor_type_len", &self.actor_type.len())
            .field("created_at", &self.created_at)
            .finish()
    }
}

/// Record of merged concepts.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SkosConceptMerge {
    pub id: Uuid,
    pub source_ids: Vec<Uuid>,
    pub target_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub performed_by: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl fmt::Debug for SkosConceptMerge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SkosConceptMerge")
            .field("id_set", &true)
            .field("source_id_count", &self.source_ids.len())
            .field("target_id_set", &true)
            .field("reason_len", &self.reason.as_ref().map(|value| value.len()))
            .field(
                "performed_by_len",
                &self.performed_by.as_ref().map(|value| value.len()),
            )
            .field("created_at", &self.created_at)
            .finish()
    }
}

/// Request to merge concepts.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct MergeConceptsRequest {
    /// Concepts to merge (will be deprecated).
    pub source_ids: Vec<Uuid>,
    /// Target concept (will receive all tags/relations).
    pub target_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub performed_by: Option<String>,
}

impl fmt::Debug for MergeConceptsRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MergeConceptsRequest")
            .field("source_id_count", &self.source_ids.len())
            .field("target_id_set", &true)
            .field("reason_len", &self.reason.as_ref().map(|value| value.len()))
            .field(
                "performed_by_len",
                &self.performed_by.as_ref().map(|value| value.len()),
            )
            .finish()
    }
}

fn json_value_class(value: &JsonValue) -> &'static str {
    match value {
        JsonValue::Null => "null",
        JsonValue::Bool(_) => "bool",
        JsonValue::Number(_) => "number",
        JsonValue::String(_) => "string",
        JsonValue::Array(_) => "array",
        JsonValue::Object(_) => "object",
    }
}

// =============================================================================
// GOVERNANCE DASHBOARD
// =============================================================================

/// Governance statistics for a concept scheme.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SkosGovernanceStats {
    pub scheme_id: Uuid,
    pub scheme_notation: String,
    pub scheme_title: String,
    pub total_concepts: i64,
    pub candidates: i64,
    pub approved: i64,
    pub deprecated: i64,
    pub orphans: i64,
    pub under_used: i64,
    pub missing_embeddings: i64,
    pub avg_note_count: f64,
    pub max_depth: i32,
}

impl fmt::Debug for SkosGovernanceStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SkosGovernanceStats")
            .field("scheme_id_set", &true)
            .field("scheme_notation_len", &self.scheme_notation.len())
            .field("scheme_title_len", &self.scheme_title.len())
            .field("total_concepts", &self.total_concepts)
            .field("candidates", &self.candidates)
            .field("approved", &self.approved)
            .field("deprecated", &self.deprecated)
            .field("orphans", &self.orphans)
            .field("under_used", &self.under_used)
            .field("missing_embeddings", &self.missing_embeddings)
            .field("avg_note_count", &self.avg_note_count)
            .field("max_depth", &self.max_depth)
            .finish()
    }
}

// =============================================================================
// SEARCH AND FILTERING
// =============================================================================

/// Request to search/filter concepts.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SearchConceptsRequest {
    /// Text query (searches labels).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,

    /// Filter by scheme.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheme_id: Option<Uuid>,

    /// Filter by status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<TagStatus>,

    /// Filter by facet type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub facet_type: Option<PmestFacet>,

    /// Filter by maximum depth.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_depth: Option<i32>,

    /// Filter to only top concepts (no broader).
    #[serde(default)]
    pub top_concepts_only: bool,

    /// Filter by antipattern presence.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_antipattern: Option<TagAntipattern>,

    /// Include deprecated concepts.
    #[serde(default)]
    pub include_deprecated: bool,

    /// Pagination limit.
    #[serde(default = "default_limit")]
    pub limit: i64,

    /// Pagination offset.
    #[serde(default)]
    pub offset: i64,
}

impl fmt::Debug for SearchConceptsRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SearchConceptsRequest")
            .field("query_len", &self.query.as_ref().map(|value| value.len()))
            .field("scheme_id_set", &self.scheme_id.is_some())
            .field("status", &self.status)
            .field("facet_type", &self.facet_type)
            .field("max_depth", &self.max_depth)
            .field("top_concepts_only", &self.top_concepts_only)
            .field("has_antipattern", &self.has_antipattern)
            .field("include_deprecated", &self.include_deprecated)
            .field("limit", &self.limit)
            .field("offset", &self.offset)
            .finish()
    }
}

fn default_limit() -> i64 {
    50
}

impl Default for SearchConceptsRequest {
    fn default() -> Self {
        Self {
            query: None,
            scheme_id: None,
            status: None,
            facet_type: None,
            max_depth: None,
            top_concepts_only: false,
            has_antipattern: None,
            include_deprecated: false,
            limit: 50,
            offset: 0,
        }
    }
}

/// Response for concept search.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SearchConceptsResponse {
    pub concepts: Vec<SkosConceptWithLabel>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

impl fmt::Debug for SearchConceptsResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SearchConceptsResponse")
            .field("concept_count", &self.concepts.len())
            .field("total", &self.total)
            .field("limit", &self.limit)
            .field("offset", &self.offset)
            .finish()
    }
}

// =============================================================================
// VALIDATION CONSTANTS
// =============================================================================

/// Maximum hierarchy depth (0-indexed, so 5 means levels 0-5).
pub const MAX_HIERARCHY_DEPTH: i32 = 5;

/// Maximum children per concept (breadth limit).
pub const MAX_CHILDREN_PER_NODE: i32 = 200;

/// Maximum parents per concept (polyhierarchy limit).
pub const MAX_PARENTS_PER_NODE: i32 = 3;

/// Literary warrant threshold for auto-promotion.
pub const LITERARY_WARRANT_THRESHOLD: i32 = 3;

// =============================================================================
// TAG INPUT PARSING
// =============================================================================

/// Default scheme notation for flat/simple tags.
pub const DEFAULT_SCHEME_NOTATION: &str = "default";

/// Maximum hierarchy depth for tag paths (0-indexed, so 5 means 5 levels).
pub const MAX_TAG_PATH_DEPTH: usize = 5;

/// Parsed tag input supporting both flat path and long-form SKOS formats.
///
/// # Tag Formats
///
/// ## Flat Path Format (Recommended for users/agents)
///
/// Hierarchical paths with `/` separator, max 5 levels deep:
///
/// - `"archive"` → single concept
/// - `"programming/rust"` → hierarchy: programming > rust
/// - `"ai/ml/transformers"` → hierarchy: ai > ml > transformers
/// - `"projects/matric/features/search"` → 4-level path
///
/// ## Long Form (SKOS YAML)
///
/// For advanced use, full SKOS specification can be provided via the `SkosTagSpec` struct.
///
/// # Examples
///
/// ```
/// use matric_core::TagInput;
///
/// // Simple flat tag
/// let simple = TagInput::parse("archive");
/// assert_eq!(simple.path(), vec!["archive"]);
/// assert!(!simple.is_hierarchical());
///
/// // Hierarchical path
/// let hier = TagInput::parse("programming/rust");
/// assert_eq!(hier.path(), vec!["programming", "rust"]);
/// assert!(hier.is_hierarchical());
/// assert_eq!(hier.leaf_label(), "rust");
/// assert_eq!(hier.parent_path(), Some(vec!["programming".to_string()]));
/// ```
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct TagInput {
    /// The hierarchical path components (e.g., ["programming", "rust"]).
    pub path: Vec<String>,
    /// Optional scheme override (defaults to "default").
    #[serde(default = "default_scheme")]
    pub scheme: String,
    /// Optional notation override (defaults to normalized path).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notation: Option<String>,
}

impl fmt::Debug for TagInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TagInput")
            .field("path_component_count", &self.path.len())
            .field(
                "path_component_lens",
                &self
                    .path
                    .iter()
                    .map(|component| component.len())
                    .collect::<Vec<_>>(),
            )
            .field("scheme_len", &self.scheme.len())
            .field(
                "notation_len",
                &self.notation.as_ref().map(|value| value.len()),
            )
            .finish()
    }
}

fn default_scheme() -> String {
    DEFAULT_SCHEME_NOTATION.to_string()
}

impl TagInput {
    /// Parse a tag string into structured input.
    ///
    /// Supports formats:
    /// - `"tag"` → single-level tag
    /// - `"topic/subtopic"` → hierarchical path
    /// - `"a/b/c/d/e"` → up to 5 levels
    ///
    /// Paths deeper than 5 levels will be truncated with a warning.
    pub fn parse(input: &str) -> Self {
        let trimmed = input.trim();

        // Split by forward slash for hierarchical paths
        let components: Vec<String> = trimmed
            .split('/')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();

        // Enforce max depth
        let path = if components.len() > MAX_TAG_PATH_DEPTH {
            components[..MAX_TAG_PATH_DEPTH].to_vec()
        } else if components.is_empty() {
            vec![trimmed.to_string()]
        } else {
            components
        };

        Self {
            path,
            scheme: DEFAULT_SCHEME_NOTATION.to_string(),
            notation: None,
        }
    }

    /// Parse multiple tags from a slice of strings.
    pub fn parse_many(inputs: &[String]) -> Vec<Self> {
        inputs.iter().map(|s| Self::parse(s)).collect()
    }

    /// Create a simple flat tag (single component).
    pub fn flat(label: impl Into<String>) -> Self {
        Self {
            path: vec![label.into()],
            scheme: DEFAULT_SCHEME_NOTATION.to_string(),
            notation: None,
        }
    }

    /// Create a hierarchical tag from path components.
    pub fn hierarchical(components: Vec<String>) -> Self {
        let path = if components.len() > MAX_TAG_PATH_DEPTH {
            components[..MAX_TAG_PATH_DEPTH].to_vec()
        } else {
            components
        };
        Self {
            path,
            scheme: DEFAULT_SCHEME_NOTATION.to_string(),
            notation: None,
        }
    }

    /// Create a tag in a specific scheme.
    pub fn in_scheme(mut self, scheme: impl Into<String>) -> Self {
        self.scheme = scheme.into();
        self
    }

    /// Get the path components.
    pub fn path(&self) -> Vec<&str> {
        self.path.iter().map(|s| s.as_str()).collect()
    }

    /// Check if this is a hierarchical (multi-level) tag.
    pub fn is_hierarchical(&self) -> bool {
        self.path.len() > 1
    }

    /// Get the depth (number of levels).
    pub fn depth(&self) -> usize {
        self.path.len()
    }

    /// Get the leaf (final) label.
    pub fn leaf_label(&self) -> &str {
        self.path.last().map(|s| s.as_str()).unwrap_or("")
    }

    /// Get the parent path (all components except the leaf).
    /// Returns None for single-component tags.
    pub fn parent_path(&self) -> Option<Vec<String>> {
        if self.path.len() > 1 {
            Some(self.path[..self.path.len() - 1].to_vec())
        } else {
            None
        }
    }

    /// Get all ancestor paths (from root to parent).
    ///
    /// For `"a/b/c/d"`, returns `[["a"], ["a", "b"], ["a", "b", "c"]]`.
    pub fn ancestor_paths(&self) -> Vec<Vec<String>> {
        let mut ancestors = Vec::new();
        for i in 1..self.path.len() {
            ancestors.push(self.path[..i].to_vec());
        }
        ancestors
    }

    /// Convert to canonical string representation (slash-separated path).
    pub fn to_canonical(&self) -> String {
        self.path.join("/")
    }

    /// Generate a notation from the path (kebab-case, lowercase).
    pub fn to_notation(&self) -> String {
        self.notation.clone().unwrap_or_else(|| {
            self.path
                .iter()
                .map(|component| {
                    component
                        .to_lowercase()
                        .chars()
                        .map(|c| if c.is_alphanumeric() { c } else { '-' })
                        .collect::<String>()
                        .split('-')
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>()
                        .join("-")
                })
                .collect::<Vec<_>>()
                .join("/")
        })
    }

    /// Generate the leaf notation only.
    pub fn leaf_notation(&self) -> String {
        self.leaf_label()
            .to_lowercase()
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .collect::<String>()
            .split('-')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("-")
    }
}

impl std::fmt::Display for TagInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_canonical())
    }
}

impl From<&str> for TagInput {
    fn from(s: &str) -> Self {
        Self::parse(s)
    }
}

impl From<String> for TagInput {
    fn from(s: String) -> Self {
        Self::parse(&s)
    }
}

// =============================================================================
// LONG FORM SKOS TAG SPECIFICATION
// =============================================================================

/// Full SKOS tag specification for advanced/long-form tag creation.
///
/// This allows specifying all SKOS properties when creating a concept,
/// including labels, relations, and documentation notes.
///
/// # Example YAML
///
/// ```yaml
/// pref_label: Machine Learning
/// alt_labels:
///   - ML
///   - machine-learning
/// definition: A subset of AI that enables systems to learn from data
/// scope_note: Use for supervised, unsupervised, and reinforcement learning
/// broader:
///   - artificial-intelligence
/// related:
///   - deep-learning
///   - neural-networks
/// facet_type: personality
/// facet_domain: computer-science
/// ```
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SkosTagSpec {
    /// Preferred label (required).
    pub pref_label: String,

    /// Alternative labels (synonyms, abbreviations).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alt_labels: Vec<String>,

    /// Hidden labels (for search, not display).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hidden_labels: Vec<String>,

    /// Definition text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub definition: Option<String>,

    /// Scope note (usage guidance).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope_note: Option<String>,

    /// Example usage.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example: Option<String>,

    /// Broader concept paths (parents).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub broader: Vec<String>,

    /// Narrower concept paths (children).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub narrower: Vec<String>,

    /// Related concept paths.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related: Vec<String>,

    /// Target scheme (defaults to "default").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheme: Option<String>,

    /// PMEST facet type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub facet_type: Option<PmestFacet>,

    /// Facet domain.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub facet_domain: Option<String>,

    /// Initial status (defaults to Candidate).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<TagStatus>,
}

impl fmt::Debug for SkosTagSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SkosTagSpec")
            .field("pref_label_len", &self.pref_label.len())
            .field("alt_label_count", &self.alt_labels.len())
            .field(
                "alt_label_lens",
                &self
                    .alt_labels
                    .iter()
                    .map(|label| label.len())
                    .collect::<Vec<_>>(),
            )
            .field("hidden_label_count", &self.hidden_labels.len())
            .field(
                "hidden_label_lens",
                &self
                    .hidden_labels
                    .iter()
                    .map(|label| label.len())
                    .collect::<Vec<_>>(),
            )
            .field(
                "definition_len",
                &self.definition.as_ref().map(|value| value.len()),
            )
            .field(
                "scope_note_len",
                &self.scope_note.as_ref().map(|value| value.len()),
            )
            .field(
                "example_len",
                &self.example.as_ref().map(|value| value.len()),
            )
            .field("broader_count", &self.broader.len())
            .field(
                "broader_lens",
                &self
                    .broader
                    .iter()
                    .map(|path| path.len())
                    .collect::<Vec<_>>(),
            )
            .field("narrower_count", &self.narrower.len())
            .field(
                "narrower_lens",
                &self
                    .narrower
                    .iter()
                    .map(|path| path.len())
                    .collect::<Vec<_>>(),
            )
            .field("related_count", &self.related.len())
            .field(
                "related_lens",
                &self
                    .related
                    .iter()
                    .map(|path| path.len())
                    .collect::<Vec<_>>(),
            )
            .field("scheme_len", &self.scheme.as_ref().map(|value| value.len()))
            .field("facet_type", &self.facet_type)
            .field(
                "facet_domain_len",
                &self.facet_domain.as_ref().map(|value| value.len()),
            )
            .field("status", &self.status)
            .finish()
    }
}

impl SkosTagSpec {
    /// Create a minimal spec with just a preferred label.
    pub fn new(pref_label: impl Into<String>) -> Self {
        Self {
            pref_label: pref_label.into(),
            alt_labels: vec![],
            hidden_labels: vec![],
            definition: None,
            scope_note: None,
            example: None,
            broader: vec![],
            narrower: vec![],
            related: vec![],
            scheme: None,
            facet_type: None,
            facet_domain: None,
            status: None,
        }
    }

    /// Add an alternative label.
    pub fn with_alt_label(mut self, label: impl Into<String>) -> Self {
        self.alt_labels.push(label.into());
        self
    }

    /// Set the definition.
    pub fn with_definition(mut self, definition: impl Into<String>) -> Self {
        self.definition = Some(definition.into());
        self
    }

    /// Add a broader concept path.
    pub fn with_broader(mut self, path: impl Into<String>) -> Self {
        self.broader.push(path.into());
        self
    }

    /// Add a related concept path.
    pub fn with_related(mut self, path: impl Into<String>) -> Self {
        self.related.push(path.into());
        self
    }

    /// Set the scheme.
    pub fn in_scheme(mut self, scheme: impl Into<String>) -> Self {
        self.scheme = Some(scheme.into());
        self
    }

    /// Convert to a TagInput (uses pref_label as path).
    pub fn to_tag_input(&self) -> TagInput {
        let mut input = TagInput::parse(&self.pref_label);
        if let Some(ref scheme) = self.scheme {
            input.scheme = scheme.clone();
        }
        input
    }
}

/// Resolved tag with its SKOS concept ID.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ResolvedTag {
    /// The parsed input.
    pub input: TagInput,
    /// The resolved or created concept ID.
    pub concept_id: Uuid,
    /// The scheme ID.
    pub scheme_id: Uuid,
    /// Whether this concept was newly created.
    pub created: bool,
}

impl fmt::Debug for ResolvedTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResolvedTag")
            .field("input", &self.input)
            .field("concept_id_set", &true)
            .field("scheme_id_set", &true)
            .field("created", &self.created)
            .finish()
    }
}

// =============================================================================
// SKOS COLLECTIONS (W3C SKOS Reference, Section 9)
// =============================================================================

/// A SKOS Collection — a labeled group of concepts.
///
/// Unlike ConceptSchemes which provide namespace/vocabulary organization,
/// Collections group concepts for presentation or organizational purposes.
/// An ordered collection preserves sequence (e.g., learning paths, workflows).
///
/// Reference: W3C SKOS Reference Section 9 — "SKOS collections are labeled
/// and/or ordered groups of SKOS concepts"
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SkosCollection {
    pub id: Uuid,
    pub uri: Option<String>,
    pub pref_label: String,
    pub definition: Option<String>,
    pub is_ordered: bool,
    pub scheme_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl fmt::Debug for SkosCollection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SkosCollection")
            .field("id_set", &true)
            .field("uri_len", &self.uri.as_ref().map(|value| value.len()))
            .field("pref_label_len", &self.pref_label.len())
            .field(
                "definition_len",
                &self.definition.as_ref().map(|value| value.len()),
            )
            .field("is_ordered", &self.is_ordered)
            .field("scheme_id_set", &self.scheme_id.is_some())
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .finish()
    }
}

/// A SKOS Collection with its member concepts.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SkosCollectionWithMembers {
    #[serde(flatten)]
    pub collection: SkosCollection,
    pub members: Vec<SkosCollectionMember>,
}

impl fmt::Debug for SkosCollectionWithMembers {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SkosCollectionWithMembers")
            .field("collection", &self.collection)
            .field("member_count", &self.members.len())
            .finish()
    }
}

/// A member entry in a SKOS Collection.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SkosCollectionMember {
    pub concept_id: Uuid,
    pub pref_label: Option<String>,
    pub position: Option<i32>,
    pub added_at: DateTime<Utc>,
}

impl fmt::Debug for SkosCollectionMember {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SkosCollectionMember")
            .field("concept_id_set", &true)
            .field(
                "pref_label_len",
                &self.pref_label.as_ref().map(|value| value.len()),
            )
            .field("position", &self.position)
            .field("added_at", &self.added_at)
            .finish()
    }
}

/// Request to create a SKOS Collection.
#[derive(Clone, Deserialize, utoipa::ToSchema)]
pub struct CreateSkosCollectionRequest {
    pub pref_label: String,
    pub definition: Option<String>,
    pub is_ordered: bool,
    pub scheme_id: Option<Uuid>,
    /// Initial concept IDs to add as members (order preserved for ordered collections)
    pub concept_ids: Option<Vec<Uuid>>,
}

impl fmt::Debug for CreateSkosCollectionRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CreateSkosCollectionRequest")
            .field("pref_label_len", &self.pref_label.len())
            .field(
                "definition_len",
                &self.definition.as_ref().map(|value| value.len()),
            )
            .field("is_ordered", &self.is_ordered)
            .field("scheme_id_set", &self.scheme_id.is_some())
            .field(
                "concept_id_count",
                &self.concept_ids.as_ref().map(|values| values.len()),
            )
            .finish()
    }
}

/// Request to update a SKOS Collection.
#[derive(Clone, Deserialize, utoipa::ToSchema)]
pub struct UpdateSkosCollectionRequest {
    pub pref_label: Option<String>,
    pub definition: Option<String>,
    pub is_ordered: Option<bool>,
}

impl fmt::Debug for UpdateSkosCollectionRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UpdateSkosCollectionRequest")
            .field(
                "pref_label_len",
                &self.pref_label.as_ref().map(|value| value.len()),
            )
            .field(
                "definition_len",
                &self.definition.as_ref().map(|value| value.len()),
            )
            .field("is_ordered", &self.is_ordered)
            .finish()
    }
}

/// Request to update member ordering in a SKOS Collection.
#[derive(Clone, Deserialize, utoipa::ToSchema)]
pub struct UpdateCollectionMembersRequest {
    /// Ordered list of concept IDs (replaces current member list)
    pub concept_ids: Vec<Uuid>,
}

impl fmt::Debug for UpdateCollectionMembersRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UpdateCollectionMembersRequest")
            .field("concept_id_count", &self.concept_ids.len())
            .finish()
    }
}

// =============================================================================
// TESTS
// =============================================================================

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

    #[test]
    fn test_semantic_relation_serialization() {
        let relations = vec![
            (SkosSemanticRelation::Broader, "broader"),
            (SkosSemanticRelation::Narrower, "narrower"),
            (SkosSemanticRelation::Related, "related"),
        ];

        for (relation, expected) in relations {
            let json = serde_json::to_string(&relation).unwrap();
            assert_eq!(json, format!("\"{}\"", expected));
            let parsed: SkosSemanticRelation = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, relation);
        }
    }

    #[test]
    fn test_mapping_relation_serialization() {
        let relations = vec![
            (SkosMappingRelation::ExactMatch, "exact_match"),
            (SkosMappingRelation::CloseMatch, "close_match"),
            (SkosMappingRelation::BroadMatch, "broad_match"),
            (SkosMappingRelation::NarrowMatch, "narrow_match"),
            (SkosMappingRelation::RelatedMatch, "related_match"),
        ];

        for (relation, expected) in relations {
            let json = serde_json::to_string(&relation).unwrap();
            assert_eq!(json, format!("\"{}\"", expected));
        }
    }

    #[test]
    fn test_label_type_default() {
        assert_eq!(SkosLabelType::default(), SkosLabelType::PrefLabel);
    }

    #[test]
    fn test_note_type_default() {
        assert_eq!(SkosNoteType::default(), SkosNoteType::Note);
    }

    #[test]
    fn test_tag_status_default() {
        assert_eq!(TagStatus::default(), TagStatus::Candidate);
    }

    #[test]
    fn test_pmest_facet_parsing() {
        assert_eq!(
            "personality".parse::<PmestFacet>().unwrap(),
            PmestFacet::Personality
        );
        assert_eq!("P".parse::<PmestFacet>().unwrap(), PmestFacet::Personality);
        assert_eq!("matter".parse::<PmestFacet>().unwrap(), PmestFacet::Matter);
        assert_eq!("energy".parse::<PmestFacet>().unwrap(), PmestFacet::Energy);
        assert_eq!("space".parse::<PmestFacet>().unwrap(), PmestFacet::Space);
        assert_eq!("time".parse::<PmestFacet>().unwrap(), PmestFacet::Time);
    }

    #[test]
    fn skos_concept_scheme_debug_redacts_metadata_and_identifiers() {
        let id = Uuid::parse_str("aaaaaaaa-1111-4222-8333-aaaaaaaaaaaa").unwrap();
        let now = Utc::now();

        let scheme = SkosConceptScheme {
            id,
            uri: Some("https://scheme.example.internal/path?token=secret".to_string()),
            notation: "secret-scheme-owner@example.internal".to_string(),
            title: "Scheme title with postgres://scheme:secret@db.internal".to_string(),
            description: Some("Scheme description /srv/fortemi/private/scheme".to_string()),
            creator: Some("creator-secret@example.internal".to_string()),
            publisher: Some("publisher sk-secret-scheme".to_string()),
            rights: Some("rights-secret-value".to_string()),
            version: "v1-secret".to_string(),
            is_active: true,
            is_system: false,
            created_at: now,
            updated_at: now,
            issued_at: Some(now),
            modified_at: Some(now),
        };
        let summary = SkosConceptSchemeSummary {
            id,
            notation: "summary-secret@example.internal".to_string(),
            title: "Summary title sk-secret-summary".to_string(),
            description: Some(
                "Summary description postgres://summary:secret@db.internal".to_string(),
            ),
            is_active: true,
            is_system: false,
            concept_count: 3,
            updated_at: now,
        };
        let create = CreateConceptSchemeRequest {
            notation: "create-secret@example.internal".to_string(),
            title: "Create title /srv/fortemi/private/create".to_string(),
            uri: Some("https://create.example.internal/scheme?token=secret".to_string()),
            description: Some("Create description sk-secret-create".to_string()),
            creator: Some("create-author@example.internal".to_string()),
            publisher: Some("create-publisher-secret".to_string()),
            rights: Some("create-rights-secret".to_string()),
            version: Some("create-version-secret".to_string()),
        };
        let update = UpdateConceptSchemeRequest {
            title: Some("Update title postgres://update:secret@db.internal".to_string()),
            description: Some("Update description /srv/fortemi/private/update".to_string()),
            creator: Some("update-author@example.internal".to_string()),
            publisher: Some("update-publisher-secret".to_string()),
            rights: Some("update-rights-secret".to_string()),
            version: Some("update-version-secret".to_string()),
            is_active: Some(false),
        };

        let scheme_debug = format!("{scheme:?}");
        assert!(scheme_debug.contains("SkosConceptScheme"));
        assert!(scheme_debug.contains("notation_len"));
        assert!(scheme_debug.contains("id_set"));
        assert_debug_excludes(
            &scheme_debug,
            &[
                "aaaaaaaa-1111-4222-8333-aaaaaaaaaaaa",
                "https://scheme.example.internal/path?token=secret",
                "secret-scheme-owner@example.internal",
                "postgres://scheme:secret@db.internal",
                "/srv/fortemi/private/scheme",
                "creator-secret@example.internal",
                "sk-secret-scheme",
                "rights-secret-value",
                "v1-secret",
            ],
        );

        let summary_debug = format!("{summary:?}");
        assert!(summary_debug.contains("SkosConceptSchemeSummary"));
        assert_debug_excludes(
            &summary_debug,
            &[
                "aaaaaaaa-1111-4222-8333-aaaaaaaaaaaa",
                "summary-secret@example.internal",
                "sk-secret-summary",
                "postgres://summary:secret@db.internal",
            ],
        );

        let create_debug = format!("{create:?}");
        assert!(create_debug.contains("CreateConceptSchemeRequest"));
        assert_debug_excludes(
            &create_debug,
            &[
                "create-secret@example.internal",
                "/srv/fortemi/private/create",
                "https://create.example.internal/scheme?token=secret",
                "sk-secret-create",
                "create-author@example.internal",
                "create-publisher-secret",
                "create-rights-secret",
                "create-version-secret",
            ],
        );

        let update_debug = format!("{update:?}");
        assert!(update_debug.contains("UpdateConceptSchemeRequest"));
        assert_debug_excludes(
            &update_debug,
            &[
                "postgres://update:secret@db.internal",
                "/srv/fortemi/private/update",
                "update-author@example.internal",
                "update-publisher-secret",
                "update-rights-secret",
                "update-version-secret",
            ],
        );
    }

    #[test]
    fn skos_label_and_note_debug_redacts_values_and_identifiers() {
        let id = Uuid::parse_str("aaaaaaaa-1111-4222-8333-aaaaaaaaaaaa").unwrap();
        let concept_id = Uuid::parse_str("bbbbbbbb-2222-4333-8444-bbbbbbbbbbbb").unwrap();
        let now = Utc::now();

        let label = SkosConceptLabel {
            id,
            concept_id,
            label_type: SkosLabelType::PrefLabel,
            value: "Label owner@example.internal postgres://tag:secret@db.internal".to_string(),
            language: "en-secret-label".to_string(),
            created_at: now,
        };
        let add_label = AddLabelRequest {
            concept_id,
            label_type: SkosLabelType::AltLabel,
            value: "Alt label /srv/fortemi/private/tag sk-secret-label".to_string(),
            language: "fr-secret-label".to_string(),
        };
        let note = SkosConceptNote {
            id,
            concept_id,
            note_type: SkosNoteType::Definition,
            value: "Definition contains bearer-secret and internal.example".to_string(),
            language: "en-secret-note".to_string(),
            author: Some("author-secret@example.internal".to_string()),
            source: Some("postgres://source:secret@db.internal/tags".to_string()),
            created_at: now,
            updated_at: now,
        };
        let add_note = AddNoteRequest {
            concept_id,
            note_type: SkosNoteType::ScopeNote,
            value: "Scope note /srv/fortemi/private/scope sk-secret-note".to_string(),
            language: "es-secret-note".to_string(),
            author: Some("request-author@example.internal".to_string()),
            source: Some("https://source.example.internal/path?token=secret".to_string()),
        };

        let label_debug = format!("{label:?}");
        assert!(label_debug.contains("SkosConceptLabel"));
        assert!(label_debug.contains("value_len"));
        assert!(label_debug.contains("concept_id_set"));
        assert_debug_excludes(
            &label_debug,
            &[
                "aaaaaaaa-1111-4222-8333-aaaaaaaaaaaa",
                "bbbbbbbb-2222-4333-8444-bbbbbbbbbbbb",
                "owner@example.internal",
                "postgres://tag:secret@db.internal",
                "en-secret-label",
            ],
        );

        let add_label_debug = format!("{add_label:?}");
        assert!(add_label_debug.contains("AddLabelRequest"));
        assert!(add_label_debug.contains("value_len"));
        assert_debug_excludes(
            &add_label_debug,
            &[
                "bbbbbbbb-2222-4333-8444-bbbbbbbbbbbb",
                "/srv/fortemi/private/tag",
                "sk-secret-label",
                "fr-secret-label",
            ],
        );

        let note_debug = format!("{note:?}");
        assert!(note_debug.contains("SkosConceptNote"));
        assert!(note_debug.contains("author_len"));
        assert!(note_debug.contains("source_len"));
        assert_debug_excludes(
            &note_debug,
            &[
                "aaaaaaaa-1111-4222-8333-aaaaaaaaaaaa",
                "bbbbbbbb-2222-4333-8444-bbbbbbbbbbbb",
                "bearer-secret",
                "internal.example",
                "en-secret-note",
                "author-secret@example.internal",
                "postgres://source:secret@db.internal/tags",
            ],
        );

        let add_note_debug = format!("{add_note:?}");
        assert!(add_note_debug.contains("AddNoteRequest"));
        assert!(add_note_debug.contains("author_len"));
        assert!(add_note_debug.contains("source_len"));
        assert_debug_excludes(
            &add_note_debug,
            &[
                "bbbbbbbb-2222-4333-8444-bbbbbbbbbbbb",
                "/srv/fortemi/private/scope",
                "sk-secret-note",
                "es-secret-note",
                "request-author@example.internal",
                "https://source.example.internal/path?token=secret",
            ],
        );
    }

    #[test]
    fn skos_concept_debug_redacts_metadata_labels_and_identifiers() {
        let concept_id = Uuid::parse_str("aaaaaaaa-1111-4222-8333-aaaaaaaaaaaa").unwrap();
        let scheme_id = Uuid::parse_str("bbbbbbbb-2222-4333-8444-bbbbbbbbbbbb").unwrap();
        let parent_id = Uuid::parse_str("cccccccc-3333-4444-8555-cccccccccccc").unwrap();
        let related_id = Uuid::parse_str("dddddddd-4444-4555-8666-dddddddddddd").unwrap();
        let mapping_id = Uuid::parse_str("eeeeeeee-5555-4666-8777-eeeeeeeeeeee").unwrap();
        let now = Utc::now();

        let concept = SkosConcept {
            id: concept_id,
            primary_scheme_id: scheme_id,
            uri: Some("https://concept.example.internal/path?token=secret".to_string()),
            notation: Some("concept-owner@example.internal".to_string()),
            facet_type: Some(PmestFacet::Personality),
            facet_source: Some("facet source postgres://facet:secret@db.internal".to_string()),
            facet_domain: Some("facet domain /srv/fortemi/private/domain".to_string()),
            facet_scope: Some("facet scope sk-secret-scope".to_string()),
            status: TagStatus::Candidate,
            promoted_at: Some(now),
            deprecated_at: Some(now),
            deprecation_reason: Some("deprecated because bearer-secret appeared".to_string()),
            replaced_by_id: Some(parent_id),
            note_count: 7,
            first_used_at: Some(now),
            last_used_at: Some(now),
            depth: 2,
            broader_count: 1,
            narrower_count: 2,
            related_count: 3,
            antipatterns: Vec::new(),
            antipattern_checked_at: Some(now),
            created_at: now,
            updated_at: now,
            embedding_model: Some("embedding-model-secret@example.internal".to_string()),
            embedded_at: Some(now),
        };
        let summary = SkosConceptSummary {
            id: concept_id,
            notation: Some("summary-notation-secret".to_string()),
            pref_label: Some("Summary label postgres://summary:secret@db.internal".to_string()),
            status: TagStatus::Approved,
            note_count: 4,
            depth: 1,
            scheme_notation: Some("scheme-notation-secret".to_string()),
        };
        let with_label = SkosConceptWithLabel {
            concept: concept.clone(),
            pref_label: Some("Preferred label /srv/fortemi/private/pref".to_string()),
            label_language: Some("en-secret-pref".to_string()),
            scheme_notation: Some("with-label-scheme-secret".to_string()),
            scheme_title: Some("Scheme title sk-secret-title".to_string()),
        };
        let full = SkosConceptFull {
            concept: concept.clone(),
            labels: vec![SkosConceptLabel {
                id: parent_id,
                concept_id,
                label_type: SkosLabelType::AltLabel,
                value: "Nested label owner@example.internal".to_string(),
                language: "en-secret-nested".to_string(),
                created_at: now,
            }],
            notes: vec![SkosConceptNote {
                id: related_id,
                concept_id,
                note_type: SkosNoteType::Definition,
                value: "Nested note postgres://note:secret@db.internal".to_string(),
                language: "en-secret-note".to_string(),
                author: Some("nested-author@example.internal".to_string()),
                source: Some("https://nested.example.internal/source?token=secret".to_string()),
                created_at: now,
                updated_at: now,
            }],
            broader: vec![summary.clone()],
            narrower: vec![summary.clone()],
            related: vec![summary.clone()],
            mappings: vec![SkosMappingRelationEdge {
                id: mapping_id,
                concept_id,
                target_uri: "https://external.example.internal/vocab?token=secret".to_string(),
                target_scheme_uri: Some("https://scheme.example.internal/secret".to_string()),
                target_label: Some("Target label secret@example.internal".to_string()),
                relation_type: SkosMappingRelation::ExactMatch,
                confidence: Some(0.9),
                is_validated: true,
                created_at: now,
                validated_at: Some(now),
                validated_by: Some("validator-secret@example.internal".to_string()),
            }],
            schemes: vec![SkosConceptSchemeSummary {
                id: scheme_id,
                notation: "nested-scheme-secret".to_string(),
                title: "Nested scheme title sk-secret".to_string(),
                description: Some("Nested scheme postgres://scheme:secret@db.internal".to_string()),
                is_active: true,
                is_system: false,
                concept_count: 1,
                updated_at: now,
            }],
        };
        let hierarchy = SkosConceptHierarchy {
            id: concept_id,
            notation: Some("hierarchy-notation-secret".to_string()),
            label: Some("Hierarchy label owner@example.internal".to_string()),
            level: 2,
            path: vec![parent_id, concept_id],
            label_path: vec![
                "Parent label /srv/fortemi/private/parent".to_string(),
                "Child label sk-secret-child".to_string(),
            ],
        };
        let create = CreateConceptRequest {
            scheme_id,
            notation: Some("create-notation-secret".to_string()),
            pref_label: "Create label secret@example.internal".to_string(),
            language: "en-secret-create".to_string(),
            status: TagStatus::Candidate,
            facet_type: Some(PmestFacet::Energy),
            facet_source: Some("create facet source sk-secret".to_string()),
            facet_domain: Some("create domain postgres://domain:secret@db.internal".to_string()),
            facet_scope: Some("create scope /srv/fortemi/private/scope".to_string()),
            definition: Some("definition includes bearer-secret".to_string()),
            scope_note: Some("scope note https://scope.example.internal?token=secret".to_string()),
            broader_ids: vec![parent_id],
            related_ids: vec![related_id],
            alt_labels: vec![
                "alternate label owner@example.internal".to_string(),
                "alternate label sk-secret-alt".to_string(),
            ],
        };
        let update = UpdateConceptRequest {
            notation: Some("update-notation-secret".to_string()),
            status: Some(TagStatus::Deprecated),
            deprecation_reason: Some(
                "update deprecation postgres://update:secret@db.internal".to_string(),
            ),
            replaced_by_id: Some(related_id),
            facet_type: Some(PmestFacet::Time),
            facet_source: Some("update facet source owner@example.internal".to_string()),
            facet_domain: Some("update domain /srv/fortemi/private/update".to_string()),
            facet_scope: Some("update scope sk-secret-update".to_string()),
        };

        let concept_debug = format!("{concept:?}");
        assert!(concept_debug.contains("SkosConcept"));
        assert!(concept_debug.contains("notation_len"));
        assert!(concept_debug.contains("replaced_by_id_set"));
        assert_debug_excludes(
            &concept_debug,
            &[
                "aaaaaaaa-1111-4222-8333-aaaaaaaaaaaa",
                "bbbbbbbb-2222-4333-8444-bbbbbbbbbbbb",
                "cccccccc-3333-4444-8555-cccccccccccc",
                "https://concept.example.internal/path?token=secret",
                "concept-owner@example.internal",
                "postgres://facet:secret@db.internal",
                "/srv/fortemi/private/domain",
                "sk-secret-scope",
                "bearer-secret",
                "embedding-model-secret@example.internal",
            ],
        );

        let summary_debug = format!("{summary:?}");
        assert!(summary_debug.contains("SkosConceptSummary"));
        assert_debug_excludes(
            &summary_debug,
            &[
                "aaaaaaaa-1111-4222-8333-aaaaaaaaaaaa",
                "summary-notation-secret",
                "postgres://summary:secret@db.internal",
                "scheme-notation-secret",
            ],
        );

        let with_label_debug = format!("{with_label:?}");
        assert!(with_label_debug.contains("SkosConceptWithLabel"));
        assert_debug_excludes(
            &with_label_debug,
            &[
                "/srv/fortemi/private/pref",
                "en-secret-pref",
                "with-label-scheme-secret",
                "sk-secret-title",
            ],
        );

        let full_debug = format!("{full:?}");
        assert!(full_debug.contains("SkosConceptFull"));
        assert!(full_debug.contains("label_count"));
        assert_debug_excludes(
            &full_debug,
            &[
                "Nested label owner@example.internal",
                "postgres://note:secret@db.internal",
                "https://external.example.internal/vocab?token=secret",
                "Target label secret@example.internal",
                "nested-scheme-secret",
                "postgres://scheme:secret@db.internal",
            ],
        );

        let hierarchy_debug = format!("{hierarchy:?}");
        assert!(hierarchy_debug.contains("SkosConceptHierarchy"));
        assert_debug_excludes(
            &hierarchy_debug,
            &[
                "aaaaaaaa-1111-4222-8333-aaaaaaaaaaaa",
                "cccccccc-3333-4444-8555-cccccccccccc",
                "hierarchy-notation-secret",
                "owner@example.internal",
                "/srv/fortemi/private/parent",
                "sk-secret-child",
            ],
        );

        let create_debug = format!("{create:?}");
        assert!(create_debug.contains("CreateConceptRequest"));
        assert_debug_excludes(
            &create_debug,
            &[
                "bbbbbbbb-2222-4333-8444-bbbbbbbbbbbb",
                "create-notation-secret",
                "secret@example.internal",
                "en-secret-create",
                "sk-secret",
                "postgres://domain:secret@db.internal",
                "/srv/fortemi/private/scope",
                "bearer-secret",
                "https://scope.example.internal?token=secret",
                "cccccccc-3333-4444-8555-cccccccccccc",
                "dddddddd-4444-4555-8666-dddddddddddd",
            ],
        );

        let update_debug = format!("{update:?}");
        assert!(update_debug.contains("UpdateConceptRequest"));
        assert_debug_excludes(
            &update_debug,
            &[
                "update-notation-secret",
                "postgres://update:secret@db.internal",
                "dddddddd-4444-4555-8666-dddddddddddd",
                "owner@example.internal",
                "/srv/fortemi/private/update",
                "sk-secret-update",
            ],
        );
    }

    #[test]
    fn skos_relation_debug_redacts_identifiers_urls_and_actors() {
        let relation_id = Uuid::parse_str("aaaaaaaa-1111-4222-8333-aaaaaaaaaaaa").unwrap();
        let concept_id = Uuid::parse_str("bbbbbbbb-2222-4333-8444-bbbbbbbbbbbb").unwrap();
        let target_id = Uuid::parse_str("cccccccc-3333-4444-8555-cccccccccccc").unwrap();
        let mapping_id = Uuid::parse_str("dddddddd-4444-4555-8666-dddddddddddd").unwrap();
        let now = Utc::now();

        let semantic_edge = SkosSemanticRelationEdge {
            id: relation_id,
            subject_id: concept_id,
            object_id: target_id,
            relation_type: SkosSemanticRelation::Related,
            inference_score: Some(0.87),
            is_inferred: true,
            is_validated: false,
            created_at: now,
            created_by: Some("semantic-author-secret@example.internal".to_string()),
        };
        let semantic_request = CreateSemanticRelationRequest {
            subject_id: concept_id,
            object_id: target_id,
            relation_type: SkosSemanticRelation::Broader,
            inference_score: Some(0.91),
            is_inferred: true,
            created_by: Some("request-author sk-secret-semantic".to_string()),
        };
        let mapping_edge = SkosMappingRelationEdge {
            id: mapping_id,
            concept_id,
            target_uri: "https://external.example.internal/vocab?token=secret".to_string(),
            target_scheme_uri: Some(
                "https://scheme.example.internal/private?access_token=secret".to_string(),
            ),
            target_label: Some("Mapped label owner@example.internal sk-secret-map".to_string()),
            relation_type: SkosMappingRelation::ExactMatch,
            confidence: Some(0.95),
            is_validated: true,
            created_at: now,
            validated_at: Some(now),
            validated_by: Some("validator-secret@example.internal".to_string()),
        };
        let mapping_request = CreateMappingRelationRequest {
            concept_id,
            target_uri: "postgres://mapping:secret@db.internal/vocab".to_string(),
            target_scheme_uri: Some("file:///srv/fortemi/private/scheme".to_string()),
            target_label: Some("Request target label bearer-secret".to_string()),
            relation_type: SkosMappingRelation::CloseMatch,
            confidence: Some(0.72),
        };

        let semantic_edge_debug = format!("{semantic_edge:?}");
        assert!(semantic_edge_debug.contains("SkosSemanticRelationEdge"));
        assert!(semantic_edge_debug.contains("subject_id_set"));
        assert_debug_excludes(
            &semantic_edge_debug,
            &[
                "aaaaaaaa-1111-4222-8333-aaaaaaaaaaaa",
                "bbbbbbbb-2222-4333-8444-bbbbbbbbbbbb",
                "cccccccc-3333-4444-8555-cccccccccccc",
                "semantic-author-secret@example.internal",
            ],
        );

        let semantic_request_debug = format!("{semantic_request:?}");
        assert!(semantic_request_debug.contains("CreateSemanticRelationRequest"));
        assert_debug_excludes(
            &semantic_request_debug,
            &[
                "bbbbbbbb-2222-4333-8444-bbbbbbbbbbbb",
                "cccccccc-3333-4444-8555-cccccccccccc",
                "request-author sk-secret-semantic",
            ],
        );

        let mapping_edge_debug = format!("{mapping_edge:?}");
        assert!(mapping_edge_debug.contains("SkosMappingRelationEdge"));
        assert!(mapping_edge_debug.contains("target_uri_len"));
        assert_debug_excludes(
            &mapping_edge_debug,
            &[
                "dddddddd-4444-4555-8666-dddddddddddd",
                "bbbbbbbb-2222-4333-8444-bbbbbbbbbbbb",
                "https://external.example.internal/vocab?token=secret",
                "https://scheme.example.internal/private?access_token=secret",
                "owner@example.internal",
                "sk-secret-map",
                "validator-secret@example.internal",
            ],
        );

        let mapping_request_debug = format!("{mapping_request:?}");
        assert!(mapping_request_debug.contains("CreateMappingRelationRequest"));
        assert_debug_excludes(
            &mapping_request_debug,
            &[
                "bbbbbbbb-2222-4333-8444-bbbbbbbbbbbb",
                "postgres://mapping:secret@db.internal/vocab",
                "file:///srv/fortemi/private/scheme",
                "bearer-secret",
            ],
        );
    }

    #[test]
    fn skos_tagging_audit_and_merge_debug_redacts_ids_sources_and_changes() {
        let note_id = Uuid::parse_str("aaaaaaaa-1111-4222-8333-aaaaaaaaaaaa").unwrap();
        let concept_id = Uuid::parse_str("bbbbbbbb-2222-4333-8444-bbbbbbbbbbbb").unwrap();
        let source_id = Uuid::parse_str("cccccccc-3333-4444-8555-cccccccccccc").unwrap();
        let target_id = Uuid::parse_str("dddddddd-4444-4555-8666-dddddddddddd").unwrap();
        let audit_id = Uuid::parse_str("eeeeeeee-5555-4666-8777-eeeeeeeeeeee").unwrap();
        let now = Utc::now();

        let tag = NoteSkosConceptTag {
            note_id,
            concept_id,
            source: "import source postgres://tag:secret@db.internal".to_string(),
            confidence: Some(0.89),
            relevance_score: 0.94,
            is_primary: true,
            created_at: now,
            created_by: Some("tagger-secret@example.internal".to_string()),
        };
        let tag_request = TagNoteRequest {
            note_id,
            concept_id,
            source: "request source /srv/fortemi/private/tag".to_string(),
            confidence: Some(0.77),
            relevance_score: 0.81,
            is_primary: false,
            created_by: Some("request-tagger sk-secret-tag".to_string()),
        };
        let batch_request = BatchTagNoteRequest {
            note_id,
            concept_ids: vec![concept_id, source_id, target_id],
            source: "batch source https://batch.example.internal?token=secret".to_string(),
            confidence: Some(0.66),
            created_by: Some("batch-tagger-secret@example.internal".to_string()),
        };
        let audit = SkosAuditLogEntry {
            id: audit_id,
            entity_type: "concept-secret@example.internal".to_string(),
            entity_id: concept_id,
            action: "merge-action sk-secret-audit".to_string(),
            changes: Some(serde_json::json!({
                "before": "postgres://audit:secret@db.internal",
                "after": "/srv/fortemi/private/audit",
                "token": "bearer-secret-audit"
            })),
            actor: "audit-actor-secret@example.internal".to_string(),
            actor_type: "operator-secret".to_string(),
            created_at: now,
        };
        let merge = SkosConceptMerge {
            id: audit_id,
            source_ids: vec![source_id, concept_id],
            target_id,
            reason: Some("merge reason postgres://merge:secret@db.internal".to_string()),
            performed_by: Some("merge-operator-secret@example.internal".to_string()),
            created_at: now,
        };
        let merge_request = MergeConceptsRequest {
            source_ids: vec![source_id, concept_id],
            target_id,
            reason: Some("request reason /srv/fortemi/private/merge".to_string()),
            performed_by: Some("request-merge sk-secret-merge".to_string()),
        };

        let tag_debug = format!("{tag:?}");
        assert!(tag_debug.contains("NoteSkosConceptTag"));
        assert!(tag_debug.contains("source_len"));
        assert_debug_excludes(
            &tag_debug,
            &[
                "aaaaaaaa-1111-4222-8333-aaaaaaaaaaaa",
                "bbbbbbbb-2222-4333-8444-bbbbbbbbbbbb",
                "postgres://tag:secret@db.internal",
                "tagger-secret@example.internal",
            ],
        );

        let tag_request_debug = format!("{tag_request:?}");
        assert!(tag_request_debug.contains("TagNoteRequest"));
        assert_debug_excludes(
            &tag_request_debug,
            &[
                "aaaaaaaa-1111-4222-8333-aaaaaaaaaaaa",
                "bbbbbbbb-2222-4333-8444-bbbbbbbbbbbb",
                "/srv/fortemi/private/tag",
                "sk-secret-tag",
            ],
        );

        let batch_debug = format!("{batch_request:?}");
        assert!(batch_debug.contains("BatchTagNoteRequest"));
        assert!(batch_debug.contains("concept_id_count"));
        assert_debug_excludes(
            &batch_debug,
            &[
                "aaaaaaaa-1111-4222-8333-aaaaaaaaaaaa",
                "bbbbbbbb-2222-4333-8444-bbbbbbbbbbbb",
                "cccccccc-3333-4444-8555-cccccccccccc",
                "dddddddd-4444-4555-8666-dddddddddddd",
                "https://batch.example.internal?token=secret",
                "batch-tagger-secret@example.internal",
            ],
        );

        let audit_debug = format!("{audit:?}");
        assert!(audit_debug.contains("SkosAuditLogEntry"));
        assert!(audit_debug.contains("changes_class"));
        assert!(audit_debug.contains("changes_len"));
        assert_debug_excludes(
            &audit_debug,
            &[
                "eeeeeeee-5555-4666-8777-eeeeeeeeeeee",
                "bbbbbbbb-2222-4333-8444-bbbbbbbbbbbb",
                "concept-secret@example.internal",
                "sk-secret-audit",
                "postgres://audit:secret@db.internal",
                "/srv/fortemi/private/audit",
                "bearer-secret-audit",
                "audit-actor-secret@example.internal",
                "operator-secret",
            ],
        );

        let merge_debug = format!("{merge:?}");
        assert!(merge_debug.contains("SkosConceptMerge"));
        assert_debug_excludes(
            &merge_debug,
            &[
                "eeeeeeee-5555-4666-8777-eeeeeeeeeeee",
                "cccccccc-3333-4444-8555-cccccccccccc",
                "bbbbbbbb-2222-4333-8444-bbbbbbbbbbbb",
                "dddddddd-4444-4555-8666-dddddddddddd",
                "postgres://merge:secret@db.internal",
                "merge-operator-secret@example.internal",
            ],
        );

        let merge_request_debug = format!("{merge_request:?}");
        assert!(merge_request_debug.contains("MergeConceptsRequest"));
        assert_debug_excludes(
            &merge_request_debug,
            &[
                "cccccccc-3333-4444-8555-cccccccccccc",
                "bbbbbbbb-2222-4333-8444-bbbbbbbbbbbb",
                "dddddddd-4444-4555-8666-dddddddddddd",
                "/srv/fortemi/private/merge",
                "sk-secret-merge",
            ],
        );
    }

    #[test]
    fn skos_governance_and_search_debug_redacts_queries_schemes_and_results() {
        let scheme_id = Uuid::parse_str("aaaaaaaa-1111-4222-8333-aaaaaaaaaaaa").unwrap();
        let concept_id = Uuid::parse_str("bbbbbbbb-2222-4333-8444-bbbbbbbbbbbb").unwrap();
        let now = Utc::now();

        let stats = SkosGovernanceStats {
            scheme_id,
            scheme_notation: "governance-secret@example.internal".to_string(),
            scheme_title: "Governance title postgres://gov:secret@db.internal".to_string(),
            total_concepts: 11,
            candidates: 2,
            approved: 7,
            deprecated: 1,
            orphans: 1,
            under_used: 3,
            missing_embeddings: 4,
            avg_note_count: 2.5,
            max_depth: 3,
        };
        let request = SearchConceptsRequest {
            query: Some(
                "search owner@example.internal /srv/fortemi/private sk-secret-query".to_string(),
            ),
            scheme_id: Some(scheme_id),
            status: Some(TagStatus::Candidate),
            facet_type: Some(PmestFacet::Space),
            max_depth: Some(4),
            top_concepts_only: true,
            has_antipattern: Some(TagAntipattern::UnderUsed),
            include_deprecated: true,
            limit: 25,
            offset: 5,
        };
        let response = SearchConceptsResponse {
            concepts: vec![SkosConceptWithLabel {
                concept: SkosConcept {
                    id: concept_id,
                    primary_scheme_id: scheme_id,
                    uri: Some("https://concept.example.internal?token=secret".to_string()),
                    notation: Some("result-notation-secret".to_string()),
                    facet_type: Some(PmestFacet::Matter),
                    facet_source: Some(
                        "result source postgres://result:secret@db.internal".to_string(),
                    ),
                    facet_domain: Some("result domain /srv/fortemi/private/result".to_string()),
                    facet_scope: Some("result scope sk-secret-result".to_string()),
                    status: TagStatus::Approved,
                    promoted_at: Some(now),
                    deprecated_at: None,
                    deprecation_reason: None,
                    replaced_by_id: None,
                    note_count: 5,
                    first_used_at: Some(now),
                    last_used_at: Some(now),
                    depth: 1,
                    broader_count: 0,
                    narrower_count: 2,
                    related_count: 1,
                    antipatterns: Vec::new(),
                    antipattern_checked_at: Some(now),
                    created_at: now,
                    updated_at: now,
                    embedding_model: Some("embedding-secret@example.internal".to_string()),
                    embedded_at: Some(now),
                },
                pref_label: Some("Result label owner@example.internal".to_string()),
                label_language: Some("en-secret-result".to_string()),
                scheme_notation: Some("result-scheme-secret".to_string()),
                scheme_title: Some("Result scheme title sk-secret-scheme".to_string()),
            }],
            total: 1,
            limit: 25,
            offset: 5,
        };

        let stats_debug = format!("{stats:?}");
        assert!(stats_debug.contains("SkosGovernanceStats"));
        assert!(stats_debug.contains("scheme_notation_len"));
        assert_debug_excludes(
            &stats_debug,
            &[
                "aaaaaaaa-1111-4222-8333-aaaaaaaaaaaa",
                "governance-secret@example.internal",
                "postgres://gov:secret@db.internal",
            ],
        );

        let request_debug = format!("{request:?}");
        assert!(request_debug.contains("SearchConceptsRequest"));
        assert!(request_debug.contains("query_len"));
        assert_debug_excludes(
            &request_debug,
            &[
                "owner@example.internal",
                "/srv/fortemi/private",
                "sk-secret-query",
                "aaaaaaaa-1111-4222-8333-aaaaaaaaaaaa",
            ],
        );

        let response_debug = format!("{response:?}");
        assert!(response_debug.contains("SearchConceptsResponse"));
        assert!(response_debug.contains("concept_count"));
        assert_debug_excludes(
            &response_debug,
            &[
                "bbbbbbbb-2222-4333-8444-bbbbbbbbbbbb",
                "aaaaaaaa-1111-4222-8333-aaaaaaaaaaaa",
                "https://concept.example.internal?token=secret",
                "result-notation-secret",
                "postgres://result:secret@db.internal",
                "/srv/fortemi/private/result",
                "sk-secret-result",
                "embedding-secret@example.internal",
                "Result label owner@example.internal",
                "en-secret-result",
                "result-scheme-secret",
                "sk-secret-scheme",
            ],
        );
    }

    #[test]
    fn skos_collection_debug_redacts_labels_definitions_and_member_identifiers() {
        let collection_id = Uuid::parse_str("aaaaaaaa-1111-4222-8333-aaaaaaaaaaaa").unwrap();
        let scheme_id = Uuid::parse_str("bbbbbbbb-2222-4333-8444-bbbbbbbbbbbb").unwrap();
        let concept_id = Uuid::parse_str("cccccccc-3333-4444-8555-cccccccccccc").unwrap();
        let second_concept_id = Uuid::parse_str("dddddddd-4444-4555-8666-dddddddddddd").unwrap();
        let now = Utc::now();

        let collection = SkosCollection {
            id: collection_id,
            uri: Some("https://taxonomy.example.internal/collections?token=secret".to_string()),
            pref_label: "Collection owner@example.internal sk-secret-label".to_string(),
            definition: Some("Definition postgres://collection:secret@db.internal".to_string()),
            is_ordered: true,
            scheme_id: Some(scheme_id),
            created_at: now,
            updated_at: now,
        };
        let member = SkosCollectionMember {
            concept_id,
            pref_label: Some("Member label /srv/fortemi/private sk-secret-member".to_string()),
            position: Some(7),
            added_at: now,
        };
        let with_members = SkosCollectionWithMembers {
            collection: collection.clone(),
            members: vec![member.clone()],
        };
        let create_request = CreateSkosCollectionRequest {
            pref_label: "Create label owner@example.internal".to_string(),
            definition: Some("Create definition /srv/fortemi/private".to_string()),
            is_ordered: true,
            scheme_id: Some(scheme_id),
            concept_ids: Some(vec![concept_id, second_concept_id]),
        };
        let update_request = UpdateSkosCollectionRequest {
            pref_label: Some("Update label sk-secret-update".to_string()),
            definition: Some("Update definition postgres://update:secret@db.internal".to_string()),
            is_ordered: Some(false),
        };
        let members_request = UpdateCollectionMembersRequest {
            concept_ids: vec![concept_id, second_concept_id],
        };

        let collection_debug = format!("{collection:?}");
        assert!(collection_debug.contains("SkosCollection"));
        assert!(collection_debug.contains("pref_label_len"));
        assert!(collection_debug.contains("scheme_id_set"));
        assert_debug_excludes(
            &collection_debug,
            &[
                "aaaaaaaa-1111-4222-8333-aaaaaaaaaaaa",
                "bbbbbbbb-2222-4333-8444-bbbbbbbbbbbb",
                "https://taxonomy.example.internal/collections?token=secret",
                "owner@example.internal",
                "sk-secret-label",
                "postgres://collection:secret@db.internal",
            ],
        );

        let member_debug = format!("{member:?}");
        assert!(member_debug.contains("SkosCollectionMember"));
        assert!(member_debug.contains("concept_id_set"));
        assert_debug_excludes(
            &member_debug,
            &[
                "cccccccc-3333-4444-8555-cccccccccccc",
                "/srv/fortemi/private",
                "sk-secret-member",
            ],
        );

        let with_members_debug = format!("{with_members:?}");
        assert!(with_members_debug.contains("SkosCollectionWithMembers"));
        assert!(with_members_debug.contains("member_count"));
        assert_debug_excludes(
            &with_members_debug,
            &[
                "aaaaaaaa-1111-4222-8333-aaaaaaaaaaaa",
                "bbbbbbbb-2222-4333-8444-bbbbbbbbbbbb",
                "cccccccc-3333-4444-8555-cccccccccccc",
                "owner@example.internal",
                "sk-secret-member",
            ],
        );

        let create_debug = format!("{create_request:?}");
        assert!(create_debug.contains("CreateSkosCollectionRequest"));
        assert!(create_debug.contains("concept_id_count"));
        assert_debug_excludes(
            &create_debug,
            &[
                "bbbbbbbb-2222-4333-8444-bbbbbbbbbbbb",
                "cccccccc-3333-4444-8555-cccccccccccc",
                "dddddddd-4444-4555-8666-dddddddddddd",
                "owner@example.internal",
                "/srv/fortemi/private",
            ],
        );

        let update_debug = format!("{update_request:?}");
        assert!(update_debug.contains("UpdateSkosCollectionRequest"));
        assert!(update_debug.contains("definition_len"));
        assert_debug_excludes(
            &update_debug,
            &["sk-secret-update", "postgres://update:secret@db.internal"],
        );

        let members_request_debug = format!("{members_request:?}");
        assert!(members_request_debug.contains("UpdateCollectionMembersRequest"));
        assert!(members_request_debug.contains("concept_id_count"));
        assert_debug_excludes(
            &members_request_debug,
            &[
                "cccccccc-3333-4444-8555-cccccccccccc",
                "dddddddd-4444-4555-8666-dddddddddddd",
            ],
        );
    }

    #[test]
    fn tag_input_skos_spec_and_resolved_tag_debug_redact_user_values() {
        let concept_id = Uuid::parse_str("aaaaaaaa-1111-4222-8333-aaaaaaaaaaaa").unwrap();
        let scheme_id = Uuid::parse_str("bbbbbbbb-2222-4333-8444-bbbbbbbbbbbb").unwrap();

        let tag_input = TagInput {
            path: vec![
                "owner@example.internal".to_string(),
                "postgres://tag:secret@db.internal".to_string(),
                "sk-secret-tag".to_string(),
            ],
            scheme: "scheme /srv/fortemi/private".to_string(),
            notation: Some("notation-secret@example.internal".to_string()),
        };
        let spec = SkosTagSpec {
            pref_label: "Preferred label owner@example.internal".to_string(),
            alt_labels: vec!["alt postgres://alt:secret@db.internal".to_string()],
            hidden_labels: vec!["hidden /srv/fortemi/private".to_string()],
            definition: Some("definition sk-secret-definition".to_string()),
            scope_note: Some("scope note owner@example.internal".to_string()),
            example: Some("example token sk-secret-example".to_string()),
            broader: vec!["broader/path/secret".to_string()],
            narrower: vec!["narrower/path/secret".to_string()],
            related: vec!["related/path/secret".to_string()],
            scheme: Some("scheme postgres://scheme:secret@db.internal".to_string()),
            facet_type: Some(PmestFacet::Personality),
            facet_domain: Some("facet-domain owner@example.internal".to_string()),
            status: Some(TagStatus::Candidate),
        };
        let resolved = ResolvedTag {
            input: tag_input.clone(),
            concept_id,
            scheme_id,
            created: true,
        };

        let input_debug = format!("{tag_input:?}");
        assert!(input_debug.contains("TagInput"));
        assert!(input_debug.contains("path_component_count"));
        assert!(input_debug.contains("scheme_len"));
        assert_debug_excludes(
            &input_debug,
            &[
                "owner@example.internal",
                "postgres://tag:secret@db.internal",
                "sk-secret-tag",
                "/srv/fortemi/private",
                "notation-secret@example.internal",
            ],
        );

        let spec_debug = format!("{spec:?}");
        assert!(spec_debug.contains("SkosTagSpec"));
        assert!(spec_debug.contains("pref_label_len"));
        assert!(spec_debug.contains("broader_count"));
        assert_debug_excludes(
            &spec_debug,
            &[
                "Preferred label owner@example.internal",
                "postgres://alt:secret@db.internal",
                "/srv/fortemi/private",
                "sk-secret-definition",
                "scope note owner@example.internal",
                "sk-secret-example",
                "broader/path/secret",
                "narrower/path/secret",
                "related/path/secret",
                "postgres://scheme:secret@db.internal",
                "facet-domain owner@example.internal",
            ],
        );

        let resolved_debug = format!("{resolved:?}");
        assert!(resolved_debug.contains("ResolvedTag"));
        assert!(resolved_debug.contains("concept_id_set"));
        assert!(resolved_debug.contains("scheme_id_set"));
        assert_debug_excludes(
            &resolved_debug,
            &[
                "aaaaaaaa-1111-4222-8333-aaaaaaaaaaaa",
                "bbbbbbbb-2222-4333-8444-bbbbbbbbbbbb",
                "owner@example.internal",
                "postgres://tag:secret@db.internal",
                "sk-secret-tag",
            ],
        );
    }

    #[test]
    fn test_antipattern_parsing() {
        assert_eq!(
            "orphan".parse::<TagAntipattern>().unwrap(),
            TagAntipattern::Orphan
        );
        assert_eq!(
            "too_broad".parse::<TagAntipattern>().unwrap(),
            TagAntipattern::TooBroad
        );
        assert_eq!(
            "circular_hierarchy".parse::<TagAntipattern>().unwrap(),
            TagAntipattern::CircularHierarchy
        );
    }

    #[test]
    fn test_create_concept_request() {
        let request = CreateConceptRequest {
            scheme_id: Uuid::new_v4(),
            notation: Some("test".to_string()),
            pref_label: "Test Concept".to_string(),
            language: "en".to_string(),
            status: TagStatus::Candidate,
            facet_type: Some(PmestFacet::Personality),
            facet_source: None,
            facet_domain: Some("testing".to_string()),
            facet_scope: None,
            definition: Some("A test concept for unit testing".to_string()),
            scope_note: None,
            broader_ids: vec![],
            related_ids: vec![],
            alt_labels: vec!["test tag".to_string(), "testing".to_string()],
        };

        let json = serde_json::to_string(&request).unwrap();
        let parsed: CreateConceptRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.pref_label, "Test Concept");
        assert_eq!(parsed.facet_type, Some(PmestFacet::Personality));
        assert_eq!(parsed.alt_labels.len(), 2);
    }

    #[test]
    fn test_validation_constants() {
        assert_eq!(MAX_HIERARCHY_DEPTH, 5);
        assert_eq!(MAX_CHILDREN_PER_NODE, 200);
        assert_eq!(MAX_PARENTS_PER_NODE, 3);
        assert_eq!(LITERARY_WARRANT_THRESHOLD, 3);
    }

    #[test]
    fn test_search_request_defaults() {
        let request = SearchConceptsRequest::default();
        assert_eq!(request.limit, 50);
        assert_eq!(request.offset, 0);
        assert!(!request.top_concepts_only);
        assert!(!request.include_deprecated);
    }

    // =========================================================================
    // TagInput Tests
    // =========================================================================

    #[test]
    fn test_tag_input_simple() {
        let tag = TagInput::parse("archive");
        assert_eq!(tag.path, vec!["archive"]);
        assert_eq!(tag.scheme, "default");
        assert!(!tag.is_hierarchical());
        assert_eq!(tag.depth(), 1);
        assert_eq!(tag.leaf_label(), "archive");
        assert_eq!(tag.to_canonical(), "archive");
    }

    #[test]
    fn test_tag_input_hierarchical_two_levels() {
        let tag = TagInput::parse("programming/rust");
        assert_eq!(tag.path, vec!["programming", "rust"]);
        assert!(tag.is_hierarchical());
        assert_eq!(tag.depth(), 2);
        assert_eq!(tag.leaf_label(), "rust");
        assert_eq!(tag.parent_path(), Some(vec!["programming".to_string()]));
        assert_eq!(tag.to_canonical(), "programming/rust");
    }

    #[test]
    fn test_tag_input_hierarchical_three_levels() {
        let tag = TagInput::parse("ai/ml/transformers");
        assert_eq!(tag.path, vec!["ai", "ml", "transformers"]);
        assert_eq!(tag.depth(), 3);
        assert_eq!(tag.leaf_label(), "transformers");
        assert_eq!(
            tag.parent_path(),
            Some(vec!["ai".to_string(), "ml".to_string()])
        );
    }

    #[test]
    fn test_tag_input_max_depth() {
        // 5 levels should work
        let tag = TagInput::parse("a/b/c/d/e");
        assert_eq!(tag.path.len(), 5);

        // 6+ levels should be truncated to 5
        let tag = TagInput::parse("a/b/c/d/e/f/g");
        assert_eq!(tag.path.len(), 5);
        assert_eq!(tag.path, vec!["a", "b", "c", "d", "e"]);
    }

    #[test]
    fn test_tag_input_ancestor_paths() {
        let tag = TagInput::parse("a/b/c/d");
        let ancestors = tag.ancestor_paths();
        assert_eq!(ancestors.len(), 3);
        assert_eq!(ancestors[0], vec!["a"]);
        assert_eq!(ancestors[1], vec!["a", "b"]);
        assert_eq!(ancestors[2], vec!["a", "b", "c"]);
    }

    #[test]
    fn test_tag_input_trimming() {
        let tag = TagInput::parse("  archive  ");
        assert_eq!(tag.leaf_label(), "archive");

        let tag = TagInput::parse(" programming / rust ");
        assert_eq!(tag.path, vec!["programming", "rust"]);
    }

    #[test]
    fn test_tag_input_notation_generation() {
        let tag = TagInput::parse("Machine Learning/Deep Learning");
        assert_eq!(tag.to_notation(), "machine-learning/deep-learning");
        assert_eq!(tag.leaf_notation(), "deep-learning");
    }

    #[test]
    fn test_tag_input_parse_many() {
        let inputs = vec![
            "archive".to_string(),
            "programming/rust".to_string(),
            "ai/ml/transformers".to_string(),
        ];
        let tags = TagInput::parse_many(&inputs);
        assert_eq!(tags.len(), 3);
        assert_eq!(tags[0].depth(), 1);
        assert_eq!(tags[1].depth(), 2);
        assert_eq!(tags[2].depth(), 3);
    }

    #[test]
    fn test_tag_input_from_str() {
        let tag: TagInput = "programming/rust".into();
        assert_eq!(tag.path, vec!["programming", "rust"]);
    }

    #[test]
    fn test_tag_input_constructors() {
        let flat = TagInput::flat("test");
        assert_eq!(flat.path, vec!["test"]);
        assert!(!flat.is_hierarchical());

        let hier = TagInput::hierarchical(vec![
            "topics".to_string(),
            "ai".to_string(),
            "ml".to_string(),
        ]);
        assert_eq!(hier.path, vec!["topics", "ai", "ml"]);
        assert!(hier.is_hierarchical());
    }

    #[test]
    fn test_tag_input_in_scheme() {
        let tag = TagInput::parse("programming/rust").in_scheme("technical");
        assert_eq!(tag.scheme, "technical");
        assert_eq!(tag.path, vec!["programming", "rust"]);
    }

    #[test]
    fn test_skos_tag_spec() {
        let spec = SkosTagSpec::new("Machine Learning")
            .with_alt_label("ML")
            .with_definition("A subset of AI")
            .with_broader("artificial-intelligence")
            .with_related("deep-learning")
            .in_scheme("topics");

        assert_eq!(spec.pref_label, "Machine Learning");
        assert_eq!(spec.alt_labels, vec!["ML"]);
        assert_eq!(spec.definition, Some("A subset of AI".to_string()));
        assert_eq!(spec.broader, vec!["artificial-intelligence"]);
        assert_eq!(spec.related, vec!["deep-learning"]);
        assert_eq!(spec.scheme, Some("topics".to_string()));

        // Convert to TagInput
        let input = spec.to_tag_input();
        assert_eq!(input.path, vec!["Machine Learning"]);
        assert_eq!(input.scheme, "topics");
    }

    #[test]
    fn tag_enum_parse_errors_report_lengths_without_raw_values() {
        let secret = "custömér/private@example.test?token=sk-live-secret";
        let cases = [
            secret.parse::<SkosSemanticRelation>().unwrap_err(),
            secret.parse::<SkosMappingRelation>().unwrap_err(),
            secret.parse::<SkosLabelType>().unwrap_err(),
            secret.parse::<SkosNoteType>().unwrap_err(),
            secret.parse::<PmestFacet>().unwrap_err(),
            secret.parse::<TagStatus>().unwrap_err(),
            secret.parse::<TagAntipattern>().unwrap_err(),
        ];

        for error in cases {
            assert!(
                error.contains(&format!("value_len={}", secret.chars().count())),
                "{error}"
            );
            assert!(
                !error.contains(&format!("value_len={}", secret.len())),
                "tag enum parser error used byte length instead of character count: {error}"
            );
            assert!(
                !error.contains(secret),
                "tag enum parser error leaked raw invalid value: {error}"
            );
            assert!(
                !error.contains("private@example.test") && !error.contains("sk-live-secret"),
                "tag enum parser error leaked secret-shaped fragment: {error}"
            );
        }
    }
}
