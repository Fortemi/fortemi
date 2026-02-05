## Shard Schema Change

**Version Change:** X.Y.Z → X.Y.Z
**Change Type:** [ ] MAJOR (breaking) [ ] MINOR (additive) [ ] PATCH (fix)

### Changes
- ...

### Definition of Done Checklist

#### Required
- [ ] **Version bump** - Updated `CURRENT_SHARD_VERSION` in `crates/matric-core/src/shard/version.rs`
- [ ] **Migration handler** - Created migration in `crates/matric-core/src/shard/migrations/` (if MAJOR change)
- [ ] **Unit tests** - Added tests for new/changed fields
- [ ] **Integration tests** - Tested full import/export cycle
- [ ] **Documentation** - Updated CHANGELOG.md and relevant docs
- [ ] **Reserved fields** - Added old field names to registry (if removing/renaming)

#### Optional (for breaking changes)
- [ ] **Reverse migration** - If safely reversible
- [ ] **Transition period** - Support both old and new simultaneously
