from __future__ import annotations

import os
import subprocess
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
ENTRYPOINT = ROOT / "docker" / "bundle-entrypoint.sh"


def credential_prelude() -> str:
    content = ENTRYPOINT.read_text(encoding="utf-8")
    marker = 'if [ -z "${DATABASE_URL:-}" ]; then'
    return content.split(marker, 1)[0]


class BundleEntrypointPasswordTests(unittest.TestCase):
    def run_prelude(self, password: str | None) -> subprocess.CompletedProcess[str]:
        env = os.environ.copy()
        env.pop("POSTGRES_PASSWORD", None)
        if password is not None:
            env["POSTGRES_PASSWORD"] = password
        return subprocess.run(
            ["bash", "-c", credential_prelude()],
            check=False,
            capture_output=True,
            text=True,
            env=env,
        )

    def test_missing_password_fails_before_database_startup(self) -> None:
        result = self.run_prelude(None)

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("POSTGRES_PASSWORD is required", result.stderr)
        self.assertNotIn("Starting PostgreSQL", result.stdout)

    def test_known_reusable_passwords_fail_closed(self) -> None:
        for password in ("matric", "fortemi-local-dev", "changeme"):
            with self.subTest(password=password):
                result = self.run_prelude(password)

                self.assertNotEqual(result.returncode, 0)
                self.assertIn("known reusable", result.stderr)
                self.assertNotIn(password, result.stdout + result.stderr)

    def test_install_specific_password_passes_without_logging_value(self) -> None:
        password = "install-specific-entrypoint-value"

        result = self.run_prelude(password)

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn(password, result.stdout + result.stderr)


class BundleEntrypointMcpSecretLoggingTests(unittest.TestCase):
    def test_mcp_secret_values_are_never_sent_to_output_commands(self) -> None:
        content = ENTRYPOINT.read_text(encoding="utf-8")
        sensitive_variables = (
            "$MCP_CLIENT_SECRET",
            "${MCP_CLIENT_SECRET",
            "$NEW_CLIENT_SECRET",
            "${NEW_CLIENT_SECRET",
        )
        output_commands = []

        for line_number, line in enumerate(content.splitlines(), start=1):
            command = line.strip()
            if not command.startswith(("echo ", "printf ")):
                continue
            if any(variable in command for variable in sensitive_variables):
                output_commands.append(f"{line_number}: {command}")

        self.assertEqual([], output_commands)

    def test_registration_response_is_not_logged(self) -> None:
        content = ENTRYPOINT.read_text(encoding="utf-8")
        response_logs = []

        for line_number, line in enumerate(content.splitlines(), start=1):
            command = line.strip()
            if not command.startswith(("echo ", "printf ")):
                continue
            if "REGISTER_RESPONSE" in command and "|" not in command:
                response_logs.append(f"{line_number}: {command}")

        self.assertEqual([], response_logs)
        self.assertIn("Secret: (masked", content)


if __name__ == "__main__":
    unittest.main()
