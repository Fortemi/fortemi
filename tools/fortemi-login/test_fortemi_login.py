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


class FileStoreTests(unittest.TestCase):
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
            helper.save_tokens("https://idp.example.org/realms/example", "fortemi-cli", {}, args)
        with self.assertRaises(helper.LoginError):
            helper.load_tokens("https://idp.example.org/realms/example", "fortemi-cli", args)

    def test_env_opt_in_allows_file_store(self) -> None:
        args = make_args(allow_file_store=False)
        with mock.patch.dict(os.environ, {"FORTEMI_ALLOW_FILE_STORE": "1"}):
            helper.save_tokens("https://idp.example.org/realms/example", "fortemi-cli", {"a": 1}, args)

    def test_file_and_dir_modes_enforced(self) -> None:
        args = make_args()
        helper.save_tokens("https://idp.example.org/realms/example", "fortemi-cli", {"a": 1}, args)
        path = Path(helper.token_file())
        self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o600)
        self.assertEqual(stat.S_IMODE(path.parent.stat().st_mode), 0o700)
        self.assertEqual(helper.read_file_store(), {"a": 1})

    def test_loose_file_mode_refused(self) -> None:
        args = make_args()
        helper.save_tokens("https://idp.example.org/realms/example", "fortemi-cli", {"a": 1}, args)
        os.chmod(helper.token_file(), 0o644)
        with self.assertRaises(helper.LoginError):
            helper.read_file_store()


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
