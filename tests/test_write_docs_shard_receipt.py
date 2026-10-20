from __future__ import annotations

import hashlib
import importlib.util
import io
import json
import subprocess
import sys
from pathlib import Path
import tarfile
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "write_docs_shard_receipt", ROOT / "scripts/ci/write-docs-shard-receipt.py"
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


class ReceiptArtifactTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.shard = self.root / "docs.shard"
        self.binary = self.root / "matric-api"
        self.binary.write_bytes(b"test-native-executable")
        self.health = {"version": "2026.10.7", "git_sha": "a" * 40}
        self.digest = hashlib.sha256(self.binary.read_bytes()).hexdigest()
        content = json.dumps({
            "version": "1.2.0", "created_at": "2026-10-20T00:00:00Z",
            "producer": {"version": "2026.10.7", "revision": "a" * 40},
        }).encode()
        with tarfile.open(self.shard, "w:gz") as archive:
            member = tarfile.TarInfo("manifest.json")
            member.size = len(content)
            archive.addfile(member, io.BytesIO(content))

    def test_native_records_binary_hash_and_importer_not_image(self) -> None:
        receipt = MODULE.build_receipt(self.shard, None, "a" * 40, self.binary, self.health, self.digest)
        self.assertNotIn("server_image", receipt)
        self.assertEqual(receipt["generator"], "scripts/rebuild-docs-shard.sh")
        self.assertEqual(receipt["server_artifact"]["sha256"], hashlib.sha256(self.binary.read_bytes()).hexdigest())
        self.assertEqual(receipt["server_artifact"]["revision"], "a" * 40)

    def test_image_receipt_remains_compatible(self) -> None:
        receipt = MODULE.build_receipt(self.shard, "api:2026.10.7", "a" * 40)
        self.assertEqual(receipt["server_image"], "api:2026.10.7")
        self.assertNotIn("server_artifact", receipt)
        self.assertEqual(receipt["generator"], "scripts/ci/rebuild-shard-in-ci.sh")

    def test_rejects_missing_or_ambiguous_artifact(self) -> None:
        for image, binary in [(None, None), ("api:2026.10.7", self.binary)]:
            with self.assertRaises(ValueError):
                MODULE.build_receipt(self.shard, image, "a" * 40, binary)

    def test_rejects_producer_revision_mismatch(self) -> None:
        with self.assertRaisesRegex(ValueError, "does not match source"):
            MODULE.build_receipt(self.shard, None, "c" * 40, self.binary)

    def test_rejects_missing_unknown_and_mismatched_runtime(self) -> None:
        for health in [None, {}, {"version": "wrong", "git_sha": "a" * 40},
                       {"version": "2026.10.7"},
                       {"version": "2026.10.7", "git_sha": "unknown"},
                       {"version": "2026.10.7", "git_sha": "c" * 40}]:
            with self.subTest(health=health), self.assertRaises(ValueError):
                MODULE.build_receipt(self.shard, None, "a" * 40, self.binary, health, self.digest)

    def test_rejects_changed_binary(self) -> None:
        self.binary.write_bytes(b"changed-after-build")
        with self.assertRaisesRegex(ValueError, "changed"):
            MODULE.build_receipt(self.shard, None, "a" * 40, self.binary, self.health, self.digest)

    def test_rejects_mixed_image_runtime_identity(self) -> None:
        with self.assertRaisesRegex(ValueError, "native runtime"):
            MODULE.build_receipt(self.shard, "api:2026.10.7", "a" * 40, None, self.health, self.digest)

    def test_native_cli_happy_path(self) -> None:
        health_file = self.root / "health.json"
        health_file.write_text(json.dumps(self.health))
        receipt_file = self.root / "receipt.json"
        result = subprocess.run([
            sys.executable, str(ROOT / "scripts/ci/write-docs-shard-receipt.py"),
            "--server-binary", str(self.binary), "--runtime-health", str(health_file),
            "--expected-binary-sha256", self.digest, "--source-commit", "a" * 40,
            "--shard", str(self.shard), "--receipt", str(receipt_file),
        ], capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(receipt_file.read_text())["runtime_health"], self.health)

    def test_cli_rejects_missing_or_conflicting_artifact_options(self) -> None:
        for options in [[], ["--server-image", "api:2026.10.7", "--server-binary", str(self.binary)]]:
            result = subprocess.run(
                [sys.executable, str(ROOT / "scripts/ci/write-docs-shard-receipt.py"), *options],
                capture_output=True, text=True, timeout=10,
            )
            self.assertEqual(result.returncode, 2)


if __name__ == "__main__":
    unittest.main()
