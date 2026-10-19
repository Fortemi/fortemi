import importlib.util
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/ci/check-buildx-policy.py"
spec = importlib.util.spec_from_file_location("check_buildx_policy", SCRIPT)
guard = importlib.util.module_from_spec(spec)
spec.loader.exec_module(guard)


class CheckBuildxPolicyTests(unittest.TestCase):
    def assert_flagged(self, content, kind):
        self.assertTrue(any(kind in message for _, message in guard.violations(content)), content)

    def test_rejects_every_known_bake_spelling(self):
        for content in (
            "docker buildx bake",
            "docker buildx --builder fortemi-multiarch bake --progress rawjson",
            "docker buildx \\\n  --builder fortemi-multiarch \\\n  bake",
            "docker buildx --builder=fortemi-multiarch bake",
            "docker buildx ba\\\nke",
            'docker buildx "ba"ke',
            "docker buildx 'b'ake",
            "REF=https://github.com/Fortemi/fortemi#main docker buildx bake",
            "run: >-\n  docker buildx\n  bake\n",
            'RUN ["docker", "buildx", "bake"]',
            "docker BUILDX BAKE",
            'run: "docker buildx bake\\n"',
            'run: "docker buildx\\tbake"',
            'RUN ["sh", "-c", "docker buildx bake\\n"]',
            "printf 'docker buildx bake\\n' | bash",
            "docker buildx bakehouse",
            "docker buildx build # never bake here",
        ):
            with self.subTest(content=content):
                self.assert_flagged(content, "bake")

    def test_rejects_every_rawjson_spelling(self):
        for content in (
            "docker buildx build --progress=rawjson",
            'docker buildx build --progress="raw"json',
            '"--progress" "rawjson"',
            "run: >-\n  docker buildx build\n  --progress\n  rawjson\n",
            "--progress=RAWJSON",
            "PROGRESS=rawjson",
        ):
            with self.subTest(content=content):
                self.assert_flagged(content, "rawjson")

    def test_allows_ordinary_builds_and_listed_bake_lines(self):
        for content in (
            "docker buildx build --progress plain",
            "docker buildx build --progress=plain -f ./Dockerfile.builder .",
        ):
            with self.subTest(content=content):
                self.assertEqual(guard.violations(content), [])
        for relative in ("scripts/ci/verify-bundle-db-secrets.py", "build/README.md"):
            with self.subTest(relative=relative):
                text = (ROOT / relative).read_text()
                self.assertEqual(guard.violations(text, relative), [])
                # A bake command added to an allow-listed file is still caught.
                self.assertTrue(guard.violations(text + "\ndocker buildx bake\n", relative))

    def test_bake_without_buildx_anywhere_in_tree_is_allowed(self):
        self.assertEqual(guard.violations("# do not bake secrets", buildx_in_tree=False), [])

    def test_buildx_in_one_file_and_bake_in_another_is_caught(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "scripts/ci").mkdir(parents=True)
            (root / "scripts/ci/env.sh").write_text("TOOL=buildx\n")
            (root / "scripts/ci/run.sh").write_text('docker "$TOOL" bake\n')
            self.assertEqual(len(guard.verify(root)), 1)

    def test_scans_all_policy_directories_and_skips_only_listed_files(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for parent in (".gitea/workflows", "scripts/ci", "build/nested", "tests"):
                (root / parent).mkdir(parents=True)
                (root / parent / "fixture").write_text("docker buildx --builder test bake\n")
            for skipped in guard.SKIP:
                (root / skipped).write_text("docker buildx bake\n")
            self.assertEqual(len(guard.verify(root)), 3)

    def test_skips_generated_bytecode_but_not_other_cache_files(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            cache = root / "scripts/ci/__pycache__"
            cache.mkdir(parents=True)
            (cache / "policy.cpython-314.pyc").write_bytes(b"docker buildx bake\n")
            self.assertEqual(guard.verify(root), [])
            # Authored files are never skipped by suffix alone.
            (root / "scripts/ci/disguised.pyc").write_text("docker buildx bake\n")
            (cache / "notes.sh").write_text("docker buildx bake\n")
            self.assertEqual(len(guard.verify(root)), 2)

    def test_rejects_non_utf8_and_nul_containing_inputs(self):
        for encoding in ("utf-16", "utf-16-le", "utf-16-be", "utf-32"):
            with self.subTest(encoding=encoding), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                (root / ".gitea/workflows").mkdir(parents=True)
                (root / ".gitea/workflows/ci.yml").write_bytes(
                    "run: docker buildx bake --progress=rawjson\n".encode(encoding))
                self.assertTrue(any("unsupported encoding" in error for error in guard.verify(root)))
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "build").mkdir()
            (root / "build/blob").write_bytes(b"\xff\xfe\x00binary")
            self.assertTrue(any("unsupported encoding" in error for error in guard.verify(root)))

    def test_repository_passes(self):
        self.assertEqual(guard.verify(ROOT), [])


if __name__ == "__main__":
    unittest.main()
