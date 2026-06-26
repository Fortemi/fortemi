#!/usr/bin/env node
"use strict";

const crypto = require("node:crypto");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");

const ROOT = path.resolve(process.argv.find((arg) => arg.startsWith("--root="))?.slice(7) || ".");
const PROFILE = process.argv.find((arg) => arg.startsWith("--profile="))?.slice(10) || "hosted_strict";
const MODE = process.env.DOCS_CONTRACT_MODE || "advisory";
const SELF_TEST = process.argv.includes("--self-test");

const DEFAULT_SCAN_PATHS = [
  ".gitea/workflows",
  "Dockerfile",
  "Dockerfile.bundle",
  "docker-compose.yml",
  "docker-compose.bundle.yml",
  "docker-compose.workstation.yml",
  "docs",
  "mcp-server/index.js",
  "mcp-server/tests",
  "scripts",
];

const EXCLUDED_DIR_NAMES = new Set([
  ".git",
  "node_modules",
  "target",
  "dist",
  "build",
  ".next",
]);

const EXCLUDED_RELATIVE_PATHS = new Set([
  // Exact detector fixture files. Broader tests/scripts remain scanned so
  // local-dev and test-fixture allowlists can be made explicit later.
  "mcp-server/tests/output-sanitizer.test.js",
  "scripts/ci/docs-contract.cjs",
]);

const RULES = [
  {
    id: "docs-token-placeholder",
    issue: "#999",
    severity: "high",
    category: "fortemi_token_placeholder",
    detect(line) {
      return /\bmm_(?:at|rt|key)_[A-Za-z0-9._~+/=-]+/.test(line);
    },
    remediation:
      "Use scanner-safe placeholders such as <ACCESS_TOKEN>, <STREAM_TOKEN>, or <API_KEY>.",
  },
  {
    id: "docs-auth-header-secret",
    issue: "#999",
    severity: "high",
    category: "credential_value_in_header",
    detect(line) {
      return /Authorization:\s*Bearer\s+(?!<ACCESS_TOKEN>)[A-Za-z0-9._~+/=-]{6,}/i.test(line);
    },
    remediation:
      "Use Authorization: Bearer <ACCESS_TOKEN> in docs and generated examples.",
  },
  {
    id: "docs-query-token-secret",
    issue: "#999",
    severity: "high",
    category: "credential_value_in_query",
    detect(line) {
      return /[?&]token=(?!<STREAM_TOKEN>|<ACCESS_TOKEN>)[A-Za-z0-9._~+/=-]{6,}/i.test(line);
    },
    remediation:
      "Use token=<STREAM_TOKEN> for EventSource examples or avoid query-token examples.",
  },
  {
    id: "docs-provider-key-placeholder",
    issue: "#999",
    severity: "high",
    category: "provider_key_placeholder",
    detect(line) {
      return /\b(?:OPENAI_API_KEY|OPENROUTER_API_KEY|MATRIC_OPENAI_API_KEY|api_key)["'\s:=]+["']?(?:sk|sk-proj|sk-or|hf)[_-][A-Za-z0-9._~+/=-]+/i.test(line);
    },
    remediation:
      "Use provider-neutral placeholders such as <OPENAI_API_KEY> or <API_KEY>.",
  },
  {
    id: "docs-client-secret-placeholder",
    issue: "#999",
    severity: "high",
    category: "client_secret_placeholder",
    detect(line) {
      return /\b(?:MCP_CLIENT_SECRET|client_secret)["'\s:=]+["']?secret_[A-Za-z0-9._~+/=-]+/i.test(line);
    },
    remediation: "Use <MCP_CLIENT_SECRET> for OAuth/MCP client-secret examples.",
  },
  {
    id: "docs-default-pgpassword",
    issue: "#1001",
    severity: "high",
    category: "default_database_password",
    detect(line) {
      return /\bPGPASSWORD\s*=\s*matric\b|\bPOSTGRES_PASSWORD\s*=\s*matric\b/.test(line);
    },
    remediation:
      "Use <POSTGRES_PASSWORD>, a secret file, or mark the snippet as local_dev/test_fixture in a future allowlist.",
  },
  {
    id: "docs-credential-dsn",
    issue: "#1001",
    severity: "high",
    category: "credential_bearing_database_url",
    detect(line) {
      return /\b(?:DATABASE_URL\s*=\s*)?postgres(?:ql)?:\/\/[^/\s"'`:@]+:[^@\s"'`]+@/i.test(line);
    },
    remediation:
      "Use <DATABASE_URL> or a passwordless placeholder such as postgres://<USER>:<PASSWORD>@<HOST>/<DB>.",
  },
];

function usage() {
  console.log(`Usage: DOCS_CONTRACT_MODE=advisory|blocking node scripts/ci/docs-contract.cjs [--root=.] [--profile=hosted_strict] [--self-test]

Scans docs/config/example surfaces for #999/#1001 secret-shaped placeholders.
Output intentionally redacts matched values and reports category + file:line.`);
}

function isTextFile(filePath) {
  const ext = path.extname(filePath).toLowerCase();
  return [
    "",
    ".cjs",
    ".conf",
    ".env",
    ".js",
    ".json",
    ".md",
    ".sh",
    ".toml",
    ".txt",
    ".yaml",
    ".yml",
  ].includes(ext) || path.basename(filePath).startsWith("Dockerfile");
}

function collectFiles(root, entries = DEFAULT_SCAN_PATHS) {
  const files = [];
  for (const entry of entries) {
    const fullPath = path.join(root, entry);
    if (!fs.existsSync(fullPath)) continue;
    const stat = fs.statSync(fullPath);
    if (stat.isFile()) {
      const relativePath = path.relative(root, fullPath);
      if (isTextFile(fullPath) && !EXCLUDED_RELATIVE_PATHS.has(relativePath)) {
        files.push(fullPath);
      }
      continue;
    }
    walk(root, fullPath, files);
  }
  return files.sort();
}

function walk(root, dir, files) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    if (entry.isDirectory()) {
      if (!EXCLUDED_DIR_NAMES.has(entry.name)) {
        walk(root, path.join(dir, entry.name), files);
      }
      continue;
    }
    const filePath = path.join(dir, entry.name);
    const relativePath = path.relative(root, filePath);
    if (
      entry.isFile() &&
      isTextFile(filePath) &&
      !EXCLUDED_RELATIVE_PATHS.has(relativePath)
    ) {
      files.push(filePath);
    }
  }
}

function fingerprint(ruleId, relativePath, lineNumber, line) {
  const normalized = line
    .replace(/\bmm_(?:at|rt|key)_[A-Za-z0-9._~+/=-]+/g, "mm_<TOKEN>")
    .replace(/Bearer\s+[A-Za-z0-9._~+/=-]+/gi, "Bearer <TOKEN>")
    .replace(/[?&]token=[A-Za-z0-9._~+/=-]+/gi, "token=<TOKEN>")
    .replace(/postgres(?:ql)?:\/\/[^/\s"'`:@]+:[^@\s"'`]+@/gi, "postgres://<USER>:<PASSWORD>@")
    .replace(/\b(?:PGPASSWORD|POSTGRES_PASSWORD)\s*=\s*\S+/g, "<DB_PASSWORD_ASSIGNMENT>");
  return crypto
    .createHash("sha256")
    .update(`${ruleId}\0${relativePath}\0${lineNumber}\0${normalized}`)
    .digest("hex")
    .slice(0, 16);
}

function scan(root) {
  const findings = [];
  for (const file of collectFiles(root)) {
    const relativePath = path.relative(root, file);
    const content = fs.readFileSync(file, "utf8");
    const lines = content.split(/\r?\n/);
    lines.forEach((line, index) => {
      for (const rule of RULES) {
        if (!rule.detect(line)) continue;
        findings.push({
          rule: rule.id,
          issue: rule.issue,
          severity: rule.severity,
          profile: PROFILE,
          category: rule.category,
          file: relativePath,
          line: index + 1,
          fingerprint: fingerprint(rule.id, relativePath, index + 1, line),
          remediation: rule.remediation,
        });
      }
    });
  }
  return findings;
}

function printFindings(findings) {
  console.log(`docs-contract profile=${PROFILE} mode=${MODE} findings=${findings.length}`);
  for (const finding of findings) {
    console.log(
      [
        `${finding.file}:${finding.line}`,
        `rule=${finding.rule}`,
        `owner=${finding.issue}`,
        `severity=${finding.severity}`,
        `profile=${finding.profile}`,
        `category=${finding.category}`,
        `fingerprint=${finding.fingerprint}`,
        `remediation=${finding.remediation}`,
      ].join(" | ")
    );
  }
}

function runSelfTest() {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "fortemi-docs-contract-"));
  try {
    fs.mkdirSync(path.join(tmp, "docs"), { recursive: true });
    fs.writeFileSync(
      path.join(tmp, "docs", "positive.md"),
      [
        'curl -H "Authorization: Bearer mm_at_realisticExample"',
        'curl "https://example.test/events?token=mm_key_realisticExample"',
        "OPENAI_API_KEY=sk-proj-realisticExample",
        "MCP_CLIENT_SECRET=secret_xyz789",
        "PGPASSWORD=matric",
        "DATABASE_URL=postgres://matric:matric@localhost/matric",
      ].join("\n")
    );
    fs.writeFileSync(
      path.join(tmp, "docs", "negative.md"),
      [
        "Use Authorization: Bearer <ACCESS_TOKEN>",
        "Use token=<STREAM_TOKEN>",
        "OPENAI_API_KEY=<OPENAI_API_KEY>",
        "MCP_CLIENT_SECRET=<MCP_CLIENT_SECRET>",
        "client_secret_basic is an OAuth auth method name",
        "secret_set: true",
        "task-specific embeddings",
        "DATABASE_URL=<DATABASE_URL>",
      ].join("\n")
    );
    const findings = scan(tmp);
    const positiveFindings = findings.filter((finding) => finding.file.endsWith("positive.md"));
    const negativeFindings = findings.filter((finding) => finding.file.endsWith("negative.md"));
    if (positiveFindings.length < 6) {
      throw new Error(`expected at least 6 positive findings, got ${positiveFindings.length}`);
    }
    if (negativeFindings.length !== 0) {
      throw new Error(`expected zero negative findings, got ${negativeFindings.length}`);
    }
    console.log("docs-contract self-test passed");
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
}

if (process.argv.includes("--help") || process.argv.includes("-h")) {
  usage();
  process.exit(0);
}

if (!["advisory", "blocking"].includes(MODE)) {
  console.error(`DOCS_CONTRACT_MODE must be advisory or blocking, got: ${MODE}`);
  process.exit(2);
}

if (SELF_TEST) {
  runSelfTest();
  process.exit(0);
}

const findings = scan(ROOT);
printFindings(findings);
if (findings.length > 0 && MODE === "blocking") {
  process.exit(1);
}
