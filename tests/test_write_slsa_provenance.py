import json
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/ci/write-slsa-provenance.py"
DIGEST = "sha256:" + "ab" * 32
REVISION = "0123456789abcdef0123456789abcdef01234567"


class WriteSlsaProvenanceTests(unittest.TestCase):
    def run_script(self, **overrides: str) -> tuple[subprocess.CompletedProcess[str], Path]:
        output = Path(self.tempdir.name) / "predicate.json"
        args = {
            "--family": "bundle",
            "--subject": f"ghcr.io/fortemi/fortemi@{DIGEST}",
            "--source-uri": "https://git.integrolabs.net/Fortemi/fortemi",
            "--source-revision": REVISION,
            "--source-ref": "refs/tags/v2026.10.0",
            "--workflow-path": ".gitea/workflows/ci-builder.yaml",
            "--event-name": "push",
            "--builder-id": "https://git.integrolabs.net/Fortemi/fortemi/actions/runners/matric-builder",
            "--invocation-id": "https://git.integrolabs.net/Fortemi/fortemi/actions/runs/1",
            "--started-on": "2026-10-08T00:00:00Z",
            "--finished-on": "2026-10-08T00:01:00Z",
            "--output": str(output),
        }
        args.update(overrides)
        argv = ["python3", str(SCRIPT)]
        for key, value in args.items():
            argv += [key, value]
        result = subprocess.run(argv, check=False, text=True, capture_output=True)
        return result, output

    def setUp(self) -> None:
        self.tempdir = tempfile.TemporaryDirectory()

    def tearDown(self) -> None:
        self.tempdir.cleanup()

    def test_binds_source_commit_and_run(self) -> None:
        result, output = self.run_script()
        self.assertEqual(result.returncode, 0, result.stderr)
        predicate = json.loads(output.read_text())
        dependency = predicate["buildDefinition"]["resolvedDependencies"][0]
        self.assertEqual(dependency["digest"]["gitCommit"], REVISION)
        self.assertEqual(
            dependency["uri"],
            "git+https://git.integrolabs.net/Fortemi/fortemi@refs/tags/v2026.10.0",
        )
        self.assertEqual(
            predicate["runDetails"]["metadata"]["invocationId"],
            "https://git.integrolabs.net/Fortemi/fortemi/actions/runs/1",
        )

    def test_rejects_tag_subject(self) -> None:
        result, _ = self.run_script(**{"--subject": "ghcr.io/fortemi/fortemi:latest"})
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("subject must be", result.stderr)

    def test_rejects_short_revision(self) -> None:
        result, _ = self.run_script(**{"--source-revision": "abc1234"})
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("40-character", result.stderr)


if __name__ == "__main__":
    unittest.main()
