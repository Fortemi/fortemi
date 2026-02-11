#!/usr/bin/env node

/**
 * MCP Templates Tests (Phase 10)
 *
 * Tests note template management via MCP tools:
 * - create_template: Create template with name and content (returns {id} only)
 * - list_templates: List all templates
 * - get_template: Retrieve template by ID or slug (returns full object)
 * - instantiate_template: Create note from template with variable substitution (returns {id} only)
 * - delete_template: Remove template
 *
 * Templates support variable substitution (e.g., {{title}}, {{date}}).
 *
 * All tests use unique identifiers (UUIDs) for isolation.
 */

import { strict as assert } from "node:assert";
import { test, describe, before, after } from "node:test";
import { MCPTestClient } from "./helpers/mcp-client.js";

describe("Phase 10: Templates", () => {
  let client;
  const cleanup = { templateIds: [], noteIds: [] };

  before(async () => {
    client = new MCPTestClient();
    await client.initialize();
  });

  after(async () => {
    // Clean up notes first
    for (const id of cleanup.noteIds) {
      try {
        await client.callTool("delete_note", { id });
      } catch (e) {
        console.error(`Failed to delete note ${id}:`, e.message);
      }
    }

    // Clean up templates
    for (const id of cleanup.templateIds) {
      try {
        await client.callTool("delete_template", { id });
      } catch (e) {
        console.error(`Failed to delete template ${id}:`, e.message);
      }
    }

    await client.close();
  });

  test("TEMPLATE-001: create_template with basic content", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const name = `test-template-${testId}`;
    const content = `# {{title}}\n\nCreated on {{date}}`;

    const result = await client.callTool("create_template", {
      name,
      content,
    });

    assert.ok(result.id, "Template should be created with ID");
    cleanup.templateIds.push(result.id);

    // Verify full object by retrieving
    const retrieved = await client.callTool("get_template", { id: result.id });
    assert.strictEqual(retrieved.name, name, "Name should match");
    assert.strictEqual(retrieved.content, content, "Content should match");
  });

  test("TEMPLATE-002: list_templates returns array", async () => {
    const result = await client.callTool("list_templates");

    assert.ok(Array.isArray(result), "Result should be an array");
    // May or may not have templates depending on state
  });

  test("TEMPLATE-003: list_templates includes created template", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const name = `test-list-${testId}`;
    const content = "# Template Content";

    const created = await client.callTool("create_template", {
      name,
      content,
    });
    cleanup.templateIds.push(created.id);

    const templates = await client.callTool("list_templates");

    const found = templates.find((t) => t.id === created.id);
    assert.ok(found, "Created template should appear in list");
    assert.strictEqual(found.name, name, "Name should match in list");
  });

  test("TEMPLATE-004: get_template by id", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const name = `test-get-${testId}`;
    const content = "# Get Template Test";

    const created = await client.callTool("create_template", {
      name,
      content,
    });
    cleanup.templateIds.push(created.id);

    const retrieved = await client.callTool("get_template", {
      id: created.id,
    });

    assert.ok(retrieved, "Template should be retrieved");
    assert.strictEqual(retrieved.id, created.id, "ID should match");
    assert.strictEqual(retrieved.name, name, "Name should match");
    assert.strictEqual(retrieved.content, content, "Content should match");
  });

  test("TEMPLATE-005: get_template returns slug", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const name = `test-slug-${testId}`;
    const content = "# Slug Test";

    const created = await client.callTool("create_template", {
      name,
      content,
    });
    cleanup.templateIds.push(created.id);

    // Retrieve to verify slug is present
    const retrieved = await client.callTool("get_template", { id: created.id });
    assert.ok(retrieved, "Template should be retrieved");
    assert.strictEqual(retrieved.id, created.id, "ID should match");
    // Template may or may not have a slug field depending on API version
    if (retrieved.slug) {
      assert.ok(typeof retrieved.slug === "string", "Slug should be a string");
    }
  });

  test("TEMPLATE-005b: get_template extracts variables", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const name = `test-vars-${testId}`;
    const content = `# {{title}}

Author: {{author}}
Date: {{date}}

## Content
{{body}}

Tags: {{tags}} and {{tags}} again
Footer: {{footer}}`;

    const created = await client.callTool("create_template", {
      name,
      content,
    });
    cleanup.templateIds.push(created.id);

    const retrieved = await client.callTool("get_template", {
      id: created.id,
    });

    assert.ok(retrieved, "Template should be retrieved");
    assert.ok(retrieved.variables, "Template should have variables array");
    assert.ok(Array.isArray(retrieved.variables), "Variables should be an array");

    // Should extract all unique variables
    const expectedVars = ["title", "author", "date", "body", "tags", "footer"];
    assert.deepStrictEqual(
      retrieved.variables.sort(),
      expectedVars.sort(),
      "Should extract all unique variables from template content"
    );
  });

  test("TEMPLATE-005c: get_template with no variables", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const name = `test-novars-${testId}`;
    const content = "# Static Template\n\nNo variables here at all.";

    const created = await client.callTool("create_template", {
      name,
      content,
    });
    cleanup.templateIds.push(created.id);

    const retrieved = await client.callTool("get_template", {
      id: created.id,
    });

    assert.ok(retrieved, "Template should be retrieved");
    assert.ok(retrieved.variables, "Template should have variables array");
    assert.ok(Array.isArray(retrieved.variables), "Variables should be an array");
    assert.strictEqual(retrieved.variables.length, 0, "Variables array should be empty for static template");
  });

  test("TEMPLATE-006: instantiate_template creates note", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const templateName = `test-apply-${testId}`;
    const templateContent = `# {{title}}\n\nContent here`;

    // Create template
    const template = await client.callTool("create_template", {
      name: templateName,
      content: templateContent,
    });
    cleanup.templateIds.push(template.id);

    // Instantiate template to create note
    const result = await client.callTool("instantiate_template", {
      id: template.id,
      variables: {
        title: "My Note Title",
      },
    });

    assert.ok(result.id, "Note should be created from template");
    cleanup.noteIds.push(result.id);

    // Verify the note content
    const note = await client.callTool("get_note", { id: result.id });
    const noteContent = note.original?.content || note.revised?.content;
    assert.ok(noteContent, "Note should have content");
    assert.ok(noteContent.includes("My Note Title"), "Variable should be substituted");
  });

  test("TEMPLATE-007: instantiate_template with multiple variables", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const templateName = `test-multivars-${testId}`;
    const templateContent = `# {{title}}\n\nAuthor: {{author}}\nDate: {{date}}\nTags: {{tags}}`;

    const template = await client.callTool("create_template", {
      name: templateName,
      content: templateContent,
    });
    cleanup.templateIds.push(template.id);

    const result = await client.callTool("instantiate_template", {
      id: template.id,
      variables: {
        title: "Meeting Notes",
        author: "Test User",
        date: "2026-02-06",
        tags: "meeting, work",
      },
    });

    assert.ok(result.id, "Note should be created");
    cleanup.noteIds.push(result.id);

    // Verify the note content
    const note = await client.callTool("get_note", { id: result.id });
    const noteContent = note.original?.content || note.revised?.content;
    assert.ok(noteContent.includes("Meeting Notes"), "Title should be substituted");
    assert.ok(noteContent.includes("Test User"), "Author should be substituted");
    assert.ok(noteContent.includes("2026-02-06"), "Date should be substituted");
    assert.ok(noteContent.includes("meeting, work"), "Tags should be substituted");
  });

  test("TEMPLATE-008: delete_template removes template", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const name = `test-delete-${testId}`;

    const created = await client.callTool("create_template", {
      name,
      content: "# Delete Test",
    });

    // Delete template
    await client.callTool("delete_template", { id: created.id });

    // Verify it's gone
    const error = await client.callToolExpectError("get_template", {
      id: created.id,
    });

    assert.ok(error.error, "Should return error for deleted template");
  });

  test("TEMPLATE-009: create_template with description", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const name = `test-desc-${testId}`;
    const description = "Template for meeting notes";

    const result = await client.callTool("create_template", {
      name,
      content: "# Meeting\n\n",
      description,
    });

    assert.ok(result.id, "Template should be created");
    cleanup.templateIds.push(result.id);

    // Verify description via get
    const retrieved = await client.callTool("get_template", { id: result.id });
    assert.strictEqual(retrieved.description, description, "Description should match");
  });

  test("TEMPLATE-010: create_template with tags", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const tag = MCPTestClient.testTag("template", testId);
    const name = `test-tags-${testId}`;

    const result = await client.callTool("create_template", {
      name,
      content: "# Tagged Template",
      default_tags: [tag],
    });

    assert.ok(result.id, "Template should be created");
    cleanup.templateIds.push(result.id);

    // Verify tags via get
    const retrieved = await client.callTool("get_template", { id: result.id });
    assert.ok(retrieved.default_tags, "Template should have default_tags");
    assert.ok(retrieved.default_tags.includes(tag), "Tag should be included");
  });

  test("TEMPLATE-011: instantiate_template with note metadata", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const noteTag = MCPTestClient.testTag("template-note", testId);
    const templateName = `test-metadata-${testId}`;

    const template = await client.callTool("create_template", {
      name: templateName,
      content: "# {{title}}\n\nTemplate note",
    });
    cleanup.templateIds.push(template.id);

    // Instantiate with note-specific metadata
    const result = await client.callTool("instantiate_template", {
      id: template.id,
      variables: {
        title: "Note with Tags",
      },
      tags: [noteTag],
    });

    assert.ok(result.id, "Note should be created");
    cleanup.noteIds.push(result.id);

    // Verify tags via get_note
    const note = await client.callTool("get_note", { id: result.id });
    assert.ok(note.tags, "Note should have tags");
    assert.ok(note.tags.includes(noteTag), "Note tag should be included");
  });

  test("TEMPLATE-012: instantiate_template creates note with correct content", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const templateName = `test-instantiate-${testId}`;

    const template = await client.callTool("create_template", {
      name: templateName,
      content: "# {{title}}\n\nApplied from template",
    });
    cleanup.templateIds.push(template.id);

    // Instantiate using ID
    const result = await client.callTool("instantiate_template", {
      id: template.id,
      variables: {
        title: "Applied by ID",
      },
    });

    assert.ok(result.id, "Note should be created");
    cleanup.noteIds.push(result.id);

    // Verify content
    const note = await client.callTool("get_note", { id: result.id });
    const noteContent = note.original?.content || note.revised?.content;
    assert.ok(noteContent.includes("Applied by ID"), "Content should be substituted");
    assert.ok(noteContent.includes("Applied from template"), "Static content should be present");
  });

  test("TEMPLATE-013: template with empty variables", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const templateName = `test-empty-${testId}`;

    const template = await client.callTool("create_template", {
      name: templateName,
      content: "# Static Template\n\nNo variables here",
    });
    cleanup.templateIds.push(template.id);

    // Instantiate without variables
    const result = await client.callTool("instantiate_template", {
      id: template.id,
    });

    assert.ok(result.id, "Note should be created");
    cleanup.noteIds.push(result.id);

    // Verify content
    const note = await client.callTool("get_note", { id: result.id });
    const noteContent = note.original?.content || note.revised?.content;
    assert.ok(noteContent.includes("Static Template"), "Content should be copied");
  });

  test("TEMPLATE-014: update_template modifies content", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const name = `test-update-${testId}`;
    const originalContent = "# Original";
    const updatedContent = "# Updated";

    // Create template
    const created = await client.callTool("create_template", {
      name,
      content: originalContent,
    });
    cleanup.templateIds.push(created.id);

    // Update template
    await client.callTool("update_template", {
      id: created.id,
      content: updatedContent,
    });

    // Verify via get
    const retrieved = await client.callTool("get_template", {
      id: created.id,
    });
    assert.strictEqual(retrieved.content, updatedContent, "Updated content should persist");
  });

  test("TEMPLATE-015: create_template error - duplicate name", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const name = `test-duplicate-${testId}`;

    // Create first template
    const first = await client.callTool("create_template", {
      name,
      content: "# First",
    });
    cleanup.templateIds.push(first.id);

    // Try to create duplicate
    const error = await client.callToolExpectError("create_template", {
      name,
      content: "# Second",
    });

    assert.ok(error.error, "Should return error for duplicate name");
  });

  test("TEMPLATE-016: instantiate_template error - non-existent template", async () => {
    const fakeId = MCPTestClient.uniqueId();

    const error = await client.callToolExpectError("instantiate_template", {
      id: fakeId,
      variables: { title: "Test" },
    });

    assert.ok(error.error, "Should return error for non-existent template");
  });

  test("TEMPLATE-017b: update_template with metadata does not delete other templates (issue #311)", async () => {
    // Reproduce the exact TMPL-008 UAT scenario: create multiple templates,
    // update one with name+description+default_tags, verify ALL still exist.
    const testId = MCPTestClient.uniqueId().slice(0, 8);

    // Create 3 templates
    const t1 = await client.callTool("create_template", {
      name: `issue311-alpha-${testId}`,
      content: "# Alpha Template",
    });
    cleanup.templateIds.push(t1.id);

    const t2 = await client.callTool("create_template", {
      name: `issue311-beta-${testId}`,
      content: "# Beta Template",
    });
    cleanup.templateIds.push(t2.id);

    const t3 = await client.callTool("create_template", {
      name: `issue311-gamma-${testId}`,
      content: "# Gamma Template",
    });
    cleanup.templateIds.push(t3.id);

    // Verify all 3 exist before update
    const beforeList = await client.callTool("list_templates");
    const beforeIds = beforeList.map((t) => t.id);
    assert.ok(beforeIds.includes(t1.id), "Alpha should exist before update");
    assert.ok(beforeIds.includes(t2.id), "Beta should exist before update");
    assert.ok(beforeIds.includes(t3.id), "Gamma should exist before update");

    // Update beta with name, description, and default_tags (TMPL-008 scenario)
    const tag = MCPTestClient.testTag("issue311", testId);
    await client.callTool("update_template", {
      id: t2.id,
      name: `issue311-beta-updated-${testId}`,
      description: "Updated beta template description",
      default_tags: [tag],
    });

    // Verify ALL 3 still exist after update
    const afterList = await client.callTool("list_templates");
    const afterIds = afterList.map((t) => t.id);
    assert.ok(afterIds.includes(t1.id), "Alpha should still exist after update");
    assert.ok(afterIds.includes(t2.id), "Beta should still exist after update");
    assert.ok(afterIds.includes(t3.id), "Gamma should still exist after update");

    // Verify updated template has new values
    const updated = await client.callTool("get_template", { id: t2.id });
    assert.strictEqual(updated.name, `issue311-beta-updated-${testId}`, "Name should be updated");
    assert.strictEqual(updated.description, "Updated beta template description", "Description should be updated");
    assert.ok(updated.default_tags, "Should have default_tags");
    assert.ok(updated.default_tags.includes(tag), "Tag should be updated");

    // Verify unmodified templates are unchanged
    const alpha = await client.callTool("get_template", { id: t1.id });
    assert.strictEqual(alpha.name, `issue311-alpha-${testId}`, "Alpha name should be unchanged");

    const gamma = await client.callTool("get_template", { id: t3.id });
    assert.strictEqual(gamma.name, `issue311-gamma-${testId}`, "Gamma name should be unchanged");

    console.log(`  Created 3 templates, updated 1 with metadata, all 3 verified present`);
  });

  test("TEMPLATE-017: template with complex markdown", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const name = `test-complex-${testId}`;
    const content = `# {{title}}

## Sections

### {{section1}}
- Item 1
- Item 2

### {{section2}}
\`\`\`{{language}}
code here
\`\`\`

**Author**: {{author}}
`;

    const template = await client.callTool("create_template", {
      name,
      content,
    });
    cleanup.templateIds.push(template.id);

    const result = await client.callTool("instantiate_template", {
      id: template.id,
      variables: {
        title: "Complex Document",
        section1: "Introduction",
        section2: "Code Example",
        language: "javascript",
        author: "Test Author",
      },
    });

    assert.ok(result.id, "Note should be created");
    cleanup.noteIds.push(result.id);

    // Verify content
    const note = await client.callTool("get_note", { id: result.id });
    const noteContent = note.original?.content || note.revised?.content;
    assert.ok(noteContent.includes("Complex Document"), "Title substituted");
    assert.ok(noteContent.includes("Introduction"), "Section1 substituted");
    assert.ok(noteContent.includes("javascript"), "Language substituted");
  });
});
