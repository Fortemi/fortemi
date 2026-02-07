import { strict as assert } from "node:assert";
import { test, describe, before, after } from "node:test";
import { MCPTestClient } from "./helpers/mcp-client.js";

describe("Phase 2b: File Attachments", () => {
  let client;
  const cleanup = { noteIds: [], fileIds: [] };

  before(async () => {
    client = new MCPTestClient();
    await client.initialize();
  });

  after(async () => {
    for (const id of cleanup.fileIds) {
      try { await client.callTool("delete_file", { id }); } catch {}
    }
    for (const id of cleanup.noteIds) {
      try { await client.callTool("delete_note", { id }); } catch {}
    }
    await client.close();
  });

  test("ATT-001: Store file attachment to a note", async () => {
    // Create a note first
    const note = await client.callTool("create_note", {
      content: `# Attachment Test Note ${MCPTestClient.uniqueId()}`,
      tags: [MCPTestClient.testTag("attachments")],
      revision_mode: "none",
    });
    assert.ok(note.id, "Should return note ID");
    cleanup.noteIds.push(note.id);

    // Store a text file attachment
    const file = await client.callTool("store_file", {
      note_id: note.id,
      filename: "test-document.txt",
      content_base64: Buffer.from("Hello, this is test content for attachment testing.").toString("base64"),
      content_type: "text/plain",
    });
    assert.ok(file.id || file.file_id, "Should return file ID");
    cleanup.fileIds.push(file.id || file.file_id);
  });

  test("ATT-002: List files by note", async () => {
    // Create note with attachment
    const note = await client.callTool("create_note", {
      content: `# List Files Test ${MCPTestClient.uniqueId()}`,
      tags: [MCPTestClient.testTag("attachments")],
      revision_mode: "none",
    });
    cleanup.noteIds.push(note.id);

    await client.callTool("store_file", {
      note_id: note.id,
      filename: "list-test.txt",
      content_base64: Buffer.from("List test content").toString("base64"),
      content_type: "text/plain",
    });

    const files = await client.callTool("list_files_by_note", {
      note_id: note.id,
    });
    assert.ok(Array.isArray(files) || files.files, "Should return files array");
    const fileList = Array.isArray(files) ? files : files.files;
    assert.ok(fileList.length > 0, "Should have at least one file");
  });

  test("ATT-003: Get file content", async () => {
    const note = await client.callTool("create_note", {
      content: `# Get Content Test ${MCPTestClient.uniqueId()}`,
      tags: [MCPTestClient.testTag("attachments")],
      revision_mode: "none",
    });
    cleanup.noteIds.push(note.id);

    const originalContent = "Retrievable test content with UTF-8: café ñ";
    const stored = await client.callTool("store_file", {
      note_id: note.id,
      filename: "retrievable.txt",
      content_base64: Buffer.from(originalContent).toString("base64"),
      content_type: "text/plain",
    });
    const fileId = stored.id || stored.file_id;
    cleanup.fileIds.push(fileId);

    const content = await client.callTool("get_file_content", {
      id: fileId,
    });
    assert.ok(content, "Should return file content");
  });

  test("ATT-004: Delete file attachment", async () => {
    const note = await client.callTool("create_note", {
      content: `# Delete File Test ${MCPTestClient.uniqueId()}`,
      tags: [MCPTestClient.testTag("attachments")],
      revision_mode: "none",
    });
    cleanup.noteIds.push(note.id);

    const stored = await client.callTool("store_file", {
      note_id: note.id,
      filename: "deletable.txt",
      content_base64: Buffer.from("Delete me").toString("base64"),
      content_type: "text/plain",
    });
    const fileId = stored.id || stored.file_id;

    const result = await client.callTool("delete_file", { id: fileId });
    assert.ok(result, "Delete should succeed");
  });
});
