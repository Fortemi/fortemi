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
    def native_receipt(self) -> dict:
        return {
            "generator": "scripts/rebuild-docs-shard.sh",
            "server_commit": "a" * 40,
            "source_commit": "a" * 40,
            "runtime_health": {"version": "2026.10.7", "git_sha": "a" * 40},
            "server_artifact": {
                "kind": "native-binary", "name": "matric-api",
                "sha256": "b" * 64, "version": "2026.10.7",
                "revision": "a" * 40,
            },
        }

    def test_accepts_native_artifact_without_claiming_image(self) -> None:
        self.assertEqual(MODULE.artifact_failures(self.native_receipt(), "2026.10.7"), [])

    def test_preserves_image_version_gate(self) -> None:
        self.assertEqual(MODULE.artifact_failures({"server_image": "api:2026.10.7"}, "2026.10.7"), [])
        self.assertTrue(MODULE.artifact_failures({"server_image": "api:2026.10.6"}, "2026.10.7"))
        self.assertTrue(MODULE.artifact_failures({"server_image": "api:2026.10.7", "server_artifact": None}, "2026.10.7"))

    def test_rejects_incomplete_or_forged_native_identity(self) -> None:
        for field, value in [("kind", "oci"), ("name", "other"), ("sha256", "bad"),
                             ("version", "2026.10.6"), ("revision", "c" * 40)]:
            with self.subTest(field=field):
                receipt = self.native_receipt()
                receipt["server_artifact"][field] = value
                self.assertTrue(MODULE.artifact_failures(receipt, "2026.10.7"))
        receipt = self.native_receipt()
        receipt["server_image"] = "pretend:2026.10.7"
        self.assertTrue(MODULE.artifact_failures(receipt, "2026.10.7"))

    def test_rejects_missing_or_mismatched_observed_runtime(self) -> None:
        for runtime in [None, {}, {"version": "2026.10.7", "git_sha": "unknown"},
                        {"version": "wrong", "git_sha": "a" * 40}]:
            receipt = self.native_receipt()
            receipt["runtime_health"] = runtime
            self.assertTrue(MODULE.artifact_failures(receipt, "2026.10.7"))
        receipt = self.native_receipt()
        receipt["server_artifact"]["image"] = "pretend:2026.10.7"
        self.assertTrue(MODULE.artifact_failures(receipt, "2026.10.7"))
        receipt = self.native_receipt()
        receipt["generator"] = "wrong"
        self.assertTrue(MODULE.artifact_failures(receipt, "2026.10.7"))

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
