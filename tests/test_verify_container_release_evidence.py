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
        for dockerfile in ("Dockerfile", "Dockerfile.bundle"):
            shutil.copy2(ROOT / dockerfile, self.root / dockerfile)
        shutil.copy2(POLICY, self.root / POLICY.relative_to(ROOT))
        shutil.copy2(
            ROOT / "scripts/ci/promote-ghcr-images.sh",
            self.root / "scripts/ci/promote-ghcr-images.sh",
        )
        shutil.copy2(
            ROOT / "scripts/ci/verify-ghcr-publication.sh",
            self.root / "scripts/ci/verify-ghcr-publication.sh",
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

    def test_unexported_release_version_fails_closed(self) -> None:
        workflow = self.root / ".gitea/workflows/ci-builder.yaml"
        workflow.write_text(
            workflow.read_text().replace(
                'export VERSION="${GITHUB_REF_NAME#v}"',
                'VERSION="${GITHUB_REF_NAME#v}"',
            )
        )
        result = self.run_verifier()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("must export VERSION", result.stderr)

    def test_rust_stack_below_observed_release_requirement_fails_closed(self) -> None:
        dockerfile = self.root / "Dockerfile"
        dockerfile.write_text(
            dockerfile.read_text().replace(
                "ARG RUST_MIN_STACK=67108864",
                "ARG RUST_MIN_STACK=33554432",
            )
        )
        result = self.run_verifier()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(
            "Dockerfile: RUST_MIN_STACK must be at least 67108864 bytes",
            result.stderr,
        )

    def test_missing_public_ghcr_gate_fails_closed(self) -> None:
        workflow = self.root / ".gitea/workflows/ci-builder.yaml"
        workflow.write_text(
            workflow.read_text().replace("  verify-ghcr-release:\n", "  removed-ghcr-release:\n")
        )
        result = self.run_verifier()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("public GHCR release verification job", result.stderr)

    def test_authenticated_public_verifier_fails_closed(self) -> None:
        verifier = self.root / "scripts/ci/verify-ghcr-publication.sh"
        verifier.write_text(
            verifier.read_text().replace(
                'export DOCKER_CONFIG="$public_docker_config"',
                'echo "not anonymous"',
            )
        )
        result = self.run_verifier()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("DOCKER_CONFIG", result.stderr)


if __name__ == "__main__":
    unittest.main()
