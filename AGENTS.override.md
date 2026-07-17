# Operator Project Direction

## Current Operating Plan

The active Fortemi project plan of record is `.aiwg/planning/roadmap.md` (`Fortemi Delivery Roadmap`). Treat it as the main operating roadmap until all planned phases are complete or the operator explicitly replaces it.

For requests like "what is next", "continue the plan", "advance the roadmap", "roadmap progress", or fresh-session delivery work, first run:

```bash
aiwg discover "advance roadmap"
```

Then use `fortemi-roadmap-skill` and follow its procedure. The roadmap owns phase order, gating dependencies, product decisions, and the open-build vs licensed-server split. Gitea `Fortemi/fortemi` remains the authoritative tracker.

When roadmap work advances, update `.aiwg/planning/roadmap.md` and `.aiwg/activity.log` in the same run so future agents can resume from the current state.

## Knowledge Shard Integration Contract

Read `docs/architecture/adr/ADR-102-canonical-knowledge-shard-contract.md`
before changing Knowledge Shard manifests, schemas, import/export behavior,
migrations, fixtures, or cross-repository integrations.

- This repository owns the canonical shard schema, profile registry, golden
  corpus, migrations, and producer/consumer conformance matrix.
- Treat `manifest.version` and `manifest.min_reader_version` as shard-schema
  SemVer. Keep producer application identity in separate metadata.
- `core-v1`, `full-v1`, and `record-v1` are explicit contracts. Never describe
  a subset or best-effort import as full parity.
- Validate the complete archive before writes. Integrity, count, relationship,
  profile, or unsupported-version failures must leave persistent state
  unchanged.
- The default export must use the richest profile the same release can
  self-import. `full-v1` becomes the default only after its rich-component and
  attachment-byte gates pass.
- Do not advertise target behavior as implemented until ADR-102 release gates
  and cross-repository fixtures pass.

Gitea `Fortemi/fortemi` is authoritative for canonical contract changes. Link
the Fortemi issue from every affected consumer issue and pull request, and add
reciprocal links to `fortemi-react`, `aiwg`, HotM, or other affected
repositories. Do not merge a contract change until its consumer impact and
schema/profile version decision are recorded.
