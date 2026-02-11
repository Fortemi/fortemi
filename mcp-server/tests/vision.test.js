import { strict as assert } from "node:assert";
import { test, describe, before, after } from "node:test";
import { MCPTestClient } from "./helpers/mcp-client.js";

describe("Vision: Image Description (curl-command pattern)", () => {
  let client;

  before(async () => {
    client = new MCPTestClient();
    await client.initialize();
  });

  after(async () => {
    await client.close();
  });

  test("VIS-001: describe_image returns curl command with file_path", async () => {
    const result = await client.callTool("describe_image", {
      file_path: "/tmp/test-photo.jpg",
    });

    assert.ok(result.curl_command, "Should return a curl_command");
    assert.ok(result.upload_url, "Should return an upload_url");
    assert.ok(result.instructions, "Should return instructions");
    assert.ok(
      result.curl_command.includes("/tmp/test-photo.jpg"),
      "curl_command should include the file path"
    );
    assert.ok(
      result.curl_command.includes("/api/v1/vision/describe"),
      "curl_command should target the vision describe endpoint"
    );
    assert.ok(
      result.curl_command.includes("-F"),
      "curl_command should use multipart form flag"
    );
  });

  test("VIS-002: describe_image with mime_type", async () => {
    const result = await client.callTool("describe_image", {
      file_path: "/tmp/photo.png",
      mime_type: "image/png",
    });

    assert.ok(result.curl_command, "Should return a curl_command");
    assert.ok(
      result.curl_command.includes("type=image/png"),
      "curl_command should include the mime_type"
    );
  });

  test("VIS-003: describe_image with custom prompt", async () => {
    const result = await client.callTool("describe_image", {
      file_path: "/tmp/diagram.png",
      prompt: "Count the objects in this image",
    });

    assert.ok(result.curl_command, "Should return a curl_command");
    assert.ok(
      result.curl_command.includes("prompt="),
      "curl_command should include the prompt"
    );
  });

  test("VIS-004: describe_image upload_url is well-formed", async () => {
    const result = await client.callTool("describe_image", {
      file_path: "/tmp/test.webp",
    });

    assert.ok(
      result.upload_url.endsWith("/api/v1/vision/describe"),
      "upload_url should end with /api/v1/vision/describe"
    );
    assert.equal(result.method, "POST", "Method should be POST");
    assert.equal(
      result.content_type,
      "multipart/form-data",
      "Content type should be multipart/form-data"
    );
  });
});

describe("Audio: Transcription (curl-command pattern)", () => {
  let client;

  before(async () => {
    client = new MCPTestClient();
    await client.initialize();
  });

  after(async () => {
    await client.close();
  });

  test("AUD-001: transcribe_audio returns curl command with file_path", async () => {
    const result = await client.callTool("transcribe_audio", {
      file_path: "/tmp/recording.mp3",
    });

    assert.ok(result.curl_command, "Should return a curl_command");
    assert.ok(result.upload_url, "Should return an upload_url");
    assert.ok(result.instructions, "Should return instructions");
    assert.ok(
      result.curl_command.includes("/tmp/recording.mp3"),
      "curl_command should include the file path"
    );
    assert.ok(
      result.curl_command.includes("/api/v1/audio/transcribe"),
      "curl_command should target the audio transcribe endpoint"
    );
  });

  test("AUD-002: transcribe_audio with mime_type", async () => {
    const result = await client.callTool("transcribe_audio", {
      file_path: "/tmp/audio.wav",
      mime_type: "audio/wav",
    });

    assert.ok(result.curl_command, "Should return a curl_command");
    assert.ok(
      result.curl_command.includes("type=audio/wav"),
      "curl_command should include the mime_type"
    );
  });

  test("AUD-003: transcribe_audio with language hint", async () => {
    const result = await client.callTool("transcribe_audio", {
      file_path: "/tmp/speech.mp3",
      language: "es",
    });

    assert.ok(result.curl_command, "Should return a curl_command");
    assert.ok(
      result.curl_command.includes("language=es"),
      "curl_command should include the language hint"
    );
  });

  test("AUD-004: transcribe_audio upload_url is well-formed", async () => {
    const result = await client.callTool("transcribe_audio", {
      file_path: "/tmp/test.flac",
    });

    assert.ok(
      result.upload_url.endsWith("/api/v1/audio/transcribe"),
      "upload_url should end with /api/v1/audio/transcribe"
    );
    assert.equal(result.method, "POST", "Method should be POST");
    assert.equal(
      result.content_type,
      "multipart/form-data",
      "Content type should be multipart/form-data"
    );
  });
});
