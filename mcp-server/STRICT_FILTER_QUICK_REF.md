# Strict Filter Quick Reference

## MCP Tool Usage

### Option 1: Enhanced `search_notes` (Backward Compatible)

```json
{
  "name": "search_notes",
  "arguments": {
    "query": "authentication methods",
    "mode": "hybrid",
    "limit": 20,
    "strict_filter": {
      "required_tags": ["security", "reviewed"],
      "excluded_tags": ["archived", "deprecated"],
      "required_schemes": ["client-acme"]
    }
  }
}
```

### Option 2: Dedicated `search_notes_strict`

```json
{
  "name": "search_notes_strict",
  "arguments": {
    "query": "login flow",
    "required_tags": ["auth", "production"],
    "any_tags": ["oauth", "saml"],
    "excluded_tags": ["draft"],
    "required_schemes": ["client-acme"],
    "mode": "hybrid",
    "limit": 10
  }
}
```

## Filter Parameters

| Parameter | Logic | Example |
|-----------|-------|---------|
| `required_tags` | AND (all must match) | `["python", "async"]` → needs BOTH tags |
| `any_tags` | OR (at least one) | `["bug", "issue"]` → needs bug OR issue |
| `excluded_tags` | NOT (none allowed) | `["archived"]` → cannot have archived tag |
| `required_schemes` | Scheme whitelist | `["client-a"]` → ONLY from client-a scheme |
| `excluded_schemes` | Scheme blacklist | `["test"]` → NOT from test scheme |

## Common Patterns

### Client Isolation (Multi-tenancy)
```json
{
  "required_schemes": ["client-acme"]
}
```

### Project Scoping
```json
{
  "required_tags": ["project:matric"],
  "excluded_tags": ["archived"]
}
```

### Priority OR Filter
```json
{
  "any_tags": ["urgent", "high-priority", "critical"]
}
```

### Reviewed & Active Content
```json
{
  "required_tags": ["reviewed"],
  "excluded_tags": ["draft", "deprecated", "archived"]
}
```

### Complex Filter (AND + OR + NOT)
```json
{
  "required_tags": ["security", "approved"],
  "any_tags": ["oauth", "jwt", "saml"],
  "excluded_tags": ["draft", "experimental"],
  "required_schemes": ["production"]
}
```

## API Wire Format

Filters are passed to API as JSON string in `filters` query parameter:

```
GET /api/v1/search?q=auth&filters=%7B%22required_tags%22%3A%5B%22security%22%5D%7D
```

Decoded:
```
GET /api/v1/search?q=auth&filters={"required_tags":["security"]}
```

## Filter Resolution Logic

When multiple filter types are used, they are combined with AND logic:

```
(required_tags[0] AND required_tags[1] AND ...)
  AND
(any_tags[0] OR any_tags[1] OR ...)
  AND NOT
(excluded_tags[0] OR excluded_tags[1] OR ...)
  AND
(scheme IN required_schemes)
  AND
(scheme NOT IN excluded_schemes)
```

## Testing Filters

```bash
cd mcp-server
node test-strict-filter.js
```

Expected output:
```
✅ All tests passed!
  ✓ buildStrictFilter() helper function added
  ✓ search_notes handler updated to process strict_filter
  ✓ search_notes_strict handler implemented
  ✓ search_notes tool schema updated with strict_filter
  ✓ search_notes_strict tool defined
```

## Implementation Notes

- Empty arrays are ignored (treated as "no filter")
- `null` or `undefined` filter parameters are ignored
- Query text is optional in `search_notes_strict`
- Filters are applied BEFORE ranking/scoring
- Guaranteed result isolation (no fuzzy matching in filters)

## Troubleshooting

**Issue:** Filters not applied
- Check JSON encoding in query parameter
- Verify API implements filter parsing
- Ensure filter arrays are not empty

**Issue:** No results with filters
- Try removing filters one at a time
- Check tag names are exact matches (case-sensitive)
- Verify schemes exist in database

**Issue:** Too many results
- Add more `required_tags` to narrow down
- Use `excluded_tags` to remove unwanted categories
- Reduce `limit` parameter
