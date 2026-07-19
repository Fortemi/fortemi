from __future__ import annotations

import datetime as dt
import hashlib
import importlib.util
import json
import shutil
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "ci" / "verify-python-sidecar-locks.py"
SPEC = importlib.util.spec_from_file_location("verify_python_sidecar_locks", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)
TODAY = dt.date(2026, 7, 19)


class PythonSidecarLockTests(unittest.TestCase):
    def test_current_repository_passes(self) -> None:
        self.assertEqual(MODULE.verify(ROOT, today=TODAY), [])

    def test_open_ended_direct_requirement_is_rejected(self) -> None:
        with self.fixture_root() as root:
            requirements = root / "build/pyannote/requirements.txt"
            requirements.write_text(
                requirements.read_text(encoding="utf-8").replace(
                    "fastapi==0.139.2",
                    "fastapi>=0.100.0",
                ),
                encoding="utf-8",
            )

            output = "\n".join(MODULE.verify(root, today=TODAY))

            self.assertIn(
                "direct requirement must use an exact == or === version",
                output,
            )
            self.assertIn("requirements_sha256 does not match", output)

    def test_unhashed_lock_entry_is_rejected(self) -> None:
        with self.fixture_root() as root:
            lock = root / "build/gliner/requirements.lock"
            lock.write_text(
                lock.read_text(encoding="utf-8").replace(
                    "    --hash=sha256:571ac1dc6991c450b25a9c2d84a3705e2ae7a53467b5d111c24fa8baabbed320 \\\n"
                    "    --hash=sha256:fbcda96e87e9c92ad167c2e53839e57503ecfda18804ea28102353485033faa4\n",
                    "",
                ),
                encoding="utf-8",
            )
            self.refresh_lock_hash(root, "gliner")

            output = "\n".join(MODULE.verify(root, today=TODAY))

            self.assertIn("annotated-doc has no SHA-256 hashes", output)

    def test_docker_install_without_hash_enforcement_is_rejected(self) -> None:
        with self.fixture_root() as root:
            dockerfile = root / "build/gliner/Dockerfile"
            dockerfile.write_text(
                dockerfile.read_text(encoding="utf-8").replace(
                    "    --require-hashes \\\n",
                    "",
                ),
                encoding="utf-8",
            )

            output = "\n".join(MODULE.verify(root, today=TODAY))

            self.assertIn("missing '--require-hashes'", output)

    def test_gliner_cuda_dependency_is_rejected(self) -> None:
        with self.fixture_root() as root:
            lock = root / "build/gliner/requirements.lock"
            lock.write_text(
                lock.read_text(encoding="utf-8")
                + "\nnvidia-cuda-runtime-cu12==12.6.77 \\\n"
                + "    --hash=sha256:" + "a" * 64 + "\n",
                encoding="utf-8",
            )
            self.refresh_lock_hash(root, "gliner")

            output = "\n".join(MODULE.verify(root, today=TODAY))

            self.assertIn("CPU lock contains accelerator packages", output)

    def test_pyannote_torch_pair_mismatch_is_rejected(self) -> None:
        with self.fixture_root() as root:
            lock = root / "build/pyannote/requirements.lock"
            lock.write_text(
                lock.read_text(encoding="utf-8").replace(
                    "torchaudio==2.11.0",
                    "torchaudio==2.10.0",
                ),
                encoding="utf-8",
            )
            self.refresh_lock_hash(root, "pyannote")

            output = "\n".join(MODULE.verify(root, today=TODAY))

            self.assertIn("torch and torchaudio versions differ", output)
            self.assertIn("torchaudio must be 2.11.0", output)

    def test_pyannote_torchcodec_strict_identity_is_required(self) -> None:
        with self.fixture_root() as root:
            lock = root / "build/pyannote/requirements.lock"
            lock.write_text(
                lock.read_text(encoding="utf-8").replace(
                    "torchcodec===0.11.1",
                    "torchcodec==0.11.1",
                ),
                encoding="utf-8",
            )
            self.refresh_lock_hash(root, "pyannote")

            output = "\n".join(MODULE.verify(root, today=TODAY))

            self.assertIn("missing 'torchcodec===0.11.1'", output)

    def test_pyannote_torchcodec_direct_identity_is_required(self) -> None:
        with self.fixture_root() as root:
            requirements = root / "build/pyannote/requirements.txt"
            requirements.write_text(
                requirements.read_text(encoding="utf-8").replace(
                    "torchcodec===0.11.1",
                    "torchcodec==0.11.1",
                ),
                encoding="utf-8",
            )
            self.refresh_requirements_hash(root, "pyannote")

            output = "\n".join(MODULE.verify(root, today=TODAY))

            self.assertIn("requirements.txt: missing 'torchcodec===0.11.1'", output)

    def test_pyannote_backend_contract_is_required(self) -> None:
        with self.fixture_root() as root:
            manifest_path = root / MODULE.MANIFEST
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
            item = next(
                entry
                for entry in manifest["locks"]
                if entry["service"] == "pyannote"
            )
            item["torchcodec_backend"] = "cu126"
            manifest_path.write_text(json.dumps(manifest), encoding="utf-8")

            output = "\n".join(MODULE.verify(root, today=TODAY))

            self.assertIn("pyannote TorchCodec backend must be cpu", output)

    def test_pyannote_audio_decode_workflow_is_required(self) -> None:
        with self.fixture_root() as root:
            workflow = root / ".gitea/workflows/build-pyannote.yaml"
            workflow.write_text(
                workflow.read_text(encoding="utf-8").replace(
                    "get_all_samples()",
                    "metadata",
                ),
                encoding="utf-8",
            )

            output = "\n".join(MODULE.verify(root, today=TODAY))

            self.assertIn("missing 'get_all_samples()'", output)

    def test_pyannote_arm64_publication_is_rejected(self) -> None:
        with self.fixture_root() as root:
            workflow = root / ".gitea/workflows/build-pyannote.yaml"
            workflow.write_text(
                workflow.read_text(encoding="utf-8")
                + "\n# --platform linux/amd64,linux/arm64\n",
                encoding="utf-8",
            )

            output = "\n".join(MODULE.verify(root, today=TODAY))

            self.assertIn("pyannote must not publish arm64", output)

    def test_unreviewed_package_feed_is_rejected(self) -> None:
        with self.fixture_root() as root:
            lock = root / "build/gliner/requirements.lock"
            lock.write_text(
                lock.read_text(encoding="utf-8")
                + "\n# from https://packages.example.invalid/simple\n",
                encoding="utf-8",
            )
            self.refresh_lock_hash(root, "gliner")

            output = "\n".join(MODULE.verify(root, today=TODAY))

            self.assertIn("unreviewed package feed URLs", output)
            self.assertIn("packages.example.invalid", output)

    def test_trusted_host_escape_is_rejected(self) -> None:
        with self.fixture_root() as root:
            lock = root / "build/gliner/requirements.lock"
            lock.write_text(
                lock.read_text(encoding="utf-8")
                + "\n--trusted-host packages.example.invalid\n",
                encoding="utf-8",
            )
            self.refresh_lock_hash(root, "gliner")

            output = "\n".join(MODULE.verify(root, today=TODAY))

            self.assertIn("unsupported lock option '--trusted-host", output)

    def test_non_sha256_hash_is_rejected(self) -> None:
        with self.fixture_root() as root:
            lock = root / "build/gliner/requirements.lock"
            lock.write_text(
                lock.read_text(encoding="utf-8").replace(
                    "--hash=sha256:571ac1dc6991c450b25a9c2d84a3705e2ae7a53467b5d111c24fa8baabbed320",
                    "--hash=md5:0123456789abcdef0123456789abcdef",
                ),
                encoding="utf-8",
            )
            self.refresh_lock_hash(root, "gliner")

            output = "\n".join(MODULE.verify(root, today=TODAY))

            self.assertIn("only SHA-256 hashes are allowed", output)

    def test_stale_lock_review_is_rejected(self) -> None:
        with self.fixture_root() as root:
            manifest_path = root / MODULE.MANIFEST
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
            manifest["locks"][0]["reviewed_at"] = "2025-01-01"
            manifest_path.write_text(json.dumps(manifest), encoding="utf-8")

            output = "\n".join(MODULE.verify(root, today=TODAY))

            self.assertIn("review is stale", output)

    @staticmethod
    def refresh_lock_hash(root: Path, service: str) -> None:
        manifest_path = root / MODULE.MANIFEST
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        item = next(entry for entry in manifest["locks"] if entry["service"] == service)
        item["lock_sha256"] = hashlib.sha256((root / item["lock"]).read_bytes()).hexdigest()
        manifest_path.write_text(json.dumps(manifest), encoding="utf-8")

    @staticmethod
    def refresh_requirements_hash(root: Path, service: str) -> None:
        manifest_path = root / MODULE.MANIFEST
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        item = next(entry for entry in manifest["locks"] if entry["service"] == service)
        item["requirements_sha256"] = hashlib.sha256(
            (root / item["requirements"]).read_bytes()
        ).hexdigest()
        manifest_path.write_text(json.dumps(manifest), encoding="utf-8")

    def fixture_root(self):
        temporary = tempfile.TemporaryDirectory()
        root = Path(temporary.name)
        manifest = json.loads((ROOT / MODULE.MANIFEST).read_text(encoding="utf-8"))
        paths = {MODULE.MANIFEST, MODULE.REGENERATOR, MODULE.CI_WORKFLOW}
        for item in manifest["locks"]:
            paths.update(Path(item[field]) for field in ("requirements", "lock", "dockerfile", "workflow"))
        for path in paths:
            source = ROOT / path
            target = root / path
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, target)
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
