import { strict as assert } from "node:assert";
import { test, describe, before, after } from "node:test";
import { MCPTestClient } from "./helpers/mcp-client.js";
import { readFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));

describe("Vision: Image Description", () => {
  let client;
  let visionAvailable = false;

  before(async () => {
    client = new MCPTestClient();
    await client.initialize();

    // Check if vision backend is configured
    const info = await client.callTool("get_system_info", {});
    visionAvailable =
      info.extraction &&
      info.extraction.vision &&
      info.extraction.vision.available === true;

    if (!visionAvailable) {
      console.log(
        "SKIP: Vision backend not available (OLLAMA_VISION_MODEL not set)"
      );
    }
  });

  after(async () => {
    await client.close();
  });

  test("VIS-001: Describe a JPEG image", async (t) => {
    if (!visionAvailable) return t.skip("Vision backend not available");

    const imagePath = join(
      __dirname,
      "../tests/uat/data/images/object-scene.jpg"
    );
    const imageData = readFileSync(imagePath).toString("base64");

    const result = await client.callTool("describe_image", {
      image_data: imageData,
      mime_type: "image/jpeg",
    });

    assert.ok(result.description, "Should return a description");
    assert.ok(
      result.description.length > 10,
      "Description should be non-trivial"
    );
    assert.ok(result.model, "Should return the model name");
    assert.ok(result.image_size > 0, "Should return image size in bytes");
  });

  test("VIS-002: Describe a PNG image", async (t) => {
    if (!visionAvailable) return t.skip("Vision backend not available");

    const imagePath = join(
      __dirname,
      "../tests/uat/data/images/png-transparent.png"
    );
    const imageData = readFileSync(imagePath).toString("base64");

    const result = await client.callTool("describe_image", {
      image_data: imageData,
      mime_type: "image/png",
    });

    assert.ok(result.description, "Should return a description");
    assert.ok(result.model, "Should return the model name");
  });

  test("VIS-003: Custom prompt for image analysis", async (t) => {
    if (!visionAvailable) return t.skip("Vision backend not available");

    const imagePath = join(
      __dirname,
      "../tests/uat/data/images/faces-group-photo.jpg"
    );
    const imageData = readFileSync(imagePath).toString("base64");

    const result = await client.callTool("describe_image", {
      image_data: imageData,
      mime_type: "image/jpeg",
      prompt: "How many people are in this image? Describe each person briefly.",
    });

    assert.ok(result.description, "Should return a description");
    assert.ok(
      result.description.length > 20,
      "Custom prompt should produce detailed response"
    );
  });

  test("VIS-004: Describe a WebP image", async (t) => {
    if (!visionAvailable) return t.skip("Vision backend not available");

    const imagePath = join(
      __dirname,
      "../tests/uat/data/images/webp-modern.webp"
    );
    const imageData = readFileSync(imagePath).toString("base64");

    const result = await client.callTool("describe_image", {
      image_data: imageData,
      mime_type: "image/webp",
    });

    assert.ok(result.description, "Should return a description");
    assert.ok(result.model, "Should return the model name");
  });

  test("VIS-005: Invalid base64 returns error", async (t) => {
    if (!visionAvailable) return t.skip("Vision backend not available");

    try {
      await client.callTool("describe_image", {
        image_data: "not-valid-base64!!!",
        mime_type: "image/png",
      });
      assert.fail("Should have thrown an error");
    } catch (err) {
      assert.ok(
        err.message.includes("base64") || err.message.includes("Invalid"),
        "Error should mention invalid base64"
      );
    }
  });

  test("VIS-006: Empty image data returns error", async (t) => {
    if (!visionAvailable) return t.skip("Vision backend not available");

    try {
      // Empty string base64-encodes to empty bytes
      await client.callTool("describe_image", {
        image_data: "",
        mime_type: "image/png",
      });
      assert.fail("Should have thrown an error");
    } catch (err) {
      assert.ok(err.message, "Should return an error message");
    }
  });

  test("VIS-007: Default mime_type works (omitted)", async (t) => {
    if (!visionAvailable) return t.skip("Vision backend not available");

    const imagePath = join(
      __dirname,
      "../tests/uat/data/images/png-transparent.png"
    );
    const imageData = readFileSync(imagePath).toString("base64");

    // Omit mime_type — should default to image/png
    const result = await client.callTool("describe_image", {
      image_data: imageData,
    });

    assert.ok(result.description, "Should return a description");
  });

  test("VIS-008: Vision not configured returns 503", async (t) => {
    // This test validates the error message when vision is disabled.
    // If vision IS available, we skip (we can't test disabled state).
    if (visionAvailable) return t.skip("Vision is available — cannot test disabled state");

    try {
      await client.callTool("describe_image", {
        image_data: "aGVsbG8=", // "hello" in base64
        mime_type: "image/png",
      });
      assert.fail("Should have thrown an error");
    } catch (err) {
      assert.ok(
        err.message.includes("not configured") ||
          err.message.includes("OLLAMA_VISION_MODEL"),
        "Error should mention vision model not configured"
      );
    }
  });
});
