"use strict";

module.exports = {
  id: "issue-996-trusted-proxy-contract",
  ownerIssue: "#996",
  profiles: [
    "local_dev",
    "self_hosted_operator",
    "hosted_strict",
    "compatibility",
  ],
  positiveFixtures: [
    "proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;",
    "FORTEMI_TRUSTED_PROXY_CIDRS=0.0.0.0/0",
  ],
  negativeFixtures: [
    "proxy_set_header X-Forwarded-For $remote_addr;",
    "FORTEMI_TRUSTED_PROXY_CIDRS=127.0.0.1/32,::1/128",
  ],
  contracts: [
    {
      id: "trusted-proxy-runtime-contract",
      file: "crates/matric-api/src/trusted_proxy.rs",
      severity: "high",
      category: "trusted_proxy_runtime_drift",
      validate(content) {
        return (
          content.includes('std::env::var("FORTEMI_TRUSTED_PROXY_CIDRS")') &&
          content.includes("ForwardedHeaderDisposition::Suppressed") &&
          content.includes("config.trusts(address)") &&
          content.includes("forwarded client chain contains an invalid IP address") &&
          content.includes("forwarded protocol must be http or https") &&
          content.includes("forwarded metadata contains ambiguous multiple values")
        );
      },
      remediation:
        "Keep immediate-peer CIDR validation and fail-closed forwarded host/proto/port/client-IP parsing centralized.",
    },
    {
      id: "trusted-proxy-server-wiring-contract",
      file: "crates/matric-api/src/main.rs",
      severity: "high",
      category: "trusted_proxy_server_wiring_drift",
      validate(content) {
        return (
          content.includes("into_make_service_with_connect_info::<SocketAddr>()") &&
          content.includes("trusted_proxy_config: TrustedProxyConfig") &&
          content.includes("ExternalRequestContext::from_request(") &&
          content.includes("incoming_webhook_external_url(external_context, uri)")
        );
      },
      remediation:
        "Attach socket peer metadata and require Twilio URL reconstruction to consume ExternalRequestContext.",
    },
    {
      id: "trusted-proxy-nginx-contract",
      file: "deploy/nginx/memory.s9.internal.conf",
      severity: "high",
      category: "trusted_proxy_edge_drift",
      validate(content) {
        return (
          !content.includes("$proxy_add_x_forwarded_for") &&
          content.includes("proxy_set_header X-Forwarded-For $remote_addr;") &&
          content.includes("proxy_set_header X-Forwarded-Host $host;") &&
          content.includes("proxy_set_header X-Forwarded-Port $server_port;") &&
          content.includes('proxy_set_header X-Forwarded-Protocol "";') &&
          content.includes('proxy_set_header Forwarded "";')
        );
      },
      remediation:
        "Keep the client-facing edge authoritative: overwrite X-Forwarded-* from nginx variables and clear client Forwarded aliases.",
    },
    {
      id: "trusted-proxy-operator-doc-contract",
      file: "docs/content/configuration.md",
      severity: "high",
      category: "trusted_proxy_documentation_drift",
      validate(content) {
        return (
          content.includes("`FORTEMI_TRUSTED_PROXY_CIDRS`") &&
          content.includes("Unset trusts no proxy") &&
          content.includes("immediate socket peer matches") &&
          content.includes("private or loopback")
        );
      },
      remediation:
        "Document the explicit immediate-peer CIDR allowlist, trust-none default, canonical edge behavior, and private listener boundary.",
    },
  ],
  rules: [
    {
      id: "forwarded-for-edge-append",
      severity: "high",
      category: "forwarded_header_client_chain_preserved",
      appliesTo(relativePath) {
        return (
          relativePath.startsWith("docs/") ||
          relativePath.startsWith("deploy/nginx/")
        );
      },
      detect(line) {
        return /\$proxy_add_x_forwarded_for/.test(line);
      },
      remediation:
        "At the authoritative client-facing edge, replace inbound X-Forwarded-For with $remote_addr.",
    },
    {
      id: "trusted-proxy-universal-cidr",
      severity: "critical",
      category: "trusted_proxy_allowlist_open",
      appliesTo(relativePath) {
        return (
          relativePath.startsWith("docs/") ||
          relativePath === ".env.example" ||
          relativePath.startsWith("docker-compose")
        );
      },
      detect(line) {
        return /FORTEMI_TRUSTED_PROXY_CIDRS\s*=\s*(?:0\.0\.0\.0\/0|::\/0)/.test(
          line
        );
      },
      remediation:
        "Trust only numeric CIDRs for immediate reverse-proxy peers; never use a universal network.",
    },
  ],
};
