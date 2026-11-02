#!/usr/bin/env python3
"""Offline behavioral checks of the real OTEL fixture's PostgreSQL gate.

No collector acceptance is claimed: cargo deliberately fails at bootstrap.
All Docker, cargo, sleep and curl commands are temporary PATH stubs. The port
allocator is stubbed too, so this fixture does not even open a local socket.
"""

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts/test-otel-collector.sh"

STUB = r'''
import json
import os
from pathlib import Path
import sys

root = Path(os.environ["READINESS_FIXTURE"])
name = Path(sys.argv[0]).name
args = sys.argv[1:]
state_path = root / "state.json"
state = json.loads(state_path.read_text()) if state_path.exists() else {"probes": 0}

def record(kind, **fields):
    with (root / "events.jsonl").open("a") as output:
        output.write(json.dumps({"kind": kind, **fields}) + "\n")

def finish(code=0):
    state_path.write_text(json.dumps(state))
    sys.exit(code)

def unexpected():
    record("unexpected", command=name, args=args)
    finish(97)

if name == "python3":
    if len(args) == 2 and args[0] == "-c" and "socket" in args[1]:
        record("port")
        print("25432")
        finish()
    # Only the existing ownership cleanup helper may use the interpreter.
    if args and Path(args[0]).resolve() == Path(os.environ["READINESS_HELPER"]):
        os.execv(os.environ["READINESS_PYTHON"],
                 [os.environ["READINESS_PYTHON"], *args])
    unexpected()
elif name == "sleep":
    record("sleep", args=args)
    finish() if args in (["1"], ["2"]) else unexpected()
elif name == "curl":
    unexpected()
elif name == "cargo":
    if args == ["run", "-q", "--locked", "-p", "matric-db",
                "--features", "migrations", "--example", "ci_database_bootstrap"]:
        record("bootstrap", disposable=os.environ.get("FORTEMI_CI_DISPOSABLE_DATABASE"))
        print("READINESS_FIXTURE_STOP_AT_BOOTSTRAP", flush=True)
        finish(73)
    unexpected()
elif name == "docker":
    mode = os.environ["READINESS_MODE"]
    if args[:2] == ["image", "inspect"]:
        finish()
    if args and args[0] == "run":
        label = "fortemi.ci.scope=" + os.environ["CI_RESOURCE_SCOPE"]
        if "--label" not in args or args[args.index("--label") + 1] != label:
            unexpected()
        state["db"] = args[args.index("--name") + 1]
        record("run", label=label)
        print("readiness-owned-db")
        finish()
    if args[:2] == ["container", "inspect"]:
        if args[2] not in (state.get("db"), "readiness-owned-db"):
            finish(1)  # Collector was never created.
        print(json.dumps([{"Id": "readiness-owned-db", "Config": {"Labels": {
            "fortemi.ci.scope": os.environ["CI_RESOURCE_SCOPE"],
            "fortemi.ci.attempt": os.environ["CI_RESOURCE_ATTEMPT"],
        }}}]))
        finish()
    if args == ["rm", "-f", "-v", "readiness-owned-db"]:
        record("cleanup", identity=args[-1])
        finish()
    if len(args) >= 3 and args[:2] == ["exec", state.get("db")]:
        if args[2] == "pg_isready":
            state["probes"] += 1
            probe = state["probes"]
            tcp = "-h" in args and args[args.index("-h") + 1] == "127.0.0.1"
            # First probe: temporary Unix-only init server. Second: restart gap.
            # Third onward: final TCP server, unless permanently unavailable.
            if probe == 1:
                ready = not tcp
            elif probe == 2:
                ready = False
            elif mode == "never_ready":
                ready = False
            elif mode == "confirmation_fails" and probe >= 4:
                ready = not tcp
            else:
                ready = True
            record("probe", tcp=tcp, ready=ready)
            finish(0 if ready else 1)
        if args[2] == "psql":
            record("extension", sql=args[-1])
            if mode == "extension_fails":
                finish(42)
            finish()
    unexpected()
else:
    unexpected()
'''


class PostgresReadinessTests(unittest.TestCase):
    def run_fixture(self, mode):
        with tempfile.TemporaryDirectory(prefix="otel-readiness-unittest-") as temporary:
            owned = Path(temporary)
            commands = owned / "bin"
            commands.mkdir()
            for command in ("docker", "cargo", "sleep", "curl", "python3"):
                path = commands / command
                path.write_text(f"#!{sys.executable}\n" + STUB, encoding="utf-8")
                path.chmod(0o700)
            # Construct an environment without inheriting credentials, real Docker
            # overrides, shell startup hooks, KEEP_OTEL_IT or cargo configuration.
            env = {
                "PATH": str(commands) + os.pathsep + os.defpath,
                "TMPDIR": str(owned),
                "LC_ALL": "C",
                "CI_RESOURCE_SCOPE": "7371319",
                "CI_RESOURCE_ATTEMPT": "1",
                "CI_SAFETY_TOOLS": str(ROOT),
                "CI_DOCKER_REAL": str(commands / "docker"),
                "READINESS_FIXTURE": str(owned),
                "READINESS_MODE": mode,
                "READINESS_PYTHON": sys.executable,
                "READINESS_HELPER": str(ROOT / "scripts/ci/owned-docker.py"),
            }
            result = subprocess.run(
                [shutil.which("bash", path=os.defpath), str(SCRIPT)],
                cwd=ROOT, env=env, text=True, capture_output=True, timeout=20,
            )
            events = [json.loads(line) for line in
                      (owned / "events.jsonl").read_text().splitlines()]
            self.assertFalse([e for e in events if e["kind"] == "unexpected"], events)
            self.assertEqual([e["identity"] for e in events if e["kind"] == "cleanup"],
                             ["readiness-owned-db"], result.stdout + result.stderr)
            # The scoped script retains WORK for quarantine; TemporaryDirectory
            # owns and removes it together with all stubs and their state.
            return result, events

    def test_waits_through_temporary_socket_and_restart_for_final_tcp(self):
        result, events = self.run_fixture("eventually_ready")
        probes = [e for e in events if e["kind"] == "probe"]
        self.assertEqual([e["ready"] for e in probes], [False, False, True, True])
        self.assertTrue(all(e["tcp"] for e in probes))
        self.assertEqual([e["args"] for e in events if e["kind"] == "sleep"],
                         [["1"], ["1"], ["2"]])
        self.assertEqual([e["sql"] for e in events if e["kind"] == "extension"], [
            "CREATE EXTENSION IF NOT EXISTS vector;",
            "CREATE EXTENSION IF NOT EXISTS postgis;",
        ])
        self.assertEqual([e["disposable"] for e in events if e["kind"] == "bootstrap"], ["1"])
        self.assertEqual(result.returncode, 1)  # Intentional fixture stop, not PASS.
        self.assertIn("READINESS_FIXTURE_STOP_AT_BOOTSTRAP", result.stdout)
        self.assertIn("FAIL: schema bootstrap failed", result.stderr)
        self.assertNotIn("[otel-it] PASS", result.stdout)

    def test_permanent_unavailability_exhausts_original_bound_without_bootstrap(self):
        result, events = self.run_fixture("never_ready")
        self.assertEqual(result.returncode, 1)
        self.assertIn("FAIL: postgres not ready", result.stderr)
        self.assertEqual(sum(e["kind"] == "probe" for e in events), 61)
        self.assertEqual([e["args"] for e in events if e["kind"] == "sleep"], [["1"]] * 60)
        self.assertFalse([e for e in events if e["kind"] in ("extension", "bootstrap")])

    def test_confirmation_probe_also_requires_tcp_and_fails_closed(self):
        result, events = self.run_fixture("confirmation_fails")
        self.assertEqual(result.returncode, 1)
        self.assertIn("FAIL: postgres not ready", result.stderr)
        self.assertEqual(sum(e["kind"] == "probe" for e in events), 4)
        self.assertFalse([e for e in events if e["kind"] in ("extension", "bootstrap")])

    def test_extension_failure_is_not_swallowed(self):
        result, events = self.run_fixture("extension_fails")
        self.assertEqual(result.returncode, 42)
        self.assertEqual(sum(e["kind"] == "extension" for e in events), 1)
        self.assertFalse([e for e in events if e["kind"] == "bootstrap"])


if __name__ == "__main__":
    unittest.main()
