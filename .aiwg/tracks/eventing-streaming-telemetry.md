# Development Track: Eventing, Streaming & Telemetry

**Epic**: [#37](https://git.integrolabs.net/Fortemi/fortemi/issues/37)
**Status**: Inception → Elaboration
**Created**: 2026-02-05
**Upstream**: Fortemi/HotM#27

## Track Overview

Restore and extend the real-time eventing system from the original HotM platform.
Detailed issue specs live in Gitea — this document provides the strategic plan and
cross-references without duplicating implementation details.

## Phase Map

| Phase | Issue | Title | Depends On | Priority |
|-------|-------|-------|------------|----------|
| 1 | [#38](https://git.integrolabs.net/Fortemi/fortemi/issues/38) | Event Bus Foundation | — | P0 |
| 2 | [#39](https://git.integrolabs.net/Fortemi/fortemi/issues/39) | WebSocket Endpoint | #38 | P0 |
| 3 | [#40](https://git.integrolabs.net/Fortemi/fortemi/issues/40) | Job Event Bridge | #38 | P1 |
| 4 | [#41](https://git.integrolabs.net/Fortemi/fortemi/issues/41) | Note Event Emission | #38, #40 | P1 |
| 5 | [#42](https://git.integrolabs.net/Fortemi/fortemi/issues/42) | Connection Management | #39 | P2 |
| 6 | [#43](https://git.integrolabs.net/Fortemi/fortemi/issues/43) | MCP Event Forwarding | #38 | P2 |
| 7 | [#44](https://git.integrolabs.net/Fortemi/fortemi/issues/44) | Webhook System | #38 | P3 |
| 8 | [#45](https://git.integrolabs.net/Fortemi/fortemi/issues/45) | Telemetry Mirror | #38 | P2 |
| 9 | [#46](https://git.integrolabs.net/Fortemi/fortemi/issues/46) | Integration Tests & Docs | All | P1 |

## Parallel Execution Strategy

```
Iteration 1 (Foundation):
  #38 Event Bus Foundation          ← CRITICAL PATH

Iteration 2 (Core Transports):      ← Can parallelize
  #39 WebSocket Endpoint            ← Depends on #38
  #40 Job Event Bridge              ← Depends on #38
  #43 MCP Event Forwarding (SSE)    ← Depends on #38

Iteration 3 (Event Sources):
  #41 Note Event Emission           ← Depends on #38, #40

Iteration 4 (Hardening):            ← Can parallelize
  #42 Connection Management         ← Depends on #39
  #44 Webhook System                ← Depends on #38
  #45 Telemetry Mirror              ← Depends on #38

Iteration 5 (Validation):
  #46 Integration Tests & Docs      ← Depends on all
```

## Architecture Decision

See [ADR-037](../architecture/ADR-037-unified-event-bus.md): Unified Event Bus with Multi-Transport Delivery.

Key choice: Single `tokio::sync::broadcast` channel with `ServerEvent` enum in `matric-core`,
bridging existing `WorkerEvent` from the job worker. Each transport subscribes independently.

## Risk Summary

See [Risk List](../gates/eventing-track/risk-list.md) for full assessment (11 risks).

**Show Stopper**: R-EVT-001 (unauthenticated WebSocket) — mitigated by optional auth in #42.
**High Impact**: Broadcast backpressure (R-EVT-004), SSRF in webhooks (R-EVT-002).

## AIWG Artifacts

| Artifact | Location |
|----------|----------|
| Vision Document | `.aiwg/gates/eventing-track/vision-document.md` |
| Risk List | `.aiwg/gates/eventing-track/risk-list.md` |
| Scope Boundaries | `.aiwg/gates/eventing-track/scope-boundaries.md` |
| Use Case Briefs | `.aiwg/gates/eventing-track/use-case-briefs.md` |
| ADR-037 | `.aiwg/architecture/ADR-037-unified-event-bus.md` |
| LOM Gate Report | `.aiwg/gates/eventing-track/lom-report.md` |
| Elaboration Gate | `.aiwg/gates/eventing-track/elaboration-gate.md` |
| Construction Gate | `.aiwg/gates/eventing-track/construction-gate.md` |

## Success Criteria

Per issue #37 acceptance criteria:
- [ ] WebSocket endpoint at `/api/v1/ws` fully functional
- [ ] All 7 message types supported
- [ ] HotM UI connects without modifications
- [ ] MCP clients receive events via SSE
- [ ] Webhook system for external integrations
- [ ] Telemetry metrics emitted for all events
- [ ] Comprehensive test coverage
- [ ] Documentation updated
