#!/usr/bin/env python3
"""Regression tests for the Knowledge Shard matrix verifier."""

from __future__ import annotations

import copy
import json
import subprocess
import tempfile
import unittest
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/ci/verify-knowledge-shard-matrix.py"
MATRIX = ROOT / "contracts/knowledge-shard/conformance/matrix.json"


class MatrixVerifierTest(unittest.TestCase):
    def run_verifier(
        self, matrix: dict[str, Any], *extra: str
    ) -> tuple[subprocess.CompletedProcess[str], dict[str, Any] | None]:
        with tempfile.TemporaryDirectory() as temp:
            temp_root = Path(temp)
            matrix_path = temp_root / "matrix.json"
            output_path = temp_root / "result.json"
            matrix_path.write_text(json.dumps(matrix), encoding="utf-8")
            result = subprocess.run(
                [
                    "python3",
                    str(SCRIPT),
                    "--root",
                    str(ROOT),
                    "--matrix",
                    str(matrix_path),
                    "--output",
                    str(output_path),
                    *extra,
                ],
                check=False,
                capture_output=True,
                text=True,
            )
            output = json.loads(output_path.read_text()) if output_path.exists() else None
            return result, output

    @classmethod
    def matrix(cls) -> dict[str, Any]:
        return json.loads(MATRIX.read_text())

    def test_valid_pending_inventory_blocks_claims_without_failing_ci(self) -> None:
        matrix = self.matrix()
        matrix["cells"][-1]["status"] = "pending"
        matrix["cells"][-1]["blockingReason"] = "synthetic pending cell"
        result, output = self.run_verifier(matrix)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIsNotNone(output)
        assert output is not None
        self.assertFalse(output["claimsAllowed"])
        self.assertEqual(output["summary"]["requiredCells"], 9)
        self.assertEqual(output["summary"]["passed"], 8)
        self.assertEqual(output["summary"]["pending"], 1)
        self.assertEqual(
            set(output["blockedClaims"]),
            {"compatibility", "portability", "backup", "parity"},
        )

    def test_release_mode_fails_closed_while_cells_are_pending(self) -> None:
        matrix = self.matrix()
        matrix["cells"][-1]["status"] = "pending"
        matrix["cells"][-1]["blockingReason"] = "synthetic pending cell"
        result, output = self.run_verifier(matrix, "--require-complete")
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIsNotNone(output)
        assert output is not None
        self.assertFalse(output["claimsAllowed"])

    def test_missing_required_cell_is_rejected(self) -> None:
        matrix = self.matrix()
        matrix["cells"] = matrix["cells"][:-1]
        result, output = self.run_verifier(matrix)
        self.assertEqual(result.returncode, 2)
        self.assertIsNone(output)
        self.assertIn("missing required cells", result.stderr)

    def test_pending_cell_cannot_be_relabelled_passed(self) -> None:
        matrix = self.matrix()
        cell = matrix["cells"][-1]
        del cell["assertions"]
        result, output = self.run_verifier(matrix)
        self.assertEqual(result.returncode, 2)
        self.assertIsNone(output)
        self.assertIn("cannot pass without all fail-closed assertions", result.stderr)

    def test_profile_coverage_policy_cannot_be_weakened(self) -> None:
        matrix = self.matrix()
        matrix["profileCoverageRequirements"]["full-v1"].remove("attachment-bytes")
        result, output = self.run_verifier(matrix)
        self.assertEqual(result.returncode, 2)
        self.assertIsNone(output)
        self.assertIn("canonical profile components", result.stderr)

    def test_complete_matrix_requires_behavior_coverage_for_each_consumer(self) -> None:
        matrix = self.matrix()
        evidence = matrix["participants"][0]["immutableInputs"][0]
        for cell in matrix["cells"]:
            cell["status"] = "passed"
            cell["blockingReason"] = None
            cell["coverage"] = [
                *matrix["profileCoverageRequirements"][cell["profile"]],
                "current",
            ]
            cell["assertions"] = {
                "cleanDestination": True,
                "semanticReexport": True,
                "zeroMutationOnFailure": True,
            }
            cell["evidence"] = [evidence]
        result, output = self.run_verifier(matrix)
        self.assertEqual(result.returncode, 2)
        self.assertIsNone(output)
        self.assertIn("consumer behavior coverage", result.stderr)

    def test_complete_matrix_requires_profile_coverage_across_cells(self) -> None:
        matrix = self.matrix()
        evidence = matrix["participants"][0]["immutableInputs"][0]
        for cell in matrix["cells"]:
            cell["status"] = "passed"
            cell["blockingReason"] = None
            cell["coverage"] = matrix["consumerBehaviorCoverageRequirements"].get(
                cell["consumer"], []
            )
            cell["assertions"] = {
                "cleanDestination": True,
                "semanticReexport": True,
                "zeroMutationOnFailure": True,
            }
            cell["evidence"] = [evidence]
        result, output = self.run_verifier(matrix)
        self.assertEqual(result.returncode, 2)
        self.assertIsNone(output)
        self.assertIn("profile coverage", result.stderr)

    def test_local_receipt_digest_drift_is_rejected(self) -> None:
        matrix = copy.deepcopy(self.matrix())
        cell = next(
            cell
            for cell in matrix["cells"]
            if cell["id"] == "fortemi-full-v1-to-fortemi"
        )
        cell["evidence"][0]["sha256"] = "0" * 64
        result, output = self.run_verifier(matrix)
        self.assertEqual(result.returncode, 2)
        self.assertIsNone(output)
        self.assertIn("SHA-256 drift", result.stderr)

    def test_failed_cell_fails_ordinary_ci(self) -> None:
        matrix = self.matrix()
        cell = matrix["cells"][-1]
        cell["status"] = "failed"
        cell["blockingReason"] = "synthetic failed cell"
        result, output = self.run_verifier(matrix)
        self.assertEqual(result.returncode, 1)
        self.assertIsNotNone(output)
        assert output is not None
        self.assertEqual(output["summary"]["failed"], 1)

    def test_unknown_cell_property_is_rejected(self) -> None:
        matrix = self.matrix()
        matrix["cells"][0]["unreviewedClaim"] = True
        result, output = self.run_verifier(matrix)
        self.assertEqual(result.returncode, 2)
        self.assertIsNone(output)
        self.assertIn("unknown keys", result.stderr)

    def test_required_producer_profile_cannot_be_removed(self) -> None:
        matrix = self.matrix()
        aiwg = next(
            participant
            for participant in matrix["participants"]
            if participant["id"] == "aiwg"
        )
        aiwg["producerProfiles"] = []
        matrix["cells"] = [
            cell for cell in matrix["cells"] if cell["producer"] != "aiwg"
        ]
        result, output = self.run_verifier(matrix)
        self.assertEqual(result.returncode, 2)
        self.assertIsNone(output)
        self.assertIn("producer profile topology", result.stderr)


if __name__ == "__main__":
    unittest.main()
