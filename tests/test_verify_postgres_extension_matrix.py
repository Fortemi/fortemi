from __future__ import annotations

import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "ci" / "verify-postgres-extension-matrix.py"
DOC = "docs/deployment/managed-postgres-compatibility.md"

BASE_MATRIX = """\
required      vector
prerequisite  postgis
optional      pg_bigm
"""

BASE_MIGRATION = """\
-- CREATE EXTENSION postgis is done by the entrypoint, not here.
CREATE EXTENSION IF NOT EXISTS vector;
DO $$
BEGIN
    BEGIN
        CREATE EXTENSION IF NOT EXISTS pg_bigm;
    EXCEPTION WHEN OTHERS THEN
        RAISE NOTICE 'pg_bigm not available';
    END;
END $$;
"""


class ExtensionMatrixTests(unittest.TestCase):
    def test_current_repository_passes(self) -> None:
        result = self.run_check(ROOT)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_fixture_passes(self) -> None:
        result = self.run_fixture(BASE_MATRIX, {"001_init.sql": BASE_MIGRATION})
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_new_required_extension_without_matrix_entry_fails(self) -> None:
        result = self.run_fixture(
            BASE_MATRIX,
            {
                "001_init.sql": BASE_MIGRATION,
                "002_new.sql": 'CREATE EXTENSION IF NOT EXISTS "pg_cron";\n',
            },
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn("pg_cron: created as required", result.stderr)

    def test_required_extension_listed_optional_fails(self) -> None:
        matrix = BASE_MATRIX.replace("required      vector", "optional      vector")
        result = self.run_fixture(matrix, {"001_init.sql": BASE_MIGRATION})
        self.assertEqual(result.returncode, 1)
        self.assertIn("vector: created without a guard", result.stderr)

    def test_guarded_extension_may_be_optional_only(self) -> None:
        result = self.run_fixture(
            BASE_MATRIX + "optional      citext\n",
            {
                "001_init.sql": BASE_MIGRATION,
                "002_guarded.sql": (
                    "DO $body$ BEGIN\n"
                    "  IF EXISTS (SELECT 1 FROM pg_available_extensions\n"
                    "             WHERE name = 'citext') THEN\n"
                    "    CREATE EXTENSION IF NOT EXISTS citext;\n"
                    "  END IF;\n"
                    "END $body$;\n"
                ),
            },
        )
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_unguarded_do_block_counts_as_required(self) -> None:
        result = self.run_fixture(
            BASE_MATRIX + "optional      hstore\n",
            {
                "001_init.sql": BASE_MIGRATION,
                "002.sql": "DO $$ BEGIN CREATE EXTENSION hstore; END $$;\n",
            },
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn("hstore: created without a guard", result.stderr)

    def test_stale_matrix_entry_fails(self) -> None:
        result = self.run_fixture(
            BASE_MATRIX + "required      ltree\n", {"001_init.sql": BASE_MIGRATION}
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn("ltree: matrix lists it as required", result.stderr)

    def test_commented_statements_are_ignored(self) -> None:
        result = self.run_fixture(
            BASE_MATRIX,
            {
                "001_init.sql": BASE_MIGRATION
                + "/* CREATE EXTENSION fuzzystrmatch; */\n"
                + "-- CREATE EXTENSION btree_gin;\n"
            },
        )
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_missing_matrix_block_fails(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "migrations").mkdir()
            (root / "docs" / "deployment").mkdir(parents=True)
            (root / DOC).write_text("# no matrix\n", encoding="utf-8")
            result = self.run_check(root)
        self.assertEqual(result.returncode, 1)
        self.assertIn("missing", result.stderr)

    def run_fixture(
        self, matrix: str, migrations: dict[str, str]
    ) -> subprocess.CompletedProcess[str]:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "migrations").mkdir()
            for name, body in migrations.items():
                (root / "migrations" / name).write_text(body, encoding="utf-8")
            (root / "docs" / "deployment").mkdir(parents=True)
            (root / DOC).write_text(
                "# Matrix\n\n<!-- extension-matrix:begin -->\n```text\n"
                + matrix
                + "```\n<!-- extension-matrix:end -->\n",
                encoding="utf-8",
            )
            return self.run_check(root)

    @staticmethod
    def run_check(root: Path) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [sys.executable, str(SCRIPT), str(root)],
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )


if __name__ == "__main__":
    unittest.main()
