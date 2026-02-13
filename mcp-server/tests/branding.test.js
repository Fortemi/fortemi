import { test } from "node:test";
import { strict as assert } from "node:assert";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const projectRoot = path.resolve(__dirname, "..");

test("package.json has correct Fortémi branding", () => {
  const packagePath = path.join(projectRoot, "package.json");
  const packageJson = JSON.parse(fs.readFileSync(packagePath, "utf8"));

  assert.equal(
    packageJson.name,
    "@fortemi/mcp",
    "package name should be @fortemi/mcp"
  );
  assert.equal(
    packageJson.description,
    "MCP server for Fortémi API",
    "description should reference Fortémi API"
  );
  assert.ok(
    packageJson.bin["fortemi-mcp"],
    "bin should have fortemi-mcp entry"
  );
  assert.equal(
    Object.keys(packageJson.bin).length,
    1,
    "bin should have exactly one entry"
  );
});

test("index.js has correct server name", () => {
  const indexPath = path.join(projectRoot, "index.js");
  const indexContent = fs.readFileSync(indexPath, "utf8");

  assert.match(
    indexContent,
    /name:\s*["']fortemi["']/,
    'server name should be "fortemi"'
  );
  assert.doesNotMatch(
    indexContent,
    /name:\s*["']matric-memory["']/,
    'server name should not be "matric-memory"'
  );
});

test("index.js uses FORTEMI_URL environment variable", () => {
  const indexPath = path.join(projectRoot, "index.js");
  const indexContent = fs.readFileSync(indexPath, "utf8");

  assert.match(
    indexContent,
    /process\.env\.FORTEMI_URL/,
    "should reference FORTEMI_URL"
  );
  assert.doesNotMatch(
    indexContent,
    /process\.env\.MATRIC_MEMORY_URL/,
    "should not reference MATRIC_MEMORY_URL"
  );
});

test(".mcp.json uses fortemi server key", { skip: !fs.existsSync(path.join(path.dirname(fileURLToPath(import.meta.url)), "..", "..", ".mcp.json")) }, () => {
  const mcpPath = path.join(projectRoot, "..", ".mcp.json");
  const mcpJson = JSON.parse(fs.readFileSync(mcpPath, "utf8"));

  assert.ok(
    mcpJson.mcpServers.fortemi,
    'mcpServers should have "fortemi" key'
  );
  assert.ok(
    !mcpJson.mcpServers["matric-memory"],
    'mcpServers should not have "matric-memory" key'
  );
  assert.ok(
    mcpJson.mcpServers.fortemi.env.FORTEMI_URL,
    "should use FORTEMI_URL env var"
  );
});

test(".claude/settings.local.json references fortemi", { skip: !fs.existsSync(path.join(path.dirname(fileURLToPath(import.meta.url)), "..", "..", ".claude", "settings.local.json")) }, () => {
  const settingsPath = path.join(
    projectRoot,
    "..",
    ".claude",
    "settings.local.json"
  );
  const settings = JSON.parse(fs.readFileSync(settingsPath, "utf8"));

  assert.ok(
    settings.enabledMcpjsonServers.includes("fortemi"),
    'enabledMcpjsonServers should include "fortemi"'
  );
  assert.ok(
    !settings.enabledMcpjsonServers.includes("matric-memory"),
    'enabledMcpjsonServers should not include "matric-memory"'
  );
});

test("README.md examples use fortemi branding", () => {
  const readmePath = path.join(projectRoot, "README.md");
  const readme = fs.readFileSync(readmePath, "utf8");

  // Check for fortemi in config examples
  assert.match(
    readme,
    /"fortemi"/,
    "README should contain fortemi in configuration examples"
  );

  // Check for FORTEMI_URL env var
  assert.match(
    readme,
    /FORTEMI_URL/,
    "README should reference FORTEMI_URL environment variable"
  );

  // Should not have old branding in config examples
  const configExampleMatches = readme.match(
    /"mcpServers":\s*\{[^}]*"matric-memory"/g
  );
  assert.ok(
    !configExampleMatches || configExampleMatches.length === 0,
    'README should not have "matric-memory" in mcpServers config examples'
  );
});
