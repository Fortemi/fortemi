"use strict";

const PROFILE_SURFACES = Object.freeze({
  local_dev: [
    "public_docs",
    "operator_docs",
    "model_visible_docs",
    "generated_docs",
    "local_dev",
    "source_example",
  ],
  test_fixture: ["test_fixture", "ci_fixture"],
  self_hosted_operator: [
    "public_docs",
    "operator_docs",
    "model_visible_docs",
    "generated_docs",
    "deployment_config",
    "source_example",
  ],
  native_distribution: [
    "public_docs",
    "operator_docs",
    "model_visible_docs",
    "generated_docs",
    "deployment_config",
    "source_example",
  ],
  hosted_strict: [
    "public_docs",
    "operator_docs",
    "model_visible_docs",
    "generated_docs",
    "deployment_config",
    "source_example",
  ],
  compatibility: [
    "public_docs",
    "operator_docs",
    "model_visible_docs",
    "generated_docs",
    "historical_decision",
    "deployment_config",
    "source_example",
  ],
});

const OPERATOR_DOC_NAMES = new Set([
  "backup.md",
  "configuration.md",
  "deployment.md",
  "mcp-deployment.md",
  "operations.md",
  "operators-guide.md",
  "releasing.md",
  "troubleshooting.md",
]);

function classifySurface(relativePath) {
  const normalized = relativePath.replaceAll("\\", "/");
  const basename = normalized.split("/").at(-1);

  if (
    normalized.startsWith("docs/architecture/adr/") ||
    normalized.startsWith("docs/adr/") ||
    normalized.startsWith(".aiwg/architecture/decisions/")
  ) {
    return "historical_decision";
  }
  if (
    normalized.startsWith("tests/") ||
    normalized.includes("/tests/") ||
    normalized.endsWith(".test.js") ||
    normalized.endsWith(".test.cjs")
  ) {
    return "test_fixture";
  }
  if (normalized.startsWith(".gitea/workflows/")) {
    return "ci_fixture";
  }
  if (
    basename?.startsWith("Dockerfile") ||
    normalized.startsWith("docker-compose") ||
    normalized.startsWith(".devcontainer/")
  ) {
    return "local_dev";
  }
  if (
    normalized.startsWith("docs/generated/") ||
    normalized.startsWith("docs/api/generated/")
  ) {
    return "generated_docs";
  }
  if (normalized === "mcp-server/index.js") {
    return "model_visible_docs";
  }
  if (normalized.startsWith("docs/content/") && OPERATOR_DOC_NAMES.has(basename)) {
    return "operator_docs";
  }
  if (normalized.startsWith("docs/")) {
    return "public_docs";
  }
  if (
    normalized.startsWith("deploy/") ||
    normalized.startsWith("systemd/") ||
    normalized.endsWith(".service")
  ) {
    return "deployment_config";
  }
  return "source_example";
}

function getProfile(name) {
  const surfaces = PROFILE_SURFACES[name];
  if (!surfaces) {
    throw new Error(
      `unknown docs-contract profile ${JSON.stringify(name)}; expected one of: ${Object.keys(
        PROFILE_SURFACES
      ).join(", ")}`
    );
  }
  return { name, surfaces: new Set(surfaces) };
}

module.exports = {
  PROFILE_SURFACES,
  classifySurface,
  getProfile,
};
