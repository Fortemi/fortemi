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
use uuid::Uuid;

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
            _ => Err(format!("Invalid SKOS semantic relation: {}", s)),
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
            _ => Err(format!("Invalid SKOS mapping relation: {}", s)),
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
            _ => Err(format!("Invalid SKOS label type: {}", s)),
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
            _ => Err(format!("Invalid SKOS note type: {}", s)),
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
            _ => Err(format!("Invalid PMEST facet: {}", s)),
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
            _ => Err(format!("Invalid tag status: {}", s)),
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
            _ => Err(format!("Invalid tag antipattern: {}", s)),
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
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
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

fn default_version() -> String {
    "1.0.0".to_string()
}

/// Summary view of a concept scheme for listings.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
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

/// Request to create a new concept scheme.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
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

/// Request to update a concept scheme.
#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
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

// =============================================================================
// SKOS CONCEPT
// =============================================================================

/// SKOS Concept - the core tag/concept entity.
///
/// Represents a single concept in the knowledge organization system,
/// with full support for SKOS properties and PMEST facets.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
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

/// Concept with its preferred label for display.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
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

/// Full concept with all labels, notes, and relations.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
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

/// Summary view of a concept for listings and relations.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
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

/// Concept in hierarchy view with path information.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SkosConceptHierarchy {
    pub id: Uuid,
    pub notation: Option<String>,
    pub label: Option<String>,
    pub level: i32,
    pub path: Vec<Uuid>,
    pub label_path: Vec<String>,
}

/// Request to create a new concept.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
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

/// Request to update a concept.
#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
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

// =============================================================================
// SKOS LABELS
// =============================================================================

/// A lexical label for a concept.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SkosConceptLabel {
    pub id: Uuid,
    pub concept_id: Uuid,
    pub label_type: SkosLabelType,
    pub value: String,
    pub language: String,
    pub created_at: DateTime<Utc>,
}

/// Request to add a label to a concept.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AddLabelRequest {
    pub concept_id: Uuid,
    #[serde(default)]
    pub label_type: SkosLabelType,
    pub value: String,
    #[serde(default = "default_language")]
    pub language: String,
}

// =============================================================================
// SKOS NOTES (Documentation)
// =============================================================================

/// A documentation note for a concept.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
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

/// Request to add a note to a concept.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
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

// =============================================================================
// SKOS RELATIONS
// =============================================================================

/// A semantic relation between two concepts.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
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

/// Request to create a semantic relation.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
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

/// A mapping relation to an external vocabulary.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
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

/// Request to create a mapping relation.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
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

// =============================================================================
// NOTE-CONCEPT TAGGING
// =============================================================================

/// A note-to-concept tagging relationship with provenance.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
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

/// Request to tag a note with a concept.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
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

fn default_source() -> String {
    "manual".to_string()
}

fn default_relevance() -> f32 {
    1.0
}

/// Batch tag request for tagging a note with multiple concepts.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
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

// =============================================================================
// GOVERNANCE AND AUDIT
// =============================================================================

/// Audit log entry for taxonomy changes.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
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

/// Record of merged concepts.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
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

/// Request to merge concepts.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
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

// =============================================================================
// GOVERNANCE DASHBOARD
// =============================================================================

/// Governance statistics for a concept scheme.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
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

// =============================================================================
// SEARCH AND FILTERING
// =============================================================================

/// Request to search/filter concepts.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
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
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SearchConceptsResponse {
    pub concepts: Vec<SkosConceptWithLabel>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

// =============================================================================
// VALIDATION CONSTANTS
// =============================================================================

/// Maximum hierarchy depth (0-indexed, so 5 means levels 0-5).
pub const MAX_HIERARCHY_DEPTH: i32 = 5;

/// Maximum children per concept (breadth limit).
pub const MAX_CHILDREN_PER_NODE: i32 = 10;

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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
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
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
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
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
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
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
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

/// A SKOS Collection with its member concepts.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SkosCollectionWithMembers {
    #[serde(flatten)]
    pub collection: SkosCollection,
    pub members: Vec<SkosCollectionMember>,
}

/// A member entry in a SKOS Collection.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SkosCollectionMember {
    pub concept_id: Uuid,
    pub pref_label: Option<String>,
    pub position: Option<i32>,
    pub added_at: DateTime<Utc>,
}

/// Request to create a SKOS Collection.
#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
pub struct CreateSkosCollectionRequest {
    pub pref_label: String,
    pub definition: Option<String>,
    pub is_ordered: bool,
    pub scheme_id: Option<Uuid>,
    /// Initial concept IDs to add as members (order preserved for ordered collections)
    pub concept_ids: Option<Vec<Uuid>>,
}

/// Request to update a SKOS Collection.
#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
pub struct UpdateSkosCollectionRequest {
    pub pref_label: Option<String>,
    pub definition: Option<String>,
    pub is_ordered: Option<bool>,
}

/// Request to update member ordering in a SKOS Collection.
#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
pub struct UpdateCollectionMembersRequest {
    /// Ordered list of concept IDs (replaces current member list)
    pub concept_ids: Vec<Uuid>,
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(MAX_CHILDREN_PER_NODE, 10);
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
}
