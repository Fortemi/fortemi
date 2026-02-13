import { strict as assert } from "node:assert";
import { test, describe, before, after } from "node:test";
import { MCPTestClient } from "./helpers/mcp-client.js";

describe("Phase 2b: File Attachments", () => {
  let client;
  const cleanup = { noteIds: [], fileIds: [] };

  before(async () => {
    client = new MCPTestClient();
    await client.initialize();
    // Ensure we're in the default archive context (prevents state leakage from other tests)
    await client.callTool("select_memory", { name: "public" });
  });

  after(async () => {
    for (const id of cleanup.fileIds) {
      try { await client.callTool("delete_attachment", { id }); } catch {}
    }
    for (const id of cleanup.noteIds) {
      try { await client.callTool("delete_note", { id }); } catch {}
    }
    await client.close();
  });

  /**
   * Helper: get upload URL from MCP, then POST multipart directly to API.
   *
   * This mirrors the real agent workflow:
   * 1. MCP tool returns the upload URL + curl command
   * 2. Agent executes the upload directly against the API
   *
   * Includes retry logic for transient errors during parallel test execution.
   */
  async function uploadFile(noteId, filename, content, contentType) {
    const maxRetries = 5;
    const baseDelay = 300;
    let lastError;

    for (let attempt = 0; attempt <= maxRetries; attempt++) {
      try {
        // Step 1: Get upload URL from MCP tool
        const info = await client.callTool("upload_attachment", {
          note_id: noteId,
          filename,
          content_type: contentType,
        });
        assert.ok(info.upload_url, "Should return upload URL");
        assert.ok(info.curl_command, "Should return curl command");

        // Step 2: Upload directly to the API via multipart/form-data
        const blob = new Blob([content], { type: contentType });
        const formData = new FormData();
        formData.append("file", blob, filename);

        // Build headers - include auth if available
        const uploadHeaders = {};
        if (client.apiKey) {
          uploadHeaders["Authorization"] = `Bearer ${client.apiKey}`;
        }

        const response = await fetch(info.upload_url, {
          method: "POST",
          body: formData,
          headers: uploadHeaders,
        });

        // Check for transient errors that should be retried
        if (!response.ok) {
          const text = await response.text();
          const isRetryable = response.status === 400 || response.status === 404 ||
            text.includes("Referenced resource not found") ||
            text.includes("Note not found") ||
            text.includes("does not exist");

          if (isRetryable && attempt < maxRetries) {
            const delay = Math.pow(2, attempt) * baseDelay + Math.random() * 100;
            await new Promise(r => setTimeout(r, delay));
            continue;
          }
          throw new Error(`Upload failed (${response.status}): ${text}`);
        }

        const result = await response.json();
        assert.ok(result.id, "Should return attachment ID");
        return result;
      } catch (error) {
        lastError = error;

        // Check if this is a retryable error
        const isRetryable = error.message?.includes("Referenced resource not found") ||
          error.message?.includes("Note not found") ||
          error.message?.includes("does not exist") ||
          error.message?.includes("Upload failed (400)") ||
          error.message?.includes("Upload failed (404)");

        if (isRetryable && attempt < maxRetries) {
          const delay = Math.pow(2, attempt) * baseDelay + Math.random() * 100;
          await new Promise(r => setTimeout(r, delay));
          continue;
        }
        throw error;
      }
    }
    throw lastError;
  }

  test("ATT-001: Store file attachment to a note", async () => {
    // Create a note first
    const note = await client.callTool("create_note", {
      content: `# Attachment Test Note ${MCPTestClient.uniqueId()}`,
      tags: [MCPTestClient.testTag("attachments")],
      revision_mode: "none",
    });
    assert.ok(note.id, "Should return note ID");
    cleanup.noteIds.push(note.id);

    // Upload a text file via MCP URL + direct API POST
    const file = await uploadFile(
      note.id,
      "test-document.txt",
      "Hello, this is test content for attachment testing.",
      "text/plain"
    );
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

    // Upload file
    await uploadFile(
      note.id,
      "list-test.txt",
      "List test content",
      "text/plain"
    );

    // List attachments using MCP tool
    const files = await client.callTool("list_attachments", {
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
    const stored = await uploadFile(
      note.id,
      "retrievable.txt",
      originalContent,
      "text/plain"
    );
    const fileId = stored.id || stored.file_id;
    cleanup.fileIds.push(fileId);

    // Get attachment metadata using MCP tool
    const attachment = await client.callTool("get_attachment", {
      id: fileId,
    });
    assert.ok(attachment, "Should return attachment metadata");
    assert.ok(attachment.id === fileId, "Should match the uploaded file ID");
    assert.ok(attachment._api_urls, "Should include API URLs for download");
  });

  test("ATT-005: Upload response reflects configured max size", async () => {
    // Create a note for the upload
    const note = await client.callTool("create_note", {
      content: `# Upload Limit Test ${MCPTestClient.uniqueId()}`,
      tags: [MCPTestClient.testTag("attachments")],
      revision_mode: "none",
    });
    cleanup.noteIds.push(note.id);

    // Call upload_attachment — the response should include max_size
    const info = await client.callTool("upload_attachment", {
      note_id: note.id,
      filename: "limit-test.txt",
      content_type: "text/plain",
    });
    assert.ok(info.max_size, "Response should include max_size field");
    // Default is 50MB; custom values end with "MB"
    assert.match(info.max_size, /^\d+MB$/, `max_size should be NMB format, got: ${info.max_size}`);

    const sizeMB = parseInt(info.max_size, 10);
    assert.ok(sizeMB > 0, "max_size should be a positive number");
    assert.ok(sizeMB <= 1024, "max_size should be reasonable (<=1GB)");
  });

  test("ATT-004: Delete file attachment", async () => {
    const note = await client.callTool("create_note", {
      content: `# Delete File Test ${MCPTestClient.uniqueId()}`,
      tags: [MCPTestClient.testTag("attachments")],
      revision_mode: "none",
    });
    cleanup.noteIds.push(note.id);

    const stored = await uploadFile(
      note.id,
      "deletable.txt",
      "Delete me",
      "text/plain"
    );
    const fileId = stored.id || stored.file_id;

    // Delete using MCP tool
    const result = await client.callTool("delete_attachment", { id: fileId });
    assert.ok(result, "Delete should succeed");
  });
});
