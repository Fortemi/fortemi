"use strict";

module.exports = {
  id: "issue-898-metering-contract",
  ownerIssue: "#898",
  profiles: [
    "local_dev",
    "self_hosted_operator",
    "native_distribution",
    "hosted_strict",
    "compatibility",
  ],
  positiveFixtures: ["unknown_price defaults to $0"],
  negativeFixtures: ["unknown_price is not $0"],
  contracts: [
    {
      id: "usage-event-ledger-contract",
      file: "docs/architecture/adr/ADR-092-usage-meter-quota-trait.md",
      severity: "high",
      category: "usage_meter_documentation_contract_drift",
      validate(content) {
        return [
          "event_id",
          "idempotency_key",
          "event_time",
          "recorded_at",
          "UsageSubject",
          "UsageQuantity",
          "UsageProducer",
          "UsageSource",
          "UsageCorrelation",
          "async fn reserve",
          "async fn finalize",
          "async fn release",
          "BillableActual",
          "NonBillableEstimate",
          "Fortemi's usage ledger is the source of truth",
          "conflicting duplicate",
          "marked partial",
          "Reversal",
          "allowlisted",
          "raw_usage",
          "unknown_price",
        ].every((term) => content.includes(term));
      },
      remediation:
        "Keep ADR-092's idempotent ledger, event timing, unit-aware quantities, stable producer, atomic reservation lifecycle, subject/source/correlation, partial/reversal, privacy, raw-usage, and unknown-price contracts explicit.",
    },
    {
      id: "provider-usage-detail-contract",
      file: "docs/architecture/adr/ADR-072-inference-provider-abstraction.md",
      severity: "high",
      category: "provider_usage_documentation_contract_drift",
      validate(content) {
        const normalized = content.replace(/\s+/g, " ");
        return [
          "input/prompt",
          "output/completion",
          "cached input",
          "reasoning output",
          "audio input/output",
          "embedding token/vector",
          "usage_source",
          "raw_usage",
          "estimator_version",
          "unknown_price",
        ].every((term) => normalized.includes(term));
      },
      remediation:
        "Preserve detailed provider usage buckets, source, scrubbed raw usage, estimation provenance, and explicit unknown pricing in ADR-072.",
    },
    {
      id: "provider-guide-current-vs-target-contract",
      file: "docs/content/inference-providers.md",
      severity: "high",
      category: "provider_usage_current_target_drift",
      validate(content) {
        return (
          content.includes(
            "does not currently expose a durable provider-usage or billing ledger"
          ) &&
          ["usage_source", "raw_usage", "estimator_version", "unknown_price"].every(
            (term) => content.includes(term)
          )
        );
      },
      remediation:
        "Keep current no-ledger behavior separate from the planned normalized provider usage and pricing contract.",
    },
    {
      id: "metering-downstream-sink-contract",
      file: "docs/architecture/adr/ADR-092-usage-meter-quota-trait.md",
      severity: "medium",
      category: "metering_sink_documentation_contract_drift",
      validate(content) {
        return [
          "https://openmeter.io/docs/metering/events/usage-events",
          "https://docs.stripe.com/api/billing/meter",
          "https://docs.stripe.com/api/billing/meter-event",
          "https://docs.stripe.com/api/v2/meter-events",
          "legacy usage records are migration context only",
        ].every((term) => content.includes(term));
      },
      remediation:
        "Keep OpenMeter and current Stripe Billing Meter/Meter Event APIs downstream of the Fortemi-owned ledger; legacy usage records are migration-only.",
    },
  ],
  rules: [
    {
      id: "docs-unknown-price-is-not-zero",
      severity: "high",
      category: "provider_usage_unknown_price_coercion",
      detect(line) {
        return /unknown_price.*(?:default|coerce|treat|map).*\$0/i.test(line);
      },
      remediation:
        "Keep unknown provider pricing explicit; only a configured local/free pricing policy may produce $0.",
    },
  ],
};
