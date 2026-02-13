import { strict as assert } from "node:assert";
import { test, describe, before, after } from "node:test";
import { MCPTestClient } from "./helpers/mcp-client.js";

describe("Phase 22: Video Processing (guidance tool)", () => {
  let client;

  before(async () => {
    client = new MCPTestClient();
    await client.initialize();
  });

  after(async () => {
    await client.close();
  });

  test("VID-001: process_video returns workflow guidance without note_id", async () => {
    const result = await client.callTool("process_video", {});

    assert.strictEqual(result.workflow, "attachment_pipeline", "Should use attachment_pipeline workflow");
    assert.ok(result.message, "Should return a message");
    assert.ok(result.message.includes("attachment pipeline"), "Message should mention attachment pipeline");
    assert.ok(Array.isArray(result.steps), "Should return steps array");
    assert.ok(result.steps.length >= 4, "Should have at least 4 steps (create note + upload + curl + wait + check)");
    assert.ok(result.steps[0].includes("create_note"), "First step should create a note when no note_id");
  });

  test("VID-002: process_video returns workflow guidance with note_id", async () => {
    const result = await client.callTool("process_video", {
      note_id: "test-note-123",
    });

    assert.strictEqual(result.workflow, "attachment_pipeline");
    assert.ok(Array.isArray(result.steps));
    assert.ok(result.steps[0].includes("upload_attachment"), "First step should be upload when note_id provided");
    assert.ok(result.steps[0].includes("test-note-123"), "Step should include the provided note_id");
  });

  test("VID-003: process_video returns workflow guidance with filename", async () => {
    const result = await client.callTool("process_video", {
      filename: "interview.mp4",
    });

    assert.ok(result.steps[0].includes("interview.mp4"), "Steps should include the provided filename");
  });

  test("VID-004: process_video returns supported_formats list", async () => {
    const result = await client.callTool("process_video", {});

    assert.ok(Array.isArray(result.supported_formats), "Should return supported_formats array");
    assert.ok(result.supported_formats.length >= 4, "Should support at least 4 video formats");

    // Verify core video formats
    const formats = result.supported_formats;
    assert.ok(formats.includes("video/mp4"), "Should support MP4");
    assert.ok(formats.includes("video/webm"), "Should support WebM");
    assert.ok(formats.includes("video/quicktime"), "Should support QuickTime/MOV");
    assert.ok(formats.includes("video/ogg"), "Should support OGG");
  });

  test("VID-005: process_video returns extended format support", async () => {
    const result = await client.callTool("process_video", {});
    const formats = result.supported_formats;

    // Extended formats
    assert.ok(formats.includes("video/x-msvideo"), "Should support AVI");
    assert.ok(formats.includes("video/x-matroska"), "Should support MKV");
    assert.ok(formats.includes("video/x-flv"), "Should support FLV");
    assert.ok(formats.includes("video/x-ms-wmv"), "Should support WMV");
  });

  test("VID-006: process_video returns requirements", async () => {
    const result = await client.callTool("process_video", {});

    assert.ok(result.requires, "Should return requires object");
    assert.ok(result.requires.ffmpeg, "Should specify ffmpeg requirement");
    assert.ok(result.requires.vision_model, "Should specify vision_model requirement");
    assert.ok(result.requires.whisper, "Should specify whisper requirement");
  });

  test("VID-007: process_video returns extraction features", async () => {
    const result = await client.callTool("process_video", {});

    assert.ok(result.extraction_features, "Should return extraction_features object");
    assert.ok(result.extraction_features.keyframe_extraction, "Should describe keyframe extraction");
    assert.ok(result.extraction_features.frame_description, "Should describe frame description");
    assert.ok(result.extraction_features.audio_transcription, "Should describe audio transcription");
    assert.ok(result.extraction_features.temporal_alignment, "Should describe temporal alignment");
  });

  test("VID-008: process_video with note_id and filename", async () => {
    const result = await client.callTool("process_video", {
      note_id: "note-abc",
      filename: "lecture.webm",
    });

    assert.strictEqual(result.workflow, "attachment_pipeline");
    assert.ok(result.steps[0].includes("note-abc"), "Should include note_id");
    assert.ok(result.steps[0].includes("lecture.webm"), "Should include filename");
    // When note_id is provided, first step is upload (not create_note)
    assert.ok(result.steps[0].includes("upload_attachment"), "First step should be upload");
  });
});

describe("Phase 22: 3D Model Processing (guidance tool)", () => {
  let client;

  before(async () => {
    client = new MCPTestClient();
    await client.initialize();
  });

  after(async () => {
    await client.close();
  });

  test("3D-001: process_3d_model returns workflow guidance without note_id", async () => {
    const result = await client.callTool("process_3d_model", {});

    assert.strictEqual(result.workflow, "attachment_pipeline", "Should use attachment_pipeline workflow");
    assert.ok(result.message, "Should return a message");
    assert.ok(result.message.includes("attachment pipeline"), "Message should mention attachment pipeline");
    assert.ok(Array.isArray(result.steps), "Should return steps array");
    assert.ok(result.steps.length >= 4, "Should have at least 4 steps");
    assert.ok(result.steps[0].includes("create_note"), "First step should create a note when no note_id");
  });

  test("3D-002: process_3d_model returns workflow guidance with note_id", async () => {
    const result = await client.callTool("process_3d_model", {
      note_id: "model-note-456",
    });

    assert.strictEqual(result.workflow, "attachment_pipeline");
    assert.ok(Array.isArray(result.steps));
    assert.ok(result.steps[0].includes("upload_attachment"), "First step should be upload when note_id provided");
    assert.ok(result.steps[0].includes("model-note-456"), "Step should include the provided note_id");
  });

  test("3D-003: process_3d_model returns workflow guidance with filename", async () => {
    const result = await client.callTool("process_3d_model", {
      filename: "scene.glb",
    });

    assert.ok(result.steps[0].includes("scene.glb"), "Steps should include the provided filename");
  });

  test("3D-004: process_3d_model returns supported_formats list", async () => {
    const result = await client.callTool("process_3d_model", {});

    assert.ok(Array.isArray(result.supported_formats), "Should return supported_formats array");
    assert.ok(result.supported_formats.length >= 6, "Should support at least 6 3D model formats");

    // Verify core 3D formats
    const formats = result.supported_formats;
    assert.ok(formats.includes("model/gltf-binary"), "Should support GLB");
    assert.ok(formats.includes("model/gltf+json"), "Should support GLTF");
    assert.ok(formats.includes("model/obj"), "Should support OBJ");
    assert.ok(formats.includes("model/stl"), "Should support STL");
  });

  test("3D-005: process_3d_model returns extended format support", async () => {
    const result = await client.callTool("process_3d_model", {});
    const formats = result.supported_formats;

    assert.ok(formats.includes("model/fbx"), "Should support FBX");
    assert.ok(formats.includes("model/ply"), "Should support PLY");
    assert.ok(formats.includes("model/step"), "Should support STEP");
    assert.ok(formats.includes("model/iges"), "Should support IGES");
    assert.ok(formats.includes("model/vnd.usdz+zip"), "Should support USDZ");
  });

  test("3D-006: process_3d_model returns requirements", async () => {
    const result = await client.callTool("process_3d_model", {});

    assert.ok(result.requires, "Should return requires object");
    assert.ok(result.requires.renderer, "Should specify renderer requirement");
    assert.ok(result.requires.renderer.includes("Three.js"), "Renderer should mention Three.js");
    assert.ok(result.requires.vision_model, "Should specify vision_model requirement");
  });

  test("3D-007: process_3d_model returns extraction features", async () => {
    const result = await client.callTool("process_3d_model", {});

    assert.ok(result.extraction_features, "Should return extraction_features object");
    assert.ok(result.extraction_features.multi_view_rendering, "Should describe multi-view rendering");
    assert.ok(result.extraction_features.view_description, "Should describe view description");
    assert.ok(result.extraction_features.composite_description, "Should describe composite description");
  });

  test("3D-008: process_3d_model with note_id and filename", async () => {
    const result = await client.callTool("process_3d_model", {
      note_id: "note-xyz",
      filename: "character.fbx",
    });

    assert.strictEqual(result.workflow, "attachment_pipeline");
    assert.ok(result.steps[0].includes("note-xyz"), "Should include note_id");
    assert.ok(result.steps[0].includes("character.fbx"), "Should include filename");
    assert.ok(result.steps[0].includes("upload_attachment"), "First step should be upload");
  });
});
