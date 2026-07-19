"use strict";

const CURRENT_CE_ENV_VARS = Object.freeze([
  "RATE_LIMIT_ENABLED",
  "RATE_LIMIT_REQUESTS",
  "RATE_LIMIT_PERIOD_SECS",
]);
const CURRENT_CE_HEADERS = Object.freeze(["Retry-After"]);
const FUTURE_HOSTED_HEADERS = Object.freeze(["RateLimit", "RateLimit-Policy"]);
const LEGACY_HEADERS = Object.freeze([
  "X-RateLimit-*",
  "RateLimit-Limit",
  "RateLimit-Remaining",
  "RateLimit-Reset",
]);

const APPROVED_CONTEXT =
  /\b(?:compatibility|deliberately unsupported|does not|future|historical|legacy|no|not emitted|not supported|target)\b/i;
const ACTIVE_CLAIM =
  /\b(?:current|emit|emits|include|includes|provide|provides|response headers?|return|returns)\b/i;

function nearbyContext(line, context) {
  const start = Math.max(0, context.lineNumber - 3);
  return context.lines.slice(start, context.lineNumber).concat(line).join(" ");
}

function hasApprovedContext(line, context) {
  if (APPROVED_CONTEXT.test(line)) return true;
  if (ACTIVE_CLAIM.test(line)) return false;
  return APPROVED_CONTEXT.test(nearbyContext(line, context));
}

module.exports = {
  id: "issue-998-rate-limit-contract",
  ownerIssue: "#998",
  profiles: [
    "local_dev",
    "self_hosted_operator",
    "native_distribution",
    "hosted_strict",
    "compatibility",
  ],
  inventory: {
    currentCeEnvVars: CURRENT_CE_ENV_VARS,
    currentCeHeaders: CURRENT_CE_HEADERS,
    futureHostedHeaders: FUTURE_HOSTED_HEADERS,
    legacyHeaders: LEGACY_HEADERS,
  },
  positiveFixtures: [
    "RATE_LIMIT_PER_MINUTE=100",
    "RATE_LIMIT_PER_TENANT=true",
    "Current response headers: X-RateLimit-Limit",
    "Primary target: RateLimit-Remaining",
  ],
  negativeFixtures: [
    "RATE_LIMIT_ENABLED=true",
    "RATE_LIMIT_REQUESTS=100",
    "RATE_LIMIT_PERIOD_SECS=60",
    "Legacy compatibility only: X-RateLimit-Limit",
    "Historical only: RateLimit-Remaining",
    "Future target: RateLimit-Policy and RateLimit",
  ],
  contracts: [
    {
      id: "rate-limit-runtime-env-contract",
      file: "crates/matric-api/src/main.rs",
      severity: "high",
      category: "rate_limit_runtime_contract_drift",
      validate(content) {
        return (
          CURRENT_CE_ENV_VARS.every((name) => content.includes(`"${name}"`)) &&
          !content.includes('"RATE_LIMIT_PER_MINUTE"') &&
          !content.includes('"RATE_LIMIT_PER_TENANT"')
        );
      },
      remediation:
        "Keep the CE runtime source of truth on RATE_LIMIT_ENABLED, RATE_LIMIT_REQUESTS, and RATE_LIMIT_PERIOD_SECS.",
    },
    {
      id: "rate-limit-public-api-contract",
      file: "docs/content/api.md",
      severity: "high",
      category: "rate_limit_documentation_contract_drift",
      validate(content) {
        return (
          CURRENT_CE_ENV_VARS.every((name) => content.includes(`\`${name}\``)) &&
          /carries only\s+`Retry-After`/.test(content) &&
          content.includes("future hosted quota")
        );
      },
      remediation:
        "Document all three current CE env vars, the current Retry-After-only 429, and a separately labeled future hosted contract.",
    },
    {
      id: "rate-limit-future-header-contract",
      file: "docs/architecture/adr/ADR-098-per-tenant-rate-limits-quotas.md",
      severity: "high",
      category: "rate_limit_future_header_contract_drift",
      validate(content) {
        return (
          FUTURE_HOSTED_HEADERS.every((name) => content.includes(`${name}:`)) &&
          !/\bRateLimit-(?:Limit|Remaining|Reset):/.test(content) &&
          content.includes("not emit legacy `X-RateLimit-*`") &&
          /(?:coarsen|omit).*(?:capacity|quota)|(?:capacity|quota).*(?:coarsen|omit)/i.test(
            content
          )
        );
      },
      remediation:
        "Use combined RateLimit and RateLimit-Policy as the future target, reject legacy headers, and retain quota-disclosure controls.",
    },
  ],
  rules: [
    {
      id: "docs-unsupported-rate-limit-env",
      severity: "high",
      category: "unsupported_rate_limit_environment_variable",
      detect(line) {
        const match = line.match(/\b(RATE_LIMIT_[A-Z0-9_]+)\s*=/);
        return Boolean(match && !CURRENT_CE_ENV_VARS.includes(match[1]));
      },
      remediation:
        "Use RATE_LIMIT_ENABLED, RATE_LIMIT_REQUESTS, and RATE_LIMIT_PERIOD_SECS for current CE configuration.",
    },
    {
      id: "docs-current-legacy-rate-limit-header",
      severity: "high",
      category: "legacy_rate_limit_header_claim",
      detect(line, context) {
        return (
          /\bX-RateLimit-(?:\*|Limit|Remaining|Reset)\b/.test(line) &&
          !hasApprovedContext(line, context)
        );
      },
      remediation:
        "Remove legacy X-RateLimit-* claims or mark them explicitly as historical/compatibility-only and not emitted.",
    },
    {
      id: "docs-split-rate-limit-header",
      severity: "high",
      category: "obsolete_split_rate_limit_header",
      detect(line, context) {
        return (
          /\b(?<!X-)RateLimit-(?:Limit|Remaining|Reset)\b/.test(line) &&
          !hasApprovedContext(line, context)
        );
      },
      remediation:
        "Use the future combined RateLimit and RateLimit-Policy fields; older split fields require an explicit historical/compatibility marker.",
    },
  ],
};
