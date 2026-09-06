# Fortemi suite release cadence

Status: accepted operating cadence under the 2026-09-06 roadmap authorization. Process rollout and first-cycle evidence: [#1143](https://git.integrolabs.net/Fortemi/fortemi/issues/1143). Phase order remains in [the living roadmap](roadmap.md).

## Calendar

| Review or release | Frequency | First occurrence | Required result |
|---|---|---|---|
| Backlog and prerequisite triage | Every Monday | 2026-09-07 | Select bounded ready work; reconcile customer impact, blockers, carryover and closure evidence |
| Candidate scope selection | Tuesday before stable window | 2026-09-15 | Freeze candidate commits and intended scope; link each selected issue and required gate |
| Candidate verification | Wednesday before stable window | 2026-09-16 | Verify immutable artifacts, supported upgrade paths and affected contract cells |
| Stable release opportunity | Every second Thursday | 2026-09-17 | Promote only passing scope; publish exact artifact identities and limitations |
| Outcome review | Friday after stable window | 2026-09-18 | Record customer smoke/observation, failures, recovery and carryover |
| Suite compatibility review | First Monday each month | 2026-10-05 | Reconcile supported version/profile/platform cells and claim permissions |

Subsequent stable opportunities: October 1, October 15, October 29, November 12 and November 26, 2026. The release owner may move a window for availability, recording the change on #1143. Calendar dates are review/promotion opportunities, not phase completion estimates. Use America/New_York for scheduling; receipts record UTC timestamps.

## Selection and capacity

Customer recovery and release integrity lead, followed by shared contracts and open-build foundations. Hosted qualification progresses independently when its environments and prerequisites exist. Gateway, Referenced storage, streaming, native expansion and mobile follow their recorded dependency chains. Existing P0 labels express importance within that scope; they do not make every future descendant immediately executable.

Keep one active delivery batch per executing maintainer, with one independent review/verification batch when capacity permits. Do not invent named staff assignments. The component maintainer accepts ownership at selection; the release maintainer owns candidate integrity; the contract authority owner owns consumer coverage. Work without an available executor stays queued.

A batch has an independently verifiable user or contract outcome. Avoid holding completed independent work merely to fill the fortnightly window. The scheduled opportunity provides a predictable stable checkpoint; qualified corrections and packages may ship earlier under repository policy.

## Promotion and failure rules

1. Record source commit, application version, build configuration, artifact digests, required checks and supported platforms. Preserve the existing application version convention; shard schema SemVer is independent of application CalVer.
2. Publish the exact package/image/binary that passed verification. A source checkout test does not establish the identity of a rebuilt publication artifact. React #415, HotM #301 and server #916/#901 retain their specific acceptance.
3. For a contract change, release the authority first; pin producer and consumer receipts; test every declared consumer at the supported version skew and named profile. Consumer merges can occur separately, but the joint claim waits for all required cells.
4. Exercise applicable fresh installation, upgrade, failure, restart and recovery behavior. Record whether rollback is supported or requires a tested forward correction. Do not roll back across incompatible persisted state without a verified recovery procedure.
5. Define the observation scope, duration and abort criteria before promotion. Hosted canaries compare the selected deployment with an appropriate control; desktop/bundle releases use supported clean-host and upgrade workflows. Neither substitutes for the other.
6. Reject promotion when required checks fail, artifacts differ, or the relevant compatibility receipt is missing. Carry affected scope forward with its failure reason and next action. Independently qualified components may still ship.
7. Keep the previous verified artifacts accessible for supported recovery. Record promotion and recovery decisions on the release issue, then update roadmap and activity log.

Urgent customer/security patches use an expedited window with the same relevant gates and artifact identity checks. Record the trigger, reduced change scope, verification, recovery path and post-release outcome. Urgency does not waive tenant isolation, integrity or pre-write validation.

## Claim boundaries

Community/open-build releases do not wait for hosted Enterprise GA. An auth library release is distinct from configured runtime tenancy. The AIWG static index, explicit Knowledge Shard bridge and live Fortemi persistence remain separate. Shard claims name `core-v1`, `full-v1` or `record-v1`, schema version, consumer and platform. The suite remains NO-GO for unqualified full parity, complete backup or portability until independent audit evidence clears that exact claim.

## Forecasting and clearing the backlog

After two operating cycles, measure completed leaf outcomes, cycle-time distribution, blocked time, failed changes and recovery observations. Report new intake and retirements separately. Parent epic closure does not count as another implementation outcome. Use observed capacity to forecast remaining batches as ranges; retain dependency gates and uncertainty. No current ticket-count-to-date conversion is justified.

The baseline backlog is cleared only when each baseline issue is completed against its acceptance or explicitly retired with rationale and successor disposition. A deferred label is not closure. At monthly review, every deferred item needs an activation trigger and review date; if the trigger remains unavailable, record the decision to retain or retire it. Report baseline completion separately from newly accepted scope.

## Research basis

[DORA's small-batch guidance](https://dora.dev/capabilities/working-in-small-batches/) supports independent increments and rapid feedback. It also cautions against regrouping completed changes into large downstream batches. The fortnightly stable opportunity is a suite operating decision, not an interval established by that research. Canonical induction: section9/research-papers#1112.

[Google SRE's Canarying Releases](https://sre.google/workbook/canarying-releases/) supports reproducible builds, small changes and limited deployment evaluation before wider rollout. The actual Fortemi observation criteria still require local definition and evidence. Canonical induction: section9/research-papers#1113. Both sources accessed 2026-09-06.
