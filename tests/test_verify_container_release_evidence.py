import json
import re
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
        for dockerfile in ("Dockerfile", "Dockerfile.bundle", "Dockerfile.mcp"):
            shutil.copy2(ROOT / dockerfile, self.root / dockerfile)
        shutil.copy2(POLICY, self.root / POLICY.relative_to(ROOT))
        shutil.copy2(
            ROOT / "scripts/ci/promote-ghcr-images.sh",
            self.root / "scripts/ci/promote-ghcr-images.sh",
        )
        for script in ("verify-ghcr-publication.sh", "sign-container-images.sh"):
            shutil.copy2(ROOT / "scripts/ci" / script, self.root / "scripts/ci" / script)
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

    def test_public_argument_review_matches_shared_workflow(self) -> None:
        policy = self.policy()
        reviewed = policy["build_inputs"]
        self.assertEqual(
            set(reviewed["allowed_public_build_args"]),
            {
                "VERSION", "GIT_SHA", "BUILD_DATE", "FORTEMI_API_FEATURES",
                "FORTEMI_CACHE_DISABLE", "FORTEMI_CACHE_FAMILY",
                "FORTEMI_CACHE_NAMESPACE",
            },
        )
        self.assertIs(reviewed["secrets_as_build_args_allowed"], False)
        workflow_path = ".gitea/workflows/ci-builder.yaml"
        workflow = (self.root / workflow_path).read_text()
        passed = set(re.findall(r"--build-arg\s+[\"']?([A-Z][A-Z0-9_]*)", workflow))
        self.assertEqual(passed, set(reviewed["allowed_public_build_args"]))
        associated = {
            family for family, details in policy["families"].items()
            if workflow_path in details["workflows"]
        }
        review = reviewed["cache_argument_review"]
        self.assertEqual(set(review["families"]), associated)
        self.assertEqual(associated, {"api", "hosted", "bundle", "mcp"})
        self.assertIn("--build-arg FORTEMI_CACHE_NAMESPACE=release-v1", workflow)
        self.assertIn('--build-arg FORTEMI_CACHE_FAMILY="${label}"', workflow)
        self.assertIn(
            '--build-arg FORTEMI_CACHE_DISABLE="${FORTEMI_CACHE_DISABLE:-0}"',
            workflow,
        )
        for family, details in review["families"].items():
            with self.subTest(family=family):
                self.assertEqual(details["release_family_value"], family)
                self.assertIn(f'build_and_push {family} --no-cache', workflow)

    def test_cache_argument_consumers_and_defaults_match_dockerfiles(self) -> None:
        review = self.policy()["build_inputs"]["cache_argument_review"]
        self.assertEqual(
            set(review["arguments"]),
            {"FORTEMI_CACHE_DISABLE", "FORTEMI_CACHE_FAMILY", "FORTEMI_CACHE_NAMESPACE"},
        )
        for family, details in review["families"].items():
            with self.subTest(family=family):
                dockerfile = self.root / details["dockerfile"]
                defaults = dict(re.findall(
                    r"^ARG (FORTEMI_CACHE_[A-Z_]+)=(\S+)$",
                    dockerfile.read_text(), re.MULTILINE,
                ))
                if family == "mcp":
                    self.assertIs(details["consumes_cache_arguments"], False)
                    self.assertIsNone(details["docker_family_default"])
                    self.assertFalse(set(defaults) & set(review["arguments"]))
                else:
                    self.assertIs(details["consumes_cache_arguments"], True)
                    self.assertEqual(
                        defaults["FORTEMI_CACHE_FAMILY"], details["docker_family_default"]
                    )
                    for name in ("FORTEMI_CACHE_DISABLE", "FORTEMI_CACHE_NAMESPACE"):
                        self.assertEqual(defaults[name], review["arguments"][name]["docker_default"])

    def test_policy_cannot_self_authorize_unknown_build_arg(self) -> None:
        policy = self.policy()
        policy["build_inputs"]["allowed_public_build_args"].append("UNREVIEWED_INPUT")
        self.write_policy(policy)
        workflow = self.root / ".gitea/workflows/ci-builder.yaml"
        workflow.write_text(workflow.read_text().replace(
            "--build-arg FORTEMI_API_FEATURES=",
            "--build-arg UNREVIEWED_INPUT=value --build-arg FORTEMI_API_FEATURES=",
            1,
        ))
        result = self.run_verifier()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("public Docker build arguments must use the reviewed allowlist", result.stderr)
        for family in ("api", "bundle", "hosted", "mcp"):
            with self.subTest(family=family):
                diagnostics = [
                    line for line in result.stderr.splitlines()
                    if line.startswith(f"- {family}:") and "unreviewed build args" in line
                ]
                self.assertTrue(diagnostics, result.stderr)
                self.assertIn("'UNREVIEWED_INPUT'", diagnostics[0])

    def test_unreviewed_build_arg_fails_closed(self) -> None:
        workflow = self.root / ".gitea/workflows/ci-builder.yaml"
        workflow.write_text(
            workflow.read_text().replace(
                "--build-arg FORTEMI_API_FEATURES=",
                "--build-arg HF_TOKEN=leak --build-arg FORTEMI_API_FEATURES=",
                1,
            )
        )
        result = self.run_verifier()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("unreviewed build args ['HF_TOKEN']", result.stderr)

    def test_single_platform_release_build_fails_closed(self) -> None:
        workflow = self.root / ".gitea/workflows/ci-builder.yaml"
        workflow.write_text(
            workflow.read_text().replace(
                "PLATFORMS=linux/amd64,linux/arm64", "PLATFORMS=linux/amd64", 1
            )
        )
        result = self.run_verifier()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("PLATFORMS=linux/amd64,linux/arm64", result.stderr)

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
                "ARG RUST_MIN_STACK=268435456",
                "ARG RUST_MIN_STACK=134217728",
            )
        )
        result = self.run_verifier()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(
            "Dockerfile: RUST_MIN_STACK must be at least 268435456 bytes",
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

    def test_unknown_control_status_fails_closed(self) -> None:
        policy = self.policy()
        policy["controls"]["signature"]["status"] = "assumed"
        self.write_policy(policy)
        result = self.run_verifier()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("signature: status must be", result.stderr)

    def test_wired_control_requires_verification_command(self) -> None:
        policy = self.policy()
        del policy["controls"]["sbom"]["verification_command"]
        self.write_policy(policy)
        result = self.run_verifier()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("sbom: wired control requires verification_command", result.stderr)

    def test_wired_controls_require_signing_job(self) -> None:
        workflow = self.root / ".gitea/workflows/ci-builder.yaml"
        workflow.write_text(
            workflow.read_text().replace("  sign-release-images:\n", "  removed-signing:\n")
        )
        result = self.run_verifier()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("require the sign-release-images CI job", result.stderr)

    def test_finalizer_must_wait_for_signing(self) -> None:
        workflow = self.root / ".gitea/workflows/ci-builder.yaml"
        workflow.write_text(
            workflow.read_text().replace(
                " && needs.sign-release-images.result == 'success'", ""
            )
        )
        result = self.run_verifier()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("finalize-releases must wait", result.stderr)

    def test_signing_job_cannot_suppress_failures(self) -> None:
        workflow = self.root / ".gitea/workflows/ci-builder.yaml"
        workflow.write_text(
            workflow.read_text().replace(
                "--out container-supply-chain-evidence",
                "--out container-supply-chain-evidence || true",
            )
        )
        result = self.run_verifier()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("must not suppress failures", result.stderr)


if __name__ == "__main__":
    unittest.main()
