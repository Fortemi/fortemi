# Fortemi backlog execution plan — September 2026

Status: approved planning baseline, 2026-09-06. This addendum and [the complete server register](backlog-2026-09.csv) update [the living roadmap](roadmap.md). Existing issue acceptance and recorded product decisions remain authoritative. [Release cadence](release-cadence-2026-09.md) governs selection and promotion opportunities, not phase completion dates.

## Baseline and completion

The audit baseline contains 355 issues across eight repositories, including 254 server issues. Four server issues (#1098/#1100/#1134/#1135) are now closed following acceptance and release reconciliation. AIWG #2296/#2298 closed concurrently. New process issue #1143 adds one coordination outcome. The current planning register therefore contains 356 rows, of which 350 remain open, pending final state verification.

Every server baseline issue remains in the CSV with its next action, phase, responsible role, recorded dependencies, acceptance extraction, review date and exit evidence. The suite workspace keeps the equivalent all-repository register and fourteen lane appendices at `.aiwg/planning/backlog-2026-09/`. Native dependency absence is not evidence of readiness: retain prose and external environment gates.

Cleared baseline means every original issue has a verified completion or an explicit retirement/supersession decision with dependency disposition. Deferred work remains outstanding. New intake is reported separately, and parent epic closure is not counted as extra implemented scope. No phase is complete from a decrease in issue count alone.

## Ordered execution

| Batch | Activation and owning issues | Required deliverable and exit |
|---|---|---|
| 1. Customer operation | #1132 first-boot supervision, then #1133 verified recovery-point reuse; #1127 durable network/browser path; HotM #304 auth-required UX | Supported fresh/upgrade bundle survives long migrations and registration failures; backup reuse validates identity and integrity; real browser auth failures do not loop indefinitely |
| 2. Consumer correctness | React #425 scope filtering, #424 post-import live state, #423 default profile; #417–#422 remote envelopes/search/errors/version negotiation | Public package API preserves requested scope and later edits; actual server fixtures pass; malformed/unsupported inputs fail before mutation |
| 3. Ready shared contracts | Closed #1090 source-upsert prerequisite → #1091 and React #405 typed retrieval; #1092 with React #406 and #900/#719 lifecycle; HotM #288 consumes delivered #1097 events | Authority and every declared backend/consumer agree on exact fixture and artifact receipts; crash/re-erasure and event semantics remain separate gates |
| 4. Open-build foundations | HotM #300, React #407, site #30, server #887/#930/#1078; dependency advisories and retrieval/embedding correctness | Artifact-specific security/build and issue acceptance pass; explicit incompatible-upstream holds remain visible |
| 5. Hosted construction | Auth #15, #2/#3/#6 → #4/#5/#19 → #8/#1; server #726 → #727 → #728 → #729 under #733/#707; #734 → #730 → #731/#1099; #710/#711/#714 | Real tenant identities, forced-RLS role, error taxonomy, KMS, policy, audit and quotas are qualified; a public auth crate is not hosted readiness |
| 6. Dataset production admission | #1128–#1131 bounded authority, AIWG #2242 and React #412/#422; #1136 → #1137 → #1138 → #1139/#1140 → #1141 | Isolation precedes durable fault recovery; restore and skew precede measured load. Use exact source/consumer/configuration/environment receipts |
| 7. Gateway / existing Phase 3 | #863; canonical #873 and #666 before #864/#865/#866/#867; #877–#879 policy/metering; #932 stream framing; #874–#876 provider expansion | Provider-specific positive/negative tests, cancellation/retry accounting, destination restrictions, budget/retention behavior; no capability claim inferred from route presence |
| 8. Referenced storage / Phase 4 | #736; #890/#903/#904/#905/#902/#775 design → #738/#739 → #740/#741 → #743 → #744/#745 → #746/#747 and #790–#802 gates | Source containment/no-write, quarantine, bounded scans/resume, derived cleanup, API/MCP write gates and rollout proof; #742 watcher remains v2 |
| 9. Streaming / Phase 5 | #586; decisions #906/#915/#939 → #593/#596 → #594/#595; #597–#602 derived freshness | Durable event publication, bounded replay/retention, tenant authorization, fan-out semantics and restart/duplicate behavior. Optional CDC/research tracks remain separately activated |
| 10. Native distribution / Phase 5 | #636; reconcile delivered #623/#641, then #637–#640/#642/#643, #938/#901/#916; #1096 Windows gate | Manifest-driven verified installation, service lifecycle, rollback and clean-host evidence per supported OS/architecture. Published binary and install support are distinct |
| 11. HotM usability and assistant | #122 residual settings review; #116 with #144–#149 advanced scope; #159/#160 defaults; #30/#31/#222/#174 onboarding; #110/#173 interaction | Test actual installed user workflow and backend behavior. Retain original acceptance or an explicit accepted amendment; a substituted checklist is insufficient |
| 12. Mobile and preview | #224 → #225 → #226/#251 → #227 → #228 → #229 → #230 | Hosted contract/rate-limit admission before signed-device/store rollout; #248–#250 fixture previews are independently bounded and are not entitlement production evidence |
| 13. Governance and independent AIWG | License decision and #720/#894/#901 notices; #1126/site #23 claims; skills #1–#4; AIWG provider and voice sequence | Approved consistent grants/notices and accurate product claims; Pi adapter/doc conformance and independent human voice evaluation retain their own gates |
| 14. Conditional tail | #603/#605/#606/#683/#742/#881/#891/#1007/#1015/#1053/#1096, React #212, AIWG #2203 | Trigger-specific evaluation and explicit adopt/retain/retire disposition. First review 2026-10-05; no age-only closure |

Batches identify outcomes, not simultaneous work assignments. Batches 1–4 lead. Hosted/environment work may proceed independently when its executor and prerequisites exist. Each later batch is decomposed by its existing issue children and the complete register. Never launch all P0 descendants simply because their labels match.

## Cross-repository handoffs

Use reciprocal producer/consumer issue links and exact immutable version/digest receipts. Typed retrieval links #1091 ↔ React #405; lifecycle #1092 ↔ React #406; events #1097 ↔ HotM #288; dataset #1128–#1131 ↔ AIWG #2242 and React #412/#422. Canonical issues already own implementation; create a new issue only for an uncovered residual, not a duplicate epic.

ITOps environment admission remains separate: scoped access and validation-environment receipts must exist before hosted/dataset production qualification. The public auth core uses immutable HTTPS tags under ADR-AUTH-002. Do not recreate private auth registry/deploy-key prerequisites. Separate private Enterprise provider/distribution work retains its own authority.

The private-distribution acceptance of auth #12/#20 is retired as superseded. Their tracker states remain open because earlier closure attempts encountered dependency constraints. Reconcile obsolete edge semantics at weekly triage and close as superseded without erasing legitimate independent runtime/release gates.

## SDLC checkpoints

**Ready:** original acceptance is understood; authority, consumers, prerequisites, executor and bounded outcome are named. Design decisions precede dependent implementation. Missing environment access is recorded as a prerequisite, never simulated into a passing gate.

**Construction complete:** source and appropriate negative/positive tests pass; requirements, SAD/ADR, fixtures, generated contracts and consumer receipts are synchronized for affected behavior. Local implementation evidence stays distinct from publication and production evidence.

**Release eligible:** repository delivery policy is satisfied, exact artifacts are verified, supported skew/migration/recovery instructions exist, and required consumer/platform cells pass. The candidate receipt lists skipped/unsupported cells and their product impact.

**Closed:** each acceptance criterion maps to evidence; no unresolved required child/native/prose gate remains. Supersession names the replacement decision and preserves residual obligations. Customer confirmation is required when the original issue explicitly made it a gate.

## Risk register and controls

| Risk | Owner role | Control and verification |
|---|---|---|
| Future P0 volume displaces customer recovery | Product/release maintainer | Use phase plus priority; `roadmap/next` and `roadmap/later` make selection order visible |
| Local green tests overstate compatibility | Contract authority owner | Real published producer-to-clean-consumer matrix, exact profile/schema/platform and negative-write checks |
| Artifact differs between verification and publication | Release maintainer | Build once, verify digests, promote exact bytes; #1143/#415/#301 ownership |
| Stale success checklist hides unmet original scope | Component maintainer | Acceptance-to-receipt matrix before closure; #122 retained for reconciliation |
| Hosted evidence mistaken for library or preview evidence | Auth/qualification owners | Separate library corpus, runtime tenant, environment and production receipts |
| Deferred backlog never reaches a decision | Product/component owner | Monthly trigger review; record retain/activate/retire and next date |
| Migration rollback revives deleted or incompatible state | Storage/qualification owner | Named-profile restore, version skew, tombstone/re-erasure and failure tests before recovery claims |
| Concurrent work invalidates a snapshot | Roadmap maintainer | Re-read target before mutation, preserve prior body, verify after state, regenerate final delta |

## Research grounding

Research informs design and verification choices; it is not evidence that Fortemi has implemented or qualified them. The exact fortnightly cadence is a project decision.

- Corpus REF-027, Cormack et al., [Reciprocal Rank Fusion](https://doi.org/10.1145/1571941.1572114), supports a rank-fusion baseline for retrieval comparison. Retain tenant/scope correctness and local relevance evaluation; do not import paper benchmark scores as Fortemi performance.
- Corpus REF-056, Wilkinson et al., [FAIR guiding principles](https://doi.org/10.1038/sdata.2016.18), informs identifier, metadata and reuse requirements. It does not certify backup completeness or require private data to be public.
- Corpus REF-062, [W3C PROV-DM](https://www.w3.org/TR/prov-dm/), supplies the entity/activity/agent vocabulary for receipt lineage. A provenance relation does not prove semantic conformance by itself.
- Corpus REF-322 is a lower-strength practitioner guide to PostgreSQL outbox design. Use it as a candidate design rationale, not an exactly-once or no-contention guarantee; #593 must independently prove restart, fan-out, acknowledgment and duplicate semantics.
- DORA and Google SRE release guidance and their induction tracking are recorded in [the cadence document](release-cadence-2026-09.md).

Canonical reference notes were retrieved from section9/research-papers on 2026-09-06. The suite reconciliation retains blob identifiers and original note content for traceability. Private corpus metadata is not a public product compatibility claim.
