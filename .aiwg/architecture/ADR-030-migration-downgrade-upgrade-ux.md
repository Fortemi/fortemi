# ADR-030: Migration Downgrade Handling and Upgrade Assistance

**Status:** Accepted
**Date:** 2026-02-01
**Deciders:** Architecture team
**Related:** ADR-028 (Shard Migration System), ADR-029 (Shard Schema Versioning)

## Context

When users import knowledge shards created by newer versions of matric-memory into older versions, data loss may occur. Fields, features, and formats introduced after the user's version cannot be preserved. Currently, users receive vague "incompatible" errors that provide no actionable guidance.

The primary users of this system are:
- **MCP agents** (Claude, other AI assistants) that need structured data for programmatic handling
- **Non-technical users** who should receive clear, jargon-free explanations
- **Operators** managing multiple matric-memory instances at different versions

Key problems with current behavior:
1. Users discover data loss *after* import completes
2. Error messages use technical jargon ("schema version mismatch")
3. No guidance on how to upgrade or what upgrading would preserve
4. MCP agents receive unstructured error strings they cannot act upon
5. Users cannot make informed decisions about proceeding with partial imports

## Decision

Implement a **downgrade impact analysis** and **upgrade assistance** system that:

1. **Analyzes data loss before import** and presents a detailed inventory
2. **Provides structured upgrade guidance** with actionable steps
3. **Uses plain language** that non-technical users can understand
4. **Returns machine-readable responses** for MCP agent consumption

### 1. DowngradeImpact Analysis

When importing a shard from a newer version, compute exactly what will be lost.

```rust
// crates/matric-core/src/shard/downgrade.rs

/// Complete inventory of what will be lost in a downgrade import.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DowngradeImpact {
    /// Shard version being imported
    pub shard_version: String,
    /// Current matric-memory version
    pub current_version: String,
    /// Features that exist in shard but not in current version
    pub features_lost: Vec<FeatureLoss>,
    /// Specific data items that will be discarded or degraded
    pub data_loss: Vec<DataLoss>,
    /// Whether import can proceed (false if critical data would corrupt)
    pub can_proceed: bool,
    /// Reason if cannot proceed
    pub block_reason: Option<String>,
    /// Human-readable summary for non-technical users
    pub summary: String,
}

/// A feature present in the shard but not supported by current version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureLoss {
    /// Feature identifier for programmatic use
    pub feature_id: String,
    /// Human-readable feature name
    pub name: String,
    /// Plain language explanation of what this feature does
    pub description: String,
    /// Version that introduced this feature
    pub introduced_in: String,
    /// Severity: "info", "warning", "critical"
    pub severity: String,
}

/// Specific data that will be lost or degraded.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataLoss {
    /// Component type: "notes", "embeddings", "links", etc.
    pub component: String,
    /// Specific field or aspect being lost
    pub field: String,
    /// Number of items affected
    pub affected_count: usize,
    /// Plain language description of the loss
    pub description: String,
    /// What happens to this data: "discarded", "degraded", "default_applied"
    pub outcome: String,
    /// Example of affected data (if safe to show)
    pub example: Option<String>,
}
```

### 2. UpgradeGuidance Response

When upgrade is required or recommended, provide actionable guidance.

```rust
// crates/matric-core/src/shard/upgrade.rs

/// Structured guidance for upgrading matric-memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradeGuidance {
    /// Current matric-memory version
    pub current_version: String,
    /// Minimum version required to fully import the shard
    pub required_version: String,
    /// Recommended version (may be higher than required)
    pub recommended_version: Option<String>,
    /// URL to upgrade documentation
    pub doc_url: String,
    /// Numbered steps to upgrade
    pub upgrade_steps: Vec<UpgradeStep>,
    /// URL to release notes for required version
    pub release_notes_url: Option<String>,
    /// Changelog highlights relevant to this upgrade
    pub relevant_changes: Vec<String>,
    /// Estimated upgrade difficulty: "simple", "moderate", "complex"
    pub difficulty: String,
    /// Whether upgrade requires database migration
    pub requires_db_migration: bool,
    /// Estimated downtime (if any)
    pub estimated_downtime: Option<String>,
}

/// A single step in the upgrade process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradeStep {
    /// Step number (1-indexed)
    pub step: usize,
    /// Plain language instruction
    pub instruction: String,
    /// Command to run (if applicable)
    pub command: Option<String>,
    /// Documentation URL for this step
    pub doc_url: Option<String>,
}
```

### 3. Import Decision Response

Combined response for import compatibility checks.

```rust
/// Response from shard compatibility check, suitable for API and MCP.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportCompatibility {
    /// Whether import can proceed without issues
    pub status: CompatibilityStatus,
    /// Downgrade impact analysis (if downgrading)
    pub downgrade_impact: Option<DowngradeImpact>,
    /// Upgrade guidance (if upgrade needed or recommended)
    pub upgrade_guidance: Option<UpgradeGuidance>,
    /// User-facing message (plain language)
    pub message: String,
    /// Suggested action: "proceed", "proceed_with_caution", "upgrade_first", "abort"
    pub suggested_action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompatibilityStatus {
    /// Fully compatible, no data loss
    Compatible,
    /// Compatible with minor data loss (user can choose)
    DowngradeRecoverable,
    /// Compatible but significant data loss (user should decide)
    DowngradeLossy,
    /// Cannot import without upgrade (critical features missing)
    UpgradeRequired,
    /// Corrupted or invalid shard
    Invalid,
}
```

### 4. User-Facing Messages

All messages follow these principles:
- Plain language, no technical jargon
- Specific counts and examples, never vague warnings
- Actionable next steps
- Never just "incompatible" without explanation

#### Scenario A: Downgrade with Recoverable Data Loss

```
This knowledge shard was created with a newer version of Matric Memory.

Your version: 2026.1.0
Shard version: 2026.3.0

Some newer features will not be imported:

  EMBEDDINGS
  - 3 notes have MRL (compact) embeddings that will be discarded
  - These notes will still import, but semantic search won't find them
    until you regenerate embeddings

  METADATA
  - 12 notes have "confidence score" fields that will be ignored
  - This is informational only; your notes will work fine

Everything else imports normally: 147 notes, 5 collections, 89 links.

What would you like to do?
  [1] Import anyway (recommended - you can upgrade later)
  [2] Cancel and upgrade first
  [3] Show me exactly what will be lost

To upgrade: https://docs.matric-memory.dev/upgrading
```

#### Scenario B: Downgrade with Critical Data Loss

```
This knowledge shard uses features that cannot be safely imported.

Your version: 2026.1.0
Shard version: 2027.1.0

Critical incompatibilities found:

  ENCRYPTION
  - 45 notes use end-to-end encryption (introduced in 2027.1.0)
  - These notes CANNOT be imported - their content would be corrupted

  STORAGE FORMAT
  - Embeddings use a new binary format your version cannot read
  - 892 embeddings would be lost with no way to recover them

Recoverable items (67 notes, 3 collections) could still be imported,
but we recommend upgrading first to preserve all your data.

Recommendation: Upgrade to Matric Memory 2027.1.0 or later.

  [1] Cancel (recommended)
  [2] Import only compatible items (45 notes will be skipped)
  [3] Show upgrade instructions

To upgrade: https://docs.matric-memory.dev/upgrading
```

#### Scenario C: Upgrade Required

```
This knowledge shard requires a newer version of Matric Memory.

Your version: 2026.1.0
Required version: 2026.3.0 or later

The shard uses features not available in your version:
  - Multi-tenant workspaces (requires 2026.2.0+)
  - Graph visualization metadata (requires 2026.3.0+)

How to upgrade:

  1. Back up your current data
     Command: curl -X POST http://localhost:3000/backup/create

  2. Download the new version
     https://github.com/matric/matric-memory/releases/tag/v2026.3.0

  3. Stop the current service
     Command: sudo systemctl stop matric-api

  4. Run database migrations
     Command: cargo run --bin migrate

  5. Start the new version
     Command: sudo systemctl start matric-api

  6. Retry the import

Full upgrade guide: https://docs.matric-memory.dev/upgrading
Release notes: https://docs.matric-memory.dev/releases/2026.3.0
```

### 5. MCP Agent Response Format

For MCP tools, return structured JSON that agents can process programmatically.

```json
{
  "tool": "import_shard",
  "status": "downgrade_recoverable",
  "can_proceed": true,
  "requires_confirmation": true,
  "compatibility": {
    "current_version": "2026.1.0",
    "shard_version": "2026.3.0",
    "features_lost": [
      {
        "feature_id": "mrl_embeddings",
        "name": "MRL Compact Embeddings",
        "severity": "warning",
        "affected_count": 3,
        "recovery_action": "Regenerate embeddings after import"
      }
    ],
    "data_loss": [
      {
        "component": "embeddings",
        "field": "mrl_dimension",
        "affected_count": 3,
        "outcome": "discarded",
        "description": "3 notes have compact embeddings that will be discarded"
      },
      {
        "component": "notes",
        "field": "confidence_score",
        "affected_count": 12,
        "outcome": "ignored",
        "description": "Metadata field not supported, safely ignored"
      }
    ],
    "preserved": {
      "notes": 147,
      "collections": 5,
      "links": 89,
      "tags": 23
    }
  },
  "upgrade_guidance": {
    "required_version": "2026.3.0",
    "doc_url": "https://docs.matric-memory.dev/upgrading",
    "difficulty": "simple",
    "requires_db_migration": true,
    "steps": [
      {"step": 1, "instruction": "Back up current data", "command": "curl -X POST .../backup/create"},
      {"step": 2, "instruction": "Download new version", "doc_url": "https://.../releases/v2026.3.0"},
      {"step": 3, "instruction": "Stop service", "command": "sudo systemctl stop matric-api"},
      {"step": 4, "instruction": "Run migrations", "command": "cargo run --bin migrate"},
      {"step": 5, "instruction": "Start service", "command": "sudo systemctl start matric-api"}
    ]
  },
  "suggested_action": "proceed_with_caution",
  "user_message": "This shard was created with a newer version. 3 notes have MRL embeddings that will be discarded. Import anyway?",
  "actions": [
    {"id": "proceed", "label": "Import anyway", "recommended": true},
    {"id": "cancel", "label": "Cancel"},
    {"id": "upgrade", "label": "Show upgrade steps"}
  ]
}
```

### 6. API Error Response Enhancement

HTTP API errors include structured compatibility data.

```http
HTTP/1.1 409 Conflict
Content-Type: application/json

{
  "error": "shard_version_mismatch",
  "message": "This shard requires Matric Memory 2026.3.0 or later",
  "details": {
    "current_version": "2026.1.0",
    "required_version": "2026.3.0",
    "shard_version": "2.0.0"
  },
  "upgrade_guidance": {
    "doc_url": "https://docs.matric-memory.dev/upgrading",
    "release_notes_url": "https://docs.matric-memory.dev/releases/2026.3.0",
    "steps": ["Back up data", "Download new version", "Run migrations"]
  },
  "help": "See https://docs.matric-memory.dev/troubleshooting/version-mismatch"
}
```

### 7. Built-in Documentation Integration

Matric-memory includes embedded documentation that can be served and referenced.

```rust
/// Built-in documentation URLs based on deployment.
pub struct DocUrls {
    base: String,
}

impl DocUrls {
    pub fn new(issuer_url: &str) -> Self {
        Self {
            base: format!("{}/docs", issuer_url),
        }
    }

    pub fn upgrading(&self) -> String {
        format!("{}/upgrading", self.base)
    }

    pub fn release_notes(&self, version: &str) -> String {
        format!("{}/releases/{}", self.base, version)
    }

    pub fn troubleshooting(&self, topic: &str) -> String {
        format!("{}/troubleshooting/{}", self.base, topic)
    }

    pub fn backup_guide(&self) -> String {
        format!("{}/backup", self.base)
    }
}
```

For self-hosted instances, URLs resolve to the instance's own documentation. For offline use, the CLI can display embedded documentation directly.

### 8. Implementation Flow

```
Import Request
      |
      v
+------------------+
| Parse Manifest   |
| Extract version  |
+------------------+
      |
      v
+------------------+
| Compare Versions |
| shard vs current |
+------------------+
      |
      +----------------+----------------+
      |                |                |
      v                v                v
  Same/Older       Minor Newer      Major Newer
  (compatible)     (downgrade)      (upgrade needed)
      |                |                |
      v                v                v
  Proceed         Analyze Impact    Check if
  directly        (DowngradeImpact) critical
      |                |                |
      |                v                v
      |           Return to user   UpgradeGuidance
      |           with choices     + block or warn
      |                |                |
      +--------+-------+--------+-------+
               |
               v
        User Decision
        (proceed/abort/upgrade)
               |
               v
        Execute Import
        (with warnings)
```

## Consequences

### Positive

- (+) **Informed decisions**: Users know exactly what they'll lose before committing
- (+) **Actionable guidance**: Clear steps to upgrade, not just "please upgrade"
- (+) **MCP agent friendly**: Structured responses enable automated decision-making
- (+) **Non-technical accessible**: Plain language, no jargon
- (+) **Preserves trust**: Never silently loses data
- (+) **Self-service**: Users can resolve issues without support

### Negative

- (-) **More prompts**: Users must acknowledge data loss before proceeding
- (-) **Documentation maintenance**: Must keep upgrade docs current
- (-) **Version matrix complexity**: Must track feature introduction versions
- (-) **Larger responses**: Structured responses are more verbose

## Implementation

### Code Location

- Impact analysis: `crates/matric-core/src/shard/downgrade.rs`
- Upgrade guidance: `crates/matric-core/src/shard/upgrade.rs`
- Feature registry: `crates/matric-core/src/shard/features.rs`
- User messages: `crates/matric-api/src/backup/messages.rs`
- MCP responses: `mcp-server/src/tools/import.ts`

### Feature Introduction Registry

Track when features were introduced for accurate impact analysis.

```rust
/// Registry of features and their introduction versions.
pub static FEATURE_REGISTRY: &[FeatureInfo] = &[
    FeatureInfo {
        id: "mrl_embeddings",
        name: "MRL Compact Embeddings",
        introduced_in: "2026.2.0",
        description: "Matryoshka embeddings for 12x storage savings",
        severity_if_missing: Severity::Warning,
    },
    FeatureInfo {
        id: "graph_metadata",
        name: "Graph Visualization Metadata",
        introduced_in: "2026.3.0",
        description: "Node positions and styles for knowledge graph",
        severity_if_missing: Severity::Info,
    },
    FeatureInfo {
        id: "e2e_encryption",
        name: "End-to-End Encryption",
        introduced_in: "2027.1.0",
        description: "Client-side encryption for sensitive notes",
        severity_if_missing: Severity::Critical,
    },
];

pub fn features_after_version(version: &str) -> Vec<&'static FeatureInfo> {
    FEATURE_REGISTRY
        .iter()
        .filter(|f| Version::parse(f.introduced_in).unwrap() > Version::parse(version).unwrap())
        .collect()
}
```

### Testing Requirements

1. Unit tests for each compatibility scenario
2. Integration tests for full import flow with version mismatches
3. Message content tests (ensure no jargon, clear actions)
4. MCP response schema validation tests
5. Documentation URL resolution tests

## Alternatives Considered

### 1. Silent Best-Effort Import

**Rejected because:** Users discover data loss only after import, damaging trust. MCP agents cannot make informed decisions.

### 2. Strict Version Matching (Block All Mismatches)

**Rejected because:** Too restrictive. Minor data loss is often acceptable, and users should decide.

### 3. Automatic Upgrade Trigger

**Rejected because:** Upgrading has side effects (downtime, migrations). Users must explicitly choose to upgrade.

### 4. Technical Error Codes Only

**Rejected because:** Non-technical users cannot act on "ERR_SCHEMA_VERSION_2_REQUIRED". Plain language is essential.

## References

- ADR-028: Shard and Archive Migration System
- ADR-029: Shard Schema Versioning Specification
- docs/content/releasing.md (Version format and release process)
- docs/content/backup.md (Shard format documentation)
- [Nielsen Norman Group: Error Message Guidelines](https://www.nngroup.com/articles/error-message-guidelines/)
