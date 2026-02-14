# UAT Test SKOS-032: Untag Note Concept

**Test ID**: SKOS-032  
**Phase**: 13 - SKOS Taxonomy  
**Date**: 2026-02-14  
**Status**: PASS ✅

## Test Description
Remove a SKOS concept tag from a note using `untag_note_concept`.

## Test Steps

1. Found note with SKOS concept tags:
   - Note ID: `019c5cee-402e-7390-a5fa-2a9a37e1f124`
   - Title: "SKOS Tagging Validation on Test 3D Model"
   - Initial concept count: 6 concepts

2. Removed concept tag:
   - Concept ID: `019c5fc-5447-7fa2-8eef-2a914fe1ca45`
   - Concept label: "semantic tagging"
   - Result: `{"success": true}`

3. Verified removal:
   - Remaining concepts: 5 (down from 6)
   - Removed concept no longer appears in list
   - Other concepts unchanged

## Result
**PASS** ✅

- Successfully removed concept tag from note
- Returns success confirmation
- Concept properly dissociated without affecting other tags
- Note content and other concepts remain intact

## Evidence
- Initial concept count: 6
- Final concept count: 5
- "semantic tagging" concept successfully removed
- All other concepts preserved
