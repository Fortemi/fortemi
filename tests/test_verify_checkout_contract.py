from __future__ import annotations

import importlib.util
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "ci" / "verify-checkout-contract.py"
SPEC = importlib.util.spec_from_file_location("verify_checkout_contract", SCRIPT)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class CheckoutContractTests(unittest.TestCase):
    def test_repository_workflows_use_immutable_checkout(self) -> None:
        self.assertEqual(MODULE.verify_workflows(MODULE.workflow_files()), [])

    def test_checker_rejects_branch_tip_clone_and_token_url(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            workflow = Path(tmp) / "bad.yml"
            workflow.write_text(
                """
git clone --depth 1 --branch main https://token:${{ github.token }}@example/repo.git .
git checkout ${GITHUB_SHA:-HEAD}
""".lstrip()
            )

            failures = MODULE.verify_workflows([workflow])

        self.assertTrue(any("shallow branch-tip clone" in item for item in failures))
        self.assertTrue(any("event SHA checkout" in item for item in failures))
        self.assertTrue(any("workflow token embedded" in item for item in failures))

    def test_exact_sha_fetch_can_checkout_a_non_tip_commit(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "source"
            bare = root / "remote.git"
            checkout = root / "checkout"
            source.mkdir()
            checkout.mkdir()

            self.run_git(source, "init", "-q")
            self.run_git(source, "config", "user.name", "CI Test")
            self.run_git(source, "config", "user.email", "ci@example.invalid")
            (source / "file.txt").write_text("first\n")
            self.run_git(source, "add", "file.txt")
            self.run_git(source, "commit", "-q", "-m", "first")
            first_sha = self.run_git(source, "rev-parse", "HEAD").stdout.strip()
            (source / "file.txt").write_text("second\n")
            self.run_git(source, "commit", "-q", "-am", "second")

            self.run_git(root, "clone", "-q", "--bare", str(source), str(bare))
            self.run_git(checkout, "init", "-q")
            self.run_git(checkout, "remote", "add", "origin", str(bare))
            self.run_git(
                checkout,
                "fetch",
                "--quiet",
                "--no-tags",
                "--depth=1",
                "origin",
                first_sha,
            )
            self.run_git(checkout, "checkout", "--quiet", "--detach", "FETCH_HEAD")

            self.assertEqual(
                self.run_git(checkout, "rev-parse", "HEAD").stdout.strip(),
                first_sha,
            )

    @staticmethod
    def run_git(cwd: Path, *args: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            ["git", *args],
            cwd=cwd,
            check=True,
            capture_output=True,
            text=True,
        )


if __name__ == "__main__":
    unittest.main()
