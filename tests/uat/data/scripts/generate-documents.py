#!/usr/bin/env python3
"""Generate document samples (Markdown, JSON, YAML, CSV)."""

from pathlib import Path
import json


MARKDOWN_SAMPLE = """# Test Document: Markdown Formatting

This document tests markdown chunking with semantic paragraph splitting.

## Introduction

Markdown is a lightweight markup language for creating formatted text. It supports:

- **Bold** and *italic* text
- `Inline code` snippets
- [Links](https://example.com)
- ![Images](image.jpg)

## Code Examples

Here's a Python code block:

```python
def hello_world():
    print("Hello, World!")
```

And a JavaScript example:

```javascript
const greet = () => console.log("Hello!");
```

## Tables

| Column 1 | Column 2 | Column 3 |
|----------|----------|----------|
| Data 1   | Data 2   | Data 3   |
| Data 4   | Data 5   | Data 6   |

## Lists

1. First item
2. Second item
   - Nested item A
   - Nested item B
3. Third item

## Blockquotes

> This is a blockquote.
> It can span multiple lines.

## Horizontal Rules

---

## Conclusion

This document covers various markdown elements for comprehensive testing.

### Key Takeaways

- Markdown is simple and readable
- Supports code, tables, and formatting
- Widely used for documentation

**Important**: Always validate your markdown rendering.
"""

JSON_CONFIG = {
    "name": "matric-memory-test-config",
    "version": "1.0.0",
    "database": {
        "host": "localhost",
        "port": 5432,
        "name": "matric_test",
        "pool": {
            "min_connections": 2,
            "max_connections": 10,
            "timeout_seconds": 30
        }
    },
    "embedding": {
        "model": "nomic-embed-text-v1.5",
        "dimensions": 768,
        "truncate_dim": 256,
        "batch_size": 100
    },
    "search": {
        "fts_script_detection": True,
        "fts_trigram_fallback": True,
        "fts_bigram_cjk": True,
        "default_limit": 50,
        "max_limit": 500
    },
    "features": [
        "semantic_search",
        "provenance_tracking",
        "multilingual_fts",
        "document_chunking"
    ],
    "logging": {
        "level": "info",
        "format": "json",
        "outputs": ["stdout", "file"]
    }
}

YAML_CONFIG = """name: matric-memory-test-config
version: 1.0.0

database:
  host: localhost
  port: 5432
  name: matric_test
  pool:
    min_connections: 2
    max_connections: 10
    timeout_seconds: 30

embedding:
  model: nomic-embed-text-v1.5
  dimensions: 768
  truncate_dim: 256
  batch_size: 100

search:
  fts_script_detection: true
  fts_trigram_fallback: true
  fts_bigram_cjk: true
  default_limit: 50
  max_limit: 500

features:
  - semantic_search
  - provenance_tracking
  - multilingual_fts
  - document_chunking

logging:
  level: info
  format: json
  outputs:
    - stdout
    - file
"""


def generate_csv(filepath: Path, rows: int = 100):
    """Generate a CSV file with test data."""
    import csv
    import random
    from datetime import datetime, timedelta

    names = [
        "Alice Johnson", "Bob Smith", "Charlie Davis", "Diana Prince",
        "Eve Williams", "Frank Miller", "Grace Lee", "Henry Taylor",
        "Ivy Chen", "Jack Wilson", "Kate Brown", "Leo Martinez",
        "Maya Patel", "Noah Garcia", "Olivia Rodriguez", "Paul Anderson"
    ]

    with open(filepath, 'w', newline='') as f:
        writer = csv.writer(f)
        writer.writerow(['id', 'name', 'email', 'created_at', 'status'])

        for i in range(1, rows + 1):
            name = random.choice(names)
            first_name = name.split()[0].lower()
            email = f"{first_name}.{i}@example.com"
            created = datetime(2024, 1, 1) + timedelta(days=i % 365)
            status = random.choice(['active', 'inactive', 'pending'])
            writer.writerow([i, name, email, created.isoformat() + 'Z', status])


def main():
    script_dir = Path(__file__).parent
    data_dir = script_dir.parent
    documents_dir = data_dir / "documents"
    documents_dir.mkdir(parents=True, exist_ok=True)

    print("Generating document samples...")

    # Markdown
    print("  Creating markdown-formatted.md...")
    (documents_dir / "markdown-formatted.md").write_text(MARKDOWN_SAMPLE)

    # JSON
    print("  Creating json-config.json...")
    (documents_dir / "json-config.json").write_text(
        json.dumps(JSON_CONFIG, indent=2)
    )

    # YAML
    print("  Creating yaml-config.yaml...")
    (documents_dir / "yaml-config.yaml").write_text(YAML_CONFIG)

    # CSV
    print("  Creating csv-data.csv...")
    generate_csv(documents_dir / "csv-data.csv", rows=100)

    print("")
    print("✓ Generated 4 document files")
    print("  Formats: Markdown, JSON, YAML, CSV")


if __name__ == "__main__":
    main()
