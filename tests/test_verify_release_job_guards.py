import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
VERIFIER = ROOT / "scripts/ci/verify-release-job-guards.py"
WORKFLOW = ROOT / ".gitea/workflows/ci-builder.yaml"


class VerifyReleaseJobGuardsTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tempdir = tempfile.TemporaryDirectory()
        self.workflow = Path(self.tempdir.name) / "ci-builder.yaml"
        shutil.copy2(WORKFLOW, self.workflow)

    def tearDown(self) -> None:
        self.tempdir.cleanup()

    def run_verifier(self) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            ["python3", str(VERIFIER), str(self.workflow)],
            cwd=ROOT,
            check=False,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )

    def mutate(self, old: str, new: str) -> None:
        text = self.workflow.read_text()
        self.assertIn(old, text)
        self.workflow.write_text(text.replace(old, new, 1))

    def test_current_release_graph_passes(self) -> None:
        result = self.run_verifier()
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_missing_finalizer_fails_closed(self) -> None:
        self.mutate("  finalize-releases:\n", "  removed-finalizer:\n")
        result = self.run_verifier()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("missing required release jobs: finalize-releases", result.stderr)

    def test_finalizer_cannot_drop_internal_publication_dependency(self) -> None:
        self.mutate(
            "needs: [publish-release, publish-github, verify-ghcr-release]",
            "needs: [publish-github, verify-ghcr-release]",
        )
        result = self.run_verifier()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("missing required publish dependencies: publish-release", result.stderr)

    def test_finalizer_must_create_gitea_release(self) -> None:
        self.mutate(
            "      - name: Fetch Gitea release token from vault\n"
            "        env:\n"
            "          VAULT_CI_ROLE_ID: ${{ secrets.VAULT_CI_ROLE_ID }}\n"
            "          VAULT_CI_SECRET_ID: ${{ secrets.VAULT_CI_SECRET_ID }}\n"
            "        run: ci/vault-fetch.sh --spec ci/vault-fetch.gitea-release.spec",
            "      - name: Missing Gitea release operation",
        )
        result = self.run_verifier()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("missing required release operation", result.stderr)

    def test_retired_duplicate_job_fails_closed(self) -> None:
        self.mutate("  finalize-releases:\n", "  create-release:\n")
        result = self.run_verifier()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("retired duplicate release jobs: create-release", result.stderr)


if __name__ == "__main__":
    unittest.main()
