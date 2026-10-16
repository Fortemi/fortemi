"""Bundle provisioning of the restricted runtime database login (#1220)."""
from __future__ import annotations

import os
import subprocess
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
ENTRYPOINT = ROOT / "docker" / "bundle-entrypoint.sh"
OWNER_PASSWORD = "install-specific-owner-value"


def url_selection() -> str:
    """Password check plus the DATABASE_URL selection block, before PGDATA work."""
    content = ENTRYPOINT.read_text(encoding="utf-8")
    end_marker = "# Non-root operation (#1173)."
    return content.split(end_marker, 1)[0]


def entrypoint_functions() -> str:
    content = ENTRYPOINT.read_text(encoding="utf-8")
    start_marker = "# --- Testable bundle runtime helpers"
    end_marker = "# --- End testable bundle runtime helpers ---"
    return start_marker + content.split(start_marker, 1)[1].split(end_marker, 1)[0] + end_marker


def startup_wiring() -> str:
    """The block between the helper section and the API start."""
    content = ENTRYPOINT.read_text(encoding="utf-8")
    after_helpers = content.split("# --- End testable bundle runtime helpers ---", 1)[1]
    return after_helpers.split("# --- Start API first", 1)[0]


REPORT = r'''
printf 'provision=%s\n' "${BUNDLE_PROVISION_RUNTIME_ROLE:-false}"
printf 'runtime_user=%s\n' "$(printf '%s' "$DATABASE_URL" | sed -E 's#^postgres://([^:]*):.*#\1#')"
printf 'migration_user=%s\n' "$(printf '%s' "${MIGRATION_DATABASE_URL:-}" | sed -E 's#^postgres://([^:]*):.*#\1#')"
printf 'exported=%s\n' "$(env | grep -c '^MIGRATION_DATABASE_URL=' || true)"
printf 'password_exported=%s\n' "$(env | grep -c '^BUNDLE_RUNTIME_DB_PASSWORD=' || true)"
'''


class RuntimeUrlSelectionTests(unittest.TestCase):
    def select(self, **extra: str) -> tuple[dict[str, str], str]:
        env = {key: value for key, value in os.environ.items()
               if key not in {"DATABASE_URL", "MIGRATION_DATABASE_URL", "FORTEMI_AUTH_MODE",
                              "FORTEMI_MULTI_TENANT"}}
        env["POSTGRES_PASSWORD"] = OWNER_PASSWORD
        env.update(extra)
        result = subprocess.run(["bash", "-c", url_selection() + REPORT], check=False,
                                capture_output=True, text=True, env=env, timeout=30)
        self.assertEqual(result.returncode, 0, result.stderr)
        values = dict(line.split("=", 1) for line in result.stdout.splitlines() if "=" in line)
        return values, result.stdout + result.stderr

    def test_community_mode_keeps_the_owner_login(self) -> None:
        values, output = self.select()
        self.assertEqual(values["provision"], "false")
        self.assertEqual(values["runtime_user"], "matric")
        self.assertEqual(values["migration_user"], "")

    def test_external_oidc_and_hosted_split_owner_and_runtime(self) -> None:
        for extra in ({"FORTEMI_AUTH_MODE": "external-oidc"}, {"FORTEMI_MULTI_TENANT": "true"},
                      {"FORTEMI_MULTI_TENANT": "1"}):
            with self.subTest(extra=extra):
                values, output = self.select(**extra)
                self.assertEqual(values["provision"], "true")
                self.assertEqual(values["runtime_user"], "fortemi_runtime")
                self.assertEqual(values["migration_user"], "matric")
                self.assertEqual(values["exported"], "1")
                self.assertEqual(values["password_exported"], "0")
                self.assertNotIn(OWNER_PASSWORD, output)

    def test_runtime_password_is_fresh_per_start(self) -> None:
        script = url_selection() + 'printf "%s\\n" "$BUNDLE_RUNTIME_DB_PASSWORD"'
        env = dict(os.environ, POSTGRES_PASSWORD=OWNER_PASSWORD, FORTEMI_AUTH_MODE="external-oidc")
        env.pop("DATABASE_URL", None)
        env.pop("MIGRATION_DATABASE_URL", None)
        first, second = (subprocess.run(["bash", "-c", script], check=True, capture_output=True,
                                         text=True, env=env).stdout.splitlines()[-1] for _ in range(2))
        self.assertRegex(first, r"^[0-9a-f]{64}$")
        self.assertNotEqual(first, second)

    def test_mode_values_match_the_server_exactly(self) -> None:
        for extra in ({"FORTEMI_MULTI_TENANT": "true:extra"}, {"FORTEMI_MULTI_TENANT": "yes"},
                      {"FORTEMI_AUTH_MODE": "external-oidc-x"}, {"FORTEMI_MULTI_TENANT": "false"}):
            with self.subTest(extra=extra):
                values, _ = self.select(**extra)
                self.assertEqual(values["provision"], "false")
                self.assertEqual(values["runtime_user"], "matric")

    def test_owner_named_like_the_runtime_login_is_refused(self) -> None:
        env = dict(os.environ, POSTGRES_PASSWORD=OWNER_PASSWORD, FORTEMI_AUTH_MODE="external-oidc",
                   POSTGRES_USER="fortemi_runtime")
        env.pop("DATABASE_URL", None)
        env.pop("MIGRATION_DATABASE_URL", None)
        result = subprocess.run(["bash", "-c", url_selection() + "echo reached"], check=False,
                                capture_output=True, text=True, env=env)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("must not be fortemi_runtime", result.stderr)
        self.assertNotIn("reached", result.stdout)

    def test_operator_supplied_urls_are_untouched(self) -> None:
        for extra in ({"DATABASE_URL": "postgres://app:x@db/app"},
                      {"MIGRATION_DATABASE_URL": "postgres://own:x@db/app"}):
            with self.subTest(extra=sorted(extra)):
                values, _ = self.select(FORTEMI_AUTH_MODE="external-oidc", **extra)
                self.assertEqual(values["provision"], "false")
                if "DATABASE_URL" in extra:
                    self.assertEqual(values["runtime_user"], "app")


class RuntimeRoleSqlTests(unittest.TestCase):
    PASSWORD = "f" * 64

    def run_functions(self, script: str, **extra: str) -> subprocess.CompletedProcess[str]:
        env = dict(os.environ, POSTGRES_USER="matric", POSTGRES_DB="matric",
                   BUNDLE_RUNTIME_DB_USER="fortemi_runtime", BUNDLE_RUNTIME_DB_PASSWORD=self.PASSWORD)
        env.update(extra)
        return subprocess.run(["bash", "-eo", "pipefail", "-c", entrypoint_functions() + "\n" + script],
                              check=False, capture_output=True, text=True, env=env, timeout=30)

    def test_sql_carries_only_a_scram_verifier_and_the_documented_grants(self) -> None:
        result = self.run_functions("bundle_runtime_role_sql")
        self.assertEqual(result.returncode, 0, result.stderr)
        sql = result.stdout
        self.assertNotIn(self.PASSWORD, sql + result.stderr)
        self.assertRegex(sql, r"\\set verifier 'SCRAM-SHA-256\$4096:[A-Za-z0-9+/=]+\$[A-Za-z0-9+/=]+:[A-Za-z0-9+/=]+'")
        for fragment in ("NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE NOREPLICATION",
                         "REVOKE CREATE ON SCHEMA public FROM PUBLIC, %I",
                         "ALTER DEFAULT PRIVILEGES FOR ROLE %I IN SCHEMA %I GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES",
                         "nspname LIKE 'archive\\_%' ESCAPE '\\'",
                         "is not managed by the bundle",
                         "REVOKE UPDATE, DELETE, TRUNCATE ON TABLE public.audit_event",
                         "REVOKE INSERT, UPDATE, DELETE, TRUNCATE, REFERENCES, TRIGGER ON TABLE public.tenant_registry",
                         "can reach privileged roles"):
            self.assertIn(fragment, sql)
        # Grants run only when the role is created; existing managed roles return early.
        self.assertLess(sql.index("RETURN;"), sql.index("GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES"))
        # Read-only exceptions run in their own block on every start, after creation.
        creation_end = sql.index("END $$;")
        self.assertGreater(sql.index("ON TABLE public.tenant_registry"), creation_end)
        self.assertGreater(sql.index("ON TABLE public.audit_event"), creation_end)

    def test_owner_sql_grants_migration_privileges(self) -> None:
        result = self.run_functions("bundle_owner_role_sql")
        self.assertEqual(result.returncode, 0, result.stderr)
        for fragment in ("\\set owner matric", 'ALTER ROLE :"owner" BYPASSRLS;',
                         'GRANT CREATE ON DATABASE :"db" TO :"owner";'):
            self.assertIn(fragment, result.stdout)
        refused = self.run_functions("bundle_owner_role_sql", POSTGRES_USER="a;b")
        self.assertNotEqual(refused.returncode, 0)

    def test_hosted_mode_skips_bundle_mcp_client(self) -> None:
        content = ENTRYPOINT.read_text(encoding="utf-8")
        start = content.index('case "${FORTEMI_MULTI_TENANT:-}" in\n    true|1)\n        # A bundle-registered')
        block = content[start:content.index("esac\n", start) + 5]
        stubs = ("validate_mcp_credentials() { echo validated; MCP_CREDS_VALID=false; }\n"
                 "register_mcp_client() { echo registered; }\n")
        for value, expected in (("true", ""), ("1", ""), ("false", "validated registered"), ("", "validated registered")):
            with self.subTest(value=value):
                result = subprocess.run(["bash", "-c", stubs + block],
                                        env=dict(os.environ, FORTEMI_MULTI_TENANT=value),
                                        check=True, capture_output=True, text=True)
                calls = " ".join(line for line in result.stdout.split() if line in {"validated", "registered"})
                self.assertEqual(calls, expected)

    def test_owner_and_runtime_must_differ(self) -> None:
        result = self.run_functions("bundle_runtime_role_sql", POSTGRES_USER="fortemi_runtime")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("must be different roles", result.stderr)

    def test_startup_provisions_before_the_api_and_fails_closed(self) -> None:
        wiring = startup_wiring()
        api = ('FORTEMI_API_BIN=fake_api\nfake_api() { echo "api:$*"; }\n'
               "prepare_bundle_owner_role() { echo owner-prepared; }\n")
        ok = subprocess.run(["bash", "-c", api + "provision_bundle_runtime_role() { echo provisioned; }\n"
                             "BUNDLE_PROVISION_RUNTIME_ROLE=true BUNDLE_RUNTIME_DB_PASSWORD=x\n"
                             + wiring + '\necho "after:${BUNDLE_RUNTIME_DB_PASSWORD:-unset}"'],
                            check=False, capture_output=True, text=True)
        self.assertEqual(ok.returncode, 0, ok.stderr)
        self.assertIn("provisioned", ok.stdout)
        self.assertLess(ok.stdout.index("owner-prepared"), ok.stdout.index("api:--migrate-only"))
        self.assertLess(ok.stdout.index("api:--migrate-only"), ok.stdout.index("provisioned"))
        self.assertIn("after:unset", ok.stdout)
        migrate_failed = subprocess.run(["bash", "-c", "FORTEMI_API_BIN=false\n"
                                         "prepare_bundle_owner_role() { :; }\n"
                                         "provision_bundle_runtime_role() { echo provisioned; }\n"
                                         "BUNDLE_PROVISION_RUNTIME_ROLE=true\n" + wiring + "\necho reached"],
                                        check=False, capture_output=True, text=True)
        self.assertNotEqual(migrate_failed.returncode, 0)
        self.assertNotIn("provisioned", migrate_failed.stdout)
        failed = subprocess.run(["bash", "-c", api + "provision_bundle_runtime_role() { return 1; }\n"
                                 "BUNDLE_PROVISION_RUNTIME_ROLE=true\n" + wiring + "\necho reached"],
                                check=False, capture_output=True, text=True)
        self.assertNotEqual(failed.returncode, 0)
        self.assertNotIn("reached", failed.stdout)
        skipped = subprocess.run(["bash", "-c", "provision_bundle_runtime_role() { echo provisioned; }\n"
                                  + wiring + "\necho reached"], check=False, capture_output=True, text=True)
        self.assertNotIn("provisioned", skipped.stdout)
        self.assertIn("reached", skipped.stdout)

    def test_unsafe_names_are_refused(self) -> None:
        for name in ("Matric", "a;b", "x y", ""):
            with self.subTest(name=name):
                result = self.run_functions("bundle_runtime_role_sql", POSTGRES_USER=name)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("plain lowercase", result.stderr)

    def test_provisioning_sends_sql_on_stdin_not_arguments(self) -> None:
        result = self.run_functions(r'''
su() { printf 'ARGS:%s\n' "$*" >&2; cat > /dev/null; }
provision_bundle_runtime_role
''')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("ARGS:postgres -c psql -X -qAt -1 -d matric", result.stderr)
        self.assertNotIn(self.PASSWORD, result.stdout + result.stderr)

    def test_provisioning_failure_is_reported(self) -> None:
        result = self.run_functions("su() { cat > /dev/null; return 3; }\n"
                                    "if provision_bundle_runtime_role; then exit 9; fi")
        self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == "__main__":
    unittest.main()
