import importlib.util
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/ci/check-no-buildx-bake.py"
spec = importlib.util.spec_from_file_location("check_no_buildx_bake", SCRIPT)
guard = importlib.util.module_from_spec(spec)
spec.loader.exec_module(guard)


class CheckNoBuildxBakeTests(unittest.TestCase):
    def test_rejects_bake_with_intervening_options_and_continuations(self):
        for command in (
            "docker buildx bake",
            "docker buildx --builder fortemi-multiarch bake --progress rawjson",
            "docker buildx \\\n  --builder fortemi-multiarch \\\n  bake",
            "docker buildx --builder=fortemi-multiarch bake",
            "docker buildx ba\\\nke",
            "docker buildx \\\r\n bake",
        ):
            with self.subTest(command=command):
                self.assertTrue(any("bake" in error for _, error in guard.violations(command)))

    def test_rejects_all_rawjson_progress_forms(self):
        for separator in ("=", " ", " \\\n  "):
            for value in ("rawjson", '"rawjson"', "'rawjson'"):
                command = f"docker buildx build --progress{separator}{value}"
                with self.subTest(command=command):
                    self.assertTrue(any("progress" in error for _, error in guard.violations(command)))

    def test_allows_builds_and_unrelated_bake_words(self):
        for content in (
            "docker buildx build --progress plain",
            "docker buildx build --progress=plain",
            "docker buildx build # do not bake secrets into images",
            "# docker buildx bake --progress=rawjson",
            'errors.append("Dockerfile.bundle must not bake POSTGRES_PASSWORD")',
            "docker buildx build\nbake",
            "docker my-buildx bake",
            "docker buildx bakehouse",
            "docker buildx build --progress=rawjsonish",
            "docker buildx build --progress=rawjson-extra",
            "docker buildx build --progress rawjson.txt",
            "docker buildx build -f ./bake.Dockerfile .",
        ):
            with self.subTest(content=content):
                self.assertEqual(guard.violations(content), [])
        self.assertEqual(
            guard.violations((ROOT / "scripts/ci/verify-bundle-db-secrets.py").read_text()), []
        )

    def test_scans_all_policy_directories_and_skips_itself(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for parent in (".gitea/workflows", "scripts/ci", "build/nested", "tests"):
                (root / parent).mkdir(parents=True)
                (root / parent / "fixture").write_text("docker buildx --builder test bake\n")
            (root / "scripts/ci/check-no-buildx-bake.py").write_text("docker buildx bake\n")
            (root / "tests/test_check_no_buildx_bake.py").write_text("docker buildx bake\n")
            self.assertEqual(len(guard.verify(root)), 3)

    def test_skips_generated_bytecode(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            cache = root / "scripts/ci/__pycache__"
            cache.mkdir(parents=True)
            (cache / "check-no-buildx-bake.cpython-314.pyc").write_bytes(b"docker buildx bake\n")
            (root / "scripts/ci/stray.pyc").write_bytes(b"docker buildx bake\n")
            self.assertEqual(guard.verify(root), [])


if __name__ == "__main__":
    unittest.main()
