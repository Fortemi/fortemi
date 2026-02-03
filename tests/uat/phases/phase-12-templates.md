# Phase 12: Templates

**Duration**: ~8 minutes
**Tools Tested**: 6 tools
**Dependencies**: Phase 0 (preflight)

---

## Overview

Templates allow creating reusable note structures with placeholder variables. This phase tests the complete template lifecycle: creation, variable substitution, and instantiation.

---

## Test Data Setup

### Template Content Examples

```javascript
const MEETING_TEMPLATE = `# {{meeting_type}} Meeting Notes

**Date**: {{date}}
**Attendees**: {{attendees}}
**Duration**: {{duration}}

## Agenda
{{agenda}}

## Discussion Points
-

## Action Items
- [ ]

## Next Steps
`;

const PROJECT_TEMPLATE = `# Project: {{project_name}}

## Overview
{{description}}

## Goals
1. {{goal_1}}
2. {{goal_2}}

## Timeline
- Start: {{start_date}}
- End: {{end_date}}

## Resources
- Budget: {{budget}}
- Team: {{team_members}}
`;

const CODE_REVIEW_TEMPLATE = `# Code Review: {{pr_title}}

**PR**: {{pr_url}}
**Author**: {{author}}
**Reviewer**: {{reviewer}}

## Summary
{{summary}}

## Checklist
- [ ] Code follows style guidelines
- [ ] Tests pass
- [ ] Documentation updated
- [ ] No security issues

## Comments
`;
```

---

## Test Cases

### TMPL-001: List Templates (Empty)

**Tool**: `list_templates`

```javascript
list_templates()
```

**Expected**:
- Returns `{ templates: [], total: 0 }` or existing templates
- Response includes template metadata

**Pass Criteria**: Returns array (may be empty)

---

### TMPL-002: Create Template - Basic

**Tool**: `create_template`

```javascript
create_template({
  name: "UAT Meeting Notes",
  content: MEETING_TEMPLATE,
  description: "Standard meeting notes template",
  format: "markdown",
  default_tags: ["uat/templates", "meetings"]
})
```

**Expected**:
- Returns `{ id: "<uuid>" }`
- Template is created with all fields

**Pass Criteria**: Valid UUID returned

**Store**: `meeting_template_id`

---

### TMPL-003: Create Template - With Collection

**Tool**: `create_template`

```javascript
// First ensure collection exists
create_collection({ name: "UAT-Templates", description: "Templates for UAT" })

create_template({
  name: "UAT Project Brief",
  content: PROJECT_TEMPLATE,
  description: "Project documentation template",
  format: "markdown",
  default_tags: ["uat/templates", "projects"],
  collection_id: "<collection_id>"
})
```

**Expected**:
- Template created with collection association
- Template inherits collection context

**Pass Criteria**: Template created with collection_id set

**Store**: `project_template_id`

---

### TMPL-004: Create Template - Code Review

**Tool**: `create_template`

```javascript
create_template({
  name: "UAT Code Review",
  content: CODE_REVIEW_TEMPLATE,
  description: "Code review checklist template",
  format: "markdown",
  default_tags: ["uat/templates", "code-review"]
})
```

**Expected**: Template created

**Store**: `code_review_template_id`

---

### TMPL-005: Get Template

**Tool**: `get_template`

```javascript
get_template({ id: meeting_template_id })
```

**Expected**:
```json
{
  "id": "<uuid>",
  "name": "UAT Meeting Notes",
  "content": "<template content>",
  "description": "Standard meeting notes template",
  "format": "markdown",
  "default_tags": ["uat/templates", "meetings"],
  "created_at": "<timestamp>",
  "updated_at": "<timestamp>"
}
```

**Pass Criteria**: All fields present and match creation values

---

### TMPL-006: List Templates (After Creation)

**Tool**: `list_templates`

```javascript
list_templates()
```

**Expected**:
- Returns at least 3 templates (created in TMPL-002, 003, 004)
- Each template has id, name, description

**Pass Criteria**: `total >= 3`

---

### TMPL-007: Update Template - Content

**Tool**: `update_template`

```javascript
update_template({
  id: meeting_template_id,
  content: MEETING_TEMPLATE + "\n## Follow-up\n{{followup}}"
})
```

**Expected**:
- Template content updated
- Other fields unchanged

**Verify**: `get_template` shows new content

---

### TMPL-008: Update Template - Metadata

**Tool**: `update_template`

```javascript
update_template({
  id: project_template_id,
  name: "UAT Project Brief v2",
  description: "Updated project documentation template",
  default_tags: ["uat/templates", "projects", "v2"]
})
```

**Expected**: Metadata fields updated

**Verify**: `get_template` shows updated name, description, tags

---

### TMPL-009: Instantiate Template - Basic

**Tool**: `instantiate_template`

```javascript
instantiate_template({
  id: meeting_template_id,
  variables: {
    meeting_type: "Sprint Planning",
    date: "2026-02-02",
    attendees: "Alice, Bob, Carol",
    duration: "1 hour",
    agenda: "1. Review sprint goals\n2. Assign tasks"
  },
  revision_mode: "none"
})
```

**Expected**:
- Returns `{ id: "<note_uuid>" }`
- Note created with variables substituted
- Note has template's default_tags

**Verify**: `get_note` shows substituted content

**Store**: `instantiated_note_id`

---

### TMPL-010: Instantiate Template - With Extra Tags

**Tool**: `instantiate_template`

```javascript
instantiate_template({
  id: project_template_id,
  variables: {
    project_name: "Matric Memory UAT",
    description: "User acceptance testing for matric-memory",
    goal_1: "Achieve 100% MCP coverage",
    goal_2: "Validate all edge cases",
    start_date: "2026-02-01",
    end_date: "2026-02-15",
    budget: "$0 (internal)",
    team_members: "Development team"
  },
  tags: ["uat/instantiated", "priority/high"],
  revision_mode: "none"
})
```

**Expected**:
- Note created with both default_tags and extra tags
- All variables substituted

**Verify**: Note has merged tag set

---

### TMPL-011: Instantiate Template - With Collection

**Tool**: `instantiate_template`

```javascript
// Create target collection
create_collection({ name: "UAT-Code-Reviews", description: "Code reviews" })

instantiate_template({
  id: code_review_template_id,
  variables: {
    pr_title: "Fix search performance",
    pr_url: "https://git.example.com/pr/123",
    author: "alice",
    reviewer: "bob",
    summary: "Optimizes hybrid search query execution"
  },
  collection_id: "<code_reviews_collection_id>",
  revision_mode: "none"
})
```

**Expected**: Note created in specified collection

**Verify**: `get_collection_notes` includes the new note

---

### TMPL-012: Instantiate Template - Missing Variables

**Tool**: `instantiate_template`

```javascript
instantiate_template({
  id: meeting_template_id,
  variables: {
    meeting_type: "Standup"
    // Missing: date, attendees, duration, agenda
  },
  revision_mode: "none"
})
```

**Expected**:
- Note created with unsubstituted placeholders OR
- Error indicating missing required variables

**Pass Criteria**: Defined behavior (either approach is valid)

---

### TMPL-013: Instantiate Template - With AI Revision

**Tool**: `instantiate_template`

```javascript
instantiate_template({
  id: meeting_template_id,
  variables: {
    meeting_type: "Retrospective",
    date: "2026-02-02",
    attendees: "Team",
    duration: "45 minutes",
    agenda: "What went well, what to improve"
  },
  revision_mode: "full"
})
```

**Expected**:
- Note created and AI revision job queued
- Note may have revised content after processing

**Verify**: Check `list_jobs` for revision job

---

### TMPL-014: Delete Template

**Tool**: `delete_template`

```javascript
delete_template({ id: code_review_template_id })
```

**Expected**: 204 No Content or success response

**Verify**:
- `get_template(id)` returns 404
- `list_templates()` no longer includes it

---

### TMPL-015: Delete Template - Verify Notes Remain

**Tool**: `delete_template` + `get_note`

```javascript
// Instantiated notes should survive template deletion
get_note({ id: instantiated_note_id })
```

**Expected**: Note still exists after template deletion

**Pass Criteria**: Notes are independent of template lifecycle

---

## Cleanup

```javascript
// Delete test templates
delete_template({ id: meeting_template_id })
delete_template({ id: project_template_id })

// Delete instantiated notes (optional - may keep for other phases)
// delete_note({ id: instantiated_note_id })

// Delete test collections
// delete_collection({ id: templates_collection_id })
// delete_collection({ id: code_reviews_collection_id })
```

---

## Success Criteria

| Test | Status | Notes |
|------|--------|-------|
| TMPL-001 | | List templates works |
| TMPL-002 | | Create basic template |
| TMPL-003 | | Create with collection |
| TMPL-004 | | Create code review template |
| TMPL-005 | | Get template by ID |
| TMPL-006 | | List shows created templates |
| TMPL-007 | | Update template content |
| TMPL-008 | | Update template metadata |
| TMPL-009 | | Basic instantiation |
| TMPL-010 | | Instantiation with extra tags |
| TMPL-011 | | Instantiation to collection |
| TMPL-012 | | Missing variables handling |
| TMPL-013 | | Instantiation with AI revision |
| TMPL-014 | | Delete template |
| TMPL-015 | | Notes survive template deletion |

**Pass Rate Required**: 100% (15/15)

---

## MCP Tools Covered

| Tool | Tests |
|------|-------|
| `list_templates` | TMPL-001, TMPL-006 |
| `create_template` | TMPL-002, TMPL-003, TMPL-004 |
| `get_template` | TMPL-005 |
| `update_template` | TMPL-007, TMPL-008 |
| `instantiate_template` | TMPL-009, TMPL-010, TMPL-011, TMPL-012, TMPL-013 |
| `delete_template` | TMPL-014, TMPL-015 |

**Coverage**: 6/6 template tools (100%)
