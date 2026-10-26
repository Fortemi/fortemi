"""Unit tests for the fortemi-login helper (run: python3 -m unittest)."""

from __future__ import annotations

import base64
import hashlib
import io
import json
import os
import stat
import sys
import tempfile
import unittest
from contextlib import redirect_stdout
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))

import fortemi_login as helper


def make_args(**overrides: object) -> SimpleNamespace:
    base = {
        "fortemi_url": "",
        "issuer": "https://idp.example.org/realms/example",
        "client_id": "fortemi-cli",
        "allow_file_store": True,
    }
    base.update(overrides)
    return SimpleNamespace(**base)


class PkceTests(unittest.TestCase):
    def test_rfc7636_vector(self) -> None:
        verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"
        digest = hashlib.sha256(verifier.encode("ascii")).digest()
        self.assertEqual(
            base64.urlsafe_b64encode(digest).rstrip(b"=").decode("ascii"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM",
        )

    def test_generated_pair_links_challenge_to_verifier(self) -> None:
        verifier, challenge = helper.pkce_pair()
        self.assertGreaterEqual(len(verifier), 43)
        self.assertLessEqual(len(verifier), 128)
        expected = base64.urlsafe_b64encode(
            hashlib.sha256(verifier.encode("ascii")).digest()
        ).rstrip(b"=").decode("ascii")
        self.assertEqual(challenge, expected)


class ParserTests(unittest.TestCase):
    def test_common_options_work_before_and_after_each_command(self) -> None:
        options = [
            "--fortemi-url", "https://fortemi.example.invalid",
            "--issuer", "https://idp.example.invalid/realms/test",
            "--client-id", "fortemi-admin-cli", "--allow-file-store",
        ]
        for command in ("login", "token", "whoami", "logout", "mcp-config"):
            tail = ["--client", "claude-code"] if command == "mcp-config" else []
            for argv in (options + [command] + tail, [command] + options + tail):
                with self.subTest(argv=argv):
                    parsed = helper.build_parser().parse_args(argv)
                    self.assertEqual(parsed.fortemi_url, "https://fortemi.example.invalid")
                    self.assertEqual(parsed.issuer, "https://idp.example.invalid/realms/test")
                    self.assertEqual(parsed.client_id, "fortemi-admin-cli")
                    self.assertTrue(parsed.allow_file_store)

    def test_option_after_command_wins_and_unset_defaults_remain(self) -> None:
        parsed = helper.build_parser().parse_args([
            "--client-id", "fortemi-cli", "login", "--client-id", "fortemi-admin-cli",
        ])
        self.assertEqual(parsed.client_id, "fortemi-admin-cli")
        self.assertEqual(parsed.issuer, "")
        self.assertFalse(parsed.allow_file_store)


class FileStoreTests(unittest.TestCase):
    ISSUER_A = "https://idp.example.invalid/realms/a"
    ISSUER_B = "https://idp.example.invalid/realms/b"

    @staticmethod
    def session(issuer: str, client: str, token: str) -> dict:
        return {
            "issuer": issuer, "client_id": client, "access_token": token,
            "refresh_token": "dummy-refresh", "expires_at": 9_999_999_999,
        }

    def setUp(self) -> None:
        self.home = tempfile.TemporaryDirectory()
        self.addCleanup(self.home.cleanup)
        self.env = mock.patch.dict(
            os.environ,
            {"HOME": self.home.name, "XDG_CONFIG_HOME": os.path.join(self.home.name, ".config")},
        )
        self.env.start()
        self.addCleanup(self.env.stop)
        self.keyring = mock.patch.object(helper, "_keyring", None)
        self.keyring.start()
        self.addCleanup(self.keyring.stop)

    def test_refuses_without_flag(self) -> None:
        args = make_args(allow_file_store=False)
        with self.assertRaises(helper.LoginError):
            helper.save_tokens(self.ISSUER_A, "fortemi-cli",
                               self.session(self.ISSUER_A, "fortemi-cli", "dummy"), args)
        with self.assertRaises(helper.LoginError):
            helper.load_tokens(self.ISSUER_A, "fortemi-cli", args)

    def test_env_opt_in_allows_file_store(self) -> None:
        args = make_args(allow_file_store=False)
        with mock.patch.dict(os.environ, {"FORTEMI_ALLOW_FILE_STORE": "1"}):
            helper.save_tokens(self.ISSUER_A, "fortemi-cli",
                               self.session(self.ISSUER_A, "fortemi-cli", "dummy"), args)
        self.assertEqual(helper.load_file_tokens(self.ISSUER_A, "fortemi-cli")["access_token"], "dummy")

    def test_file_and_dir_modes_enforced(self) -> None:
        args = make_args()
        session = self.session(self.ISSUER_A, "fortemi-cli", "dummy")
        helper.save_tokens(self.ISSUER_A, "fortemi-cli", session, args)
        path = Path(helper.token_file())
        self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o600)
        self.assertEqual(stat.S_IMODE(path.parent.stat().st_mode), 0o700)
        self.assertEqual(helper.load_tokens(self.ISSUER_A, "fortemi-cli", args), session)

    def test_loose_file_mode_refused(self) -> None:
        args = make_args()
        helper.save_tokens(self.ISSUER_A, "fortemi-cli",
                           self.session(self.ISSUER_A, "fortemi-cli", "dummy"), args)
        os.chmod(helper.token_file(), 0o644)
        with self.assertRaises(helper.LoginError):
            helper.read_file_store()

    def test_file_sessions_isolate_admin_default_and_issuers(self) -> None:
        args = make_args()
        for issuer, client, token in (
            (self.ISSUER_A, "fortemi-admin-cli", "dummy-admin-a"),
            (self.ISSUER_A, "fortemi-cli", "dummy-default-a"),
            (self.ISSUER_B, "fortemi-admin-cli", "dummy-admin-b"),
        ):
            helper.save_tokens(issuer, client, self.session(issuer, client, token), args)
        self.assertEqual(helper.fresh_access_token({}, self.ISSUER_A, "fortemi-cli", args),
                         "dummy-default-a")
        self.assertEqual(helper.load_tokens(self.ISSUER_A, "fortemi-admin-cli", args)["access_token"],
                         "dummy-admin-a")
        self.assertEqual(helper.load_tokens(self.ISSUER_B, "fortemi-admin-cli", args)["access_token"],
                         "dummy-admin-b")
        self.assertEqual(helper.load_tokens(self.ISSUER_B, "fortemi-cli", args), {})
        helper.delete_tokens(self.ISSUER_A, "fortemi-admin-cli", args)
        self.assertEqual(helper.load_tokens(self.ISSUER_A, "fortemi-admin-cli", args), {})
        self.assertEqual(helper.load_tokens(self.ISSUER_A, "fortemi-cli", args)["access_token"],
                         "dummy-default-a")
        self.assertEqual(helper.load_tokens(self.ISSUER_B, "fortemi-admin-cli", args)["access_token"],
                         "dummy-admin-b")

    def test_matching_legacy_session_is_migrated_without_cross_client_use(self) -> None:
        args = make_args()
        legacy = self.session(self.ISSUER_A, "fortemi-admin-cli", "dummy-admin")
        helper.write_file_store(legacy)
        self.assertEqual(helper.load_tokens(self.ISSUER_A, "fortemi-cli", args), {})
        helper.save_tokens(self.ISSUER_A, "fortemi-cli",
                           self.session(self.ISSUER_A, "fortemi-cli", "dummy-default"), args)
        self.assertEqual(helper.read_file_store()["version"], helper.FILE_STORE_VERSION)
        self.assertEqual(helper.load_tokens(self.ISSUER_A, "fortemi-admin-cli", args)["access_token"],
                         "dummy-admin")
        self.assertEqual(helper.load_tokens(self.ISSUER_A, "fortemi-cli", args)["access_token"],
                         "dummy-default")

    def test_unbound_legacy_and_mismatched_session_fail_closed(self) -> None:
        args = make_args()
        helper.write_file_store({"access_token": "dummy-secret"})
        with self.assertRaises(helper.LoginError) as error:
            helper.load_tokens(self.ISSUER_A, "fortemi-cli", args)
        self.assertNotIn("dummy-secret", str(error.exception))
        helper.write_file_store({"version": helper.FILE_STORE_VERSION, "sessions": {
            self.ISSUER_A: {"fortemi-cli": self.session(
                self.ISSUER_A, "fortemi-admin-cli", "dummy-secret")}}})
        with self.assertRaises(helper.LoginError) as error:
            helper.load_tokens(self.ISSUER_A, "fortemi-cli", args)
        self.assertNotIn("dummy-secret", str(error.exception))

    def test_unavailable_keyring_uses_file_only_with_explicit_opt_in(self) -> None:
        class UnavailableKeyring:
            def get_password(self, *args: object) -> None:
                raise helper._NoKeyringError()

            def set_password(self, *args: object) -> None:
                raise helper._NoKeyringError()

            def delete_password(self, *args: object) -> None:
                raise helper._NoKeyringError()

        session = self.session(self.ISSUER_A, "fortemi-cli", "dummy-secret")
        with mock.patch.object(helper, "_keyring", UnavailableKeyring()):
            no_file = make_args(allow_file_store=False)
            for action in (
                lambda: helper.load_tokens(self.ISSUER_A, "fortemi-cli", no_file),
                lambda: helper.save_tokens(self.ISSUER_A, "fortemi-cli", session, no_file),
                lambda: helper.delete_tokens(self.ISSUER_A, "fortemi-cli", no_file),
            ):
                with self.assertRaises(helper.LoginError) as error:
                    action()
                self.assertIn("plaintext", str(error.exception))
                self.assertNotIn("dummy-secret", str(error.exception))
            self.assertFalse(Path(helper.token_file()).exists())
            with_file = make_args(allow_file_store=True)
            helper.save_tokens(self.ISSUER_A, "fortemi-cli", session, with_file)
            self.assertEqual(helper.load_tokens(self.ISSUER_A, "fortemi-cli", with_file), session)
            helper.delete_tokens(self.ISSUER_A, "fortemi-cli", with_file)
            self.assertEqual(helper.load_tokens(self.ISSUER_A, "fortemi-cli", with_file), {})

    def test_unexpected_keyring_error_does_not_downgrade_or_echo_token(self) -> None:
        backend = mock.Mock()
        backend.get_password.side_effect = RuntimeError("dummy-secret")
        backend.set_password.side_effect = RuntimeError("dummy-secret")
        backend.delete_password.side_effect = RuntimeError("dummy-secret")
        session = self.session(self.ISSUER_A, "fortemi-cli", "dummy-secret")
        with mock.patch.object(helper, "_keyring", backend):
            for action in (
                lambda: helper.load_tokens(self.ISSUER_A, "fortemi-cli", make_args()),
                lambda: helper.save_tokens(self.ISSUER_A, "fortemi-cli", session, make_args()),
                lambda: helper.delete_tokens(self.ISSUER_A, "fortemi-cli", make_args()),
            ):
                with self.assertRaises(helper.LoginError) as error:
                    action()
                self.assertNotIn("dummy-secret", str(error.exception))
        self.assertFalse(Path(helper.token_file()).exists())


class MemoryKeyring:
    """A process-local fake; never imports or contacts an OS keyring."""

    def __init__(self) -> None:
        self.values: dict[tuple[str, str], str] = {}

    def get_password(self, service: str, account: str) -> str | None:
        return self.values.get((service, account))

    def set_password(self, service: str, account: str, value: str) -> None:
        self.values[(service, account)] = value

    def delete_password(self, service: str, account: str) -> None:
        del self.values[(service, account)]


class KeyringContextTests(unittest.TestCase):
    ISSUER_A = "https://idp.example.invalid/realms/a"
    ISSUER_B = "https://idp.example.invalid/realms/b"

    def setUp(self) -> None:
        self.home = tempfile.TemporaryDirectory()
        self.addCleanup(self.home.cleanup)
        self.env = mock.patch.dict(os.environ, {
            "HOME": self.home.name,
            "XDG_CONFIG_HOME": os.path.join(self.home.name, ".config"),
        })
        self.env.start()
        self.addCleanup(self.env.stop)
        self.keyring = MemoryKeyring()
        patch = mock.patch.object(helper, "_keyring", self.keyring)
        patch.start()
        self.addCleanup(patch.stop)

    @staticmethod
    def session(issuer: str, client: str, token: str) -> dict:
        return FileStoreTests.session(issuer, client, token)

    def test_same_host_realms_and_admin_default_have_distinct_accounts(self) -> None:
        args = make_args()
        self.assertEqual(helper.issuer(make_args(issuer=self.ISSUER_A + "/")), self.ISSUER_A)
        self.assertNotEqual(helper.account_name(self.ISSUER_A, "fortemi-cli"),
                            helper.account_name(self.ISSUER_B, "fortemi-cli"))
        for issuer, client, token in (
            (self.ISSUER_A, "fortemi-cli", "dummy-default-a"),
            (self.ISSUER_B, "fortemi-cli", "dummy-default-b"),
            (self.ISSUER_A, "fortemi-admin-cli", "dummy-admin-a"),
            (self.ISSUER_B, "fortemi-admin-cli", "dummy-admin-b"),
        ):
            helper.save_tokens(issuer, client, self.session(issuer, client, token), args)
        self.assertEqual(len(self.keyring.values), 4)
        self.assertEqual(helper.fresh_access_token({}, self.ISSUER_A, "fortemi-cli", args),
                         "dummy-default-a")
        self.assertEqual(helper.fresh_access_token({}, self.ISSUER_B, "fortemi-cli", args),
                         "dummy-default-b")
        self.assertEqual(helper.load_tokens(self.ISSUER_A, "fortemi-admin-cli", args)["access_token"],
                         "dummy-admin-a")
        self.assertEqual(helper.load_tokens(self.ISSUER_B, "fortemi-admin-cli", args)["access_token"],
                         "dummy-admin-b")

        metadata = {"token_endpoint": "https://idp.example.invalid/token"}
        with mock.patch.object(helper, "http_json", return_value={
            "access_token": "dummy-refreshed-a", "refresh_token": "dummy-rotated",
            "expires_in": 300,
        }):
            helper.refresh_session(metadata, self.ISSUER_A, "fortemi-cli",
                                   helper.load_tokens(self.ISSUER_A, "fortemi-cli", args), args)
        self.assertEqual(helper.load_tokens(self.ISSUER_A, "fortemi-cli", args)["access_token"],
                         "dummy-refreshed-a")
        self.assertEqual(helper.load_tokens(self.ISSUER_B, "fortemi-cli", args)["access_token"],
                         "dummy-default-b")

        logout_args = make_args(issuer=self.ISSUER_A, client_id="fortemi-admin-cli")
        with (
            mock.patch.object(helper, "discover", return_value={
                "revocation_endpoint": "https://idp.example.invalid/revoke"}),
            mock.patch.object(helper, "http_json", return_value={}),
        ):
            self.assertEqual(helper.cmd_logout(logout_args), 0)
        self.assertEqual(helper.load_tokens(self.ISSUER_A, "fortemi-admin-cli", args), {})
        self.assertEqual(helper.load_tokens(self.ISSUER_B, "fortemi-admin-cli", args)["access_token"],
                         "dummy-admin-b")
        self.assertEqual(helper.load_tokens(self.ISSUER_A, "fortemi-cli", args)["access_token"],
                         "dummy-refreshed-a")

    def test_legacy_account_migrates_only_matching_identity(self) -> None:
        client = "fortemi-cli"
        old_account = helper.legacy_account_name(self.ISSUER_A, client)
        new_account = helper.account_name(self.ISSUER_A, client)
        legacy = self.session(self.ISSUER_A, client, "dummy-legacy-a")
        self.keyring.set_password(helper.KEYRING_SERVICE, old_account, json.dumps(legacy))
        args = make_args()
        self.assertEqual(helper.load_tokens(self.ISSUER_B, client, args), {})
        helper.delete_tokens(self.ISSUER_B, client, args)
        self.assertIn((helper.KEYRING_SERVICE, old_account), self.keyring.values)
        self.assertNotIn((helper.KEYRING_SERVICE, helper.account_name(self.ISSUER_B, client)),
                         self.keyring.values)
        self.assertEqual(helper.load_tokens(self.ISSUER_A, client, args), legacy)
        self.assertIn((helper.KEYRING_SERVICE, new_account), self.keyring.values)
        self.assertIn((helper.KEYRING_SERVICE, old_account), self.keyring.values)
        helper.save_tokens(self.ISSUER_B, client,
                           self.session(self.ISSUER_B, client, "dummy-new-b"), args)
        helper.delete_tokens(self.ISSUER_A, client, args)
        self.assertNotIn((helper.KEYRING_SERVICE, old_account), self.keyring.values)
        self.assertNotIn((helper.KEYRING_SERVICE, new_account), self.keyring.values)
        self.assertEqual(helper.load_tokens(self.ISSUER_B, client, args)["access_token"],
                         "dummy-new-b")

    def test_missing_default_context_cannot_reuse_admin_session(self) -> None:
        admin = self.session(self.ISSUER_A, "fortemi-admin-cli", "dummy-admin")
        helper.save_tokens(self.ISSUER_A, "fortemi-admin-cli", admin, make_args())
        self.assertEqual(helper.load_tokens(self.ISSUER_A, "fortemi-cli", make_args()), {})
        self.assertEqual(helper.load_tokens(self.ISSUER_B, "fortemi-admin-cli", make_args()), {})

    def test_unbound_legacy_and_tampered_exact_entry_fail_closed(self) -> None:
        client = "fortemi-cli"
        old_account = helper.legacy_account_name(self.ISSUER_A, client)
        self.keyring.set_password(helper.KEYRING_SERVICE, old_account,
                                  json.dumps({"access_token": "dummy-secret"}))
        with self.assertRaises(helper.LoginError) as error:
            helper.load_tokens(self.ISSUER_A, client, make_args())
        self.assertNotIn("dummy-secret", str(error.exception))
        self.keyring.delete_password(helper.KEYRING_SERVICE, old_account)
        exact_account = helper.account_name(self.ISSUER_A, client)
        self.keyring.set_password(helper.KEYRING_SERVICE, exact_account, json.dumps(
            self.session(self.ISSUER_B, client, "dummy-secret")))
        with self.assertRaises(helper.LoginError) as error:
            helper.load_tokens(self.ISSUER_A, client, make_args())
        self.assertNotIn("dummy-secret", str(error.exception))

    def test_healthy_keyring_miss_never_reads_existing_file_fallback(self) -> None:
        file_session = self.session(self.ISSUER_A, "fortemi-cli", "dummy-file-admin")
        helper.write_file_store(file_session)
        args = make_args(allow_file_store=True)
        self.assertEqual(helper.load_tokens(self.ISSUER_A, "fortemi-cli", args), {})
        helper.delete_tokens(self.ISSUER_A, "fortemi-cli", args)
        self.assertEqual(helper.read_file_store(), file_session)
        self.assertEqual(len(self.keyring.values), 0)


class TokenRefreshTests(unittest.TestCase):
    def test_expired_session_refreshes(self) -> None:
        args = make_args()
        session = {"access_token": "old", "refresh_token": "refresh", "expires_at": 1}
        metadata = {"token_endpoint": "https://idp.example.org/realms/example/protocol/openid-connect/token"}
        fresh = {"access_token": "new", "refresh_token": "rotated", "expires_in": 300}
        with (
            mock.patch.object(helper, "load_tokens", return_value=session),
            mock.patch.object(helper, "http_json", return_value=fresh) as http,
            mock.patch.object(helper, "save_tokens") as save,
        ):
            token = helper.fresh_access_token(metadata, args.issuer, "fortemi-cli", args)
        self.assertEqual(token, "new")
        body = http.call_args[0][1]
        self.assertEqual(body["grant_type"], "refresh_token")
        self.assertEqual(body["refresh_token"], "refresh")
        saved = save.call_args[0][3 - 1]
        self.assertEqual(saved["access_token"], "new")

    def test_valid_session_needs_no_network(self) -> None:
        args = make_args()
        session = {"access_token": "live", "expires_at": 9_999_999_999}
        with (
            mock.patch.object(helper, "load_tokens", return_value=session),
            mock.patch.object(helper, "http_json") as http,
        ):
            self.assertEqual(helper.fresh_access_token({}, args.issuer, "c", args), "live")
        http.assert_not_called()


class LogoutTests(unittest.TestCase):
    def test_logout_revokes_and_deletes(self) -> None:
        args = make_args()
        session = {"access_token": "access", "refresh_token": "refresh"}
        metadata = {"revocation_endpoint": "https://idp.example.org/revoke"}
        calls: list[dict] = []

        def fake_http(url: str, data: dict | None = None, **kwargs: object) -> dict:
            calls.append({"url": url, "data": data or {}})
            return {}

        with (
            mock.patch.object(helper, "load_tokens", return_value=session),
            mock.patch.object(helper, "discover", return_value=metadata),
            mock.patch.object(helper, "http_json", side_effect=fake_http),
            mock.patch.object(helper, "delete_tokens") as delete,
        ):
            self.assertEqual(helper.cmd_logout(args), 0)
        revoked = {call["data"]["token"] for call in calls}
        self.assertEqual(revoked, {"access", "refresh"})
        delete.assert_called_once()


class McpConfigTests(unittest.TestCase):
    def test_all_clients_emit_mcp_url(self) -> None:
        for client in helper.MCP_CLIENTS:
            config = helper.mcp_config(client, "https://fortemi.example.org")
            self.assertIn("https://fortemi.example.org/mcp", json.dumps(config))

    def test_unknown_client_rejected(self) -> None:
        with self.assertRaises(helper.LoginError):
            helper.mcp_config("chatgpt", "https://fortemi.example.org")

    def test_command_prints_json(self) -> None:
        args = make_args(client="claude-code")
        buffer = io.StringIO()
        with redirect_stdout(buffer):
            self.assertEqual(helper.cmd_mcp_config(args), 0)
        parsed = json.loads(buffer.getvalue())
        self.assertIn("https://fortemi.example.org/mcp", json.dumps(parsed))


if __name__ == "__main__":
    unittest.main()
