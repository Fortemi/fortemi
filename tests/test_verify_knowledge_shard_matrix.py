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

    def test_complete_inventory_allows_only_registered_profile_claims(self) -> None:
        matrix = self.matrix()
        result, output = self.run_verifier(matrix)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIsNotNone(output)
        assert output is not None
        self.assertFalse(output["claimsAllowed"])
        self.assertTrue(output["registeredProfileClaimsAllowed"])
        self.assertEqual(output["claimScope"], "registered-profile-cells-only")
        self.assertEqual(output["summary"]["requiredCells"], 9)
        self.assertEqual(output["summary"]["passed"], 9)
        self.assertEqual(output["summary"]["pending"], 0)
        self.assertEqual(
            set(output["blockedClaims"]),
            {"compatibility", "portability", "backup", "parity"},
        )

    def test_release_mode_accepts_complete_registered_profile_matrix(self) -> None:
        matrix = self.matrix()
        result, output = self.run_verifier(matrix, "--require-complete")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIsNotNone(output)
        assert output is not None
        self.assertFalse(output["claimsAllowed"])
        self.assertTrue(output["registeredProfileClaimsAllowed"])
        self.assertEqual(
            set(output["blockedClaims"]),
            {"compatibility", "portability", "backup", "parity"},
        )

    def test_missing_required_cell_is_rejected(self) -> None:
        matrix = self.matrix()
        matrix["cells"] = matrix["cells"][:-1]
        result, output = self.run_verifier(matrix)
        self.assertEqual(result.returncode, 2)
        self.assertIsNone(output)
        self.assertIn("missing required cells", result.stderr)

    def test_incomplete_cell_cannot_be_relabelled_passed(self) -> None:
        matrix = self.matrix()
        cell = next(
            cell
            for cell in matrix["cells"]
            if cell["id"] == "aiwg-core-v1-to-pglite"
        )
        cell["coverage"].remove("hierarchy")
        result, output = self.run_verifier(matrix)
        self.assertEqual(result.returncode, 2)
        self.assertIsNone(output)
        self.assertIn("cannot pass without complete per-cell coverage", result.stderr)

    def test_profile_coverage_policy_cannot_be_weakened(self) -> None:
        matrix = self.matrix()
        matrix["profileCoverageRequirements"]["full-v1"].remove("attachment-bytes")
        result, output = self.run_verifier(matrix)
        self.assertEqual(result.returncode, 2)
        self.assertIsNone(output)
        self.assertIn("canonical profile components", result.stderr)

    def test_passed_cell_requires_every_consumer_behavior_dimension(self) -> None:
        matrix = self.matrix()
        cell = next(
            cell
            for cell in matrix["cells"]
            if cell["id"] == "recordstore-record-v1-to-recordstore"
        )
        cell["coverage"].remove("resource-limits")
        result, output = self.run_verifier(matrix)
        self.assertEqual(result.returncode, 2)
        self.assertIsNone(output)
        self.assertIn("complete per-cell coverage: resource-limits", result.stderr)

    def test_passed_cell_requires_every_profile_semantic_dimension(self) -> None:
        matrix = self.matrix()
        cell = next(
            cell
            for cell in matrix["cells"]
            if cell["id"] == "recordstore-record-v1-to-recordstore"
        )
        cell["coverage"].remove("metadata")
        result, output = self.run_verifier(matrix)
        self.assertEqual(result.returncode, 2)
        self.assertIsNone(output)
        self.assertIn("complete per-cell coverage: metadata", result.stderr)

    def test_result_reports_field_level_coverage_outcomes_per_cell(self) -> None:
        result, output = self.run_verifier(self.matrix())
        self.assertEqual(result.returncode, 0, result.stderr)
        assert output is not None
        aiwg_to_fortemi = next(
            cell for cell in output["cells"] if cell["id"] == "aiwg-core-v1-to-fortemi"
        )
        self.assertTrue(aiwg_to_fortemi["coverageComplete"])
        self.assertTrue(all(aiwg_to_fortemi["coverageOutcomes"].values()))
        self.assertEqual(aiwg_to_fortemi["missingCoverage"], [])
        aiwg_to_pglite = next(
            cell for cell in output["cells"] if cell["id"] == "aiwg-core-v1-to-pglite"
        )
        self.assertTrue(aiwg_to_pglite["coverageComplete"])
        self.assertTrue(all(aiwg_to_pglite["coverageOutcomes"].values()))
        self.assertEqual(aiwg_to_pglite["missingCoverage"], [])
        passed = next(
            cell
            for cell in output["cells"]
            if cell["id"] == "recordstore-record-v1-to-recordstore"
        )
        self.assertTrue(passed["coverageComplete"])
        self.assertTrue(all(passed["coverageOutcomes"].values()))
        self.assertEqual(passed["missingCoverage"], [])
        core_self = next(
            cell
            for cell in output["cells"]
            if cell["id"] == "fortemi-core-v1-to-fortemi"
        )
        self.assertTrue(core_self["coverageComplete"])
        self.assertTrue(all(core_self["coverageOutcomes"].values()))
        self.assertEqual(core_self["missingCoverage"], [])
        pglite_self = next(
            cell
            for cell in output["cells"]
            if cell["id"] == "pglite-core-v1-to-pglite"
        )
        self.assertTrue(pglite_self["coverageComplete"])
        self.assertTrue(all(pglite_self["coverageOutcomes"].values()))
        self.assertEqual(pglite_self["missingCoverage"], [])
        pglite_to_fortemi = next(
            cell
            for cell in output["cells"]
            if cell["id"] == "pglite-core-v1-to-fortemi"
        )
        self.assertTrue(pglite_to_fortemi["coverageComplete"])
        self.assertTrue(all(pglite_to_fortemi["coverageOutcomes"].values()))
        self.assertEqual(pglite_to_fortemi["missingCoverage"], [])
        fortemi_to_pglite = next(
            cell
            for cell in output["cells"]
            if cell["id"] == "fortemi-core-v1-to-pglite"
        )
        self.assertTrue(fortemi_to_pglite["coverageComplete"])
        self.assertTrue(all(fortemi_to_pglite["coverageOutcomes"].values()))
        self.assertEqual(fortemi_to_pglite["missingCoverage"], [])
        recordstore_to_fortemi = next(
            cell
            for cell in output["cells"]
            if cell["id"] == "recordstore-record-v1-to-fortemi"
        )
        self.assertTrue(recordstore_to_fortemi["coverageComplete"])
        self.assertTrue(all(recordstore_to_fortemi["coverageOutcomes"].values()))
        self.assertEqual(recordstore_to_fortemi["missingCoverage"], [])

    def test_passed_cell_requires_digest_pinned_coverage_binding(self) -> None:
        matrix = self.matrix()
        cell = next(
            cell
            for cell in matrix["cells"]
            if cell["id"] == "recordstore-record-v1-to-recordstore"
        )
        del cell["evidence"][1]["expect"]["/coverage"]
        result, output = self.run_verifier(matrix)
        self.assertEqual(result.returncode, 2)
        self.assertIsNone(output)
        self.assertIn("digest-pinned receipt binding /coverage", result.stderr)

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

    def test_claim_scope_cannot_be_widened_to_suite_level(self) -> None:
        matrix = self.matrix()
        matrix["claimPolicy"]["scope"] = "suite-wide"
        result, output = self.run_verifier(matrix)
        self.assertEqual(result.returncode, 2)
        self.assertIsNone(output)
        self.assertIn("must not authorize suite-wide claims", result.stderr)


if __name__ == "__main__":
    unittest.main()
