import { strict as assert } from "node:assert";
import { describe, test } from "node:test";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { CORE_TOOL_NAMES } from "../constants/core-tools.js";

const here = path.dirname(fileURLToPath(import.meta.url));
const phasesDir = path.resolve(here, "../../tests/uat/phases");
const phaseFiles = fs.readdirSync(phasesDir)
  .filter(name => /^phase-\d+.*\.md$/.test(name))
  .sort();
const phaseText = phaseFiles
  .map(name => fs.readFileSync(path.join(phasesDir, name), "utf8"))
  .join("\n");
const readme = fs.readFileSync(path.join(phasesDir, "README.md"), "utf8");

describe("MCP UAT core coverage", () => {
  test("all manually testable core tools appear in phase procedures", () => {
    const infrastructureOnly = new Set(["manage_encryption", "manage_backups"]);
    const missing = CORE_TOOL_NAMES.filter(name =>
      !infrastructureOnly.has(name) && !new RegExp(`\\b${name}\\b`).test(phaseText)
    );
    assert.deepEqual(missing, []);
  });

  test("infrastructure-only exceptions are explicit", () => {
    assert.match(readme, /manage_encryption.*manage_backups|manage_backups.*manage_encryption/s);
    assert.match(readme, /automated integration/i);
  });

  test("suite metadata matches the production core contract", () => {
    const testCount = (phaseText.match(/^### [A-Z0-9]+(?:-[A-Z0-9]+)*:/gm) || []).length;
    assert.equal(CORE_TOOL_NAMES.length, 43);
    assert.equal(testCount, 184);
    assert.match(readme, /43 Core MCP Tools/);
    assert.match(readme, /16 phases \(0-15\)/);
    assert.match(readme, /Total Tests\*\*: 184/);
    assert.ok(phaseFiles.includes("phase-14-mcp-operations.md"));
    assert.ok(phaseFiles.includes("phase-15-cleanup.md"));
    assert.ok(!phaseFiles.includes("phase-14-cleanup.md"));
  });
});
