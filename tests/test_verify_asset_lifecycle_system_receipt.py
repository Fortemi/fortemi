from __future__ import annotations

import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "ci" / "verify-asset-lifecycle-system-receipt.py"


def run_verifier(path: Path, *args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["python3", str(SCRIPT), *args, str(path)],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )


class VerifyAssetLifecycleSystemReceiptTests(unittest.TestCase):
    def write_receipt(self, path: Path) -> dict:
        result = run_verifier(path, "--write")
        self.assertEqual(result.returncode, 0, result.stderr)
        return json.loads(path.read_text(encoding="utf-8"))

    def test_generated_local_receipt_passes(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "receipt.json"
            self.write_receipt(path)

            result = run_verifier(path)

            self.assertEqual(result.returncode, 0, result.stderr)

    def test_tampered_source_hash_fails(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "receipt.json"
            receipt = self.write_receipt(path)
            receipt["sources"][0]["sha256"] = "0" * 64
            path.write_text(json.dumps(receipt), encoding="utf-8")

            result = run_verifier(path)

            self.assertNotEqual(result.returncode, 0)
            self.assertIn(
                "source receipts do not match the checked-out artifacts",
                result.stderr,
            )

    def test_dirty_clean_checkout_claim_fails(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "receipt.json"
            receipt = self.write_receipt(path)
            receipt["execution"]["cleanCheckoutReproduced"] = True
            receipt["execution"]["worktreeDirty"] = True
            path.write_text(json.dumps(receipt), encoding="utf-8")

            result = run_verifier(path)

            self.assertNotEqual(result.returncode, 0)
            self.assertIn("clean checkout cannot be dirty", result.stderr)

    def test_sensitive_database_url_fails(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "receipt.json"
            receipt = self.write_receipt(path)
            receipt["execution"]["databaseMode"] = (
                "postgresql://operator:secret@example.invalid/fortemi"
            )
            path.write_text(json.dumps(receipt), encoding="utf-8")

            result = run_verifier(path)

            self.assertNotEqual(result.returncode, 0)
            self.assertIn("prohibited sensitive pattern", result.stderr)

    def test_unsupported_broad_claim_fails(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "receipt.json"
            receipt = self.write_receipt(path)
            receipt["claims"]["suiteWidePortability"] = True
            path.write_text(json.dumps(receipt), encoding="utf-8")

            result = run_verifier(path)

            self.assertNotEqual(result.returncode, 0)
            self.assertIn(
                "unsupported claim suiteWidePortability must remain false",
                result.stderr,
            )

    def test_mismatched_ci_commit_fails_before_write(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "receipt.json"
            env = os.environ.copy()
            env["GITHUB_SHA"] = "0" * 40
            result = subprocess.run(
                ["python3", str(SCRIPT), "--write", str(path)],
                cwd=ROOT,
                env=env,
                text=True,
                capture_output=True,
                check=False,
            )

            self.assertNotEqual(result.returncode, 0)
            self.assertIn("does not match checked-out commit", result.stderr)


if __name__ == "__main__":
    unittest.main()
