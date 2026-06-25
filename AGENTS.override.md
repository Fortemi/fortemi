# Operator Project Direction

## Current Operating Plan

The active Fortemi project plan of record is `.aiwg/planning/roadmap.md` (`Fortemi Delivery Roadmap`). Treat it as the main operating roadmap until all planned phases are complete or the operator explicitly replaces it.

For requests like "what is next", "continue the plan", "advance the roadmap", "roadmap progress", or fresh-session delivery work, first run:

```bash
aiwg discover "advance roadmap"
```

Then use `fortemi-roadmap-skill` and follow its procedure. The roadmap owns phase order, gating dependencies, product decisions, and the open-build vs licensed-server split. Gitea `Fortemi/fortemi` remains the authoritative tracker.

When roadmap work advances, update `.aiwg/planning/roadmap.md` and `.aiwg/activity.log` in the same run so future agents can resume from the current state.
