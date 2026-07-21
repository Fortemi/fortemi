from __future__ import annotations

import importlib.util
from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "verify_docs_shard_freshness",
    ROOT / "scripts" / "ci" / "verify-docs-shard-freshness.py",
)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class ManifestServerVersionTests(unittest.TestCase):
    def test_reads_canonical_producer_version(self) -> None:
        manifest = {
            "producer": {"name": "fortemi", "version": "2026.7.10"},
            "matric_version": "legacy-value",
        }

        self.assertEqual(MODULE.manifest_server_version(manifest), "2026.7.10")

    def test_accepts_legacy_manifest_version(self) -> None:
        manifest = {"matric_version": "2026.7.1"}

        self.assertEqual(MODULE.manifest_server_version(manifest), "2026.7.1")


if __name__ == "__main__":
    unittest.main()
