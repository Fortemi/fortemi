# Matric Memory MCP Server

MCP (Model Context Protocol) server that exposes the Matric Memory API as tools for AI agents.

## Installation

```bash
cd mcp-server
npm install
```

## Usage

### With Claude Desktop

Add to your Claude Desktop config (`~/.config/claude/claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "matric-memory": {
      "command": "node",
      "args": ["/path/to/matric-memory/mcp-server/index.js"],
      "env": {
        "MATRIC_MEMORY_URL": "https://memory.integrolabs.net"
      }
    }
  }
}
```

### With Claude Code

Add to your project's `.mcp.json`:

```json
{
  "mcpServers": {
    "matric-memory": {
      "command": "node",
      "args": ["./mcp-server/index.js"]
    }
  }
}
```

## Available Tools

| Tool | Description |
|------|-------------|
| `list_notes` | List all notes with summaries |
| `get_note` | Get full note details |
| `create_note` | Create a new note |
| `update_note` | Update note content/status |
| `delete_note` | Soft delete a note |
| `search_notes` | Full-text and semantic search |
| `list_tags` | List all tags |
| `set_note_tags` | Set tags for a note |
| `get_note_links` | Get note relationships |
| `create_job` | Queue AI processing jobs |

## Environment Variables

- `MATRIC_MEMORY_URL` - API base URL (default: `https://memory.integrolabs.net`)

## Example

```
User: Search my notes for anything about API design

Claude: [uses search_notes tool with query "API design"]

Found 3 notes about API design:
1. "REST API Best Practices" - discusses versioning and error handling
2. "GraphQL vs REST" - comparison of approaches
3. "API Documentation" - notes on OpenAPI specs
```
