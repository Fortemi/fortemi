# ADR-082: Queue-Based Tier Escalation for Model Fallback

**Status:** Accepted
**Date:** 2026-02-18
**Deciders:** roctinam

## Context

Inference jobs (title generation, concept extraction) were previously handled with inline model fallback: if the primary model failed, the handler would immediately call the secondary model synchronously before returning. This created several problems:

1. **VRAM contention:** Two models (e.g., fast `granite4:3b` and standard `gpt-oss:20b`) would be loaded into VRAM simultaneously during fallback, exceeding available GPU memory.
2. **Long-running jobs:** Fallback extended job execution time unpredictably; the worker's timeout could trigger before the secondary call completed.
3. **Lost observability:** Inline retries were invisible to the job queue — the job appeared to be running for its full duration with no record of the escalation.
4. **No tier separation:** The worker's tiered drain loop (CPU → fast GPU → standard GPU) was bypassed when models were called inline rather than through the queue.

## Decision

Replace inline model fallback with queue-based tier escalation: when a job on tier-1 (fast GPU) fails or produces insufficient output, queue a new job at tier-2 (standard GPU) rather than calling the secondary model in the same execution context.

**Pattern:**

1. Tier-1 job (`cost_tier = FAST_GPU`) runs using the fast model.
2. On failure or insufficient quality (e.g., fewer concepts than target), the handler calls `queue_escalation(note_id, schema, STANDARD_GPU, prior_tier = FAST_GPU)`.
3. The escalation job carries `prior_tier` in its payload so the tier-2 handler knows the fast model was already attempted.
4. The tier-1 job completes (with partial results if any). The tier-2 job is processed after the fast-GPU drain completes.
5. Tier-2 job runs using the standard model.

**Escalation thresholds for concept extraction:**

- GLiNER → tier-1 (fast GPU): escalate when concept count < target (default 5)
- Tier-1 → tier-2 (standard GPU): escalate when concept count < target/2 (default 3)

**Title generation escalation:**

- Tier-1 → tier-2: escalate on any fast model failure or invalid title output.

**Alternatives Considered:**

| Alternative | Rejected Because |
|-------------|-----------------|
| Inline synchronous fallback | VRAM contention; long job duration; bypasses tier drain ordering |
| Retry the same tier | Same model failure is likely to repeat; no quality improvement |
| Always use the expensive model | Wasteful; most jobs succeed at tier-1; doubles GPU hours |
| Configurable fallback list in handler | Still synchronous; doesn't fix VRAM or timeout issues |

## Consequences

### Positive
- (+) VRAM usage is bounded: only one generation model loaded per tier drain phase
- (+) Escalated jobs are visible in the queue with `escalated: true` payload metadata
- (+) Tier-2 jobs run after tier-1 drain is complete (natural ordering via tiered drain loop)
- (+) Each job completes quickly (one model attempt); no unpredictable long-running jobs
- (+) Threshold-based escalation allows partial results to be accepted at tier-1

### Negative
- (-) End-to-end latency for escalated jobs increases: tier-2 job waits behind full tier-2 queue
- (-) A note may have incomplete results between tier-1 completion and tier-2 execution
- (-) Escalation logic is replicated in each handler that supports it (title, concepts)
- (-) Payload must carry enough context for tier-2 to continue from where tier-1 left off

## Implementation

**Code Location:**
- Worker drain loop: `crates/matric-jobs/src/worker.rs` (`Worker::run`)
- Title escalation: `crates/matric-api/src/handlers/jobs.rs` (`TitleGenerationHandler::queue_tier_escalation`)
- Concept escalation: `crates/matric-api/src/handlers/jobs.rs` (`ExtractionHandler::queue_escalation`)
- Tier constants: `crates/matric-core/src/defaults.rs` (`cost_tier::FAST_GPU`, `cost_tier::STANDARD_GPU`, `cost_tier::CPU_NER`)

**Tier Assignment:**

| Tier | `cost_tier` Value | Model | Jobs |
|------|------------------|-------|------|
| CPU/agnostic | NULL or 0 | CPU models (GLiNER, embeddings) | NER, embedding |
| Fast GPU | 1 | `granite4:3b` (or configured fast model) | Title gen, concept extraction |
| Standard GPU | 2 | `gpt-oss:20b` (or configured standard model) | Escalated title/concepts |

**Escalation Payload:**

```json
{
  "escalated": true,
  "prior_tier": 1,
  "prior_concepts": ["concept1", "concept2"]
}
```

## References

- ADR-073: Graph Quality Pipeline Architecture (GraphMaintenance job type)
- ADR-079: Global Job Deduplication by Job Type
- `crates/matric-jobs/src/worker.rs` — tiered drain loop
- `crates/matric-api/src/handlers/jobs.rs` — handler implementations
