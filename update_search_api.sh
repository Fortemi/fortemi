#!/bin/bash
# Update matric-api/src/main.rs to add search deduplication support

FILE="crates/matric-api/src/main.rs"

# 1. Update the import line
sed -i '94s|use matric_search::{HybridSearchConfig, HybridSearchEngine, SearchRequest};|use matric_search::{deduplicate_search_results, DeduplicationConfig, EnhancedSearchHit, HybridSearchConfig, HybridSearchEngine, SearchRequest};|' "$FILE"

# 2. Update SearchQuery struct - add deduplicate_chains and expand_chains fields before the closing brace
# First, find the line with "since: Option<String>," and add two lines after it
sed -i '/since: Option<String>,/a\    /// Deduplicate chunks from the same document (default: true)\n    deduplicate_chains: Option<bool>,\n    /// Expand chains to include full document content (default: false)\n    expand_chains: Option<bool>,' "$FILE"

# 3. Update SearchResponse struct to use EnhancedSearchHit
sed -i 's/results: Vec<SearchHit>,/results: Vec<EnhancedSearchHit>,/' "$FILE"

echo "Search API updated successfully"
