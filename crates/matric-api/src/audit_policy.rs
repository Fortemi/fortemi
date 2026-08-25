//! Executable audit producer and outage policy inventory (#711).
//!
//! This catalog is the API-side authority for which security-relevant producer
//! families are mandatory in hosted mode. Individual producers still construct
//! domain-specific `AuditEvent` values, but outage behavior is selected here so
//! it cannot drift independently across handlers.

use matric_core::{AuditAvailabilityPhase, AuditFailureDisposition, AuditFailurePolicy};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum AuditProducerCoverage {
    Wired,
    Partial,
    Pending,
    External,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuditProducerPolicy {
    pub key: &'static str,
    pub event_family: &'static str,
    pub failure_policy: AuditFailurePolicy,
    pub coverage: AuditProducerCoverage,
}

impl AuditProducerPolicy {
    pub fn outage_disposition(
        self,
        hosted: bool,
        phase: AuditAvailabilityPhase,
    ) -> AuditFailureDisposition {
        if hosted {
            self.failure_policy.disposition_when_unavailable(phase)
        } else {
            // CE's TracingSink remains best-effort and never changes request behavior.
            AuditFailureDisposition::Continue
        }
    }
}

use AuditFailurePolicy::{BestEffort, DegradeWithAlert, FailClosed};
use AuditProducerCoverage::{External, Partial, Pending, Wired};

pub const MANDATORY_AUDIT_PRODUCERS: &[AuditProducerPolicy] = &[
    producer("auth_lifecycle", "auth.lifecycle", FailClosed, Partial),
    producer("authorization", "auth.decision", FailClosed, Wired),
    producer(
        "tenant_boundary",
        "system.cross_tenant_access",
        FailClosed,
        Partial,
    ),
    producer("key_lifecycle", "key.lifecycle", FailClosed, Partial),
    producer("mcp_tool", "mcp.tool", FailClosed, Pending),
    producer("admin_config", "admin.config", FailClosed, Partial),
    producer("plugin_lifecycle", "plugin.lifecycle", FailClosed, External),
    producer("privacy_dsar", "privacy.dsar", FailClosed, Pending),
    producer("data_lifecycle", "data.lifecycle", FailClosed, Partial),
    producer("quota_decision", "quota.decision", FailClosed, Wired),
    producer("runtime_lifecycle", "process.lifecycle", BestEffort, Wired),
    producer(
        "dependency_degradation",
        "system.dependency",
        DegradeWithAlert,
        Partial,
    ),
];

const fn producer(
    key: &'static str,
    event_family: &'static str,
    failure_policy: AuditFailurePolicy,
    coverage: AuditProducerCoverage,
) -> AuditProducerPolicy {
    AuditProducerPolicy {
        key,
        event_family,
        failure_policy,
        coverage,
    }
}

pub fn producer_policy(key: &str) -> Option<&'static AuditProducerPolicy> {
    MANDATORY_AUDIT_PRODUCERS
        .iter()
        .find(|producer| producer.key == key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn mandatory_producer_matrix_is_unique_and_complete() {
        let expected = BTreeSet::from([
            "admin_config",
            "auth_lifecycle",
            "authorization",
            "data_lifecycle",
            "dependency_degradation",
            "key_lifecycle",
            "mcp_tool",
            "plugin_lifecycle",
            "privacy_dsar",
            "quota_decision",
            "runtime_lifecycle",
            "tenant_boundary",
        ]);
        let actual: BTreeSet<_> = MANDATORY_AUDIT_PRODUCERS
            .iter()
            .map(|producer| producer.key)
            .collect();

        assert_eq!(actual, expected);
        assert_eq!(actual.len(), MANDATORY_AUDIT_PRODUCERS.len());
        assert!(MANDATORY_AUDIT_PRODUCERS.iter().all(|producer| {
            !producer.event_family.is_empty()
                && !producer.event_family.contains(char::is_whitespace)
        }));
    }

    #[test]
    fn hosted_ready_outage_matrix_rejects_every_fail_closed_producer() {
        for producer in MANDATORY_AUDIT_PRODUCERS {
            let disposition = producer.outage_disposition(true, AuditAvailabilityPhase::Ready);
            match producer.failure_policy {
                FailClosed => assert_eq!(
                    disposition,
                    AuditFailureDisposition::RejectOperation,
                    "{} must reject when hosted audit is unavailable",
                    producer.key
                ),
                DegradeWithAlert => {
                    assert_eq!(disposition, AuditFailureDisposition::DegradeWithAlert)
                }
                BestEffort => assert_eq!(disposition, AuditFailureDisposition::Continue),
            }
        }
    }

    #[test]
    fn bootstrap_and_community_outage_behavior_are_explicit() {
        for producer in MANDATORY_AUDIT_PRODUCERS {
            assert_eq!(
                producer.outage_disposition(false, AuditAvailabilityPhase::Ready),
                AuditFailureDisposition::Continue,
                "CE producer {} must preserve best-effort behavior",
                producer.key
            );
            if producer.failure_policy == FailClosed {
                assert_eq!(
                    producer.outage_disposition(true, AuditAvailabilityPhase::Bootstrap),
                    AuditFailureDisposition::DegradeWithAlert,
                    "hosted producer {} must avoid bootstrap deadlock",
                    producer.key
                );
            }
        }
    }

    #[test]
    fn unresolved_producer_coverage_remains_machine_visible() {
        let unresolved: BTreeSet<_> = MANDATORY_AUDIT_PRODUCERS
            .iter()
            .filter(|producer| producer.coverage != Wired)
            .map(|producer| producer.key)
            .collect();

        assert!(unresolved.contains("mcp_tool"));
        assert!(unresolved.contains("privacy_dsar"));
        assert!(unresolved.contains("plugin_lifecycle"));
        assert!(!unresolved.contains("authorization"));
        assert!(!unresolved.contains("quota_decision"));
    }
}
