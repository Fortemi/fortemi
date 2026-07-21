from __future__ import annotations

import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import textwrap
import unittest


ROOT = Path(__file__).resolve().parents[1]


FAKE_CURL = r"""#!/usr/bin/env python3
import io
import json
import os
from pathlib import Path
import sys
import tarfile

args = sys.argv[1:]
url = next((arg for arg in args if arg.startswith("http")), "")
state = Path(os.environ["FAKE_CURL_STATE"])
output = Path(args[args.index("-o") + 1]) if "-o" in args else None

if url.endswith("/health") or url.endswith("/api/v1/archives"):
    raise SystemExit(0)

if url.endswith("/api/v1/notes"):
    if "--data-binary" not in args or args[args.index("--data-binary") + 1] != "@-":
        print("note payload was not streamed", file=sys.stderr)
        raise SystemExit(2)
    payload = json.load(sys.stdin)
    marker = os.environ.get("FAKE_CURL_FAIL_MARKER")
    if marker and marker in payload["content"]:
        if output:
            output.write_text('{"error":"injected API failure"}', encoding="utf-8")
        raise SystemExit(22)
    with state.open("a", encoding="utf-8") as handle:
        handle.write(json.dumps(payload) + "\n")
    if output:
        output.write_text("{}", encoding="utf-8")
    raise SystemExit(0)

if url.endswith("/api/v1/backup/knowledge-shard"):
    if os.environ.get("FAKE_CURL_EMPTY_EXPORT"):
        output.write_bytes(b"")
        raise SystemExit(0)
    payloads = [json.loads(line) for line in state.read_text(encoding="utf-8").splitlines()]
    notes = "".join(
        json.dumps({
            "title": payload["title"],
            "original_content": payload["content"],
            "revised_content": payload["content"],
        }) + "\n"
        for payload in payloads
    ).encode()
    manifest = json.dumps({"counts": {"notes": len(payloads), "links": 0, "tags": 0}}).encode()
    with tarfile.open(output, "w:gz") as archive:
        for name, data in (("notes.jsonl", notes), ("manifest.json", manifest)):
            info = tarfile.TarInfo(name)
            info.size = len(data)
            archive.addfile(info, io.BytesIO(data))
    raise SystemExit(0)

print(f"unexpected curl request: {args}", file=sys.stderr)
raise SystemExit(2)
"""


class RebuildDocsShardTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp_dir = tempfile.TemporaryDirectory()
        self.repo = Path(self.temp_dir.name)
        (self.repo / "scripts" / "ci").mkdir(parents=True)
        (self.repo / "tests").mkdir()
        (self.repo / "docs").mkdir()
        (self.repo / ".aiwg").mkdir()
        (self.repo / "docker" / "seed-data").mkdir(parents=True)
        shutil.copy2(ROOT / "scripts" / "rebuild-docs-shard.sh", self.repo / "scripts")
        shutil.copy2(
            ROOT / "scripts" / "ci" / "verify-docs-shard-coverage.py",
            self.repo / "scripts" / "ci",
        )
        subprocess.run(["git", "init", "-q"], cwd=self.repo, check=True)

        self.bin_dir = self.repo / "bin"
        self.bin_dir.mkdir()
        curl = self.bin_dir / "curl"
        curl.write_text(FAKE_CURL, encoding="utf-8")
        curl.chmod(0o755)
        sleep = self.bin_dir / "sleep"
        sleep.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
        sleep.chmod(0o755)

        self.state = self.repo / "curl-state.jsonl"
        self.state.write_text("", encoding="utf-8")
        self.env = os.environ.copy()
        self.env["PATH"] = f"{self.bin_dir}:{self.env['PATH']}"
        self.env["FAKE_CURL_STATE"] = str(self.state)
        (self.repo / ".aiwg" / "plan.md").write_text("# Plan\nbody\n", encoding="utf-8")
        (self.repo / "CHANGELOG.md").write_text("# Changes\n", encoding="utf-8")
        (self.repo / "README.md").write_text("# Readme\n", encoding="utf-8")

    def tearDown(self) -> None:
        self.temp_dir.cleanup()

    def run_rebuild(self) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            ["bash", "scripts/rebuild-docs-shard.sh", "http://fortemi.test"],
            cwd=self.repo,
            env=self.env,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )

    def test_streams_note_larger_than_128_kib_and_verifies_coverage(self) -> None:
        content = "# Large source\n" + ("x" * (128 * 1024 + 4096))
        (self.repo / "docs" / "large.md").write_text(content, encoding="utf-8")

        result = self.run_rebuild()

        self.assertEqual(result.returncode, 0, result.stdout)
        payloads = [json.loads(line) for line in self.state.read_text().splitlines()]
        self.assertIn(content, [payload["content"] for payload in payloads])
        self.assertIn("Docs shard source coverage passed: 4/4 sources.", result.stdout)

    def test_first_unexpected_import_failure_blocks_export(self) -> None:
        (self.repo / "docs" / "fail.md").write_text(
            "# Failure\ninjected-marker\n", encoding="utf-8"
        )
        self.env["FAKE_CURL_FAIL_MARKER"] = "injected-marker"

        result = self.run_rebuild()

        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn("FAILED: docs/fail.md", result.stdout)
        self.assertIn("injected API failure", result.stdout)
        self.assertIn("Refusing to export a shard missing tracked source", result.stdout)
        self.assertFalse((self.repo / "docker" / "seed-data" / "fortemi-docs.shard").exists())

    def test_empty_export_does_not_replace_existing_shard(self) -> None:
        (self.repo / "docs" / "source.md").write_text("# Source\n", encoding="utf-8")
        shard = self.repo / "docker" / "seed-data" / "fortemi-docs.shard"
        shard.write_bytes(b"known-good-shard")
        self.env["FAKE_CURL_EMPTY_EXPORT"] = "1"

        result = self.run_rebuild()

        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn("Shard export returned an empty body", result.stdout)
        self.assertEqual(shard.read_bytes(), b"known-good-shard")


if __name__ == "__main__":
    unittest.main()
