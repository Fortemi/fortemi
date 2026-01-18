#!/usr/bin/env python3
"""
Update matric-api/src/main.rs to add search deduplication support.
"""

import re

FILE_PATH = "crates/matric-api/src/main.rs"

def update_file():
    with open(FILE_PATH, 'r') as f:
        content = f.read()

    # 1. Update import line 94
    old_import = "use matric_search::{HybridSearchConfig, HybridSearchEngine, SearchRequest};"
    new_import = "use matric_search::{deduplicate_search_results, DeduplicationConfig, EnhancedSearchHit, HybridSearchConfig, HybridSearchEngine, SearchRequest};"

    content = content.replace(old_import, new_import)

    # 2. Update SearchQuery struct - add two new fields after "since: Option<String>,"
    search_query_pattern = r'(struct SearchQuery \{[^}]*since: Option<String>,)'
    search_query_replacement = r'\1\n    /// Deduplicate chunks from the same document (default: true)\n    deduplicate_chains: Option<bool>,\n    /// Expand chains to include full document content (default: false)\n    expand_chains: Option<bool>,'

    content = re.sub(search_query_pattern, search_query_replacement, content, count=1)

    # 3. Update SearchResponse struct results field type
    old_response = "    results: Vec<SearchHit>,"
    new_response = "    results: Vec<EnhancedSearchHit>,"

    # Only replace in SearchResponse struct context
    content = re.sub(
        r'(struct SearchResponse \{[^}]*)results: Vec<SearchHit>,',
        r'\1results: Vec<EnhancedSearchHit>,',
        content,
        count=1
    )

    # 4. Update search_notes function to use deduplication
    # Find the section that creates SearchResponse
    old_search_fn_end = r'''(    let results = request\.execute\(&state\.search\)\.await\?;
    let total = results\.len\(\);

    Ok\(Json\(SearchResponse \{
        results,
        query: query\.q,
        total,
    \}\)\))'''

    new_search_fn_end = '''    let results = request.execute(&state.search).await?;

    // Apply deduplication based on query parameters
    let dedup_config = DeduplicationConfig {
        deduplicate_chains: query.deduplicate_chains.unwrap_or(true),
        expand_chains: query.expand_chains.unwrap_or(false),
    };

    let deduplicated = deduplicate_search_results(results, &dedup_config);
    let total = deduplicated.len();

    Ok(Json(SearchResponse {
        results: deduplicated,
        query: query.q,
        total,
    }))'''

    content = re.sub(old_search_fn_end, new_search_fn_end, content, count=1)

    with open(FILE_PATH, 'w') as f:
        f.write(content)

    print("Successfully updated main.rs")

if __name__ == "__main__":
    update_file()
