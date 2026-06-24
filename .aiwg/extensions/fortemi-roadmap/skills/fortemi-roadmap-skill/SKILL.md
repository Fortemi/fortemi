---
name: fortemi-roadmap-skill
description: Advance the Fortemi delivery roadmap by one concrete increment and document the progress. Reads .aiwg/planning/roadmap.md, picks the next actionable item respecting phase gating, does or dispatches the work, then records progress back into the roadmap. Built for fresh sessions and simple loops.
triggers:
  - roadmap
  - advance roadmap
  - continue roadmap
  - roadmap progress
  - what's next
  - next roadmap item
  - work the plan
  - advance the plan
metadata:
  scope: project
  category: project-management
---

# Fortemi Roadmap Advance

You are the roadmap driver. Your job each run: **advance the Fortemi delivery roadmap by one concrete, verifiable increment, then document that progress back into the roadmap.** Designed to be invoked in a brand-new session (including under `/loop`) and always make forward progress.

The roadmap is the plan of record at `.aiwg/planning/roadmap.md`. Tracker is Gitea `Fortemi/fortemi` (authoritative).

## When to use

- "advance the roadmap", "what's next", "continue the plan", "roadmap progress", or any fresh session whose goal is to move the delivery plan forward.
- Under `/loop`: each iteration advances one increment and records it, so the loop keeps making progress without re-deciding scope.

## Procedure (one increment per run)

1. **Read the roadmap.** Load `.aiwg/planning/roadmap.md`. Identify the lowest-numbered phase that is not complete, and within it the first actionable item (`[ ]` or `[~]`) whose **gating dependency is satisfied**. Respect the phase ordering and the `tier/open-build` vs `tier/licensed-server` split (#853) — open-build (Phase 1) and licensed-server (Phase 2) may both be in flight in parallel; bridge/referenced/streaming gate on their stated prerequisites.

2. **Re-read live state before acting** (hard rule — this prevents duplicate work). For the chosen item: read the owning Gitea issue body + latest comments, and the current code/docs it touches. If the work is already done or superseded, mark it `[x]` with a one-line note and move to the next item — do not redo it.

3. **Advance it.** Do the smallest complete next step that moves the item forward:
   - If it's implementable now, implement it (code + tests + docs), following the recorded product decision for that issue (see the roadmap's decisions index) and the project rules.
   - If it needs decomposition, dispatch focused subagents (respect the project parallelism cap) or produce a concrete implementation boundary + first PR.
   - If it's blocked, mark it `[!]` with the blocker and pick the next unblocked item instead — never stall.
   - If it needs a product decision you can't make, surface ONE focused question (per `human-authorization`), record the open question on the owning issue, and move to the next actionable item.

4. **Document progress (always — this is half the job).** Update `.aiwg/planning/roadmap.md`:
   - Flip the item's checkbox (`[ ]`→`[~]`→`[x]`) and append a short parenthetical (what landed + issue/PR/commit ref).
   - Update the `Last updated` line to today's date.
   - If the increment is notable, add a one-line entry to a `## Progress log` section at the bottom (create it if absent): `- YYYY-MM-DD — <issue#> <what advanced>`.
   - Append an activity-log entry: `aiwg activity-log append update "roadmap: advanced <issue#> — <summary>"`.

5. **Commit per delivery policy.** Read `.aiwg/aiwg.config` `delivery.mode`. For `direct` (current): commit the roadmap update (and any code that landed) straight to the default branch with a conventional message and **no AI attribution** (project `no-attribution` rule is CRITICAL), then push to `remotes.primary`. For `feature-branch`/`pr-required`: branch/PR accordingly. Reference issues with `Refs #N` / `Closes #N`.

## Guardrails

- **Always advance.** Never end a run without either advancing an item or recording a blocker/decision that unblocks the next run. "Context is long" is not a stop reason — checkpoint and continue.
- **Non-duplicative.** Re-read live issue comments before commenting or filing; the backlog was comprehensively audited 2026-06-21..23 and most questions are already on-issue. Add only net-new, evidence-based content.
- **Respect scope & authorization.** A roadmap item is authorization to advance *that* item, not to expand scope. Destructive/irreversible/out-of-scope actions need explicit human authorization.
- **Honor recorded decisions.** The roadmap's decisions index maps each area to its owning issue's "Operator product decision" comment — implement to those, don't relitigate.
- **Keep the roadmap honest.** Only mark `[x]` when verifiably done (tests pass / issue closed / artifact exists). Use `[~]` for partial.

## Loop usage

```
/loop advance the roadmap
```

Each iteration: read → pick next gated item → re-read live state → advance one increment → document + commit → stop. The next iteration picks up the new next item. Stop the loop when the active phase has no actionable unblocked items (report what remains and why).

## Completion / stop conditions

- All phases `[x]` → report roadmap complete.
- Active phase fully blocked → report each blocker and the decision/dependency needed; stop until resolved.
- A required product decision is missing → ask one focused question, record it on the issue, continue with other actionable items.

## Inputs / outputs

- **Reads:** `.aiwg/planning/roadmap.md`, Gitea `Fortemi/fortemi` issues, project code/docs, `.aiwg/aiwg.config`.
- **Writes:** updated `.aiwg/planning/roadmap.md` (checkboxes + progress log + Last-updated), issue comments/PRs as warranted, `.aiwg/activity.log`, commits to the configured remote.
