#!/bin/bash
# Self-Maintenance Demo: Index Fortémi's own codebase
# Demonstrates code document types, chunking, and semantic search

set -e

API_URL="${FORTEMI_API_URL:-http://localhost:3000}"

echo "=== Fortémi Self-Maintenance Demo ==="
echo "Indexing Fortémi codebase into itself..."
echo ""

# Create a collection for the codebase
echo "Creating collection for codebase..."
COLLECTION_ID=$(curl -s -X POST "$API_URL/api/v1/collections" \
  -H "Content-Type: application/json" \
  -d '{"name": "fortemi-codebase", "description": "Self-indexed codebase for semantic code search"}' \
  | jq -r '.id')

echo "Created collection: $COLLECTION_ID"
echo ""

# Index key Rust files from matric-core
echo "Indexing Rust source files..."
indexed_count=0
for file in crates/matric-core/src/*.rs crates/matric-db/src/*.rs; do
  if [ -f "$file" ]; then
    content=$(cat "$file" | jq -Rs .)
    filename=$(basename "$file")

    curl -s -X POST "$API_URL/api/v1/notes" \
      -H "Content-Type: application/json" \
      -d "{
        \"content\": $content,
        \"format\": \"rust\",
        \"source\": \"self-index\",
        \"collection_id\": \"$COLLECTION_ID\",
        \"tags\": [\"rust\", \"source-code\"]
      }" > /dev/null

    echo "  Indexed: $file"
    indexed_count=$((indexed_count + 1))
  fi
done

echo ""
echo "Indexed $indexed_count Rust files"
echo ""

# Index TypeScript MCP server files
echo "Indexing TypeScript MCP server..."
ts_count=0
for file in mcp-server/*.ts; do
  if [ -f "$file" ]; then
    content=$(cat "$file" | jq -Rs .)
    filename=$(basename "$file")

    curl -s -X POST "$API_URL/api/v1/notes" \
      -H "Content-Type: application/json" \
      -d "{
        \"content\": $content,
        \"format\": \"typescript\",
        \"source\": \"self-index\",
        \"collection_id\": \"$COLLECTION_ID\",
        \"tags\": [\"typescript\", \"mcp-server\"]
      }" > /dev/null

    echo "  Indexed: $file"
    ts_count=$((ts_count + 1))
  fi
done

echo ""
echo "Indexed $ts_count TypeScript files"
echo ""

# Index SQL migrations
echo "Indexing SQL migrations..."
sql_count=0
for file in migrations/*.sql; do
  if [ -f "$file" ]; then
    content=$(cat "$file" | jq -Rs .)
    filename=$(basename "$file")

    curl -s -X POST "$API_URL/api/v1/notes" \
      -H "Content-Type: application/json" \
      -d "{
        \"content\": $content,
        \"format\": \"sql\",
        \"source\": \"self-index\",
        \"collection_id\": \"$COLLECTION_ID\",
        \"tags\": [\"sql\", \"migration\"]
      }" > /dev/null

    echo "  Indexed: $file"
    sql_count=$((sql_count + 1))
  fi
done

echo ""
echo "Indexed $sql_count SQL migration files"
echo ""

# Index key documentation files
echo "Indexing documentation..."
doc_count=0
for file in docs/content/architecture.md docs/content/api.md docs/content/chunking.md; do
  if [ -f "$file" ]; then
    content=$(cat "$file" | jq -Rs .)
    filename=$(basename "$file")

    curl -s -X POST "$API_URL/api/v1/notes" \
      -H "Content-Type: application/json" \
      -d "{
        \"content\": $content,
        \"format\": \"markdown\",
        \"source\": \"self-index\",
        \"collection_id\": \"$COLLECTION_ID\",
        \"tags\": [\"documentation\"]
      }" > /dev/null

    echo "  Indexed: $file"
    doc_count=$((doc_count + 1))
  fi
done

echo ""
echo "Indexed $doc_count documentation files"
echo ""

# Wait a moment for embeddings to be generated
echo "Waiting 5 seconds for embeddings to be generated..."
sleep 5

# Demonstrate semantic code search
echo "=== Semantic Code Search Demo ==="
echo ""

echo "Query 1: 'embedding repository trait'"
echo "---------------------------------------"
curl -s "$API_URL/api/v1/search?q=embedding+repository+trait&limit=3" \
  | jq -r '.notes[] | "  - \(.title // "Untitled") (score: \(.score))\n    Tags: \(.tags | join(", "))"'

echo ""
echo "Query 2: 'document type detection'"
echo "-----------------------------------"
curl -s "$API_URL/api/v1/search?q=document+type+detection&limit=3" \
  | jq -r '.notes[] | "  - \(.title // "Untitled") (score: \(.score))\n    Tags: \(.tags | join(", "))"'

echo ""
echo "Query 3: 'chunking strategies'"
echo "------------------------------"
curl -s "$API_URL/api/v1/search?q=chunking+strategies&limit=3" \
  | jq -r '.notes[] | "  - \(.title // "Untitled") (score: \(.score))\n    Tags: \(.tags | join(", "))"'

echo ""
echo "Query 4: 'SQL schema migrations'"
echo "--------------------------------"
curl -s "$API_URL/api/v1/search?q=SQL+schema+migrations&limit=3" \
  | jq -r '.notes[] | "  - \(.title // "Untitled") (score: \(.score))\n    Tags: \(.tags | join(", "))"'

echo ""
echo "=== Demo Complete ==="
echo ""
echo "Collection ID: $COLLECTION_ID"
echo "Total indexed: $((indexed_count + ts_count + sql_count + doc_count)) files"
echo ""
echo "You can now:"
echo "  1. Search the codebase semantically via the API"
echo "  2. Use MCP tools to explore relationships between code files"
echo "  3. Find relevant implementations by describing functionality"
echo ""
