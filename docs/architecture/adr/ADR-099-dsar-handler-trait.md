# ADR-099: Data Subject Request Handler Trait (GDPR / CCPA)

**Status:** Proposed (Experimental)
**Date:** 2026-05-20
**Deciders:** roctinam, privacy/legal review TBD
**Related:** ADR-088, ADR-090 (tenancy), ADR-091 (audit)

Operational logs, retained event payloads, DLQ entries, and protected
diagnostics use the hosted telemetry classes in
`docs/architecture/hosted-telemetry-classification.md`. DSAR implementations
must map those classes into retention, beyond-use, export, and legal-hold
behavior instead of treating broad logs as a single undifferentiated store.

## Context

GDPR (EU 2016/679 Articles 15-22), CCPA (Cal. Civ. Code §1798.100 et seq.), and emerging laws (UK GDPR, LGPD, Quebec Law 25) grant data subjects rights to:
- **Access** — receive a copy of their personal data (DSAR-Access)
- **Portability** — receive structured, machine-readable data they can re-import elsewhere
- **Rectification** — correct inaccurate data
- **Erasure** ("right to be forgotten") — delete their data
- **Restriction** — pause processing
- **Objection** — refuse certain processing (e.g., direct marketing)

Fortemi today has no formal handler for these requests. For EE multi-tenant SaaS targeting EU customers, this is a launch-blocking compliance gap.

This is an **Experimental** trait — concrete implementation requirements depend on EE customer use cases that have not been collected. Land the surface; refine before promoting to Beta/Stable.

## Decision

**Introduce a `DataSubjectRequestHandler` trait (Experimental tier) in `matric-core`. Ship `NotImplementedHandler` as the CE default. EE implementations build the actual DSAR workflows.**

### Trait

```rust
// crates/matric-core/src/privacy.rs

#[async_trait]
pub trait DataSubjectRequestHandler: Send + Sync {
    /// Initiate a request. Returns a request handle for tracking.
    async fn submit(
        &self,
        tenant: &TenantId,
        request: DsarRequest,
    ) -> Result<DsarHandle, DsarError>;

    /// Check status of an in-flight request.
    async fn status(&self, handle: &DsarHandle)
        -> Result<DsarStatus, DsarError>;

    /// Retrieve the export bundle for an Access or Portability request.
    /// Returns the bundle as a tus-resumable upload URL or pre-signed object URL.
    async fn fetch_export(&self, handle: &DsarHandle)
        -> Result<DsarExport, DsarError>;

    /// Confirm and execute a pending Erasure request (after the verification grace period).
    async fn confirm_erasure(&self, handle: &DsarHandle, confirmation_token: &str)
        -> Result<DsarErasureReceipt, DsarError>;
}

pub enum DsarKind {
    Access,         // Article 15 / CCPA §1798.110
    Portability,    // Article 20
    Rectification,  // Article 16
    Erasure,        // Article 17 / CCPA §1798.105
    Restriction,    // Article 18
    Objection,      // Article 21
}

pub struct DsarRequest {
    pub kind: DsarKind,
    pub subject_identifier: SubjectIdentifier,  // email, user_id, etc.
    pub requestor_identity: Verified,            // Verified subject identity (out of band)
    pub jurisdiction: Jurisdiction,              // EU / California / UK / etc.
    pub specific_data: Option<Vec<String>>,      // Optional: scope to specific data categories
    pub stated_reason: Option<String>,
}

pub enum DsarStatus {
    Submitted,
    VerifyingIdentity,
    Processing,
    AwaitingConfirmation,  // For Erasure
    Completed,
    Rejected { reason: String },
}
```

### Mandatory audit events (per ADR-091)

| Operation | Event | Severity |
|---|---|---|
| `submit` | `privacy.dsar_received` | Notice |
| Identity verification | `privacy.dsar_identity_verified` / `_failed` | Notice / Warn |
| Processing started | `privacy.dsar_processing_started` | Info |
| Erasure executed | `privacy.dsar_erasure_completed` (with byte/row counts) | Notice |
| Export delivered | `privacy.dsar_export_delivered` | Info |
| Request rejected | `privacy.dsar_rejected` | Notice |

### Data scope (what counts as the data subject's data)

Implementation MUST identify and process the data subject's data across:
- `notes` rows where `created_by` or `author_id` matches
- `attachments` linked to their notes
- `embeddings` derived from their content
- `links`, `tags`, `collections` they created
- `oauth_tokens`, `api_keys` issued to them
- `audit_events` they are the principal of (retained per audit-retention policy with explicit DSAR exemption documented per Article 17(3)(e))
- Cross-tenant: if user is a member of multiple tenants in EE, the request scope is tenant-scoped unless cross-tenant explicitly requested and authorized

### Erasure semantics

Erasure is a **hard delete with verification**:
1. Submit creates the request and emits an audit event
2. Verification grace period (default 14 days) during which the user can cancel
3. Confirmation token sent out-of-band (email)
4. On confirmation, data is deleted from all relevant tables AND from blob storage AND from search/embedding indices
5. Audit-event retention policy applies — events about the user persist if the lawful-basis exemption applies (compliance/legal hold)
6. A receipt (`DsarErasureReceipt`) is generated containing tables/byte counts and the legal basis for retained items

Soft deletes are not GDPR-compliant erasure. Soft-delete records MUST be hard-deleted as part of erasure.

### Implementation classes

- `NotImplementedHandler` (CE default) — every operation returns `Err(DsarError::NotImplemented)`. Operator must configure an actual handler before going to production with EU/CA users.
- EE `fortemi-enterprise-privacy-builtin` — basic workflow with email verification, JSON export, hard delete
- EE `fortemi-enterprise-privacy-onetrust` — integration with OneTrust DSAR engine
- EE `fortemi-enterprise-privacy-custom` — base scaffolding for customer-specific workflows

## Consequences

### Positive
- (+) Compliance-ready surface
- (+) Auditable workflow
- (+) Pluggable for vendor integrations (OneTrust, TrustArc, custom)
- (+) Erasure has explicit verification + receipt

### Negative
- (-) Hard delete across all stores is non-trivial (especially HNSW indices which require rebuild)
- (-) Vector search indices may retain residual derived features even after source delete; documentation MUST disclose this
- (-) Cross-tenant identity (same user in two tenants) requires explicit ADR-090 elevation

### Neutral
- (~) Operational SLA on DSAR completion is jurisdiction-dependent (GDPR: 1 month + extensions; CCPA: 45 days). Configurable.

## Implementation

**Code location:** `crates/matric-core/src/privacy.rs` (new), EE handlers in private crates.

**Phases:**
1. Land trait + types in matric-core (Experimental)
2. Implement DEFAULT NotImplementedHandler
3. Build `fortemi-enterprise-privacy-builtin` as Beta proof-of-implementation
4. Validate against actual customer DSAR; promote trait to Beta
5. Add identity-verification flow (email + optional MFA) as a separate handler

**Testing:**
- End-to-end: submit Erasure → verify → confirm → audit shows complete chain → all subject data gone
- Audit-retention test: events about subject retained with documented lawful basis
- Cross-tenant test: handler refuses cross-tenant scope without explicit elevation

## References

- GDPR Articles 15-22, 17(3) exceptions
- CCPA §1798.100, .105, .110, .115
- ICO guidance on right of erasure
- NIST Privacy Framework
- ADR-091 (audit), ADR-090 (tenancy)
