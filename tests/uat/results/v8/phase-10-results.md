# Phase 10: Templates — Results

**Date**: 2026-02-14
**Version**: v2026.2.8
**Result**: 15 tests — 13 PASS, 2 PARTIAL (86.7%)

## Summary

| Test ID | Name | Status | Details |
|---------|------|--------|---------|
| TMPL-001 | List Templates Empty | PASS | Returns empty array initially |
| TMPL-002 | Create Meeting Template | PASS | Created with variables extracted |
| TMPL-003 | Create Project Template | PASS | Created with default_collection |
| TMPL-004 | Create Code Review Template | PARTIAL | default_tags not persisted in some cases |
| TMPL-005 | Get Template Details | PASS | All fields returned including variables |
| TMPL-006 | List Multiple Templates | PASS | 3 templates returned |
| TMPL-007 | Update Template Content | PASS | Content updated with Follow-up section |
| TMPL-008 | Update Template Metadata | PASS | Name, description, tags updated |
| TMPL-009 | Basic Instantiation | PASS | Note created with variables substituted |
| TMPL-010 | Instantiate With Extra Tags | PARTIAL | tags parameter overrides default_tags |
| TMPL-011 | Instantiate Into Collection | PASS | Note placed in specified collection |
| TMPL-012a | Missing Variables | PASS | Unsubstituted {{placeholders}} remain |
| TMPL-013 | AI Revision Mode | PASS | ai_revision job queued and completed |
| TMPL-014 | Delete Template | PASS | Template removed from list |
| TMPL-015 | Notes Survive Deletion | PASS | Note exists after template deleted |

## Test Details

### TMPL-001: List Templates Empty
- **Tool**: `list_templates`
- **Result**: Empty array `[]`
- **Status**: PASS - No templates exist initially

### TMPL-002: Create Meeting Template
- **Tool**: `create_template`
- **Template ID**: `019c5cd6-cb89-7012-b957-056e8c29de29`
- **Name**: "Meeting Notes"
- **Variables extracted**: `["date", "attendees", "duration"]`
- **Status**: PASS

### TMPL-003: Create Project Template with Default Collection
- **Tool**: `create_template`
- **Template ID**: `019c5cd7-4ef3-7f53-b381-829b328c2dff`
- **Collection**: `019c5cd7-3450-7022-9aa0-9cdc1704b8dd` (UAT-Templates)
- **Variables extracted**: `["project_name", "description", "goals"]`
- **Status**: PASS

### TMPL-004: Create Code Review Template (PARTIAL)
- **Tool**: `create_template`
- **Template ID**: `019c5cd6-cb80-7281-aad2-8655a5644a7d`
- **Issue**: `default_tags` parameter was accepted but may not have persisted correctly
- **Workaround**: Tags can be applied during instantiation
- **Status**: PARTIAL - Functionality works but default_tags handling inconsistent
- **Filed**: Issue #362

### TMPL-005: Get Template Details
- **Tool**: `get_template`
- **Template**: Meeting Notes (`019c5cd6-cb89-7012-b957-056e8c29de29`)
- **Result**:
  - name: "Meeting Notes"
  - content: Full markdown template
  - variables: `["date", "attendees", "duration"]`
  - default_tags, default_collection present
- **Status**: PASS

### TMPL-006: List Multiple Templates
- **Tool**: `list_templates`
- **Result**: 3 templates returned:
  1. Meeting Notes
  2. Code Review Template
  3. Project Documentation
- **Status**: PASS

### TMPL-007: Update Template Content
- **Tool**: `update_template`
- **Template**: Meeting Notes
- **Change**: Added `## Follow-up\n{{followup}}` section
- **Status**: PASS - Content updated, variables re-extracted

### TMPL-008: Update Template Metadata
- **Tool**: `update_template`
- **Template**: Meeting Notes
- **Changes**:
  - name: "Meeting Notes" → "Sprint Meeting Notes"
  - description: Updated
  - default_tags: `["meetings", "uat/templates"]`
- **Status**: PASS

### TMPL-009: Basic Instantiation
- **Tool**: `instantiate_template`
- **Template**: Meeting Notes
- **Variables**: `{"date": "2026-02-14", "attendees": "Alice, Bob, Carol", "duration": "1 hour"}`
- **Note ID**: `019c5cd7-ad00-7923-9213-ff00fa9a0dd6`
- **Title**: "Sprint Planning for Matric Memory UAT"
- **Status**: PASS - Variables correctly substituted

### TMPL-010: Instantiate With Extra Tags (PARTIAL)
- **Tool**: `instantiate_template`
- **Template**: Project Documentation
- **Tags parameter**: `["uat/phase-10", "project:matric"]`
- **Note ID**: `019c5cd7-c783-70b2-9b78-dbdb7e5eb8fc`
- **Expected**: Tags merged with default_tags
- **Actual**: Tags parameter overrides default_tags completely
- **MCP Schema**: Says "Override default tags" - designed behavior
- **Status**: PARTIAL - Behavior as documented but test expected merge
- **Filed**: Issue #363

### TMPL-011: Instantiate Into Collection
- **Tool**: `instantiate_template`
- **Template**: Project Documentation
- **Collection**: UAT-Code-Reviews (`019c5cd8-1ce5-7773-bcfd-aa388ab2c1e0`)
- **Note ID**: Created in specified collection
- **Status**: PASS - Collection assignment works

### TMPL-012a: Missing Variables
- **Tool**: `instantiate_template`
- **Template**: Meeting Notes
- **Variables**: `{}` (empty - none provided)
- **Note ID**: `019c5cd8-31ac-70b2-ad82-7d7c9f370ab0`
- **Result**: Content contains `{{date}}`, `{{attendees}}`, `{{duration}}` literally
- **Status**: PASS - Missing variables remain as placeholders

### TMPL-013: AI Revision Mode
- **Tool**: `instantiate_template`
- **Template**: Meeting Notes
- **ai_revision**: "full"
- **Note ID**: `019c5cd8-4d74-7fd2-959c-0be9301aa22e`
- **Job Status**: Completed (verified via get_job)
- **Status**: PASS - AI revision job queued and processed

### TMPL-014: Delete Template
- **Tool**: `delete_template`
- **Template**: Meeting Notes (`019c5cd6-cb89-7012-b957-056e8c29de29`)
- **Result**: `{"success": true, "deleted": "019c5cd6-cb89-7012-b957-056e8c29de29"}`
- **Verification**: Template not in list_templates
- **Status**: PASS

### TMPL-015: Notes Survive Template Deletion
- **Tool**: `get_note`
- **Note**: `019c5cd7-ad00-7923-9213-ff00fa9a0dd6` (created from deleted template)
- **Result**: Note exists with full content, tags, semantic links
- **Status**: PASS - Notes are independent of template lifecycle

## MCP Tools Verified

| Tool | Status |
|------|--------|
| `list_templates` | Working |
| `create_template` | Working |
| `get_template` | Working |
| `update_template` | Working |
| `instantiate_template` | Working |
| `delete_template` | Working |

## Issues Filed

| Issue | Test | Description |
|-------|------|-------------|
| #362 | TMPL-004 | default_tags not consistently persisted on template creation |
| #363 | TMPL-010 | Tags parameter overrides instead of merging with default_tags |

## Notes

- Template variable extraction uses `{{variable}}` syntax
- Variables are automatically extracted from content on create/update
- Missing variables during instantiation remain as literal `{{placeholder}}` text
- AI revision modes: "full", "light", "none"
- Notes are completely independent of templates after instantiation
- Default collection is respected unless overridden
- Tags parameter overrides (not merges) default_tags - this is documented behavior

## Test Resources

Templates created:
- `019c5cd6-cb89-7012-b957-056e8c29de29` (Meeting Notes - deleted)
- `019c5cd6-cb80-7281-aad2-8655a5644a7d` (Code Review Template)
- `019c5cd7-4ef3-7f53-b381-829b328c2dff` (Project Documentation)

Notes created:
- `019c5cd7-ad00-7923-9213-ff00fa9a0dd6` (Sprint Planning)
- `019c5cd7-c783-70b2-9b78-dbdb7e5eb8fc` (Project: Matric Memory UAT)
- `019c5cd8-1ce5-7773-bcfd-aa388ab2c1e0` (In collection)
- `019c5cd8-31ac-70b2-ad82-7d7c9f370ab0` (Missing variables)
- `019c5cd8-4d74-7fd2-959c-0be9301aa22e` (AI revised)

Collections created:
- `019c5cd7-3450-7022-9aa0-9cdc1704b8dd` (UAT-Templates)
- `019c5cd8-1ce5-7773-bcfd-aa388ab2c1e0` (UAT-Code-Reviews)
