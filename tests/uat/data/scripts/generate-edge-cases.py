#!/usr/bin/env python3
"""Generate edge case test files."""

import os
from pathlib import Path


def main():
    script_dir = Path(__file__).parent
    data_dir = script_dir.parent
    edge_cases_dir = data_dir / "edge-cases"
    edge_cases_dir.mkdir(parents=True, exist_ok=True)

    print("Generating edge case files...")

    # 1. Empty file
    print("  Creating empty.txt...")
    (edge_cases_dir / "empty.txt").write_text("")

    # 2. Large text file (>100KB)
    print("  Creating large-text-100kb.txt...")
    lorem = """Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum.

"""
    large_file = edge_cases_dir / "large-text-100kb.txt"
    with open(large_file, 'w') as f:
        while f.tell() < 100_000:
            f.write(lorem)
    print(f"    Size: {large_file.stat().st_size} bytes")

    # 3. Binary file with wrong extension
    print("  Creating binary-wrong-ext.jpg...")
    (edge_cases_dir / "binary-wrong-ext.jpg").write_bytes(
        os.urandom(10240)  # 10KB random bytes
    )

    # 4. Unicode filename
    print("  Creating unicode-filename-测试.txt...")
    (edge_cases_dir / "unicode-filename-测试.txt").write_text(
        "This file has Unicode in its filename: 测试\n\n"
        "Testing Chinese characters, emoji 🎉, and special symbols: ™©®\n"
    )

    # 5. Whitespace-only file
    print("  Creating whitespace-only.txt...")
    (edge_cases_dir / "whitespace-only.txt").write_text(
        "    \n\t\t  \n\n    \n" * 20
    )

    # 6. Malformed JSON
    print("  Creating malformed-json.json...")
    malformed_json = """{
  "name": "test",
  "value": 123,
  "nested": {
    "key": "value"
    "missing_comma": true
  },
  "trailing_comma": true,
}"""
    (edge_cases_dir / "malformed-json.json").write_text(malformed_json)

    print("")
    print("✓ Generated 6 edge case files")
    print("  Types: empty, large, binary-mismatch, unicode, whitespace, malformed")


if __name__ == "__main__":
    main()
