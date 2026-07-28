import importlib.util
import json
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MODULE_PATH = ROOT / "scripts/ci/verify-suite-platform-matrix.py"
SPEC = importlib.util.spec_from_file_location("suite_platform_matrix", MODULE_PATH)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


COMMIT_A = "a" * 40
COMMIT_R = "b" * 40
COMMIT_H = "c" * 40


def manifest():
    return {
        "schema_version": 1,
        "matrix_id": "fortemi.suite-conformance.test-v1",
        "issue": "Fortemi/fortemi#1095",
        "authority": {
            "repository": "Fortemi/fortemi",
            "schema_commit": COMMIT_A,
            "runtime_commit": "9" * 40,
            "contract_path": "contracts/knowledge-shard/2.0.0/contract.json",
            "contract_sha256": "1" * 64,
            "contract_revision": "21",
            "schema_bundle_sha256": "2" * 64,
            "profile": "2.0.0/full-v1",
        },
        "participants": {
            "fortemi_react": {
                "repository": "Fortemi/fortemi-react",
                "commit": COMMIT_R,
            },
            "hotm": {
                "repository": "Fortemi/HotM",
                "commit": COMMIT_H,
            },
        },
        "required_platforms": [
            {
                "id": "linux-x86_64",
                "os": "linux",
                "architecture": "x86_64",
                "runner": "matric-builder",
            },
            {
                "id": "macos-arm64",
                "os": "macos",
                "architecture": "arm64",
                "runner": "mutsu",
            },
        ],
        "required_gates": [
            "authority.workspace-tests",
            "react-core.portable-contract",
            "hotm.consumer",
        ],
        "deferred_platforms": ["windows", "linux-arm64"],
        "claim_boundary": {
            "supported_platforms_only": True,
            "universal_portability": False,
            "complete_backup": False,
            "one_universal_schema": False,
            "launched_gui": False,
        },
    }


def platform_receipt(platform_id):
    os_name, arch = {
        "linux-x86_64": ("linux", "x86_64"),
        "macos-arm64": ("macos", "arm64"),
    }[platform_id]
    return {
        "schema_version": 1,
        "matrix_id": "fortemi.suite-conformance.test-v1",
        "issue": "Fortemi/fortemi#1095",
        "status": "passed",
        "platform": {
            "id": platform_id,
            "os": os_name,
            "architecture": arch,
            "filesystem": "testfs",
        },
        "identity": {
            "authority_schema": COMMIT_A,
            "authority_runtime": "9" * 40,
            "fortemi_react": COMMIT_R,
            "hotm": COMMIT_H,
        },
        "required_gates": {
            "authority.workspace-tests": True,
            "react-core.portable-contract": True,
            "hotm.consumer": True,
        },
        "child_receipts": {
            "authority": "d" * 64,
            "fortemi_react": "e" * 64,
            "hotm": "f" * 64,
        },
        "claims": manifest()["claim_boundary"],
    }


def authority_receipt(platform_id):
    os_name, arch = {
        "linux-x86_64": ("linux", "x86_64"),
        "macos-arm64": ("macos", "arm64"),
    }[platform_id]
    return {
        "schema_version": 1,
        "issue": "Fortemi/fortemi#1095",
        "status": "passed",
        "platform": {
            "id": platform_id,
            "os": os_name,
            "architecture": arch,
            "filesystem": "testfs",
        },
        "database": {
            "engine": "PostgreSQL",
            "provisioning": "managed-docker",
            "architecture": "amd64",
            "version": "18.0",
            "extensions": ["plpgsql", "postgis", "vector"],
        },
        "identity": {
            "schema_commit": COMMIT_A,
            "runtime_commit": "9" * 40,
            "contract_sha256": "1" * 64,
            "contract_revision": "21",
            "schema_bundle_sha256": "2" * 64,
            "openapi_sha256": "3" * 64,
            "asyncapi_sha256": "4" * 64,
        },
        "required_gates": {"authority.workspace-tests": True},
        "claims": manifest()["claim_boundary"],
    }


def react_receipt(platform_id):
    return {
        "schemaVersion": "fortemi.platform-contract-receipt.v1",
        "status": "passed",
        "repository": {"commit": COMMIT_R},
        "platform": {
            "id": {
                "linux-x86_64": "linux/x86_64",
                "macos-arm64": "darwin/arm64",
            }[platform_id]
        },
        "authority": [
            {
                "schemaVersion": "2.0.0",
                "commit": COMMIT_A,
                "contractRevision": "21",
                "contractSha256": "1" * 64,
                "schemaBundleSha256": "2" * 64,
            }
        ],
        "claims": {
            "suiteWide": False,
            "completeBackup": False,
            "universalPlatformPortability": False,
        },
    }


def hotm_receipt(platform_id):
    os_name, arch = {
        "linux-x86_64": ("linux", "x86_64"),
        "macos-arm64": ("darwin", "arm64"),
    }[platform_id]
    return {
        "schemaVersion": "hotm.live-asset-ci-receipt.v1",
        "issue": "Fortemi/HotM#284",
        "status": "passed",
        "identity": {
            "hotmCommit": COMMIT_H,
            "fortemiCommit": "9" * 40,
        },
        "execution": {"os": os_name, "arch": arch},
        "claims": {
            "launchedDesktopGui": False,
            "interactiveNativeDialogs": False,
            "suiteWidePortability": False,
        },
    }


class SuitePlatformMatrixTests(unittest.TestCase):
    def test_accepts_exact_manifest_and_both_platforms(self):
        value = manifest()
        MODULE.validate_manifest(value)
        for platform_id in ("linux-x86_64", "macos-arm64"):
            self.assertEqual(
                MODULE.validate_platform_receipt(platform_receipt(platform_id), value)[0],
                platform_id,
            )

    def test_accepts_bounded_authority_receipts(self):
        value = manifest()
        value["required_gates"] = [
            "authority.workspace-tests",
            "react-core.portable-contract",
            "hotm.consumer",
        ]
        MODULE.validate_authority_receipt(
            authority_receipt("linux-x86_64"), value
        )
        MODULE.validate_authority_receipt(
            authority_receipt("macos-arm64"), value
        )

    def test_rejects_missing_or_broad_platform_claim(self):
        value = manifest()
        receipt = platform_receipt("linux-x86_64")
        receipt["claims"]["universal_portability"] = True
        with self.assertRaisesRegex(MODULE.VerificationError, "claim boundary drift"):
            MODULE.validate_platform_receipt(receipt, value)

    def test_rejects_participant_revision_drift(self):
        value = manifest()
        receipt = platform_receipt("macos-arm64")
        receipt["identity"]["hotm"] = "9" * 40
        with self.assertRaisesRegex(MODULE.VerificationError, "identity drift"):
            MODULE.validate_platform_receipt(receipt, value)

    def test_validates_child_receipt_platform_and_authority_bindings(self):
        value = manifest()
        for platform_id in ("linux-x86_64", "macos-arm64"):
            MODULE.validate_react_receipt(
                react_receipt(platform_id), value, platform_id
            )
            MODULE.validate_hotm_receipt(
                hotm_receipt(platform_id), value, platform_id
            )

        stale = react_receipt("linux-x86_64")
        stale["authority"][0]["contractRevision"] = "20"
        with self.assertRaisesRegex(MODULE.VerificationError, "authority binding"):
            MODULE.validate_react_receipt(stale, value, "linux-x86_64")

        wrong_authority = hotm_receipt("macos-arm64")
        wrong_authority["identity"]["fortemiCommit"] = "8" * 40
        with self.assertRaisesRegex(MODULE.VerificationError, "runtime drift"):
            MODULE.validate_hotm_receipt(wrong_authority, value, "macos-arm64")

    def test_aggregate_requires_both_distinct_platforms(self):
        value = manifest()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            linux = root / "linux.json"
            duplicate = root / "duplicate.json"
            output = root / "aggregate.json"
            linux.write_text(json.dumps(platform_receipt("linux-x86_64")))
            duplicate.write_text(json.dumps(platform_receipt("linux-x86_64")))
            with self.assertRaisesRegex(MODULE.VerificationError, "duplicate platform"):
                MODULE.aggregate_receipts([linux, duplicate], value, output)

    def test_aggregate_writes_bounded_claim(self):
        value = manifest()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            linux = root / "linux.json"
            macos = root / "macos.json"
            output = root / "aggregate.json"
            linux.write_text(json.dumps(platform_receipt("linux-x86_64")))
            macos.write_text(json.dumps(platform_receipt("macos-arm64")))
            MODULE.aggregate_receipts([linux, macos], value, output)
            aggregate = json.loads(output.read_text())
            self.assertEqual(aggregate["status"], "passed")
            self.assertFalse(aggregate["claims"]["universal_portability"])
            self.assertEqual(
                set(aggregate["platform_receipts"]),
                {"linux-x86_64", "macos-arm64"},
            )


if __name__ == "__main__":
    unittest.main()
