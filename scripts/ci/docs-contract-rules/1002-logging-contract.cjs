"use strict";

module.exports = {
  id: "issue-1002-logging-contract",
  ownerIssue: "#1002",
  profiles: [
    "local_dev",
    "test_fixture",
    "self_hosted_operator",
    "native_distribution",
    "hosted_strict",
    "compatibility",
  ],
  positiveFixtures: ["LOG_FORMAT=pretty", "RUST_LOG=debug"],
  negativeFixtures: ["LOG_FORMAT=text", "LOG_FORMAT=json", "RUST_LOG=info"],
  contracts: [
    {
      id: "logging-runtime-contract",
      file: "crates/matric-api/src/main.rs",
      severity: "high",
      category: "logging_runtime_contract_drift",
      validate(content) {
        return (
          content.includes('const DEFAULT_RUST_LOG: &str = "info";') &&
          content.includes('"text" => LogFormat::Text') &&
          content.includes('"json" => LogFormat::Json') &&
          content.includes('strict_bool_value("LOG_ANSI"') &&
          content.includes("RUST_LOG contains invalid filter directives.")
        );
      },
      remediation:
        "Keep hosted-safe info fallback and fail-closed text/json, ANSI, and filter parsing in the runtime source of truth.",
    },
    {
      id: "logging-configuration-doc-contract",
      file: "docs/content/configuration.md",
      severity: "high",
      category: "logging_documentation_contract_drift",
      validate(content) {
        return (
          content.includes("| `RUST_LOG` | String | `info` |") &&
          content.includes("| `LOG_FORMAT` | String | `text` |") &&
          content.includes("| `LOG_FILE` | String | None |") &&
          content.includes("| `LOG_ANSI` | Boolean | auto |") &&
          content.includes("protected diagnostic mode")
        );
      },
      remediation:
        "Document the runtime logging defaults, exact values, and protected debug/trace diagnostic boundary.",
    },
    {
      id: "logging-operations-doc-contract",
      file: "docs/content/operations.md",
      severity: "high",
      category: "logging_documentation_contract_drift",
      validate(content) {
        return (
          content.includes("| `RUST_LOG` | `info` |") &&
          content.includes("| `LOG_FORMAT` | `text` |") &&
          content.includes("| `LOG_FILE` | (none) |") &&
          content.includes("| `LOG_ANSI` | (auto-detected; off for files) |") &&
          content.includes("Invalid `LOG_FORMAT`, `LOG_ANSI`, or `RUST_LOG` values fail startup")
        );
      },
      remediation:
        "Keep the operator logging table aligned with the validated runtime contract.",
    },
    {
      id: "logging-env-example-contract",
      file: ".env.example",
      severity: "high",
      category: "logging_deployment_contract_drift",
      validate(content) {
        return (
          content.includes("RUST_LOG=info") &&
          content.includes("LOG_FORMAT accepts only text or json") &&
          !/LOG_FORMAT=(?:pretty|compact)\b/.test(content)
        );
      },
      remediation:
        "Keep .env.example on the hosted-safe info default and text/json format vocabulary.",
    },
  ],
  rules: [
    {
      id: "docs-unsupported-log-format",
      severity: "high",
      category: "unsupported_log_format",
      detect(line) {
        return /\bLOG_FORMAT\b.*\b(?:pretty|compact)\b/i.test(line);
      },
      remediation: "Use only LOG_FORMAT=text or LOG_FORMAT=json.",
    },
    {
      id: "docs-unsafe-rust-log-default",
      severity: "high",
      category: "unsafe_logging_default",
      appliesTo(relativePath) {
        return (
          relativePath === ".env.example" ||
          relativePath.startsWith("deploy/") ||
          relativePath.startsWith("systemd/") ||
          relativePath.startsWith("docker-compose") ||
          relativePath.startsWith("Dockerfile")
        );
      },
      detect(line) {
        return /\bRUST_LOG\s*=\s*(?:debug|trace|matric_api=debug,tower_http=debug)\b/.test(line);
      },
      remediation:
        "Use a hosted-safe info default; debug/trace must be an explicit protected diagnostic override.",
    },
  ],
};
