import importlib.util
import tempfile
import unittest
from pathlib import Path
from unittest import mock


SCRIPT = Path(__file__).resolve().parents[1] / "scripts/pre-migration-recovery.py"
SPEC = importlib.util.spec_from_file_location("pre_migration_recovery", SCRIPT)
recovery = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(recovery)


class FinalStateCleanupTest(unittest.TestCase):
    def test_export_snapshot_failure_removes_new_artifact(self):
        with tempfile.TemporaryDirectory() as tmp:
            backup_dest = Path(tmp)
            artifact = backup_dest / "pre-migration-from-to.sql.gz"
            metadata = backup_dest / "pre-migration-from-to.sql.gz.recovery.meta"
            artifact.write_text("new dump", encoding="utf-8")
            metadata.write_text("should not survive", encoding="utf-8")

            with mock.patch.object(recovery, "export_snapshot", side_effect=RuntimeError("snapshot unavailable")):
                with self.assertRaises(RuntimeError):
                    recovery.validate_final_state("matric", backup_dest, artifact.name, ("state", "seq", "schema"))

            self.assertFalse(artifact.exists())
            self.assertFalse(metadata.exists())


if __name__ == "__main__":
    unittest.main()
