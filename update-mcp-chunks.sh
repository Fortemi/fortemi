#!/bin/bash
# Script to update MCP server tools for chunk-aware document handling

set -e

FILE="/home/roctinam/dev/matric-memory/mcp-server/index.js"
BACKUP="${FILE}.backup-$(date +%Y%m%d-%H%M%S)"

# Create backup
cp "$FILE" "$BACKUP"
echo "Created backup: $BACKUP"

# 1. Update get_note handler to add full_document parameter
sed -i '/case "get_note":/,/break;/ {
  s|result = await apiRequest("GET", `/api/v1/notes/\${args.id}`);|const params = new URLSearchParams();\
          if (args.full_document) params.set("full_document", "true");\
          const query = params.toString() ? `?\${params}` : "";\
          result = await apiRequest("GET", `/api/v1/notes/\${args.id}\${query}`);|
}' "$FILE"

# 2. Update search_notes handler to add deduplicate_chains and expand_chains
sed -i '/case "search_notes": {/,/break;/ {
  /if (args.set) params.set("set", args.set);/a\
          if (args.deduplicate_chains !== undefined) params.set("deduplicate_chains", args.deduplicate_chains);\
          if (args.expand_chains) params.set("expand_chains", "true");
}' "$FILE"

echo "Updated handlers successfully"
echo "Backup saved to: $BACKUP"
echo ""
echo "Next steps:"
echo "1. Manually update tool schemas in the tools array (lines 817-903)"
echo "2. Add get_document_chain tool definition after get_note_links"
echo "3. Add get_document_chain handler in switch statement"
