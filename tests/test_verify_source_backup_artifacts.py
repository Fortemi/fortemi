from __future__ import annotations

import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "ci" / "verify-source-backup-artifacts.py"
ALLOWLIST = "scripts/ci/source-backup-artifacts.allowlist"


class SourceBackupArtifactTests(unittest.TestCase):
    def test_current_repository_passes(self) -> None:
        result = self.run_check(ROOT)

        self.assertEqual(result.returncode, 0, result.stderr)

    def test_tracked_backup_suffixes_are_rejected(self) -> None:
        for relative in (
            "crates/core/src/model.rs.bak",
            "docs/design.md.orig",
            "config/service.toml~",
            "src/UPPER.BAK",
        ):
            with self.subTest(relative=relative):
                result = self.run_fixture({relative: "stale\n"})

                self.assertEqual(result.returncode, 1)
                self.assertIn("tracked backup/editor artifact", result.stderr)

    def test_backup_domain_files_are_not_matched_by_keyword(self) -> None:
        result = self.run_fixture(
            {
                "scripts/backup.sh": "#!/usr/bin/env bash\n",
                "docs/content/backup.md": "# Runtime backups\n",
                "src/backup_service.rs": "pub struct BackupService;\n",
            }
        )

        self.assertEqual(result.returncode, 0, result.stderr)

    def test_exact_allowlist_with_rationale_is_allowed(self) -> None:
        artifact = "docs/fixtures/legacy.orig"
        result = self.run_fixture(
            {
                artifact: "fixture\n",
                ALLOWLIST: (
                    f"{artifact}\tRequired byte-for-byte compatibility fixture\n"
                ),
            }
        )

        self.assertEqual(result.returncode, 0, result.stderr)

    def test_allowlist_requires_rationale(self) -> None:
        artifact = "docs/fixtures/legacy.orig"
        result = self.run_fixture(
            {
                artifact: "fixture\n",
                ALLOWLIST: f"{artifact}\tshort\n",
            }
        )

        self.assertEqual(result.returncode, 1)
        self.assertIn("rationale must be at least", result.stderr)

    def test_allowlist_rejects_non_normalized_or_traversing_paths(self) -> None:
        entries = (
            "./docs/legacy.orig\tRequired compatibility fixture",
            "../outside.orig\tRequired compatibility fixture",
            "docs//legacy.orig\tRequired compatibility fixture",
        )
        for entry in entries:
            with self.subTest(entry=entry):
                result = self.run_fixture({ALLOWLIST: f"{entry}\n"})

                self.assertEqual(result.returncode, 1)
                self.assertIn("must be normalized and relative", result.stderr)

    def test_stale_allowlist_entry_is_rejected(self) -> None:
        result = self.run_fixture(
            {
                ALLOWLIST: (
                    "docs/fixtures/missing.orig\tRequired compatibility fixture\n"
                )
            }
        )

        self.assertEqual(result.returncode, 1)
        self.assertIn("stale allowlist entry", result.stderr)

    def test_untracked_local_artifact_does_not_change_tracked_result(self) -> None:
        result = self.run_fixture(
            {"crates/core/src/local.rs.bak": "untracked\n"},
            untracked={"crates/core/src/local.rs.bak"},
        )

        self.assertEqual(result.returncode, 0, result.stderr)

    def run_fixture(
        self, files: dict[str, str], *, untracked: set[str] | None = None
    ) -> subprocess.CompletedProcess[str]:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.run_git(root, "init", "-q")
            for relative, content in files.items():
                path = root / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(content, encoding="utf-8")

            allowlist = root / ALLOWLIST
            if not allowlist.exists():
                allowlist.parent.mkdir(parents=True, exist_ok=True)
                allowlist.write_text("# path<TAB>rationale\n", encoding="utf-8")

            excluded = untracked or set()
            tracked = [relative for relative in files if relative not in excluded]
            tracked.append(ALLOWLIST)
            self.run_git(root, "add", "-f", "--", *sorted(set(tracked)))
            return self.run_check(root)

    @staticmethod
    def run_git(root: Path, *args: str) -> None:
        subprocess.run(
            ["git", "-C", str(root), *args],
            check=True,
            capture_output=True,
            text=True,
        )

    @staticmethod
    def run_check(root: Path) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            ["python3", str(SCRIPT), str(root)],
            check=False,
            capture_output=True,
            text=True,
        )


if __name__ == "__main__":
    unittest.main()
