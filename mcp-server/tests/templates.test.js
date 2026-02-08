#!/usr/bin/env node

/**
 * MCP Templates Tests (Phase 10)
 *
 * Tests note template management via MCP tools:
 * - create_template: Create template with name and content
 * - list_templates: List all templates
 * - get_template: Retrieve template by ID or slug
 * - apply_template: Create note from template with variable substitution
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
    assert.ok(result.slug, "Template should have slug");
    assert.strictEqual(result.name, name, "Name should match");
    assert.strictEqual(result.content, content, "Content should match");

    cleanup.templateIds.push(result.id);
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

  test("TEMPLATE-005: get_template by slug", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const name = `test-slug-${testId}`;
    const content = "# Slug Test";

    const created = await client.callTool("create_template", {
      name,
      content,
    });
    cleanup.templateIds.push(created.id);

    const retrieved = await client.callTool("get_template", {
      slug: created.slug,
    });

    assert.ok(retrieved, "Template should be retrieved by slug");
    assert.strictEqual(retrieved.id, created.id, "ID should match");
    assert.strictEqual(retrieved.slug, created.slug, "Slug should match");
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

  test("TEMPLATE-006: apply_template creates note", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const templateName = `test-apply-${testId}`;
    const templateContent = `# {{title}}\n\nContent here`;

    // Create template
    const template = await client.callTool("create_template", {
      name: templateName,
      content: templateContent,
    });
    cleanup.templateIds.push(template.id);

    // Apply template to create note
    const result = await client.callTool("apply_template", {
      template_id: template.id,
      variables: {
        title: "My Note Title",
      },
    });

    assert.ok(result.id, "Note should be created from template");
    assert.ok(result.content, "Note should have content");
    assert.ok(result.content.includes("My Note Title"), "Variable should be substituted");

    cleanup.noteIds.push(result.id);
  });

  test("TEMPLATE-007: apply_template with multiple variables", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const templateName = `test-multivars-${testId}`;
    const templateContent = `# {{title}}\n\nAuthor: {{author}}\nDate: {{date}}\nTags: {{tags}}`;

    const template = await client.callTool("create_template", {
      name: templateName,
      content: templateContent,
    });
    cleanup.templateIds.push(template.id);

    const result = await client.callTool("apply_template", {
      template_id: template.id,
      variables: {
        title: "Meeting Notes",
        author: "Test User",
        date: "2026-02-06",
        tags: "meeting, work",
      },
    });

    assert.ok(result.id, "Note should be created");
    assert.ok(result.content.includes("Meeting Notes"), "Title should be substituted");
    assert.ok(result.content.includes("Test User"), "Author should be substituted");
    assert.ok(result.content.includes("2026-02-06"), "Date should be substituted");
    assert.ok(result.content.includes("meeting, work"), "Tags should be substituted");

    cleanup.noteIds.push(result.id);
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
    assert.strictEqual(result.description, description, "Description should match");

    cleanup.templateIds.push(result.id);
  });

  test("TEMPLATE-010: create_template with tags", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const tag = MCPTestClient.testTag("template", testId);
    const name = `test-tags-${testId}`;

    const result = await client.callTool("create_template", {
      name,
      content: "# Tagged Template",
      tags: [tag],
    });

    assert.ok(result.id, "Template should be created");
    assert.ok(result.tags, "Template should have tags");
    assert.ok(result.tags.includes(tag), "Tag should be included");

    cleanup.templateIds.push(result.id);
  });

  test("TEMPLATE-011: apply_template with note metadata", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const noteTag = MCPTestClient.testTag("template-note", testId);
    const templateName = `test-metadata-${testId}`;

    const template = await client.callTool("create_template", {
      name: templateName,
      content: "# {{title}}\n\nTemplate note",
    });
    cleanup.templateIds.push(template.id);

    // Apply with note-specific metadata
    const result = await client.callTool("apply_template", {
      template_id: template.id,
      variables: {
        title: "Note with Tags",
      },
      tags: [noteTag],
    });

    assert.ok(result.id, "Note should be created");
    assert.ok(result.tags, "Note should have tags");
    assert.ok(result.tags.includes(noteTag), "Note tag should be included");

    cleanup.noteIds.push(result.id);
  });

  test("TEMPLATE-012: apply_template by slug", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const templateName = `test-slug-apply-${testId}`;

    const template = await client.callTool("create_template", {
      name: templateName,
      content: "# {{title}}",
    });
    cleanup.templateIds.push(template.id);

    // Apply using slug instead of ID
    const result = await client.callTool("apply_template", {
      template_slug: template.slug,
      variables: {
        title: "Applied by Slug",
      },
    });

    assert.ok(result.id, "Note should be created");
    assert.ok(result.content.includes("Applied by Slug"), "Content should be substituted");

    cleanup.noteIds.push(result.id);
  });

  test("TEMPLATE-013: template with empty variables", async () => {
    const testId = MCPTestClient.uniqueId().slice(0, 8);
    const templateName = `test-empty-${testId}`;

    const template = await client.callTool("create_template", {
      name: templateName,
      content: "# Static Template\n\nNo variables here",
    });
    cleanup.templateIds.push(template.id);

    // Apply without variables
    const result = await client.callTool("apply_template", {
      template_id: template.id,
    });

    assert.ok(result.id, "Note should be created");
    assert.ok(result.content.includes("Static Template"), "Content should be copied");

    cleanup.noteIds.push(result.id);
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
    const updated = await client.callTool("update_template", {
      id: created.id,
      content: updatedContent,
    });

    assert.strictEqual(updated.content, updatedContent, "Content should be updated");

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

  test("TEMPLATE-016: apply_template error - non-existent template", async () => {
    const fakeId = MCPTestClient.uniqueId();

    const error = await client.callToolExpectError("apply_template", {
      template_id: fakeId,
      variables: { title: "Test" },
    });

    assert.ok(error.error, "Should return error for non-existent template");
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

    const result = await client.callTool("apply_template", {
      template_id: template.id,
      variables: {
        title: "Complex Document",
        section1: "Introduction",
        section2: "Code Example",
        language: "javascript",
        author: "Test Author",
      },
    });

    assert.ok(result.id, "Note should be created");
    assert.ok(result.content.includes("Complex Document"), "Title substituted");
    assert.ok(result.content.includes("Introduction"), "Section1 substituted");
    assert.ok(result.content.includes("javascript"), "Language substituted");

    cleanup.noteIds.push(result.id);
  });
});
