import copy
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import tomllib
import unittest
from unittest.mock import patch


spec = importlib.util.spec_from_file_location(
    "zip_guard", Path(__file__).with_name("zip-exception-guard.py")
)
guard = importlib.util.module_from_spec(spec)
spec.loader.exec_module(guard)

REGISTRY = "registry+https://github.com/rust-lang/crates.io-index"


def fixture():
    """Resolved ZIP 9 graph, including renamed and target-specific deps."""
    entries = [
        ("jobs", "matric-jobs", "0.1.0"),
        ("calamine", "calamine", "0.37.0"),
        ("swagger", "utoipa-swagger-ui", "10.0.2"),
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
        "resolve": {"root": "jobs", "nodes": [
            {"id": key, "deps": [], "dependencies": []} for key, _, _ in entries
        ]},
    }
    for parent, child, kind in [
        ("jobs", "calamine", None), ("jobs", "swagger", None),
        ("jobs", "zip9", None), ("calamine", "zip9", None),
        ("swagger", "zip9", "build"), ("zip9", "bzip2", None),
        ("bzip2", "libbz2", None),
    ]:
        add_edge(metadata, parent, child, kind)
    return metadata


def node(metadata, key):
    return next(n for n in metadata["resolve"]["nodes"] if n["id"] == key)


def package(metadata, key):
    return next(p for p in metadata["packages"] if p["id"] == key)


def add_edge(metadata, parent, child, kind=None):
    entry = node(metadata, parent)
    entry["deps"].append({
        "name": "renamed_dependency", "pkg": child,
        "dep_kinds": [{"kind": kind, "target": None}],
    })
    entry["dependencies"].append(child)


def add_old_zip(metadata, version="2.4.2", parent="calamine", kind=None):
    metadata["packages"].append({
        "id": "oldzip", "name": "zip", "version": version, "source": REGISTRY,
    })
    metadata["resolve"]["nodes"].append({"id": "oldzip", "deps": [], "dependencies": []})
    if parent:
        add_edge(metadata, parent, "oldzip", kind)


def lock_fixture(metadata):
    packages = {p["id"]: p for p in metadata["packages"]}
    locked = []
    for entry in metadata["resolve"]["nodes"]:
        item = copy.deepcopy(packages[entry["id"]])
        del item["id"]
        if item["source"] is None:
            del item["source"]
        item["dependencies"] = [
            f'{packages[d["pkg"]]["name"]} {packages[d["pkg"]]["version"]}'
            for d in entry["deps"]
        ]
        locked.append(item)
    return {"version": 4, "package": locked}


class ZipExceptionGuardTests(unittest.TestCase):
    def setUp(self):
        self.metadata = fixture()

    def check(self, lock=None):
        guard.check(self.metadata, lock if lock is not None else lock_fixture(self.metadata))

    def test_current_zip9_only_passes(self):
        self.check()

    def test_old_zip_fails_even_through_formerly_approved_parents(self):
        for version, parent, parent_version in [
            ("2.4.2", "calamine", "0.26.1"), ("3.0.0", "swagger", "9.0.2"),
        ]:
            for kind in [None, "build", "dev"]:
                with self.subTest(version=version, kind=kind):
                    self.metadata = fixture()
                    package(self.metadata, parent)["version"] = parent_version
                    add_old_zip(self.metadata, version, parent, kind)
                    with self.assertRaisesRegex(guard.GuardError, "unapproved"):
                        self.check()

    def test_old_zip_with_additional_target_kind_fails(self):
        add_old_zip(self.metadata, "3.0.0", "swagger", "build")
        node(self.metadata, "swagger")["deps"][-1]["dep_kinds"].append(
            {"kind": None, "target": "cfg(unix)"}
        )
        with self.assertRaisesRegex(guard.GuardError, "unapproved"):
            self.check()

    def test_old_zip_extra_unapproved_workspace_or_disconnected_fails(self):
        for parent in ["calamine", "swagger", "jobs", None]:
            with self.subTest(parent=parent):
                self.metadata = fixture()
                add_old_zip(self.metadata, parent=parent)
                if parent == "swagger":
                    add_edge(self.metadata, "calamine", "oldzip")
                with self.assertRaisesRegex(guard.GuardError, "unapproved"):
                    self.check()

    def test_old_zip_as_workspace_root_fails(self):
        add_old_zip(self.metadata, parent=None)
        self.metadata["workspace_members"].append("oldzip")
        with self.assertRaisesRegex(guard.GuardError, "unapproved"):
            self.check()

    def test_updated_former_parent_does_not_exempt_old_zip(self):
        package(self.metadata, "calamine")["version"] = "0.26.2"
        add_old_zip(self.metadata)
        with self.assertRaisesRegex(guard.GuardError, "unapproved"):
            self.check()

    def test_extra_libbz2_path_fails_direct_and_shared_transitive(self):
        for child in ["libbz2", "bzip2"]:
            with self.subTest(child=child):
                self.metadata = fixture()
                add_edge(self.metadata, "jobs", child)
                with self.assertRaisesRegex(guard.GuardError, "without zip 9"):
                    self.check()

    def test_unapproved_zip_versions_fail(self):
        for version in ["2.4.2", "3.0.0", "2.4.3", "3.0.1", "8.9.0", "10.0.0",
                        "9.0.0-pre2", "9.0.5-rc.1", "9.0.5+build", "9.0.3", "9.0.2",
                        "9.0.1", "9.0.0", "9.01.0", "9.-1.5"]:
            with self.subTest(version=version):
                self.metadata = fixture()
                package(self.metadata, "zip9")["version"] = version
                with self.assertRaisesRegex(guard.GuardError, "unapproved"):
                    self.check()

    def test_new_stable_zip9_versions_pass(self):
        for version in ["9.0.4", "9.0.5", "9.1.0", "9.10.12"]:
            with self.subTest(version=version):
                self.metadata = fixture()
                package(self.metadata, "zip9")["version"] = version
                self.check()

    def test_lock_only_old_zip_fails_even_with_former_parent_or_no_parent(self):
        for parent in ["calamine", "utoipa-swagger-ui", "matric-jobs", None]:
            with self.subTest(parent=parent):
                lock = lock_fixture(self.metadata)
                lock["package"].append({"name": "zip", "version": "2.4.2", "source": REGISTRY})
                if parent:
                    next(p for p in lock["package"] if p["name"] == parent)["dependencies"].append("zip 2.4.2")
                with self.assertRaisesRegex(guard.GuardError, "Cargo.lock: unapproved"):
                    self.check(lock=lock)

    def test_lock_only_extra_libbz2_path_fails(self):
        for child in ["libbz2-rs-sys", "bzip2"]:
            with self.subTest(child=child):
                lock = lock_fixture(self.metadata)
                next(p for p in lock["package"] if p["name"] == "calamine")["dependencies"].append(child)
                with self.assertRaisesRegex(guard.GuardError, "Cargo.lock: libbz2"):
                    self.check(lock=lock)

    def test_metadata_lock_package_mismatch_fails(self):
        lock = lock_fixture(self.metadata)
        lock["package"] = [p for p in lock["package"] if p["name"] != "libbz2-rs-sys"]
        with self.assertRaisesRegex(guard.GuardError, "absent from Cargo.lock"):
            self.check(lock=lock)

    def test_metadata_lock_edge_mismatch_fails(self):
        lock = lock_fixture(self.metadata)
        next(p for p in lock["package"] if p["name"] == "calamine")["dependencies"] = []
        with self.assertRaisesRegex(guard.GuardError, "edge absent from Cargo.lock"):
            self.check(lock=lock)

    def test_unresolved_or_ambiguous_lock_dependency_fails(self):
        for dependency in ["missing", "zip 9.0.4", "zip 9.0.5 (wrong-source)", "zip"]:
            with self.subTest(dependency=dependency):
                lock = lock_fixture(self.metadata)
                if dependency == "zip":
                    lock["package"].append({"name": "zip", "version": "9.1.0", "source": REGISTRY})
                lock["package"][0]["dependencies"].append(dependency)
                with self.assertRaisesRegex(guard.GuardError, "unresolved or ambiguous"):
                    self.check(lock=lock)

    def test_duplicate_lock_identity_fails(self):
        lock = lock_fixture(self.metadata)
        lock["package"].append(copy.deepcopy(lock["package"][0]))
        with self.assertRaisesRegex(guard.GuardError, "duplicate package identity"):
            self.check(lock=lock)

    def test_incomplete_or_malformed_metadata_fails_closed(self):
        valid_lock = lock_fixture(self.metadata)
        mutations = [
            lambda m: m["resolve"].update(nodes=[]),
            lambda m: m.update(workspace_members=[]),
            lambda m: m.update(workspace_members=["unknown"]),
            lambda m: m["resolve"].update(root="unknown"),
            lambda m: m["packages"].append(copy.deepcopy(m["packages"][0])),
            lambda m: m["resolve"]["nodes"].append(copy.deepcopy(m["resolve"]["nodes"][0])),
            lambda m: m["resolve"]["nodes"].pop(),
            lambda m: node(m, "jobs")["deps"][0].update(pkg="unknown"),
            lambda m: node(m, "jobs")["deps"][0].update(dep_kinds=[]),
            lambda m: node(m, "jobs")["deps"][0].update(dep_kinds=[{"kind": "invalid", "target": None}]),
            lambda m: node(m, "jobs")["deps"][0].update(dep_kinds=[{"kind": None}]),
            lambda m: node(m, "jobs")["deps"][0]["dep_kinds"][0].update(target=42),
            lambda m: node(m, "jobs")["dependencies"].append("unknown"),
            lambda m: m.update(resolve=None),
            lambda m: m.update(packages=None),
        ]
        for mutate in mutations:
            with self.subTest(mutation=mutate):
                self.metadata = fixture()
                mutate(self.metadata)
                with self.assertRaises(guard.GuardError):
                    self.check(lock=valid_lock)

    def test_removed_zip_waivers_and_expiry_logic_are_absent(self):
        root = Path(__file__).resolve().parents[2]
        for path in [root / ".cargo/audit.toml", root / "deny.toml"]:
            text = path.read_text()
            ignores = tomllib.loads(text)["advisories"]["ignore"]
            for advisory in ["RUSTSEC-2026-0367", "RUSTSEC-2026-0368", "RUSTSEC-2026-0369", "RUSTSEC-2026-0429"]:
                self.assertNotIn(advisory, ignores)
            self.assertFalse(any(item.startswith("zip@") for item in ignores))
            self.assertNotIn("#1221 Option B", text)
            self.assertNotIn("EXCEPTION_EXPIRY", text)
            self.assertNotIn("both paths; <=30 days", text)
        source = Path(guard.__file__).read_text()
        self.assertNotIn("EXCEPTION_EXPIRY", source)
        self.assertNotIn("APPROVED_PARENTS", source)
        self.assertNotIn("datetime", source)

    def test_cli_file_stdin_and_failure_exit(self):
        with tempfile.TemporaryDirectory() as directory:
            metadata_path = Path(directory) / "metadata.json"
            lock_path = Path(directory) / "Cargo.lock"
            metadata_path.write_text(json.dumps(self.metadata))
            lock = lock_fixture(self.metadata)
            lock_path.write_text("version = 4\n" + "".join(
                "\n[[package]]\n" + "".join(f"{k} = {json.dumps(v)}\n" for k, v in p.items())
                for p in lock["package"]
            ))
            with patch("sys.stdout", new_callable=io.StringIO) as output:
                self.assertEqual(guard.main([str(metadata_path), "--lockfile", str(lock_path)]), 0)
                self.assertNotIn("expires", output.getvalue())
                with patch("sys.stdin", io.StringIO(json.dumps(self.metadata))):
                    self.assertEqual(guard.main(["--lockfile", str(lock_path)]), 0)
            add_old_zip(self.metadata)
            metadata_path.write_text(json.dumps(self.metadata))
            with patch("sys.stderr", new_callable=io.StringIO) as errors:
                self.assertEqual(guard.main([str(metadata_path), "--lockfile", str(lock_path)]), 1)
                self.assertIn("unapproved", errors.getvalue())
            metadata_path.write_text("{")
            with patch("sys.stderr", new_callable=io.StringIO):
                self.assertEqual(guard.main([str(metadata_path), "--lockfile", str(lock_path)]), 1)
                self.assertEqual(guard.main([str(metadata_path) + ".missing", "--lockfile", str(lock_path)]), 1)


if __name__ == "__main__":
    unittest.main()
