import json
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
VERIFIER = ROOT / "scripts/ci/verify-container-release-evidence.py"
POLICY = ROOT / "docker/container-release-evidence-policy.json"
WORKFLOWS = (
    ".gitea/workflows/ci-builder.yaml",
    ".gitea/workflows/build-builder.yaml",
    ".gitea/workflows/build-gliner.yaml",
    ".gitea/workflows/build-pyannote.yaml",
)


class VerifyContainerReleaseEvidenceTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tempdir = tempfile.TemporaryDirectory()
        self.root = Path(self.tempdir.name)
        (self.root / "docker").mkdir()
        (self.root / "scripts/ci").mkdir(parents=True)
        (self.root / ".gitea/workflows").mkdir(parents=True)
        shutil.copy2(POLICY, self.root / POLICY.relative_to(ROOT))
        shutil.copy2(
            ROOT / "scripts/ci/promote-ghcr-images.sh",
            self.root / "scripts/ci/promote-ghcr-images.sh",
        )
        for workflow in WORKFLOWS:
            shutil.copy2(ROOT / workflow, self.root / workflow)

    def tearDown(self) -> None:
        self.tempdir.cleanup()

    def run_verifier(self) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            ["python3", str(VERIFIER)],
            cwd=self.root,
            check=False,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )

    def policy(self) -> dict:
        return json.loads((self.root / POLICY.relative_to(ROOT)).read_text())

    def write_policy(self, policy: dict) -> None:
        (self.root / POLICY.relative_to(ROOT)).write_text(json.dumps(policy))

    def test_current_policy_and_wiring_pass(self) -> None:
        result = self.run_verifier()
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_missing_family_fails_closed(self) -> None:
        policy = self.policy()
        del policy["families"]["builder"]
        self.write_policy(policy)
        result = self.run_verifier()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("families must be exactly", result.stderr)

    def test_false_oidc_claim_fails_closed(self) -> None:
        policy = self.policy()
        policy["publish_path_profiles"]["ghcr-from-gitea-pat"]["oidc_identity"] = True
        self.write_policy(policy)
        result = self.run_verifier()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("must not claim OIDC", result.stderr)


if __name__ == "__main__":
    unittest.main()
