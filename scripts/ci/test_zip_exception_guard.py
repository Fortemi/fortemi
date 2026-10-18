import copy
from datetime import date, datetime, timezone
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch


spec = importlib.util.spec_from_file_location(
    "zip_guard", Path(__file__).with_name("zip-exception-guard.py")
)
guard = importlib.util.module_from_spec(spec)
spec.loader.exec_module(guard)

TODAY = date(2026, 10, 17)
REGISTRY = "registry+https://github.com/rust-lang/crates.io-index"


def fixture():
    """Cargo metadata shape, including renamed deps and target-specific kinds."""
    entries = [
        ("jobs", "matric-jobs", "0.1.0"),
        ("calamine", "calamine", "0.26.1"),
        ("swagger", "utoipa-swagger-ui", "9.0.2"),
        ("zip2", "zip", "2.4.2"),
        ("zip3", "zip", "3.0.0"),
        ("zip9", "zip", "9.0.5"),
        ("bzip2", "bzip2", "0.6.1"),
        ("libbz2", "libbz2-rs-sys", "0.2.5"),
    ]
    metadata = {
        "version": 1,
        "workspace_members": ["jobs"],
        "packages": [
            {"id": key, "name": name, "version": version,
             "source": None if key == "jobs" else REGISTRY}
            for key, name, version in entries
        ],
        "resolve": {"nodes": [{"id": key, "deps": []} for key, _, _ in entries]},
    }
    for parent, child, kind in [
        ("jobs", "calamine", None), ("jobs", "swagger", None),
        ("jobs", "zip9", None), ("calamine", "zip2", None),
        ("swagger", "zip3", "build"), ("zip9", "bzip2", None),
        ("bzip2", "libbz2", None),
    ]:
        add_edge(metadata, parent, child, kind)
    return metadata


def add_edge(metadata, parent, child, kind=None):
    node = next(n for n in metadata["resolve"]["nodes"] if n["id"] == parent)
    node["deps"].append({
        "name": "renamed_dependency", "pkg": child,
        "dep_kinds": [{"kind": kind, "target": None}],
    })


def lock_fixture(metadata):
    packages = {p["id"]: p for p in metadata["packages"]}
    locked = []
    for node in metadata["resolve"]["nodes"]:
        package = copy.deepcopy(packages[node["id"]])
        del package["id"]
        if package["source"] is None:
            del package["source"]
        package["dependencies"] = [
            f'{packages[d["pkg"]]["name"]} {packages[d["pkg"]]["version"]}'
            for d in node["deps"]
        ]
        locked.append(package)
    return {"version": 4, "package": locked}


class ZipExceptionGuardTests(unittest.TestCase):
    def setUp(self):
        self.metadata = fixture()

    def check(self, today=TODAY, lock=None):
        guard.check(self.metadata, lock if lock is not None else lock_fixture(self.metadata), today)

    def test_approved_graph_passes_including_expiry_day(self):
        self.check()
        self.check(today=guard.EXCEPTION_EXPIRY)

    def test_extra_old_zip_consumer_fails(self):
        add_edge(self.metadata, "swagger", "zip2")
        with self.assertRaisesRegex(guard.GuardError, "extra consumer"):
            self.check()

    def test_wrong_edge_kind_fails(self):
        for parent, kind in [("calamine", "build"), ("swagger", None), ("swagger", "dev")]:
            with self.subTest(parent=parent, kind=kind):
                self.metadata = fixture()
                node = next(n for n in self.metadata["resolve"]["nodes"] if n["id"] == parent)
                node["deps"][0]["dep_kinds"][0]["kind"] = kind
                with self.assertRaisesRegex(guard.GuardError, "wrong edge kind"):
                    self.check()

    def test_additional_target_kind_fails(self):
        node = next(n for n in self.metadata["resolve"]["nodes"] if n["id"] == "swagger")
        node["deps"][0]["dep_kinds"].append({"kind": None, "target": "cfg(unix)"})
        with self.assertRaisesRegex(guard.GuardError, "wrong edge kind"):
            self.check()

    def test_workspace_crate_on_old_zip_fails_even_if_renamed(self):
        add_edge(self.metadata, "jobs", "zip2", "dev")
        with self.assertRaisesRegex(guard.GuardError, "workspace crate"):
            self.check()

    def test_extra_libbz2_path_fails_direct_and_shared_transitive(self):
        for child in ["libbz2", "bzip2"]:
            with self.subTest(child=child):
                self.metadata = fixture()
                add_edge(self.metadata, "jobs", child)
                with self.assertRaisesRegex(guard.GuardError, "without zip 9"):
                    self.check()

    def test_post_expiry_fails(self):
        with self.assertRaisesRegex(guard.GuardError, "expired"):
            self.check(today=date(2026, 11, 17))

    def test_unapproved_zip_versions_fail(self):
        for version in ["2.4.3", "3.0.1", "8.9.0", "10.0.0", "9.0.0-pre2", "9.0.5-rc.1", "9.0.3", "9.0.0"]:
            with self.subTest(version=version):
                self.metadata = fixture()
                next(p for p in self.metadata["packages"] if p["id"] == "zip9")["version"] = version
                with self.assertRaisesRegex(guard.GuardError, "unapproved"):
                    self.check()

    def test_updated_exception_parent_version_fails(self):
        next(p for p in self.metadata["packages"] if p["id"] == "calamine")["version"] = "0.26.2"
        with self.assertRaisesRegex(guard.GuardError, "extra consumer"):
            self.check()

    def test_new_zip9_version_passes(self):
        next(p for p in self.metadata["packages"] if p["id"] == "zip9")["version"] = "9.1.0"
        self.check()

    def test_lock_only_extra_consumer_fails(self):
        lock = lock_fixture(self.metadata)
        next(p for p in lock["package"] if p["name"] == "matric-jobs")["dependencies"].append("zip 2.4.2")
        with self.assertRaisesRegex(guard.GuardError, "Cargo.lock: workspace crate"):
            self.check(lock=lock)

    def test_lock_only_extra_libbz2_path_fails(self):
        lock = lock_fixture(self.metadata)
        next(p for p in lock["package"] if p["name"] == "calamine")["dependencies"].append("bzip2")
        with self.assertRaisesRegex(guard.GuardError, "Cargo.lock: libbz2"):
            self.check(lock=lock)

    def test_incomplete_metadata_fails(self):
        self.metadata["resolve"]["nodes"] = []
        with self.assertRaisesRegex(guard.GuardError, "missing workspace roots"):
            self.check()

    def test_metadata_lock_mismatch_fails(self):
        lock = lock_fixture(self.metadata)
        lock["package"] = [p for p in lock["package"] if p["name"] != "libbz2-rs-sys"]
        with self.assertRaisesRegex(guard.GuardError, "absent from Cargo.lock"):
            self.check(lock=lock)

    def test_policy_comments_match_single_expiry(self):
        root = Path(__file__).resolve().parents[2]
        for path in [root / ".cargo/audit.toml", root / "deny.toml"]:
            text = path.read_text().split("# #1221 Option B", 1)[1]
            self.assertIn(f"Expiry: {guard.EXCEPTION_EXPIRY} (both paths; <=30 days)", text)

    def test_cli_file_stdin_and_failure_exit(self):
        with tempfile.TemporaryDirectory() as directory:
            metadata_path = Path(directory) / "metadata.json"
            lock_path = Path(directory) / "Cargo.lock"
            metadata_path.write_text(json.dumps(self.metadata))
            lock = lock_fixture(self.metadata)
            # Fixture values are JSON strings/arrays, also valid TOML values.
            lock_path.write_text("version = 4\n" + "".join(
                "\n[[package]]\n" + "".join(f"{k} = {json.dumps(v)}\n" for k, v in p.items())
                for p in lock["package"]
            ))
            with patch.object(guard, "datetime") as clock, patch("sys.stdout", new_callable=io.StringIO):
                clock.now.return_value = datetime(2026, 10, 17, tzinfo=timezone.utc)
                self.assertEqual(guard.main([str(metadata_path), "--lockfile", str(lock_path)]), 0)
                clock.now.assert_called_with(timezone.utc)
                with patch("sys.stdin", io.StringIO(json.dumps(self.metadata))):
                    self.assertEqual(guard.main(["--lockfile", str(lock_path)]), 0)
                clock.now.return_value = datetime(2026, 11, 17, tzinfo=timezone.utc)
                with patch("sys.stderr", new_callable=io.StringIO) as errors:
                    self.assertEqual(guard.main([str(metadata_path), "--lockfile", str(lock_path)]), 1)
                    self.assertIn("expired", errors.getvalue())


if __name__ == "__main__":
    unittest.main()
