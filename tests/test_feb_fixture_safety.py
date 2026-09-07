"""Exercise fixture rejection paths without touching Docker or a database."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/ci/run-feb-to-current-fixture.sh"
MATRIC_DB_LIB = ROOT / "crates/matric-db/src/lib.rs"


class FixtureSafetyTests(unittest.TestCase):
    def run_fixture(self, seed, extra_env=None):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            calls = root / "calls"
            docker = root / "docker"
            docker.write_text(
                "#!/usr/bin/env python3\n"
                "import os, sys\n"
                "with open(os.environ['FIXTURE_CALLS'], 'a') as f:\n"
                "    f.write(' '.join(sys.argv[1:]) + '\\n')\n"
                "if sys.argv[1:3] == ['image', 'inspect']: sys.exit(0)\n"
                "if sys.argv[1] == 'run':\n"
                "    print('container name already exists', file=sys.stderr)\n"
                "    sys.exit(125)\n"
                "sys.exit(99)\n"
            )
            docker.chmod(0o755)
            env = dict(os.environ, PATH=f"{root}:{os.environ['PATH']}",
                       FIXTURE_CALLS=str(calls), FORTEMI_SEED_NOTES=seed,
                       FORTEMI_TESTDB_CONTAINER="existing-customer-container")
            if extra_env:
                env.update(extra_env)
            result = subprocess.run(["bash", str(SCRIPT)], env=env,
                                    capture_output=True, text=True, timeout=10)
            return result, calls.read_text() if calls.exists() else ""

    def test_undersized_or_invalid_seed_never_calls_docker(self):
        for seed in ["0", "99999", "-1", "invalid", "1000000000"]:
            with self.subTest(seed=seed):
                result, calls = self.run_fixture(seed)
                self.assertEqual(result.returncode, 2)
                self.assertEqual(calls, "")

    def test_name_collision_does_not_remove_existing_container(self):
        result, calls = self.run_fixture("100000")
        self.assertEqual(result.returncode, 125)
        self.assertIn("container name already exists", result.stderr)
        self.assertEqual([line.split()[0] for line in calls.splitlines()],
                         ["image", "run"])

    def test_small_fixture_requires_explicit_test_switch(self):
        result, calls = self.run_fixture("1000")
        self.assertEqual(result.returncode, 2)
        self.assertEqual(calls, "")

        result, calls = self.run_fixture(
            "1000",
            {"FORTEMI_ALLOW_SMALL_FEB_FIXTURE": "true"},
        )
        self.assertEqual(result.returncode, 125)
        self.assertIn("container name already exists", result.stderr)
        self.assertEqual([line.split()[0] for line in calls.splitlines()],
                         ["image", "run"])

    def test_fixture_declares_production_profile_and_baseline_gaps(self):
        script = SCRIPT.read_text()
        for required in [
            "note_revision",
            "embedding",
            "job_queue",
            "archive_registry",
            "attachment_blob",
            "attachment",
            "attachment_embedding",
        ]:
            with self.subTest(required=required):
                self.assertIn(required, script)
        for later_only in [
            "inbound_source",
            "event_outbox",
            "incoming_webhook_receiver",
        ]:
            with self.subTest(later_only=later_only):
                self.assertIn(f"{later_only}=baseline-missing", script)
        self.assertIn("FORTEMI_SEEDED_TABLE_COUNTS", script)
        self.assertIn("restore_drill_counts", script)
        self.assertIn("validate_fixture_invariants", script)
        self.assertIn("chunk_sequence_bad_count", script)
        self.assertIn("archive_fixture_research.note", script)
        self.assertIn("archive_schema_note_count", script)
        self.assertIn("gen_uuid_v7_default_count", script)

    def test_legacy_archive_repair_runs_after_ledger_validation(self):
        source = MATRIC_DB_LIB.read_text()
        dirty_check = source.index("dirty_version")
        missing_version_check = source.index("MigrateError::VersionMissing")
        checksum_check = source.index("MigrateError::VersionMismatch")
        repair_call = source.index("repair_legacy_archive_tenant_note_indexes")
        apply_call = source.index("conn.apply(migration)")

        self.assertLess(dirty_check, repair_call)
        self.assertLess(missing_version_check, repair_call)
        self.assertLess(checksum_check, repair_call)
        self.assertLess(repair_call, apply_call)


if __name__ == "__main__":
    unittest.main()
