# Native Membership Scope Regression

Unreleased `2.0.0/full-v1` regression corpus accepted under producer ADR-102,
Fortemi #1147 and React #424. `capture.json` binds exact package, generator and
historical source hashes. This does not replace the published profile tuple.

The suite's `verify-full-v1-native-state.mjs` generated these fixed bytes from
the schema-2 adapted revision-19 synthetic fixture. Both the old and corrected
clean-installed Core packages generated identical input/replacement/expected
hashes. The old package fails the new scope and retained-coordinate checks;
the corrected candidate passes. Native PostgreSQL tests consume the same bytes.

- `input.shard`: original records plus one excluded note's membership in a
  selected set and one selected note's membership in an excluded set.
- `replacement.shard`: original selected notes/sets, no incoming memberships.
- `expected.shard`: both excluded-endpoint memberships survive; selected pairs
  are omitted. A declared shared set is not whole-set omission authority.

Tests cover retained composite RESTRICT references, rejected omission rollback,
repeat replacement and clean-destination membership export. This is bounded
local evidence, not a new live producer capture, release, authenticated route,
all-component parity claim or removal of suite NO-GO. Publish and pin through
the canonical contract delivery workflow before widening supported matrix cells.
