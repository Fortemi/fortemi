//! Hosted single-tenant dedicated route qualification derived from the route inventory (#1154).
//!
//! Every `ROUTE_POLICY_INVENTORY` operation is assigned a route class and a
//! qualification status. The status is derived from
//! `route_policy::hosted_tenant_transaction_ready`, so the published matrix in
//! `docs/deployment/hosted-route-qualification.md` cannot drift from the gate
//! that `auth_middleware` enforces.

use axum::http::Method;
use serde::Serialize;

use crate::route_policy::{self, PolicyClass, RoutePolicy, ROUTE_POLICY_INVENTORY};

/// Name of the deployment profile qualified by the published matrix.
pub const HOSTED_PROFILE_SINGLE_TENANT_DEDICATED: &str = "single_tenant_dedicated";

/// Methods the runtime summary probes; the matrix test proves every ready
/// operation is a registered router operation.
const READINESS_PROBE_METHODS: [Method; 5] = [
    Method::GET,
    Method::POST,
    Method::PUT,
    Method::PATCH,
    Method::DELETE,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum RouteClass {
    Search,
    Notes,
    LinksGraph,
    ArchivesMemories,
    Export,
    JobsEmbeddings,
    RealtimeMcp,
    Collections,
    Taxonomy,
    Inference,
    Attachments,
    Provenance,
    Templates,
    Account,
    VoiceCalls,
    KnowledgeHealth,
    Operator,
    PublicProtocol,
}

impl RouteClass {
    pub const ALL: [RouteClass; 18] = [
        RouteClass::Search,
        RouteClass::Notes,
        RouteClass::LinksGraph,
        RouteClass::ArchivesMemories,
        RouteClass::Export,
        RouteClass::JobsEmbeddings,
        RouteClass::RealtimeMcp,
        RouteClass::Collections,
        RouteClass::Taxonomy,
        RouteClass::Inference,
        RouteClass::Attachments,
        RouteClass::Provenance,
        RouteClass::Templates,
        RouteClass::Account,
        RouteClass::VoiceCalls,
        RouteClass::KnowledgeHealth,
        RouteClass::Operator,
        RouteClass::PublicProtocol,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            RouteClass::Search => "search",
            RouteClass::Notes => "notes",
            RouteClass::LinksGraph => "links_graph",
            RouteClass::ArchivesMemories => "archives_memories",
            RouteClass::Export => "export",
            RouteClass::JobsEmbeddings => "jobs_embeddings",
            RouteClass::RealtimeMcp => "realtime_mcp",
            RouteClass::Collections => "collections",
            RouteClass::Taxonomy => "taxonomy",
            RouteClass::Inference => "inference",
            RouteClass::Attachments => "attachments",
            RouteClass::Provenance => "provenance",
            RouteClass::Templates => "templates",
            RouteClass::Account => "account",
            RouteClass::VoiceCalls => "voice_calls",
            RouteClass::KnowledgeHealth => "knowledge_health",
            RouteClass::Operator => "operator",
            RouteClass::PublicProtocol => "public_protocol",
        }
    }

    /// Classes a graph/knowledge workload on a dedicated deployment depends on.
    /// Unready operations in these classes are gaps; elsewhere they are exclusions.
    #[cfg(test)]
    pub const fn workload_required(self) -> bool {
        matches!(
            self,
            RouteClass::Search
                | RouteClass::Notes
                | RouteClass::LinksGraph
                | RouteClass::ArchivesMemories
                | RouteClass::Export
                | RouteClass::JobsEmbeddings
                | RouteClass::RealtimeMcp
                | RouteClass::Collections
        )
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Qualification {
    /// Admitted by the hosted tenant-transaction gate.
    Qualified,
    /// Needed by the graph/knowledge workload but not tenant-bound in hosted mode.
    Gap,
    /// Not offered to tenants of a hosted deployment.
    Excluded,
    /// Protocol, probe, or inline-proof route that carries no tenant data.
    Public,
}

#[cfg(test)]
impl Qualification {
    pub const fn as_str(self) -> &'static str {
        match self {
            Qualification::Qualified => "qualified",
            Qualification::Gap => "gap",
            Qualification::Excluded => "excluded",
            Qualification::Public => "public",
        }
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Enforcement {
    /// Runs on the verified tenant's transaction-bound connection.
    TenantTransaction,
    /// Bearer route rejected with 503 by the hosted migration gate (401 without a bearer).
    Hosted503,
    /// Bypasses bearer authentication entirely.
    AuthExempt,
}

#[cfg(test)]
impl Enforcement {
    pub const fn as_str(self) -> &'static str {
        match self {
            Enforcement::TenantTransaction => "tenant_transaction",
            Enforcement::Hosted503 => "hosted_503",
            Enforcement::AuthExempt => "auth_exempt",
        }
    }
}

/// Public-safe hosted profile summary reported by system compatibility.
#[derive(Clone, Debug, Serialize)]
pub struct HostedProfileSummary {
    pub name: &'static str,
    pub qualified_route_classes: Vec<&'static str>,
}

pub fn hosted_profile_summary() -> HostedProfileSummary {
    HostedProfileSummary {
        name: HOSTED_PROFILE_SINGLE_TENANT_DEDICATED,
        qualified_route_classes: qualified_route_classes(),
    }
}

/// Classes with at least one operation admitted by the hosted gate, in matrix order.
pub fn qualified_route_classes() -> Vec<&'static str> {
    RouteClass::ALL
        .into_iter()
        .filter(|class| {
            ROUTE_POLICY_INVENTORY.iter().any(|policy| {
                route_class(policy) == *class
                    && READINESS_PROBE_METHODS.iter().any(|method| {
                        route_policy::hosted_tenant_transaction_ready(method, policy.path)
                    })
            })
        })
        .map(RouteClass::as_str)
        .collect()
}

#[cfg(test)]
pub fn enforcement(method: &Method, policy: &RoutePolicy) -> Enforcement {
    if route_policy::hosted_tenant_transaction_ready(method, policy.path) {
        return Enforcement::TenantTransaction;
    }
    // Community-exempt routes hosted mode closes behind a bearer and the gate.
    if crate::hosted_exempt_routes::hosted_requires_bearer(method, policy.path) {
        return Enforcement::Hosted503;
    }
    let inline_proof = *method == Method::POST
        && route_policy::policy_class_for_request(policy, method)
            == PolicyClass::PublicWithInlineProof;
    if route_policy::is_public_without_bearer(policy.path) || inline_proof {
        Enforcement::AuthExempt
    } else {
        Enforcement::Hosted503
    }
}

#[cfg(test)]
pub fn qualification(method: &Method, policy: &RoutePolicy) -> Qualification {
    if route_policy::hosted_tenant_transaction_ready(method, policy.path) {
        return Qualification::Qualified;
    }
    let class = route_class(policy);
    let effective = route_policy::policy_class_for_request(policy, method);
    if class == RouteClass::PublicProtocol
        || matches!(
            effective,
            PolicyClass::Public | PolicyClass::OAuth | PolicyClass::PublicWithInlineProof
        )
    {
        return Qualification::Public;
    }
    if class.workload_required() {
        Qualification::Gap
    } else {
        Qualification::Excluded
    }
}

pub fn route_class(policy: &RoutePolicy) -> RouteClass {
    let path = policy.path;
    let under = |prefix: &str| path == prefix || path.starts_with(&format!("{prefix}/"));
    let note_sub = |suffix: &str| {
        path.strip_prefix("/api/v1/notes/{id}/")
            .is_some_and(|rest| rest == suffix || rest.starts_with(&format!("{suffix}/")))
    };

    if under("/api/v1/search")
        || path == "/api/v1/memories/search"
        || policy.action_family == "search"
    {
        RouteClass::Search
    } else if under("/api/v1/graph") || ["links", "backlinks", "related"].into_iter().any(note_sub)
    {
        RouteClass::LinksGraph
    } else if is_export_route(path) {
        RouteClass::Export
    } else if under("/api/v1/archives") || under("/api/v1/memories") || under("/api/v1/memory") {
        RouteClass::ArchivesMemories
    } else if under("/api/v1/jobs")
        || path.starts_with("/api/v1/embedding-")
        || path == "/api/v1/notes/reprocess"
        || note_sub("reprocess")
    {
        RouteClass::JobsEmbeddings
    } else if matches!(
        path,
        "/api/v1/events" | "/api/v1/ws" | "/api/v1/ingest/stream"
    ) {
        RouteClass::RealtimeMcp
    } else if under("/api/v1/collections") || note_sub("move") {
        RouteClass::Collections
    } else if under("/api/v1/concepts") || note_sub("concepts") {
        RouteClass::Taxonomy
    } else if under("/api/v1/attachments") || note_sub("attachments") {
        RouteClass::Attachments
    } else if under("/api/v1/provenance") || note_sub("provenance") || note_sub("memory-provenance")
    {
        RouteClass::Provenance
    } else if under("/api/v1/templates") {
        RouteClass::Templates
    } else if under("/api/v1/notes") || under("/api/v1/lifecycle-purge") || path == "/api/v1/tags" {
        RouteClass::Notes
    } else if under("/api/v1/calls") || under("/api/v1/realtime") {
        RouteClass::VoiceCalls
    } else if under("/api/v1/webhooks") {
        // Receiver POSTs carry an inline provider proof and are reported as public.
        RouteClass::Operator
    } else {
        class_by_policy(policy)
    }
}

fn is_export_route(path: &str) -> bool {
    matches!(
        path,
        "/api/v1/notes/{id}/export"
            | "/api/v1/collections/{id}/export"
            | "/api/v1/backup/export"
            | "/api/v1/backup/import"
            | "/api/v1/backup/memory/{name}"
            | "/api/v1/memory/export"
    ) || path.starts_with("/api/v1/backup/knowledge-shard")
        || path.starts_with("/api/v1/backup/knowledge-archive")
}

fn class_by_policy(policy: &RoutePolicy) -> RouteClass {
    let path = policy.path;
    let ai_execution = matches!(
        policy.action_family,
        "ai_execution" | "model_catalog" | "document_type_catalog"
    ) && policy.class != PolicyClass::AdminOperator;
    if ai_execution {
        return RouteClass::Inference;
    }
    if matches!(
        policy.action_family,
        "user_secret" | "token_verification" | "rate_limit" | "pke"
    ) {
        return RouteClass::Account;
    }
    // Liveness, readiness and the aggregate streaming probe are public protocol;
    // only the tenant-reading knowledge diagnostics form this class (#1164).
    if crate::hosted_exempt_routes::is_knowledge_diagnostic(path) {
        return RouteClass::KnowledgeHealth;
    }
    match policy.class {
        PolicyClass::Public
        | PolicyClass::OAuth
        | PolicyClass::PublicWithInlineProof
        | PolicyClass::SystemHealth => RouteClass::PublicProtocol,
        _ => RouteClass::Operator,
    }
}

#[cfg(test)]
pub mod matrix {
    //! Rendering of the committed matrix region; test-only.
    use super::*;
    use std::collections::BTreeMap;

    pub const BEGIN: &str = "<!-- BEGIN GENERATED hosted-route-qualification -->";
    pub const END: &str = "<!-- END GENERATED hosted-route-qualification -->";

    #[derive(Serialize)]
    struct Row<'a> {
        method: &'a str,
        path: &'a str,
        class: &'static str,
        status: &'static str,
        enforcement: &'static str,
    }

    #[derive(Default)]
    struct Counts {
        qualified: usize,
        gap: usize,
        excluded: usize,
        public: usize,
    }

    impl Counts {
        fn add(&mut self, status: Qualification) {
            match status {
                Qualification::Qualified => self.qualified += 1,
                Qualification::Gap => self.gap += 1,
                Qualification::Excluded => self.excluded += 1,
                Qualification::Public => self.public += 1,
            }
        }

        fn class_status(&self) -> &'static str {
            let unready = self.gap + self.excluded;
            match (self.qualified > 0, unready > 0) {
                (true, false) => "qualified",
                (true, true) => "partial",
                (false, true) if self.gap > 0 => "gap",
                (false, true) => "excluded",
                (false, false) => "public",
            }
        }
    }

    /// Render the generated region for registered `(path, method)` operations.
    pub fn render(operations: &[(&'static str, &'static str)]) -> String {
        let mut rows = Vec::new();
        let mut counts: BTreeMap<RouteClass, Counts> = BTreeMap::new();
        for &(path, method_name) in operations {
            let policy = route_policy::route_policy_for_path(path)
                .unwrap_or_else(|| panic!("{path} has no policy row"));
            let method = Method::from_bytes(method_name.as_bytes()).expect("known method");
            let class = route_class(policy);
            let status = qualification(&method, policy);
            counts.entry(class).or_default().add(status);
            rows.push((
                class,
                Row {
                    method: method_name,
                    path,
                    class: class.as_str(),
                    status: status.as_str(),
                    enforcement: enforcement(&method, policy).as_str(),
                },
            ));
        }
        rows.sort_by(|(a, ra), (b, rb)| (a, ra.path, ra.method).cmp(&(b, rb.path, rb.method)));

        let mut out = format!("{BEGIN}\n\n");
        out.push_str("| Route class | Workload required | Class status | Qualified | Gap | Excluded | Public |\n");
        out.push_str("|---|---|---|---:|---:|---:|---:|\n");
        let mut total = Counts::default();
        for class in RouteClass::ALL {
            let Some(c) = counts.get(&class) else {
                continue;
            };
            out.push_str(&format!(
                "| `{}` | {} | {} | {} | {} | {} | {} |\n",
                class.as_str(),
                if class.workload_required() {
                    "yes"
                } else {
                    "no"
                },
                c.class_status(),
                c.qualified,
                c.gap,
                c.excluded,
                c.public
            ));
            total.qualified += c.qualified;
            total.gap += c.gap;
            total.excluded += c.excluded;
            total.public += c.public;
        }
        out.push_str(&format!(
            "| **total** | | | {} | {} | {} | {} |\n\n",
            total.qualified, total.gap, total.excluded, total.public
        ));
        out.push_str("```json\n{\n");
        out.push_str(&format!(
            "  \"profile\": \"{HOSTED_PROFILE_SINGLE_TENANT_DEDICATED}\",\n  \"operations\": [\n"
        ));
        let rendered: Vec<String> = rows
            .iter()
            .map(|(_, row)| format!("    {}", serde_json::to_string(row).expect("row json")))
            .collect();
        out.push_str(&rendered.join(",\n"));
        out.push_str("\n  ]\n}\n```\n\n");
        out.push_str(END);
        out
    }

    /// Extract the generated region (inclusive of markers) from a document.
    pub fn committed_region(document: &str) -> Option<&str> {
        let start = document.find(BEGIN)?;
        let end = document[start..].find(END)? + start + END.len();
        Some(&document[start..end])
    }
}
