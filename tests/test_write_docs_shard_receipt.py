from __future__ import annotations

import importlib.util
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "write_docs_shard_receipt",
    ROOT / "scripts" / "ci" / "write-docs-shard-receipt.py",
)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class WriteDocsShardReceiptTests(unittest.TestCase):
    def write_shard(self, path: Path, revision: str) -> None:
        manifest = {
            "created_at": "2026-07-29T18:17:55Z",
            "producer": {
                "name": "fortemi",
                "revision": revision,
                "version": "2026.7.19",
            },
            "version": "1.2.0",
        }
        data = json.dumps(manifest).encode()
        with tarfile.open(path, "w:gz") as archive:
            info = tarfile.TarInfo("manifest.json")
            info.size = len(data)
            archive.addfile(info, io.BytesIO(data))

    def test_builds_receipt_from_canonical_manifest(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            shard = Path(temp_dir) / "docs.shard"
            self.write_shard(shard, "abc123")
            expected_size = shard.stat().st_size

            receipt = MODULE.build_receipt(
                shard, "registry.example/fortemi:2026.7.19", "abc123"
            )

        self.assertEqual(receipt["server_commit"], "abc123")
        self.assertEqual(receipt["source_commit"], "abc123")
        self.assertEqual(receipt["server_version"], "2026.7.19")
        self.assertEqual(receipt["manifest_version"], "1.2.0")
        self.assertEqual(receipt["byte_length"], expected_size)
        self.assertEqual(len(receipt["sha256"]), 64)

    def test_rejects_revision_mismatch(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            shard = Path(temp_dir) / "docs.shard"
            self.write_shard(shard, "wrong")

            with self.assertRaisesRegex(ValueError, "does not match source commit"):
                MODULE.build_receipt(
                    shard, "registry.example/fortemi:2026.7.19", "expected"
                )


if __name__ == "__main__":
    unittest.main()
