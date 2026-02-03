# Naming Decision: Fortémi

**Status:** FINAL
**Decision Date:** 2026-02-03
**Effective:** Immediate

---

## Selected Name

**Fortémi** (for-TAY-mee)

### Etymology
- **Italian/Musical** *forte* (forté): strength, strong point
- **Japanese** *美* (mi): harmony, beauty

Cross-language synthesis conveying: **strong harmony** - the balance of strength and elegance in knowledge management.

### Tagline Options
- "Knowledge that endures"
- "Resilient knowledge infrastructure"
- "Strong foundations for thought"

---

## Clearance Verification

### Domain Availability (Verified 2026-02-03)

| Domain | Status |
|--------|--------|
| fortemi.com | ✅ **REGISTERED** |
| fortemi.io | ✅ **REGISTERED** |
| fortemi.info | ✅ **REGISTERED** |
| fortemi.dev | ✅ Available |

### Registry Availability

| Registry | Status |
|----------|--------|
| npm (fortemi, @fortemi/*) | ✅ Available |
| crates.io (fortemi, fortemi-*) | ✅ Available |
| GitHub (fortemi org) | ✅ Available |
| PyPI (fortemi) | ✅ Available |

### Trademark Search

| Database | Result |
|----------|--------|
| USPTO | ✅ No conflicts |
| EUIPO | ✅ No conflicts |
| Web search | ✅ No competing products |

**Closest matches (not conflicting):**
- Forte Software (acquired by Sun 1999) - different name
- Forte (blockchain gaming) - different name
- FortiSIEM (Fortinet security) - different name and domain

---

## Brand Identity

### Name Usage

| Context | Format |
|---------|--------|
| Product name | Fortémi |
| Repository | fortemi/fortemi |
| Rust crates | fortemi-core, fortemi-db, fortemi-api |
| npm packages | @fortemi/mcp, @fortemi/client |
| CLI command | fortemi |
| API paths | /api/v1/... (no name in path) |

### Visual Identity

- **Primary color:** TBD (consider deep blue/teal for trust + intelligence)
- **Logo concept:** Interlocking/woven pattern suggesting connections
- **Typography:** Clean sans-serif, professional but approachable

### Voice & Tone

- Technical but accessible
- Confident without being arrogant
- Focus on empowerment ("your knowledge", "your data")

---

## Migration Plan

### Phase 1: GitHub Publication
1. Create `fortemi` organization on GitHub
2. Create `fortemi/fortemi` repository
3. Push current codebase with internal references intact
4. Update CI/CD to publish to `ghcr.io/fortemi/fortemi`

### Phase 2: Internal Rename (Future)
1. Rename crates: matric-* → fortemi-*
2. Update internal references
3. Update documentation
4. Maintain matric-memory as alias/redirect

### Phase 3: Full Brand Launch (Future)
1. Register domains
2. Create brand assets
3. Public documentation site
4. npm/crates.io publication

---

## Decision Rationale

1. **Cross-cultural appeal**: Italian/musical + Japanese synthesis is distinctive and memorable
2. **Meaning resonance**: "Strong harmony" perfectly fits knowledge infrastructure
3. **Phonetics**: Easy to pronounce in English, Spanish, Japanese, German
4. **Brandability**: Short (7 letters), unique, no negative associations
5. **Technical fit**: Works as CLI command, package name, API namespace
6. **Full clearance**: All domains, registries, and trademarks available

---

*Decision made by: roctinam*
*Document created: 2026-02-03*
