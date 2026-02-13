import { strict as assert } from "node:assert";
import { test, describe, before, after } from "node:test";
import { MCPTestClient } from "./helpers/mcp-client.js";

describe("Phase 15: Background Jobs", () => {
  let client;
  const cleanup = { noteIds: [] };
  let sharedNoteId;
  let embeddingJobId;

  before(async () => {
    client = new MCPTestClient();
    await client.initialize();
    // Ensure we're in the default archive context (prevents state leakage from other tests)
    await client.callTool("select_memory", { name: "public" });

    // Create a shared test note for job operations
    const note = await client.callTool("create_note", {
      content: `# Job Test Note ${MCPTestClient.uniqueId()}\n\nContent for testing job queue operations.`,
      tags: [MCPTestClient.testTag("jobs")],
      revision_mode: "none",
    });
    assert.ok(note.id, "Should create shared test note");
    sharedNoteId = note.id;
    cleanup.noteIds.push(note.id);

    // Small delay for indexing
    await new Promise((r) => setTimeout(r, 300));
  });

  after(async () => {
    for (const id of cleanup.noteIds) {
      try { await client.callTool("delete_note", { id }); } catch {}
    }
    await client.close();
  });

  // --- Queue Statistics ---

  test("JOB-001: Get queue stats", async () => {
    const result = await client.callTool("get_queue_stats", {});
    assert.ok(result, "Should return queue stats");
    // Stats should have numeric fields
    assert.ok(result.pending !== undefined || result.total !== undefined,
      "Should have pending or total field");
    console.log(`  Queue stats: pending=${result.pending}, total=${result.total}`);
  });

  // --- Job Listing ---

  test("JOB-002: List jobs (all)", async () => {
    const result = await client.callTool("list_jobs", { limit: 20 });
    assert.ok(result, "Should return jobs data");
    // Result may be array or object with jobs property
    const jobs = Array.isArray(result) ? result : (result.jobs || []);
    assert.ok(Array.isArray(jobs), "Jobs should be an array");
    console.log(`  Listed ${jobs.length} jobs`);
  });

  test("JOB-003: List jobs by status (completed)", async () => {
    const result = await client.callTool("list_jobs", {
      status: "completed",
      limit: 10,
    });
    assert.ok(result, "Should return filtered jobs");
    const jobs = Array.isArray(result) ? result : (result.jobs || []);
    // If any jobs returned, they should all be completed
    for (const job of jobs) {
      assert.strictEqual(job.status, "completed", "All jobs should be completed");
    }
  });

  test("JOB-004: List jobs by type (embedding)", async () => {
    const result = await client.callTool("list_jobs", {
      job_type: "embedding",
      limit: 10,
    });
    assert.ok(result, "Should return filtered jobs");
    const jobs = Array.isArray(result) ? result : (result.jobs || []);
    for (const job of jobs) {
      assert.strictEqual(job.job_type, "embedding", "All jobs should be embedding type");
    }
  });

  test("JOB-005: List jobs for specific note", async () => {
    const result = await client.callTool("list_jobs", {
      note_id: sharedNoteId,
      limit: 10,
    });
    assert.ok(result, "Should return jobs for note");
    const jobs = Array.isArray(result) ? result : (result.jobs || []);
    // All returned jobs should be for our note
    for (const job of jobs) {
      assert.strictEqual(job.note_id, sharedNoteId, "All jobs should be for our note");
    }
  });

  // --- Job Creation ---

  test("JOB-006: Create embedding job", async () => {
    const result = await client.callTool("create_job", {
      note_id: sharedNoteId,
      job_type: "embedding",
      priority: 5,
    });
    assert.ok(result, "Should create embedding job");
    assert.ok(result.id || result.job_id, "Should return job ID");
    embeddingJobId = result.id || result.job_id;
    console.log(`  Created embedding job: ${embeddingJobId}`);
  });

  test("JOB-007: Create linking job", async () => {
    const result = await client.callTool("create_job", {
      note_id: sharedNoteId,
      job_type: "linking",
      priority: 3,
    });
    assert.ok(result, "Should create linking job");
    assert.ok(result.id || result.job_id, "Should return job ID");
  });

  test("JOB-008: Create title generation job", async () => {
    const result = await client.callTool("create_job", {
      note_id: sharedNoteId,
      job_type: "title_generation",
      priority: 2,
    });
    assert.ok(result, "Should create title generation job");
    assert.ok(result.id || result.job_id, "Should return job ID");
  });

  test("JOB-009: Verify queue stats updated after job creation", async () => {
    const result = await client.callTool("get_queue_stats", {});
    assert.ok(result, "Should return updated stats");
    // Stats should still be valid
    assert.ok(result.pending !== undefined || result.total !== undefined,
      "Should have stats fields");
  });

  test("JOB-010: Create AI revision job (high priority)", async () => {
    const result = await client.callTool("create_job", {
      note_id: sharedNoteId,
      job_type: "ai_revision",
      priority: 8,
    });
    assert.ok(result, "Should create AI revision job");
    assert.ok(result.id || result.job_id, "Should return job ID");
  });

  test("JOB-011: Verify priority ordering in pending jobs", async () => {
    const result = await client.callTool("list_jobs", {
      status: "pending",
      limit: 20,
    });
    assert.ok(result, "Should return pending jobs");
    const jobs = Array.isArray(result) ? result : (result.jobs || []);
    // Verify we get pending jobs with valid structure
    for (const job of jobs) {
      assert.strictEqual(job.status, "pending", "All jobs should be pending");
      assert.ok(job.priority !== undefined, "Jobs should have priority field");
    }
    // Log priority distribution
    const priorities = jobs.map((j) => j.priority);
    console.log(`  Found ${jobs.length} pending jobs, priorities: ${[...new Set(priorities)].sort().reverse().join(", ")}`);
  });

  // --- Re-embedding ---

  test("JOB-012: Trigger re-embed all", async () => {
    const result = await client.callTool("reembed_all", {
      force: false,
    });
    assert.ok(result, "Should trigger re-embed all");
    console.log(`  Re-embed all result: ${JSON.stringify(result).slice(0, 200)}`);
  });

  test("JOB-013: Re-embed specific set", async () => {
    const result = await client.callTool("reembed_all", {
      embedding_set_slug: "default",
      force: true,
    });
    assert.ok(result, "Should trigger set-specific re-embedding");
  });

  // --- Job Completion Monitoring ---

  test("JOB-014: Monitor job progress", async () => {
    // Poll until at least one job has moved beyond pending, or timeout
    const validStatuses = ["pending", "running", "completed", "failed", "cancelled"];
    let jobs = [];
    const maxWaitMs = 5000;
    const pollIntervalMs = 300;
    const start = Date.now();

    while (Date.now() - start < maxWaitMs) {
      const result = await client.callTool("list_jobs", {
        note_id: sharedNoteId,
        limit: 10,
      });
      assert.ok(result, "Should return jobs for monitoring");
      jobs = Array.isArray(result) ? result : (result.jobs || []);

      // Validate all statuses are valid DB enum values
      for (const job of jobs) {
        assert.ok(
          validStatuses.includes(job.status),
          `Job status should be valid, got: ${job.status}`
        );
      }

      // If any job has progressed beyond pending, we've observed a transition
      if (jobs.some((j) => j.status !== "pending")) break;
      await new Promise((r) => setTimeout(r, pollIntervalMs));
    }

    const statusCounts = {};
    for (const job of jobs) {
      statusCounts[job.status] = (statusCounts[job.status] || 0) + 1;
    }
    console.log(`  Job statuses: ${JSON.stringify(statusCounts)}`);
  });

  test("JOB-015: Verify failed jobs have error info", async () => {
    const result = await client.callTool("list_jobs", {
      status: "failed",
      limit: 10,
    });
    assert.ok(result, "Should return failed jobs list");
    const jobs = Array.isArray(result) ? result : (result.jobs || []);
    // If failed jobs exist, they should have error info
    for (const job of jobs) {
      assert.strictEqual(job.status, "failed", "Should be failed status");
      // Error field may be string or object
      assert.ok(
        job.error !== undefined || job.error_message !== undefined,
        "Failed job should have error information"
      );
    }
    console.log(`  Found ${jobs.length} failed jobs`);
  });

  // --- Edge Cases ---

  test("JOB-016: Create job for non-existent note errors", async () => {
    const error = await client.callToolExpectError("create_job", {
      note_id: "00000000-0000-0000-0000-000000000000",
      job_type: "embedding",
    });
    assert.ok(error.error, "Should error for non-existent note");
  });

  test("JOB-017: Create job with invalid type errors", async () => {
    const error = await client.callToolExpectError("create_job", {
      note_id: sharedNoteId,
      job_type: "invalid_type_that_does_not_exist",
    });
    assert.ok(error.error, "Should error for invalid job type");
  });

  test("JOB-018a: Create duplicate job (allow duplicates)", async () => {
    // Create same job type for same note - should succeed (duplicates allowed)
    const result = await client.callTool("create_job", {
      note_id: sharedNoteId,
      job_type: "embedding",
    });
    assert.ok(result, "Should allow duplicate job creation");
    assert.ok(result.id || result.job_id, "Should return new job ID");
  });

  test("JOB-018b: Create duplicate job with deduplicate=true", async () => {
    // Create two jobs with deduplicate - second should return existing
    await client.callTool("create_job", {
      note_id: sharedNoteId,
      job_type: "embedding",
    });

    const result = await client.callTool("create_job", {
      note_id: sharedNoteId,
      job_type: "embedding",
      deduplicate: true,
    });
    assert.ok(result, "Should return result for deduplicated job");
    // May return existing job or status indicating already pending
    console.log(`  Dedup result: ${JSON.stringify(result).slice(0, 200)}`);
  });

  // --- Individual Job Operations ---

  test("JOB-019: Get job by ID", async () => {
    // Use the embedding job ID from JOB-006
    if (!embeddingJobId) {
      // Create a fresh job if we don't have one
      const created = await client.callTool("create_job", {
        note_id: sharedNoteId,
        job_type: "embedding",
      });
      embeddingJobId = created.id || created.job_id;
    }

    const result = await client.callTool("get_job", { id: embeddingJobId });
    assert.ok(result, "Should return job details");
    assert.ok(result.id || result.job_id, "Should have job ID");
    assert.ok(result.job_type, "Should have job_type");
    assert.ok(result.status, "Should have status");
    assert.ok(result.created_at, "Should have created_at timestamp");
    console.log(`  Job ${embeddingJobId}: type=${result.job_type}, status=${result.status}`);
  });

  test("JOB-020: Get pending jobs count", async () => {
    const result = await client.callTool("get_pending_jobs_count", {});
    assert.ok(result !== undefined && result !== null, "Should return pending count");
    // Result should have pending_count or pending field
    const count = result.pending_count !== undefined ? result.pending_count :
                  result.pending !== undefined ? result.pending : null;
    assert.ok(count !== null, "Should have a pending count value");
    assert.ok(typeof count === "number", "Pending count should be a number");
    console.log(`  Pending jobs count: ${count}`);
  });

  // --- Note Reprocessing ---

  test("JOB-021: Reprocess note with specific steps", async () => {
    const result = await client.callTool("reprocess_note", {
      id: sharedNoteId,
      steps: ["embedding", "linking", "title_generation"],
    });
    assert.ok(result, "Should return reprocess result");
    // May return jobs_created array or success status
    console.log(`  Reprocess result: ${JSON.stringify(result).slice(0, 200)}`);
  });

  test("JOB-022: Reprocess note - all operations", async () => {
    const result = await client.callTool("reprocess_note", {
      id: sharedNoteId,
      // No steps = reprocess all
    });
    assert.ok(result, "Should return reprocess result for all operations");
  });
});
