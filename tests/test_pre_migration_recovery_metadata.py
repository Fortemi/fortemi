"""Filesystem regression cases for recovery-point eligibility."""
import hashlib
import importlib.util
from pathlib import Path
import tempfile
import time
import unittest


SPEC = importlib.util.spec_from_file_location(
    "pre_migration_recovery",
    Path(__file__).resolve().parents[1] / "scripts/pre-migration-recovery.py",
)
recovery = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(recovery)


class RecoveryMetadataTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.artifact = self.root / "pre-migration-test.sql.gz"
        self.artifact.write_bytes(b"verified fixture artifact")
        self.meta = self.root / (self.artifact.name + ".recovery.meta")
        self.expected = {
            "from_version": "20260205000000",
            "to_version": "20260903010000",
            "db_identity_sha256": "a" * 64,
            "migration_manifest_sha256": "b" * 64,
            "state_sha256": "c" * 64,
        }
        self.values = dict(
            self.expected,
            format_version="1",
            verified="true",
            artifact_file=self.artifact.name,
            artifact_sha256=hashlib.sha256(self.artifact.read_bytes()).hexdigest(),
            artifact_size_bytes=str(self.artifact.stat().st_size),
            created_at_epoch=str(int(time.time())),
        )
        self.write_metadata()

    def write_metadata(self, **changes):
        values = dict(self.values, **changes)
        self.meta.write_text("".join(f"{key}={value}\n" for key, value in values.items()))

    def eligible(self, max_age=3600):
        return recovery.invalid_reason(self.meta, self.expected, self.root, max_age)[0]

    def test_recent_matching_artifact_is_eligible(self):
        self.assertTrue(self.eligible())

    def test_each_database_upgrade_and_state_binding_is_required(self):
        for key in self.expected:
            with self.subTest(key=key):
                self.write_metadata(**{key: "different"})
                self.assertFalse(self.eligible())

    def test_missing_or_changed_artifact_is_ineligible(self):
        self.artifact.unlink()
        self.assertFalse(self.eligible())
        self.artifact.write_bytes(b"tampered fixture artifact")
        self.assertFalse(self.eligible())

    def test_missing_unverified_or_stale_metadata_is_ineligible(self):
        self.meta.unlink()
        self.assertFalse(self.eligible())
        self.write_metadata(verified="false")
        self.assertFalse(self.eligible())
        self.write_metadata(created_at_epoch=str(int(time.time()) - 3601))
        self.assertFalse(self.eligible())

    def test_future_timestamp_is_not_a_recent_verified_point(self):
        self.write_metadata(created_at_epoch=str(int(time.time()) + 3600))
        self.assertFalse(self.eligible())

    def test_zero_age_setting_disables_reuse(self):
        self.assertFalse(self.eligible(max_age=0))

    def test_duplicate_binding_is_corrupt_metadata(self):
        with self.meta.open("a") as stream:
            stream.write("state_sha256=conflicting-value\n")
        self.assertFalse(self.eligible())

    def test_unknown_or_missing_metadata_version_is_ineligible(self):
        self.write_metadata(format_version="99")
        self.assertFalse(self.eligible())
        del self.values["format_version"]
        self.write_metadata()
        self.assertFalse(self.eligible())


if __name__ == "__main__":
    unittest.main()
