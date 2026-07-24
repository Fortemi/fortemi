"use strict";

const SHARD_GUIDES = new Set([
  "docs/content/backup.md",
  "docs/content/shard-migration.md",
]);

function isNegatedOrGated(line) {
  return /\b(?:no|not|never|without|blocked|gated|pending|prohibited|must not|do not)\b/i.test(line);
}

module.exports = {
  id: "issue-1085-knowledge-shard-claims",
  ownerIssue: "#1085",
  profiles: [
    "local_dev",
    "self_hosted_operator",
    "native_distribution",
    "hosted_strict",
    "compatibility",
  ],
  positiveFixtures: [
    "The current Knowledge Shard schema is 1.0.0.",
    "min_reader_version is the minimum Fortemi application release.",
    'knowledge_shard_import({ file_path: "/srv/backups/data.shard" })',
    "Fortemi provides complete backup and schema parity across the suite.",
  ],
  negativeFixtures: [
    "Schema 1.0.0 is an immutable historical authority.",
    "min_reader_version is the minimum Knowledge Shard schema reader.",
    "Upload archive bytes through the multipart import boundary.",
    "Complete backup and schema parity claims remain blocked.",
  ],
  contracts: [
    {
      id: "knowledge-shard-backup-guide-contract",
      file: "docs/content/backup.md",
      severity: "high",
      category: "knowledge_shard_guidance_drift",
      validate(content) {
        return (
          content.includes("**Default export schema**: `1.2.0`") &&
          content.includes("**Opt-in reader/export schema**: exact `2.0.0` tuples") &&
          /`min_reader_version` is a schema\s+compatibility floor/.test(content) &&
          content.includes("### Profiles and evidence boundaries") &&
          content.includes("Fortemi-to-Fortemi boundary") &&
          content.includes("hardened nine-cell schema `1.2.0`") &&
          /Fortemi\s+#1084 and fortemi-react #382/.test(content) &&
          content.includes("/api/v1/backup/knowledge-shard/import/upload") &&
          !content.includes("knowledge_shard_import({ file_path:")
        );
      },
      remediation:
        "Keep the backup guide on exact schema/profile tuples, schema-reader semantics, receipt-scoped claims, and the multipart byte boundary.",
    },
    {
      id: "knowledge-shard-migration-guide-contract",
      file: "docs/content/shard-migration.md",
      severity: "high",
      category: "knowledge_shard_guidance_drift",
      validate(content) {
        return (
          /default export\s+contract is schema `1\.2\.0`/.test(content) &&
          content.includes("Exact schema `2.0.0` tuples") &&
          content.includes("`min_reader_version` always names a Knowledge Shard schema reader floor") &&
          content.includes("hardened nine-cell schema `1.2.0` matrix") &&
          content.includes("Fortemi #1084 and fortemi-react #382") &&
          content.includes("Shard: 3.0.0, Maximum reader: 2.0.0")
        );
      },
      remediation:
        "Document the 1.2 default, exact 2.0 opt-in reader, schema-only minimum reader, and bounded matrix evidence.",
    },
    {
      id: "knowledge-shard-legacy-adr-supersession-contract",
      file: "docs/architecture/adr/ADR-028-shard-archive-migration-system.md",
      severity: "medium",
      category: "knowledge_shard_legacy_adr_drift",
      validate(content) {
        return (
          content.includes("**Status:** Partially superseded") &&
          content.includes("**Superseded by:** ADR-102 and ADR-103 for Knowledge Shards") &&
          content.includes("historical and must not be used as implementation guidance")
        );
      },
      remediation:
        "Keep ADR-028 explicitly superseded for canonical Knowledge Shard behavior.",
    },
    {
      id: "knowledge-shard-version-adr-supersession-contract",
      file: "docs/architecture/adr/ADR-029-shard-schema-versioning.md",
      severity: "medium",
      category: "knowledge_shard_legacy_adr_drift",
      validate(content) {
        return (
          content.includes("**Status:** Partially superseded") &&
          content.includes("**Superseded by:** ADR-102 and ADR-103 for canonical Knowledge Shards") &&
          content.includes("minimum Knowledge Shard **schema** reader version")
        );
      },
      remediation:
        "Keep ADR-029 explicitly superseded where ADR-102/103 define current schema and reader semantics.",
    },
  ],
  rules: [
    {
      id: "docs-shard-stale-current-schema",
      severity: "high",
      category: "knowledge_shard_stale_current_schema",
      appliesTo(relativePath) {
        return SHARD_GUIDES.has(relativePath) || relativePath.endsWith("positive.md");
      },
      detect(line) {
        if (/^\s*Shard:/.test(line) || /\b(?:historical|registered older)\b/i.test(line)) {
          return false;
        }
        return /\bcurrent\b[^\n]*(?:schema|shard|manifest)[^\n]*\b1\.[01]\.0\b/i.test(line)
          || /\b(?:schema|shard|manifest)[^\n]*\b1\.[01]\.0\b[^\n]*\bcurrent\b/i.test(line);
      },
      remediation:
        "Name 1.2.0 as the default export schema and 2.0.0 only as an exact opt-in tuple; label 1.0/1.1 historical.",
    },
    {
      id: "docs-shard-application-min-reader",
      severity: "high",
      category: "knowledge_shard_application_min_reader",
      appliesTo(relativePath) {
        return SHARD_GUIDES.has(relativePath) || relativePath.endsWith("positive.md");
      },
      detect(line) {
        return /min_reader_version[^\n]*(?:application|Fortemi|Fortémi|matric-memory|release)/i.test(line)
          || /minimum\s+(?:Fortemi|Fortémi|matric-memory)\s+version/i.test(line);
      },
      remediation:
        "Define min_reader_version only as a Knowledge Shard schema reader floor; producer application releases are metadata.",
    },
    {
      id: "docs-shard-path-import-example",
      severity: "medium",
      category: "knowledge_shard_path_import_example",
      appliesTo(relativePath) {
        return SHARD_GUIDES.has(relativePath) || relativePath.endsWith("positive.md");
      },
      detect(line) {
        return /knowledge_shard_import[^\n]*file_path/i.test(line)
          || /^\s*[-*]\s+`?file_path`?\s*[-:]/i.test(line);
      },
      remediation:
        "Use the multipart archive-byte boundary; do not present a server-local path as the portable import contract.",
    },
    {
      id: "docs-shard-unqualified-parity-claim",
      severity: "high",
      category: "knowledge_shard_unqualified_parity_claim",
      appliesTo(relativePath) {
        return relativePath.startsWith("docs/") || relativePath.endsWith("positive.md");
      },
      detect(line) {
        return /\b(?:full portability|complete backup|schema parity|100% parity)\b/i.test(line)
          && !isNegatedOrGated(line);
      },
      remediation:
        "Qualify every portability/backup/parity claim by exact schema, profile, producer, consumer, and immutable receipt, or state that it remains gated.",
    },
  ],
};
