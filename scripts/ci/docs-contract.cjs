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
const UPDATE_BASELINE = process.argv.includes("--update-baseline");
const DEFAULT_BASELINE_PATH = path.join(ROOT, "scripts/ci/docs-contract.baseline.json");
const DEFAULT_ALLOWLIST_PATH = path.join(ROOT, "scripts/ci/docs-contract.allowlist.json");
const BASELINE_PATH = path.resolve(
  process.argv.find((arg) => arg.startsWith("--baseline="))?.slice(11) ||
    process.env.DOCS_CONTRACT_BASELINE ||
    DEFAULT_BASELINE_PATH
);
const ALLOWLIST_PATH = path.resolve(
  process.argv.find((arg) => arg.startsWith("--allowlist="))?.slice(12) ||
    process.env.DOCS_CONTRACT_ALLOWLIST ||
    DEFAULT_ALLOWLIST_PATH
);

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
  "scripts/ci/docs-contract.allowlist.json",
  "scripts/ci/docs-contract.baseline.json",
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
    id: "docs-passphrase-placeholder",
    issue: "#999",
    severity: "high",
    category: "passphrase_or_webhook_secret_placeholder",
    appliesTo(relativePath) {
      return relativePath.startsWith("docs/") || relativePath === "mcp-server/index.js";
    },
    detect(line) {
      return (
        /\bpassphrase["']?\s*[:=]\s*["'](?!(?:<PKE_PASSPHRASE>|\.\.\.))[^"']{6,}["']/i.test(line) ||
        /\bsecret["']?\s*[:=]\s*["'](?!(?:<WEBHOOK_SECRET>|<MCP_CLIENT_SECRET>|\.\.\.))[^"']{6,}["']/i.test(line) ||
        /\b(?:save_private_key|load_private_key|encrypt_private_key|decrypt_private_key)\([^)]*["'](?!(?:<PKE_PASSPHRASE>|\.\.\.))[^"']*passphrase[^"']*["']/i.test(line) ||
        /(?:^|\s)(?<!mkdir\s)-p\s+["'](?!(?:<PKE_PASSPHRASE>|\.\.\.))[^"']{6,}["']/.test(line)
      );
    },
    remediation:
      "Use <PKE_PASSPHRASE> or <WEBHOOK_SECRET> instead of realistic passphrase/secret examples.",
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
  console.log(`Usage: DOCS_CONTRACT_MODE=advisory|blocking node scripts/ci/docs-contract.cjs [--root=.] [--profile=hosted_strict] [--baseline=path] [--update-baseline] [--self-test]

Scans docs/config/example surfaces for #999/#1001 secret-shaped placeholders.
Output intentionally redacts matched values and reports category + file:line.
Baselines store fingerprints and metadata only; raw matched values are never written.`);
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
    .replace(/\bpassphrase["']?\s*[:=]\s*["'][^"']+["']/gi, "passphrase: <PKE_PASSPHRASE>")
    .replace(/\bsecret["']?\s*[:=]\s*["'][^"']+["']/gi, "secret: <SECRET>")
    .replace(/(?:^|\s)-p\s+["'][^"']+["']/g, " -p <PKE_PASSPHRASE>")
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
        if (rule.appliesTo && !rule.appliesTo(relativePath)) continue;
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

function validateAllowlistEntry(entry) {
  const required = ["fingerprint", "rule", "issue", "classification", "reason", "owner_issue", "review_after"];
  for (const field of required) {
    if (!entry[field] || typeof entry[field] !== "string") {
      throw new Error(`allowlist entry missing ${field}: ${JSON.stringify(entry)}`);
    }
  }
  if (!/^#\d+$/.test(entry.owner_issue)) {
    throw new Error(`allowlist owner_issue must be an issue ref: ${entry.owner_issue}`);
  }
  if (!/^\d{4}-\d{2}-\d{2}$/.test(entry.review_after)) {
    throw new Error(`allowlist review_after must be YYYY-MM-DD: ${entry.review_after}`);
  }
  if (entry.file === "*" || entry.line === "*") {
    throw new Error(`broad allowlist entries are not allowed: ${JSON.stringify(entry)}`);
  }
}

function loadAllowlist(filePath) {
  if (!fs.existsSync(filePath)) {
    return { filePath, exists: false, entries: [], byFingerprint: new Map() };
  }
  const parsed = JSON.parse(fs.readFileSync(filePath, "utf8"));
  const entries = Array.isArray(parsed.entries) ? parsed.entries : [];
  for (const entry of entries) {
    validateAllowlistEntry(entry);
  }
  return {
    filePath,
    exists: true,
    entries,
    byFingerprint: new Map(entries.map((entry) => [entry.fingerprint, entry])),
  };
}

function normalizeBaselineEntry(finding, allowlist) {
  const allowlistEntry = allowlist.byFingerprint.get(finding.fingerprint);
  return {
    fingerprint: finding.fingerprint,
    rule: finding.rule,
    issue: finding.issue,
    profile: finding.profile,
    severity: finding.severity,
    category: finding.category,
    file: finding.file,
    line: finding.line,
    classification:
      allowlistEntry?.classification ||
      (finding.issue === "#1001" ? "existing_credential_example" : "existing_placeholder"),
    reason: allowlistEntry?.reason || "Initial advisory baseline; cleanup tracked by owner issue.",
    owner_issue: allowlistEntry?.owner_issue || finding.issue,
    review_after: allowlistEntry?.review_after || "2026-07-31",
  };
}

function loadBaseline(filePath) {
  if (!fs.existsSync(filePath)) {
    return { filePath, exists: false, entries: [], byFingerprint: new Map() };
  }
  const parsed = JSON.parse(fs.readFileSync(filePath, "utf8"));
  const entries = Array.isArray(parsed.entries) ? parsed.entries : [];
  return {
    filePath,
    exists: true,
    entries,
    byFingerprint: new Map(entries.map((entry) => [entry.fingerprint, entry])),
  };
}

function writeBaseline(filePath, findings, allowlist) {
  const baseline = {
    version: 1,
    profile: PROFILE,
    generated_at: new Date().toISOString().slice(0, 10),
    owner_issues: Array.from(new Set(findings.map((finding) => finding.issue))).sort(),
    policy:
      "Redacted docs-contract baseline. Entries store stable fingerprints and finding metadata only; raw matched values are intentionally excluded.",
    allowlist: allowlist.exists ? path.relative(ROOT, allowlist.filePath) : null,
    entries: findings.map((finding) => normalizeBaselineEntry(finding, allowlist)).sort((a, b) => {
      return (
        a.file.localeCompare(b.file) ||
        a.line - b.line ||
        a.rule.localeCompare(b.rule) ||
        a.fingerprint.localeCompare(b.fingerprint)
      );
    }),
  };
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, `${JSON.stringify(baseline, null, 2)}\n`);
  return baseline;
}

function classifyFindings(findings, baseline) {
  return findings.map((finding) => ({
    ...finding,
    baseline_state: baseline.byFingerprint.has(finding.fingerprint) ? "known" : "new",
    classification: baseline.byFingerprint.get(finding.fingerprint)?.classification || "unclassified",
  }));
}

function countByState(findings) {
  return findings.reduce(
    (counts, finding) => {
      counts[finding.baseline_state] += 1;
      counts.classifications[finding.classification] =
        (counts.classifications[finding.classification] || 0) + 1;
      return counts;
    },
    { known: 0, new: 0, classifications: {} }
  );
}

function staleBaselineCount(findings, baseline) {
  const currentFingerprints = new Set(findings.map((finding) => finding.fingerprint));
  return baseline.entries.filter((entry) => !currentFingerprints.has(entry.fingerprint)).length;
}

function printFindings(findings, baseline) {
  const counts = countByState(findings);
  const stale = staleBaselineCount(findings, baseline);
  const classificationSummary = Object.entries(counts.classifications)
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([name, count]) => `${name}:${count}`)
    .join(",");
  console.log(
    [
      `docs-contract profile=${PROFILE}`,
      `mode=${MODE}`,
      `findings=${findings.length}`,
      `known=${counts.known}`,
      `new=${counts.new}`,
      `baseline=${baseline.exists ? path.relative(ROOT, baseline.filePath) : "none"}`,
      `stale_baseline=${stale}`,
      `classifications=${classificationSummary || "none"}`,
    ].join(" ")
  );
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
        `baseline=${finding.baseline_state}`,
        `classification=${finding.classification}`,
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
        'passphrase: "secure-passphrase-123"',
        'save_private_key(&keypair.private, "/path/to/private.key", "your-passphrase")?;',
        '-p "your-secure-passphrase-123"',
        '"secret": "my-webhook-secret"',
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
        'passphrase: "<PKE_PASSPHRASE>"',
        'load_private_key("/path/to/private.key", "<PKE_PASSPHRASE>")?;',
        '-p "<PKE_PASSPHRASE>"',
        '"secret": "<WEBHOOK_SECRET>"',
        "client_secret_basic is an OAuth auth method name",
        "secret_set: true",
        "task-specific embeddings",
        "DATABASE_URL=<DATABASE_URL>",
      ].join("\n")
    );
    const findings = scan(tmp);
    const positiveFindings = findings.filter((finding) => finding.file.endsWith("positive.md"));
    const negativeFindings = findings.filter((finding) => finding.file.endsWith("negative.md"));
    if (positiveFindings.length < 10) {
      throw new Error(`expected at least 10 positive findings, got ${positiveFindings.length}`);
    }
    if (negativeFindings.length !== 0) {
      throw new Error(`expected zero negative findings, got ${negativeFindings.length}`);
    }
    const baselinePath = path.join(tmp, "docs-contract.baseline.json");
    const allowlist = loadAllowlist(path.join(tmp, "docs-contract.allowlist.json"));
    const writtenBaseline = writeBaseline(baselinePath, findings, allowlist);
    const baseline = loadBaseline(baselinePath);
    const classified = classifyFindings(findings, baseline);
    const counts = countByState(classified);
    if (writtenBaseline.entries.some((entry) => "matched_text" in entry || "snippet" in entry)) {
      throw new Error("baseline must not store raw matched text or snippets");
    }
    if (counts.known !== positiveFindings.length || counts.new !== 0) {
      throw new Error(`expected baseline to classify all positives as known, got ${JSON.stringify(counts)}`);
    }
    fs.appendFileSync(path.join(tmp, "docs", "positive.md"), "\nPOSTGRES_PASSWORD=matric");
    const expandedFindings = classifyFindings(scan(tmp), baseline);
    const expandedCounts = countByState(expandedFindings);
    if (expandedCounts.new !== 1) {
      throw new Error(`expected one new finding after baseline, got ${JSON.stringify(expandedCounts)}`);
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
const allowlist = loadAllowlist(ALLOWLIST_PATH);
if (UPDATE_BASELINE) {
  writeBaseline(BASELINE_PATH, findings, allowlist);
}
const baseline = loadBaseline(BASELINE_PATH);
const classifiedFindings = classifyFindings(findings, baseline);
printFindings(classifiedFindings, baseline);
if (UPDATE_BASELINE) {
  console.log(`docs-contract baseline updated: ${path.relative(ROOT, BASELINE_PATH)} entries=${findings.length}`);
}
const counts = countByState(classifiedFindings);
if (MODE === "blocking" && ((baseline.exists && counts.new > 0) || (!baseline.exists && findings.length > 0))) {
  process.exit(1);
}
