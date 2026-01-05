# Matric Memory MCP Server

MCP (Model Context Protocol) server that exposes the Matric Memory API as tools for AI agents.

## Installation

```bash
cd mcp-server
npm install
```

## Transport Modes

The server supports two transport modes:

### Stdio Transport (Default)

For local CLI integration with Claude Desktop or Claude Code. No authentication required - uses API key.

### HTTP Transport

For remote access with OAuth2 authentication. Supports both:
- **StreamableHTTP** - Modern transport using `POST/GET/DELETE /` with `MCP-Session-Id` header
- **SSE** - Legacy transport using `GET /sse` + `POST /messages?sessionId=X`

## Usage

### With Claude Desktop (Stdio)

Add to your Claude Desktop config (`~/.config/claude/claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "matric-memory": {
      "command": "node",
      "args": ["/path/to/matric-memory/mcp-server/index.js"],
      "env": {
        "MATRIC_MEMORY_URL": "https://memory.integrolabs.net",
        "MATRIC_MEMORY_API_KEY": "your-api-key"
      }
    }
  }
}
```

### With Claude Code (Stdio)

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

### HTTP Mode (Remote Access)

Start the server in HTTP mode:

```bash
MCP_TRANSPORT=http MCP_PORT=3001 node index.js
```

The server exposes:

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/` | POST | StreamableHTTP: Initialize session or send messages |
| `/` | GET | StreamableHTTP: Receive server messages (SSE stream) |
| `/` | DELETE | StreamableHTTP: Terminate session |
| `/sse` | GET | SSE: Open SSE connection |
| `/messages` | POST | SSE: Send messages to session |
| `/health` | GET | Health check with session counts |
| `/.well-known/oauth-authorization-server` | GET | OAuth2 authorization server metadata |
| `/.well-known/oauth-protected-resource` | GET | OAuth2 protected resource metadata (RFC 9728) |

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

| Variable | Default | Description |
|----------|---------|-------------|
| `MATRIC_MEMORY_URL` | `https://memory.integrolabs.net` | API base URL |
| `MATRIC_MEMORY_API_KEY` | - | API key for stdio mode |
| `MCP_TRANSPORT` | `stdio` | Transport mode: `stdio` or `http` |
| `MCP_PORT` | `3001` | HTTP server port (http mode only) |
| `MCP_BASE_URL` | `http://localhost:${MCP_PORT}` | Base URL for OAuth metadata |
| `MCP_BASE_PATH` | - | Path prefix when behind proxy (e.g., `/mcp`) |
| `MCP_CLIENT_ID` | - | OAuth client ID for token introspection |
| `MCP_CLIENT_SECRET` | - | OAuth client secret for token introspection |

## OAuth2 Authentication (HTTP Mode)

The HTTP transport requires OAuth2 bearer tokens. The server validates tokens against the main API's introspection endpoint.

Required scopes: `mcp` or `read`

401 responses include RFC 9728 compliant `WWW-Authenticate` headers pointing to the protected resource metadata.

## Example

```
User: Search my notes for anything about API design

Claude: [uses search_notes tool with query "API design"]

Found 3 notes about API design:
1. "REST API Best Practices" - discusses versioning and error handling
2. "GraphQL vs REST" - comparison of approaches
3. "API Documentation" - notes on OpenAPI specs
```
