from __future__ import annotations

import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "ci" / "verify-sidecar-publication.py"
SIDECAR_WORKFLOW = ROOT / ".gitea" / "workflows" / "publish-sidecar.yml"
TEST_WORKFLOW = ROOT / ".gitea" / "workflows" / "test.yml"
RELEASE_GUARD = "startsWith(github.ref, 'refs/tags/v')"


class SidecarPublicationPolicyTests(unittest.TestCase):
    def run_verifier(
        self,
        sidecar_workflow: Path = SIDECAR_WORKFLOW,
        test_workflow: Path = TEST_WORKFLOW,
    ) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [
                "python3",
                str(SCRIPT),
                str(sidecar_workflow),
                str(test_workflow),
            ],
            cwd=ROOT,
            check=False,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )

    def write_fixture(self, directory: Path, name: str, text: str) -> Path:
        path = directory / name
        path.write_text(text, encoding="utf-8")
        return path

    def test_current_release_only_policy_passes(self) -> None:
        result = self.run_verifier()

        self.assertEqual(result.returncode, 0, result.stderr)

    def test_missing_test_handoff_tag_guard_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            sidecar = self.write_fixture(
                root,
                "publish-sidecar.yml",
                SIDECAR_WORKFLOW.read_text(encoding="utf-8"),
            )
            test = self.write_fixture(
                root,
                "test.yml",
                TEST_WORKFLOW.read_text(encoding="utf-8").replace(
                    f" && {RELEASE_GUARD}",
                    "",
                    1,
                ),
            )

            result = self.run_verifier(sidecar, test)

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("handoff-sidecar is missing release-only tag guard", result.stderr)

    def test_missing_sidecar_entry_tag_guard_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            sidecar = self.write_fixture(
                root,
                "publish-sidecar.yml",
                SIDECAR_WORKFLOW.read_text(encoding="utf-8").replace(
                    f"    if: {RELEASE_GUARD}\n",
                    "",
                    1,
                ),
            )
            test = self.write_fixture(
                root,
                "test.yml",
                TEST_WORKFLOW.read_text(encoding="utf-8"),
            )

            result = self.run_verifier(sidecar, test)

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("runner-capacity is missing release-only tag guard", result.stderr)

    def test_main_branch_publisher_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            sidecar_text = SIDECAR_WORKFLOW.read_text(encoding="utf-8")
            sidecar_text += (
                "\n  publish-sidecar-latest:\n"
                "    if: github.ref == 'refs/heads/main'\n"
                "    steps:\n"
                "      - run: scripts/ci/publish-sidecar-release.sh rolling\n"
            )
            sidecar = self.write_fixture(root, "publish-sidecar.yml", sidecar_text)
            test = self.write_fixture(
                root,
                "test.yml",
                TEST_WORKFLOW.read_text(encoding="utf-8"),
            )

            result = self.run_verifier(sidecar, test)

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("non-release sidecar path remains", result.stderr)


if __name__ == "__main__":
    unittest.main()
