from __future__ import annotations

import datetime as dt
import importlib.util
import json
import shutil
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "ci" / "verify-third-party-dependencies.py"
SPEC = importlib.util.spec_from_file_location(
    "verify_third_party_dependencies",
    SCRIPT,
)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)
TODAY = dt.date(2026, 7, 19)


class ThirdPartyDependencyTests(unittest.TestCase):
    def test_current_repository_passes(self) -> None:
        self.assertEqual(MODULE.verify(ROOT, today=TODAY), [])

    def test_unreviewed_mutable_compose_image_is_rejected(self) -> None:
        with self.fixture_root() as root:
            compose = root / "docker-compose.bundle.yml"
            compose.write_text(
                compose.read_text(encoding="utf-8")
                + "\n  injected:\n    image: attacker/example:latest\n",
                encoding="utf-8",
            )

            errors = MODULE.verify(root, today=TODAY)

            self.assertIn("mutable or unreviewed", "\n".join(errors))
            self.assertIn("attacker/example:latest", "\n".join(errors))

    def test_unknown_digest_is_rejected(self) -> None:
        with self.fixture_root() as root:
            compose = root / "docker-compose.bundle.yml"
            compose.write_text(
                compose.read_text(encoding="utf-8")
                + "\n  injected:\n    image: example.invalid/image:1"
                + "@sha256:" + "a" * 64 + "\n",
                encoding="utf-8",
            )

            errors = MODULE.verify(root, today=TODAY)

            self.assertIn("digest is absent", "\n".join(errors))

    def test_fortemi_namespace_lookalike_is_rejected(self) -> None:
        with self.fixture_root() as root:
            compose = root / "docker-compose.bundle.yml"
            compose.write_text(
                compose.read_text(encoding="utf-8")
                + "\n  injected:\n"
                + "    image: registry.invalid/fortemi/fortemi:latest\n",
                encoding="utf-8",
            )

            errors = MODULE.verify(root, today=TODAY)

            self.assertIn("mutable or unreviewed", "\n".join(errors))
            self.assertIn("registry.invalid", "\n".join(errors))

    def test_manifest_digest_drift_is_rejected(self) -> None:
        with self.fixture_root() as root:
            manifest_path = root / MODULE.MANIFEST
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
            manifest["images"][0]["digest"] = "sha256:" + "b" * 64
            manifest_path.write_text(json.dumps(manifest), encoding="utf-8")

            errors = MODULE.verify(root, today=TODAY)

            self.assertIn("must end with its digest", "\n".join(errors))

    def test_malformed_manifest_values_fail_with_diagnostics(self) -> None:
        with self.fixture_root() as root:
            manifest_path = root / MODULE.MANIFEST
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
            manifest["images"][0]["id"] = []
            manifest["images"][0]["immutable_reference"] = []
            manifest_path.write_text(json.dumps(manifest), encoding="utf-8")

            errors = MODULE.verify(root, today=TODAY)
            output = "\n".join(errors)

            self.assertIn(".id must be a non-empty string", output)
            self.assertIn(".immutable_reference must end with its digest", output)

    def test_unreviewed_external_package_feed_is_rejected(self) -> None:
        with self.fixture_root() as root:
            dockerfile = root / "build" / "Dockerfile.builder"
            dockerfile.write_text(
                dockerfile.read_text(encoding="utf-8")
                + '\nRUN echo "deb https://packages.example.invalid stable main"\n',
                encoding="utf-8",
            )

            errors = MODULE.verify(root, today=TODAY)

            self.assertIn("external package feed is unreviewed", "\n".join(errors))

    def test_stale_review_and_expired_exception_are_rejected(self) -> None:
        with self.fixture_root() as root:
            manifest_path = root / MODULE.MANIFEST
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
            manifest["images"][0]["reviewed_at"] = "2025-01-01"
            manifest["package_feeds"][0]["exception_expires"] = "2026-07-18"
            manifest_path.write_text(json.dumps(manifest), encoding="utf-8")

            errors = MODULE.verify(root, today=TODAY)
            output = "\n".join(errors)

            self.assertIn("review is stale", output)
            self.assertIn("exception is expired", output)

    def test_retired_llamacpp_namespace_is_rejected(self) -> None:
        with self.fixture_root() as root:
            compose = root / "docker-compose.llamacpp.yml"
            compose.write_text(
                compose.read_text(encoding="utf-8").replace(
                    "ghcr.io/ggml-org/llama.cpp:server@",
                    "ghcr.io/ggerganov/llama.cpp:server@",
                ),
                encoding="utf-8",
            )

            errors = MODULE.verify(root, today=TODAY)

            self.assertIn("digest is absent", "\n".join(errors))
            self.assertIn("ggerganov", "\n".join(errors))

    def fixture_root(self):
        temporary = tempfile.TemporaryDirectory()
        root = Path(temporary.name)
        manifest = json.loads((ROOT / MODULE.MANIFEST).read_text(encoding="utf-8"))
        surfaces = {
            surface
            for image in manifest["images"]
            for surface in image["surfaces"]
        }
        surfaces.update(feed["surface"] for feed in manifest["package_feeds"])
        for surface in surfaces:
            source = ROOT / surface
            target = root / surface
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, target)
        manifest_target = root / MODULE.MANIFEST
        manifest_target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / MODULE.MANIFEST, manifest_target)
        return FixtureDirectory(temporary, root)


class FixtureDirectory:
    def __init__(self, temporary: tempfile.TemporaryDirectory, root: Path):
        self.temporary = temporary
        self.root = root

    def __enter__(self) -> Path:
        return self.root

    def __exit__(self, *args) -> None:
        self.temporary.cleanup()


if __name__ == "__main__":
    unittest.main()
