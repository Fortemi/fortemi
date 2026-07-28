import importlib.util
import hashlib
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
            "server_compatibility_revision": "2026-07-06",
            "schema_bundle_sha256": "2" * 64,
            "profile": "2.0.0/full-v1",
        },
        "participants": {
            "fortemi_react": {
                "repository": "Fortemi/fortemi-react",
                "commit": COMMIT_R,
                "package": "@fortemi/core",
                "package_version": "2026.7.14",
                "package_tarball_sha256": "8" * 64,
                "profile": "2.0.0/full-v1",
            },
            "hotm": {
                "repository": "Fortemi/HotM",
                "commit": COMMIT_H,
                "profile": "2.0.0/full-v1",
                "sidecar_release": "sidecar-test",
                "sidecar_assets": {
                    "linux-x86_64": "6" * 64,
                    "macos-arm64": "7" * 64,
                },
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
        "required_gates": list(MODULE.REQUIRED_GATES),
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
            "fortemi_react_package": "8" * 64,
            "hotm": COMMIT_H,
        },
        "required_gates": {
            gate: True for gate in MODULE.REQUIRED_GATES
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
        "required_gates": {
            gate: True
            for gate in MODULE.REQUIRED_GATES
            if gate.startswith("authority.")
        },
        "claims": manifest()["claim_boundary"],
    }


def write_platform_bundle(root, platform_id, name=None):
    directory = root / (name or platform_id)
    directory.mkdir()
    receipt = platform_receipt(platform_id)
    for child, filename in {
        "authority": "authority-receipt.json",
        "fortemi_react": "react-receipt.json",
        "hotm": "hotm-receipt.json",
    }.items():
        child_path = directory / filename
        child_path.write_text(f"{platform_id}:{child}\n")
        receipt["child_receipts"][child] = hashlib.sha256(
            child_path.read_bytes()
        ).hexdigest()
    receipt_path = directory / "platform-receipt.json"
    receipt_path.write_text(json.dumps(receipt))
    return receipt_path


def react_receipt(platform_id):
    return {
        "schemaVersion": "fortemi.platform-contract-receipt.v1",
        "status": "passed",
        "repository": {"commit": COMMIT_R},
        "package": {"name": "@fortemi/core", "version": "2026.7.14"},
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
        "liveServer": {
            "status": "passed",
            "server": {
                "compatibility": {
                    "contractRevision": "2026-07-06",
                    "authRequired": True,
                }
            },
            "claims": {
                "liveServerToCore": True,
                "cleanDestination": True,
                "zeroMutationOnRejection": True,
            },
        },
        "claims": {
            "suiteWide": False,
            "completeBackup": False,
            "universalPlatformPortability": False,
        },
    }


def hotm_receipt(platform_id):
    os_name, arch, target, desktop_target = {
        "linux-x86_64": (
            "linux",
            "x86_64",
            "x86_64-unknown-linux-gnu",
            "tauri-command-core-linux-x86_64",
        ),
        "macos-arm64": (
            "darwin",
            "arm64",
            "aarch64-apple-darwin",
            "tauri-command-core-darwin-arm64",
        ),
    }[platform_id]
    return {
        "schemaVersion": "hotm.live-asset-ci-receipt.v1",
        "issue": "Fortemi/HotM#284",
        "status": "passed",
        "profile": "2.0.0/full-v1",
        "identity": {
            "hotmCommit": COMMIT_H,
            "hotmWorktreeDirty": False,
            "fortemiCommit": "9" * 40,
            "fortemiHealthCommit": "9" * 40,
            "sidecarRelease": "sidecar-test",
            "sidecarSha256": {
                "linux-x86_64": "6" * 64,
                "macos-arm64": "7" * 64,
            }[platform_id],
            "fixture": MODULE.HOTM_FIXTURE_IDENTITY,
        },
        "execution": {
            "os": os_name,
            "arch": arch,
            "target": target,
            "desktopTarget": desktop_target,
            "headless": True,
            "authenticationRequired": True,
            "storageBackend": "filesystem",
            "browserTarget": "playwright-chromium",
        },
        "children": {
            "browser": {"status": "passed", "sha256": "3" * 64},
            "tauri": {"status": "passed", "sha256": "4" * 64},
            "authorityContracts": {"status": "passed", "sha256": "5" * 64},
        },
        "claims": {
            **{claim: True for claim in MODULE.HOTM_REQUIRED_TRUE_CLAIMS},
            "launchedDesktopGui": False,
            "interactiveNativeDialogs": False,
            "suiteWidePortability": False,
        },
    }


class SuitePlatformMatrixTests(unittest.TestCase):
    def test_runner_defines_portable_package_digest_helper(self):
        runner = (
            ROOT / "scripts/ci/run-suite-platform-contract.sh"
        ).read_text(encoding="utf-8")
        self.assertIn("sha256_file() {", runner)
        self.assertIn('hashlib.file_digest(handle, "sha256")', runner)

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

    def test_rejects_manifest_that_omits_a_required_gate(self):
        value = manifest()
        value["required_gates"].remove("react-core.live-server")
        with self.assertRaisesRegex(MODULE.VerificationError, "exact suite gate set"):
            MODULE.validate_manifest(value)

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

        stale_live = react_receipt("linux-x86_64")
        stale_live["liveServer"]["server"]["compatibility"]["contractRevision"] = "20"
        with self.assertRaisesRegex(MODULE.VerificationError, "live authority-to-core"):
            MODULE.validate_react_receipt(stale_live, value, "linux-x86_64")

        wrong_package = react_receipt("linux-x86_64")
        wrong_package["package"]["version"] = "2026.7.13"
        with self.assertRaisesRegex(MODULE.VerificationError, "package identity"):
            MODULE.validate_react_receipt(wrong_package, value, "linux-x86_64")

        wrong_authority = hotm_receipt("macos-arm64")
        wrong_authority["identity"]["fortemiCommit"] = "8" * 40
        with self.assertRaisesRegex(MODULE.VerificationError, "runtime drift"):
            MODULE.validate_hotm_receipt(wrong_authority, value, "macos-arm64")

        wrong_sidecar = hotm_receipt("macos-arm64")
        wrong_sidecar["identity"]["sidecarSha256"] = "6" * 64
        with self.assertRaisesRegex(MODULE.VerificationError, "sidecar asset drift"):
            MODULE.validate_hotm_receipt(wrong_sidecar, value, "macos-arm64")

    def test_aggregate_requires_both_distinct_platforms(self):
        value = manifest()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            linux = write_platform_bundle(root, "linux-x86_64", "linux")
            duplicate = write_platform_bundle(root, "linux-x86_64", "duplicate")
            output = root / "aggregate.json"
            with self.assertRaisesRegex(MODULE.VerificationError, "duplicate platform"):
                MODULE.aggregate_receipts([linux, duplicate], value, output)

    def test_aggregate_writes_bounded_claim(self):
        value = manifest()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            linux = write_platform_bundle(root, "linux-x86_64")
            macos = write_platform_bundle(root, "macos-arm64")
            output = root / "aggregate.json"
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
