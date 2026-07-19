"use strict";

const profileContract = require("../../../crates/matric-api/oauth-profiles.json");

const ACTIVE_PROFILE = profileContract.profiles[profileContract.active_runtime_profile];
const ACTIVE_SCOPES = Object.freeze(ACTIVE_PROFILE.scopes);
const ACTIVE_PKCE_METHODS = Object.freeze(ACTIVE_PROFILE.pkce_methods);
const jsonArray = (values) => `[${values.map((value) => JSON.stringify(value)).join(", ")}]`;

module.exports = {
  id: "issue-1003-oauth-profile-contract",
  ownerIssue: "#1003",
  profiles: [
    "local_dev",
    "self_hosted_operator",
    "hosted_strict",
    "compatibility",
  ],
  positiveFixtures: [
    '"code_challenge_methods_supported": ["S256", "plain"]',
    '"scopes_supported": ["read", "write", "delete", "admin", "mcp"]',
    '"client_id_metadata_document_supported": true',
  ],
  negativeFixtures: [
    '"code_challenge_methods_supported": ["S256"]',
    '"scopes_supported": ["read", "write", "admin", "mcp"]',
    '"client_id_metadata_document_supported": false',
  ],
  contracts: [
    {
      id: "oauth-profile-fixture-contract",
      file: "crates/matric-api/oauth-profiles.json",
      severity: "high",
      category: "oauth_profile_fixture_drift",
      validate(content) {
        const contract = JSON.parse(content);
        const names = Object.keys(contract.profiles || {}).sort();
        const hosted = contract.profiles?.hosted_strict;
        const legacyDelete = contract.non_advertised_legacy_seed_scopes?.delete;
        return (
          contract.version === 1 &&
          contract.active_runtime_profile === "self_hosted_operator" &&
          names.join(",") ===
            "compatibility,hosted_strict,local_dev,self_hosted_operator" &&
          hosted?.availability === "dependency_gated" &&
          hosted?.registration_mode === "unavailable" &&
          hosted?.advertise_registration_endpoint === false &&
          hosted?.registration_management_supported === false &&
          hosted?.client_id_metadata_document_supported === false &&
          hosted?.allow_local_issuer === false &&
          hosted?.token_endpoint_auth_methods?.length === 0 &&
          hosted?.pkce_methods?.join(",") === "S256" &&
          !hosted?.scopes?.includes("delete") &&
          legacyDelete?.owner_issue === "#924"
        );
      },
      remediation:
        "Keep all four OAuth profiles explicit and leave hosted_strict dependency-gated without DCR, CIMD, management, localhost, or unsupported auth claims.",
    },
    {
      id: "oauth-profile-runtime-loader-contract",
      file: "crates/matric-api/src/oauth_profile.rs",
      severity: "high",
      category: "oauth_profile_runtime_contract_drift",
      validate(content) {
        return (
          content.includes('include_str!("../oauth-profiles.json")') &&
          content.includes("unknown OAuth deployment profile") &&
          content.includes("hosted_strict must remain unavailable and fail closed") &&
          content.includes("delete must not be advertised before #924 resolves it") &&
          content.includes("pub fn active_oauth_capabilities()") &&
          content.includes("pub fn is_allowed_oauth_scope(scope: &str)")
        );
      },
      remediation:
        "Load and validate the embedded structured profile contract; unknown profiles and hosted capability widening must fail closed.",
    },
    {
      id: "oauth-profile-runtime-consumer-contract",
      file: "crates/matric-api/src/main.rs",
      severity: "high",
      category: "oauth_profile_runtime_consumer_drift",
      validate(content) {
        return (
          content.includes("let capabilities = active_oauth_capabilities();") &&
          content.includes(
            "token_endpoint_auth_methods_supported: capabilities.token_endpoint_auth_methods.clone()"
          ) &&
          content.includes("scopes_supported: capabilities.scopes.clone()") &&
          content.includes("Some(capabilities.pkce_methods.clone())") &&
          content.includes('"scopes_supported": capabilities.scopes') &&
          content.includes("if !is_allowed_oauth_scope(s)") &&
          !content.includes("const ALLOWED_SCOPES")
        );
      },
      remediation:
        "Build discovery, protected-resource scopes, and registration validation from active_oauth_capabilities().",
    },
    {
      id: "oauth-profile-authentication-doc-contract",
      file: "docs/content/authentication.md",
      severity: "high",
      category: "oauth_profile_documentation_drift",
      validate(content) {
        return (
          content.includes("self-hosted operator compatibility profile") &&
          content.includes("not a hosted-strict launch profile") &&
          content.includes(
            `"scopes_supported": ${jsonArray(ACTIVE_SCOPES)}`
          ) &&
          content.includes(
            `"code_challenge_methods_supported": ${jsonArray(ACTIVE_PKCE_METHODS)}`
          ) &&
          /does not\s+implement RFC 7592 client management routes/.test(content)
        );
      },
      remediation:
        "Label the current OAuth profile and keep its discovery example aligned with oauth-profiles.json without implying hosted-strict or RFC 7592 support.",
    },
    {
      id: "oauth-profile-mcp-doc-contract",
      file: "docs/content/configuration.md",
      severity: "high",
      category: "oauth_profile_mcp_documentation_drift",
      validate(content) {
        return content.includes(
          `"scopes_supported": ${jsonArray(ACTIVE_SCOPES.filter((scope) => scope !== "admin"))}`
        );
      },
      remediation:
        "Keep the MCP protected-resource example within the active OAuth scope set.",
    },
  ],
  rules: [
    {
      id: "docs-oauth-plain-pkce-claim",
      severity: "high",
      category: "oauth_unsupported_plain_pkce_claim",
      appliesTo(relativePath) {
        return relativePath.startsWith("docs/");
      },
      detect(line) {
        return /code_challenge_methods_supported.*\bplain\b/i.test(line);
      },
      remediation:
        "Advertise S256 only; plain PKCE requires a separately implemented and named compatibility profile.",
    },
    {
      id: "docs-oauth-delete-scope-claim",
      severity: "high",
      category: "oauth_unsupported_delete_scope_claim",
      appliesTo(relativePath) {
        return relativePath.startsWith("docs/");
      },
      detect(line) {
        return /"scopes_supported"\s*:\s*\[[^\]]*"delete"/.test(line);
      },
      remediation:
        "Use only scopes accepted by the active OAuth profile; delete remains a legacy seed owned by #924.",
    },
    {
      id: "docs-oauth-premature-cimd-claim",
      severity: "high",
      category: "oauth_unimplemented_cimd_claim",
      appliesTo(relativePath) {
        return relativePath.startsWith("docs/");
      },
      detect(line) {
        return /"client_id_metadata_document_supported"\s*:\s*true/.test(line);
      },
      remediation:
        "Do not advertise CIMD until #972 delivers strict metadata fetch and validation.",
    },
  ],
};
