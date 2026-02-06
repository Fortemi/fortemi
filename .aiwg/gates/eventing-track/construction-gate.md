# Construction Gate Check

**Track**: Eventing, Streaming & Telemetry
**Epic**: #37
**Gate**: Elaboration → Construction
**Date**: 2026-02-05
**Reviewer**: Flow Gate Orchestrator

---

## Gate Criteria

### Issues #38-#46: Eventing Development Track

| Criterion | Status | Notes |
|-----------|--------|-------|
| Requirements documented | PASS | Vision, scope boundaries, 5 use case briefs, 9 detailed issues |
| Architecture designed | PASS | SAD (eventing extension), ADR-037 (unified event bus) |
| Component structure defined | PASS | matric-core (events), matric-api (handlers), matric-db (webhooks) |
| Existing infrastructure validated | PASS | WorkerEvent broadcast, SSE transport, nginx WS headers verified |
| Risk retirement >=70% | PASS | 82% (9/11 mitigated, 2 accepted) |
| Test strategy defined | PASS | Master test plan with 64 test cases, coverage targets |
| Dependencies identified | PASS | axum[ws], tokio-tungstenite, wiremock, reqwest |
| Issue backlog with dependency graph | PASS | 9 issues with 5-iteration parallel execution plan |

### Pre-Construction Checklist

| Check | Status | Notes |
|-------|--------|-------|
| LOM gate passed | PASS | `lom-report.md` — all inception criteria met |
| ABM gate passed | PASS | `elaboration-gate.md` — all elaboration criteria met |
| No unresolved show-stoppers | PASS | R-EVT-001 mitigated by auth pattern in #42 |
| Architecture decision documented | PASS | ADR-037 accepted |
| Test plan approved | PASS | `master-test-plan.md` baselined |
| CI pipeline supports new tests | PASS | Existing `.gitea/workflows/test.yml` handles Rust tests |
| HotM client protocol documented | PASS | 7 message types specified in issues + vision |

---

**Result: PASS - Ready for Construction**

---

## Construction Plan

### RALPH LOOP Execution Order

Per the development track parallel execution strategy:

**Iteration 1** (Foundation — CRITICAL PATH):
- #38 Event Bus Foundation: ServerEvent enum + EventBus in matric-core

**Iteration 2** (Core Transports — parallel):
- #39 WebSocket Endpoint: Axum WS handler at /api/v1/ws
- #40 Job Event Bridge: WorkerEvent → ServerEvent bridge task
- #43 MCP Event Forwarding: SSE endpoint at /api/v1/events

**Iteration 3** (Event Sources):
- #41 Note Event Emission: NoteUpdated from CRUD handlers

**Iteration 4** (Hardening — parallel):
- #42 Connection Management: Auth, heartbeat, tracking
- #44 Webhook System: Outbound HTTP webhooks
- #45 Telemetry Mirror: Structured metrics

**Iteration 5** (Validation):
- #46 Integration Tests & Docs: E2E tests, OpenAPI, developer guide

### RALPH LOOP Completion Criteria

For each issue:
- [ ] Implementation complete
- [ ] Tests pass (per master test plan)
- [ ] `cargo clippy -- -D warnings` clean
- [ ] `cargo fmt --check` clean
- [ ] Issue commented with completion status
- [ ] Issue closed when verified

### Success Criteria (Track Complete)

- [ ] All 9 issues (#38-#46) resolved
- [ ] `cargo test --workspace` passes
- [ ] WebSocket endpoint functional at /api/v1/ws
- [ ] All 7 message types delivered to HotM client
- [ ] SSE endpoint functional at /api/v1/events
- [ ] Webhook delivery with HMAC signatures
- [ ] Telemetry metrics emitted
- [ ] Documentation updated (CHANGELOG, OpenAPI, developer guide)
