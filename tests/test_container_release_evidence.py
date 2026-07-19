import importlib.util
import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock


SCRIPT = Path("scripts/ci/capture-container-release-evidence.py")
SPEC = importlib.util.spec_from_file_location("capture_container_release_evidence", SCRIPT)
assert SPEC and SPEC.loader
capture_module = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(capture_module)


class ContainerReleaseEvidenceTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tempdir = tempfile.TemporaryDirectory()
        self.policy = Path(self.tempdir.name) / "policy.json"
        self.policy.write_text(
            json.dumps(
                {
                    "policy_issue": 888,
                    "artifact": {"format": "fortemi.container-release-evidence.v1"},
                    "publish_path_profiles": {"ghcr-from-gitea-pat": {}},
                    "controls": {
                        "digest": {"status": "implemented"},
                        "sbom": {"status": "deferred"},
                        "provenance": {"status": "deferred"},
                        "signature": {"status": "deferred"},
                    },
                    "license_notices": {"status": "pending-gate"},
                    "families": {
                        "api": {
                            "sbom_scope": "final-stage runtime filesystem",
                            "registries": {
                                "ghcr.io": {
                                    "platforms": ["linux/amd64"],
                                    "publish_path_profile": "ghcr-from-gitea-pat",
                                }
                            },
                            "immutable_tag_patterns": ["^sha-[0-9a-f]{7}$"],
                        }
                    },
                }
            )
        )
        self.manifest = json.dumps(
            {
                "schemaVersion": 2,
                "mediaType": "application/vnd.oci.image.manifest.v1+json",
                "config": {"digest": "sha256:" + "1" * 64},
                "layers": [],
            },
            separators=(",", ":"),
        ).encode()

    def tearDown(self) -> None:
        self.tempdir.cleanup()

    def capture(self):
        return capture_module.capture(
            self.policy,
            "api",
            "a" * 40,
            "dev",
            "ghcr.io/fortemi/fortemi:sha-abcdef0",
            ["ghcr.io/fortemi/fortemi:main"],
            "2026-07-17T00:00:00Z",
        )

    def test_records_digest_and_alias_binding(self) -> None:
        with mock.patch.object(capture_module, "raw_manifest", return_value=self.manifest):
            receipt = self.capture()
        self.assertEqual(receipt["subject"]["platforms"], ["linux/amd64"])
        self.assertEqual(receipt["aliases"][0]["digest"], receipt["subject"]["digest"])
        self.assertTrue(
            receipt["subject"]["immutable_reference"].startswith(
                "ghcr.io/fortemi/fortemi@sha256:"
            )
        )

    def test_rejects_mutable_subject(self) -> None:
        with self.assertRaisesRegex(capture_module.EvidenceError, "approved immutable tag"):
            capture_module.capture(
                self.policy,
                "api",
                "a" * 40,
                "dev",
                "ghcr.io/fortemi/fortemi:main",
                [],
                "2026-07-17T00:00:00Z",
            )

    def test_rejects_alias_drift(self) -> None:
        different = self.manifest.replace(
            b'"layers":[]',
            b'"layers":[{"digest":"sha256:' + b"2" * 64 + b'"}]',
        )
        with mock.patch.object(
            capture_module, "raw_manifest", side_effect=[self.manifest, different]
        ):
            with self.assertRaisesRegex(capture_module.EvidenceError, "alias drift"):
                self.capture()

    def test_rejects_platform_drift(self) -> None:
        index = json.dumps(
            {
                "schemaVersion": 2,
                "mediaType": "application/vnd.oci.image.index.v1+json",
                "manifests": [
                    {"platform": {"os": "linux", "architecture": "arm64"}}
                ],
            }
        ).encode()
        with mock.patch.object(capture_module, "raw_manifest", return_value=index):
            with self.assertRaisesRegex(capture_module.EvidenceError, "platform mismatch"):
                self.capture()

    def test_rejects_short_source_revision(self) -> None:
        with self.assertRaisesRegex(capture_module.EvidenceError, "full lowercase"):
            capture_module.capture(
                self.policy,
                "api",
                "abcdef0",
                "dev",
                "ghcr.io/fortemi/fortemi:sha-abcdef0",
                [],
                "2026-07-17T00:00:00Z",
            )

    def test_single_platform_falls_back_when_buildx_is_unavailable(self) -> None:
        unavailable = capture_module.EvidenceError(
            "cannot inspect image: docker: 'buildx' is not a docker command"
        )
        expected = ("sha256:" + "3" * 64, ["linux/amd64"])
        with (
            mock.patch.object(capture_module, "raw_manifest", side_effect=unavailable),
            mock.patch.object(
                capture_module, "docker_manifest_descriptor", return_value=expected
            ) as fallback,
        ):
            receipt = self.capture()
        self.assertEqual(receipt["subject"]["digest"], expected[0])
        self.assertEqual(fallback.call_count, 2)

    def test_multi_platform_never_uses_single_platform_fallback(self) -> None:
        unavailable = capture_module.EvidenceError(
            "cannot inspect image: docker: 'buildx' is not a docker command"
        )
        with (
            mock.patch.object(capture_module, "raw_manifest", side_effect=unavailable),
            mock.patch.object(capture_module, "docker_manifest_descriptor") as fallback,
            self.assertRaisesRegex(capture_module.EvidenceError, "not a docker command"),
        ):
            capture_module.inspect_reference(
                "ghcr.io/fortemi/fortemi:bundle-2026.7.1",
                ["linux/amd64", "linux/arm64"],
            )
        fallback.assert_not_called()


if __name__ == "__main__":
    unittest.main()
