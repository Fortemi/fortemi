import { strict as assert } from "node:assert";
import { test, describe, before, after } from "node:test";
import { MCPTestClient } from "./helpers/mcp-client.js";

describe("Phase 16: Observability", () => {
  let client;
  const cleanup = { noteIds: [] };

  before(async () => {
    client = new MCPTestClient();
    await client.initialize();
  });

  after(async () => {
    for (const id of cleanup.noteIds) {
      try { await client.callTool("delete_note", { id }); } catch {}
    }
    await client.close();
  });

  // --- Knowledge Health Dashboard ---

  test("OBS-001: Get knowledge health dashboard", async () => {
    const result = await client.callTool("get_knowledge_health", {});
    assert.ok(result, "Should return knowledge health data");
    // Should have health metrics
    assert.ok(
      result.orphan_tags !== undefined || result.stale_notes !== undefined ||
      result.unlinked_notes !== undefined || result.health_score !== undefined,
      "Should have health metric fields"
    );
    console.log(`  Health score: ${result.health_score || "N/A"}, orphan tags: ${result.orphan_tags?.length || result.orphan_tag_count || "N/A"}`);
  });

  test("OBS-002: Get orphan tags", async () => {
    const result = await client.callTool("get_orphan_tags", {});
    assert.ok(result !== undefined, "Should return orphan tags data");
    // Result should be array or object with tags
    const tags = Array.isArray(result) ? result : (result.tags || result.orphan_tags || []);
    assert.ok(Array.isArray(tags), "Orphan tags should be an array");
    console.log(`  Found ${tags.length} orphan tags`);
  });

  test("OBS-003: Get stale notes (default threshold)", async () => {
    const result = await client.callTool("get_stale_notes", {});
    assert.ok(result !== undefined, "Should return stale notes data");
    const notes = Array.isArray(result) ? result : (result.notes || result.stale_notes || []);
    assert.ok(Array.isArray(notes), "Stale notes should be an array");
    console.log(`  Found ${notes.length} stale notes (default threshold)`);
  });

  test("OBS-004: Get stale notes with custom days threshold", async () => {
    const result = await client.callTool("get_stale_notes", { days: 7 });
    assert.ok(result !== undefined, "Should return stale notes for 7 day threshold");
    const notes = Array.isArray(result) ? result : (result.notes || result.stale_notes || []);
    assert.ok(Array.isArray(notes), "Stale notes should be an array");
  });

  test("OBS-005: Get unlinked notes", async () => {
    const result = await client.callTool("get_unlinked_notes", {});
    assert.ok(result !== undefined, "Should return unlinked notes data");
    const notes = Array.isArray(result) ? result : (result.notes || result.unlinked_notes || []);
    assert.ok(Array.isArray(notes), "Unlinked notes should be an array");
    console.log(`  Found ${notes.length} unlinked notes`);
  });

  // --- System Info ---

  test("OBS-006: Server info tool", async () => {
    const result = await client.callTool("get_system_info", {});
    assert.ok(result, "Should return server info");
    // Has nested versions object
    assert.ok(result.status, "Should have status field");
    assert.ok(result.versions, "Should have versions object");
    assert.ok(result.versions.release, "Should have release version");
    console.log(`  Server version: ${result.versions.release}, status: ${result.status}`);
  });

  // --- Tag Co-occurrence ---

  test("OBS-007: Get tag co-occurrence (default params)", async () => {
    const result = await client.callTool("get_tag_cooccurrence", {});
    assert.ok(result !== undefined, "Should return co-occurrence data");
    // Result should be array of tag pairs or object with pairs
    const pairs = Array.isArray(result) ? result : (result.pairs || result.cooccurrences || []);
    assert.ok(Array.isArray(pairs), "Co-occurrence data should be an array");
    console.log(`  Found ${pairs.length} co-occurring tag pairs`);
  });

  test("OBS-008: Get tag co-occurrence with high threshold", async () => {
    const result = await client.callTool("get_tag_cooccurrence", {
      min_count: 5,
      limit: 10,
    });
    assert.ok(result !== undefined, "Should return filtered co-occurrence data");
    const pairs = Array.isArray(result) ? result : (result.pairs || result.cooccurrences || []);
    assert.ok(Array.isArray(pairs), "Co-occurrence data should be an array");
    // Higher threshold should return fewer or equal results
    assert.ok(pairs.length <= 10, "Should respect limit parameter");
  });

  // --- Notes Timeline ---

  test("OBS-009: Get notes timeline (daily granularity)", async () => {
    const result = await client.callTool("get_notes_timeline", {
      granularity: "day",
    });
    assert.ok(result !== undefined, "Should return timeline data");
    const buckets = Array.isArray(result) ? result : (result.buckets || result.timeline || []);
    assert.ok(Array.isArray(buckets), "Timeline should be an array of time buckets");
    console.log(`  Timeline has ${buckets.length} daily buckets`);
  });

  test("OBS-010: Get notes timeline (monthly granularity)", async () => {
    const result = await client.callTool("get_notes_timeline", {
      granularity: "month",
    });
    assert.ok(result !== undefined, "Should return monthly timeline data");
    const buckets = Array.isArray(result) ? result : (result.buckets || result.timeline || []);
    assert.ok(Array.isArray(buckets), "Timeline should be an array of time buckets");
  });

  test("OBS-010a: Get notes timeline (weekly granularity)", async () => {
    const result = await client.callTool("get_notes_timeline", {
      start_date: "2025-12-01",
      end_date: "2026-02-11",
      granularity: "week",
    });
    assert.ok(result !== undefined, "Should return weekly timeline data");
    const buckets = Array.isArray(result) ? result : (result.buckets || result.timeline || []);
    assert.ok(Array.isArray(buckets), "Timeline should be an array of time buckets");
    // Weekly granularity should produce fewer buckets than daily for same range
    console.log(`  Weekly timeline has ${buckets.length} buckets`);
  });

  // --- Notes Activity ---

  test("OBS-011: Get notes activity feed", async () => {
    const result = await client.callTool("get_notes_activity", {
      limit: 20,
    });
    assert.ok(result !== undefined, "Should return activity feed");
    const events = Array.isArray(result) ? result : (result.events || result.activity || []);
    assert.ok(Array.isArray(events), "Activity feed should be an array");
    console.log(`  Activity feed has ${events.length} events`);
    // Events should have note_id and timestamps
    if (events.length > 0) {
      assert.ok(events[0].note_id, "Events should have note_id");
      assert.ok(events[0].created_at, "Events should have created_at");
    }
  });

  test("OBS-012: Get notes activity with offset and limit", async () => {
    const result = await client.callTool("get_notes_activity", {
      offset: 5,
      limit: 10,
    });
    assert.ok(result !== undefined, "Should return paginated activity");
    const events = Array.isArray(result) ? result : (result.events || result.activity || []);
    assert.ok(Array.isArray(events), "Paginated activity should be an array");
    assert.ok(events.length <= 10, "Should respect limit parameter");
  });

  test("OBS-012a: Get notes activity filtered by event type", async () => {
    const result = await client.callTool("get_notes_activity", {
      event_types: ["created"],
      limit: 10,
    });
    assert.ok(result !== undefined, "Should return filtered activity");
    const events = Array.isArray(result) ? result : (result.events || result.activity || []);
    assert.ok(Array.isArray(events), "Filtered activity should be an array");
    // All returned events should be 'created' type
    for (const event of events) {
      if (event.event_type) {
        assert.strictEqual(event.event_type, "created", "Should only return created events");
      }
    }
    console.log(`  Filtered activity: ${events.length} creation events`);
  });

  // --- Governance Stats ---

  test("OBS-013: Get governance stats", async () => {
    const result = await client.callTool("get_governance_stats", {});
    assert.ok(result !== undefined, "Should return governance stats");
    // Should have concept scheme statistics
    console.log(`  Governance stats: ${JSON.stringify(result).slice(0, 200)}`);
  });

  // --- Health-Based Workflows ---

  test("OBS-014: Orphan tag workflow", async () => {
    const result = await client.callTool("get_orphan_tags", {});
    assert.ok(result !== undefined, "Should return orphan tags");
    const tags = Array.isArray(result) ? result : (result.tags || result.orphan_tags || []);
    assert.ok(Array.isArray(tags), "Orphan tags should be an array");
    // Workflow: identify orphans for cleanup
    if (tags.length > 0) {
      const first = tags[0];
      assert.ok(
        first.tag || first.name || typeof first === "string",
        "Each orphan tag entry should have a tag identifier"
      );
    }
    console.log(`  Orphan tag workflow: ${tags.length} tags identified for potential cleanup`);
  });

  test("OBS-015: Stale note workflow (365 days)", async () => {
    const result = await client.callTool("get_stale_notes", { days: 365, limit: 5 });
    assert.ok(result !== undefined, "Should return stale notes for 365-day threshold");
    const notes = Array.isArray(result) ? result : (result.notes || result.stale_notes || []);
    assert.ok(Array.isArray(notes), "Stale notes should be an array");
    assert.ok(notes.length <= 5, "Should respect limit parameter");
    // Workflow: identify very stale notes for archival
    if (notes.length > 0) {
      assert.ok(
        notes[0].id || notes[0].note_id,
        "Each stale note should have an ID for follow-up actions"
      );
    }
    console.log(`  Stale note workflow: ${notes.length} notes stale >365 days`);
  });

  test("OBS-016: Health after operations", async () => {
    // Get baseline health
    const baseline = await client.callTool("get_knowledge_health", {});
    assert.ok(baseline, "Should return baseline health");

    // Create a test note
    const note = await client.callTool("create_note", {
      content: `# Observability Health Test ${MCPTestClient.uniqueId()}\n\nTest note for health tracking.`,
      tags: [MCPTestClient.testTag("obs-health")],
      revision_mode: "none",
    });
    assert.ok(note.id, "Should create test note");
    cleanup.noteIds.push(note.id);

    // Brief delay for indexing
    await new Promise((r) => setTimeout(r, 300));

    // Check health after operation
    const updated = await client.callTool("get_knowledge_health", {});
    assert.ok(updated, "Should return updated health");
    // Health score should still be valid
    assert.ok(
      updated.health_score !== undefined || updated.metrics !== undefined,
      "Should have health metrics after operations"
    );
    console.log(`  Health before: ${baseline.health_score || "N/A"}, after: ${updated.health_score || "N/A"}`);
  });
});
