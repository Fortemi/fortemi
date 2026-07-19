from __future__ import annotations

import os
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "ci" / "forbid-insecure-auth-defaults.sh"


class InsecureAuthDefaultGuardTests(unittest.TestCase):
    def make_fixture(self, root: Path) -> None:
        for directory in (
            root / "installer",
            root / "scripts",
            root / "docs" / "content",
            root / "crates",
        ):
            directory.mkdir(parents=True, exist_ok=True)
        (root / "docker-compose.bundle.yml").write_text(
            "services: {}\n",
            encoding="utf-8",
        )
        (root / "docker-compose.workstation.yml").write_text(
            "REQUIRE_AUTH=${REQUIRE_AUTH:-false}\n",
            encoding="utf-8",
        )
        (root / "installer" / "configure.sh").write_text(
            "REQUIRE_AUTH=${REQUIRE_AUTH:-true}\n",
            encoding="utf-8",
        )
        (root / "scripts" / "safe.sh").write_text("true\n", encoding="utf-8")
        (root / "README.md").write_text("Auth defaults secure.\n", encoding="utf-8")
        (root / "docs" / "content" / "auth.md").write_text(
            "Authentication is required by default.\n",
            encoding="utf-8",
        )
        (root / "CLAUDE.md").write_text("Auth policy.\n", encoding="utf-8")
        (root / "crates" / "handler.rs").write_text(
            "fn handler() {}\n",
            encoding="utf-8",
        )

    def tool_path(self, root: Path, *, grep: bool = True) -> Path:
        tools = root / "tools"
        tools.mkdir()
        (tools / "bash").symlink_to("/usr/bin/bash")
        if grep:
            (tools / "grep").symlink_to("/usr/bin/grep")
        return tools

    def run_guard(self, root: Path, tools: Path) -> subprocess.CompletedProcess[str]:
        env = os.environ.copy()
        env["PATH"] = str(tools)
        return subprocess.run(
            [str(SCRIPT), str(root)],
            check=False,
            capture_output=True,
            text=True,
            env=env,
        )

    def test_grep_fallback_distinguishes_no_match_from_excluded_match(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.make_fixture(root)
            tools = self.tool_path(root)

            result = self.run_guard(root, tools)

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("search_tool=grep", result.stdout)

    def test_prohibited_match_fails(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.make_fixture(root)
            tools = self.tool_path(root)
            (root / "docker-compose.bundle.yml").write_text(
                "REQUIRE_AUTH=${REQUIRE_AUTH:-false}\n",
                encoding="utf-8",
            )

            result = self.run_guard(root, tools)

            self.assertEqual(result.returncode, 1)
            self.assertIn("must not default REQUIRE_AUTH to false", result.stderr)

    def test_grep_fallback_detects_optional_auth_handler_branch(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.make_fixture(root)
            tools = self.tool_path(root)
            (root / "crates" / "handler.rs").write_text(
                "if auth.is_some() { allow(); }\n",
                encoding="utf-8",
            )

            result = self.run_guard(root, tools)

            self.assertEqual(result.returncode, 1)
            self.assertIn(
                "handlers must not branch on optional auth presence",
                result.stderr,
            )

    def test_missing_search_tools_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.make_fixture(root)
            tools = self.tool_path(root, grep=False)

            result = self.run_guard(root, tools)

            self.assertEqual(result.returncode, 2)
            self.assertIn("requires rg or grep", result.stderr)

    def test_selected_search_tool_error_is_not_treated_as_no_match(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.make_fixture(root)
            tools = self.tool_path(root)
            fake_rg = tools / "rg"
            fake_rg.write_text("#!/bin/sh\nexit 2\n", encoding="utf-8")
            fake_rg.chmod(0o755)

            result = self.run_guard(root, tools)

            self.assertEqual(result.returncode, 2)
            self.assertIn("rg exited 2", result.stderr)


if __name__ == "__main__":
    unittest.main()
