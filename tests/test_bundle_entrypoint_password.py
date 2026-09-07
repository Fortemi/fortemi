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


def entrypoint_functions() -> str:
    content = ENTRYPOINT.read_text(encoding="utf-8")
    start_marker = "# --- Testable bundle runtime helpers"
    end_marker = "# --- End testable bundle runtime helpers ---"
    return start_marker + content.split(start_marker, 1)[1].split(end_marker, 1)[0] + end_marker


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


class BundleEntrypointReadinessAndMcpTests(unittest.TestCase):
    def run_functions(self, script: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            ["bash", "-eo", "pipefail", "-c", entrypoint_functions() + "\n" + script],
            check=False,
            capture_output=True,
            text=True,
            env=os.environ.copy(),
            timeout=80,
        )

    def test_decimal_and_invalid_wait_settings(self) -> None:
        result = self.run_functions(r'''
VALUE=08
[ "$(bundle_positive_int_or_default VALUE 30 false)" = 8 ]
VALUE=999999999999999999999999999
[ "$(bundle_positive_int_or_default VALUE 30 false)" = 30 ]
VALUE=0
[ "$(bundle_positive_int_or_default VALUE 1 false)" = 1 ]
''')
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_registration_storage_failure_preserves_api_process(self) -> None:
        result = self.run_functions(r'''
MCP_CREDS_FILE=/nonexistent-fortemi-test-directory/credentials
curl() { printf '%s' '{"client_id":"id","client_secret":"secret"}'; }
register_mcp_client
[ "$MCP_CLIENT_ID" = id ]
echo api-can-continue
''')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("api-can-continue", result.stdout)
        self.assertIn("could not be persisted", result.stderr)

    def test_registration_http_failure_preserves_process(self) -> None:
        result = self.run_functions(r'''
curl() { return 22; }
register_mcp_client
echo api-can-continue
''')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("api-can-continue", result.stdout)

    def test_registration_rejects_multiline_credentials(self) -> None:
        result = self.run_functions(r'''
curl() { printf '%s' '{"client_id":"id\nother","client_secret":"secret"}'; }
register_mcp_client
[ -z "${MCP_CLIENT_ID:-}" ]
''')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("auto-registration failed", result.stdout)

    def test_api_wait_survives_beyond_sixty_seconds_and_reaches_mcp_startup(self) -> None:
        script = r'''
API_PID=$$
PORT=3000
API_STARTUP_TIMEOUT_SECONDS=65
API_STARTUP_PROGRESS_SECONDS=20
API_STARTUP_POLL_SECONDS=1
curl_calls=0
curl() {
    curl_calls=$((curl_calls + 1))
    [ "$curl_calls" -ge 62 ]
}
wait_for_api_ready
[ "$API_READY" = "true" ]
[ "$curl_calls" -eq 62 ]
'''

        result = self.run_functions(script)

        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        self.assertIn("API is healthy", result.stdout)
        self.assertNotIn("timed out after 60s", result.stdout)

    def test_api_wait_fails_promptly_when_api_process_dies(self) -> None:
        script = r'''
( exit 0 ) &
API_PID=$!
wait "$API_PID" 2>/dev/null || true
PORT=3000
API_STARTUP_TIMEOUT_SECONDS=60
API_STARTUP_PROGRESS_SECONDS=20
curl() { return 7; }
wait_for_api_ready
'''

        result = self.run_functions(script)

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("API process died during startup", result.stderr + result.stdout)

    def test_main_startup_reaches_mcp_only_after_api_readiness(self) -> None:
        content = ENTRYPOINT.read_text(encoding="utf-8")
        startup_body = content.split("/app/matric-api &", 1)[1]

        wait_index = startup_body.index("wait_for_api_ready")
        mcp_index = startup_body.index("# --- MCP Credential Management ---")

        self.assertLess(wait_index, mcp_index)
        self.assertNotIn("continuing anyway", content)
        self.assertNotIn("for i in {1..60}", content)

    def test_registration_failure_paths_do_not_exit_under_pipefail(self) -> None:
        for response in ("", "not-json", '{"client_id":"only-id"}'):
            with self.subTest(response=response):
                script = f'''
MCP_CREDS_FILE="$(mktemp)"
rm -f "$MCP_CREDS_FILE"
MCP_REGISTER_RESPONSE={response!r}
curl() {{ printf '%s' "$MCP_REGISTER_RESPONSE"; }}
register_mcp_client
'''

                result = self.run_functions(script)

                self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
                self.assertIn("MCP client auto-registration failed", result.stdout)

    def test_registration_accepts_whitespace_formatted_json(self) -> None:
        script = r'''
MCP_CREDS_FILE="$(mktemp)"
rm -f "$MCP_CREDS_FILE"
curl() {
    cat <<'JSON'
{
  "client_id" : "client-whitespace",
  "client_secret" : "secret-whitespace"
}
JSON
}
register_mcp_client
[ "$MCP_CLIENT_ID" = "client-whitespace" ]
[ "$MCP_CLIENT_SECRET" = "secret-whitespace" ]
[ "$(stat -c '%a' "$MCP_CREDS_FILE")" = "600" ]
'''

        result = self.run_functions(script)

        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        self.assertIn("Registered MCP client: client-whitespace", result.stdout)
        self.assertNotIn("secret-whitespace", result.stdout + result.stderr)

    def test_temporary_validation_outage_preserves_existing_credentials(self) -> None:
        script = r'''
MCP_CLIENT_ID=existing-client
MCP_CLIENT_SECRET=existing-secret
MCP_CREDS_VALID=false
curl() { return 7; }
validate_mcp_credentials
[ "$MCP_CREDS_VALID" = "true" ]
'''

        result = self.run_functions(script)

        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        self.assertIn("preserving existing MCP credentials", result.stdout)
        self.assertNotIn("existing-secret", result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
