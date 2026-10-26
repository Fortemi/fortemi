#!/usr/bin/env python3
"""fortemi-login: sign in once, use the API, MCP and agents as yourself.

Interactive loopback PKCE is the default; ``--device`` serves headless hosts.
Refresh tokens live in the OS keychain via ``keyring`` when available. A file
fallback exists for hosts without a keychain but requires ``--allow-file-store``
(or ``FORTEMI_ALLOW_FILE_STORE=1``) and enforces mode 0600 inside a 0700 dir.
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
import secrets
import stat
import sys
import time
import urllib.parse
import urllib.request
import webbrowser
from http.server import BaseHTTPRequestHandler, HTTPServer

try:
    import keyring as _keyring
    from keyring.errors import NoKeyringError as _NoKeyringError
except ImportError:
    _keyring = None

    class _NoKeyringError(Exception):
        """Placeholder used only when the optional keyring package is absent."""

KEYRING_SERVICE = "fortemi-login"
DEFAULT_CLIENT_ID = "fortemi-cli"
DEFAULT_FORTEMI_URL = "https://fortemi.example.org"
MCP_CLIENTS = ("claude-code", "claude-desktop", "cursor", "vscode")
FILE_MODE = 0o600
DIR_MODE = 0o700
FILE_STORE_VERSION = 2


class LoginError(Exception):
    """Fatal helper error; message is safe to print."""


def fortemi_url(args: argparse.Namespace) -> str:
    return os.environ.get("FORTEMI_URL", args.fortemi_url or DEFAULT_FORTEMI_URL).rstrip("/")


def issuer(args: argparse.Namespace) -> str:
    value = os.environ.get("FORTEMI_ISSUER", args.issuer or "")
    if not value:
        raise LoginError("set --issuer or FORTEMI_ISSUER to the realm issuer URL")
    return value.rstrip("/")


def client_id(args: argparse.Namespace) -> str:
    return os.environ.get("FORTEMI_CLIENT_ID", args.client_id or DEFAULT_CLIENT_ID)


def file_store_allowed(args: argparse.Namespace) -> bool:
    return bool(args.allow_file_store or os.environ.get("FORTEMI_ALLOW_FILE_STORE") == "1")


def account_name(issuer_url: str, client: str) -> str:
    identity = json.dumps([issuer_url, client], separators=(",", ":"), ensure_ascii=False)
    return "v2:" + hashlib.sha256(identity.encode("utf-8")).hexdigest()


def legacy_account_name(issuer_url: str, client: str) -> str:
    host = urllib.parse.urlsplit(issuer_url).netloc or "idp"
    return f"{client}@{host}"


def pkce_pair() -> tuple[str, str]:
    verifier = secrets.token_urlsafe(32)
    digest = hashlib.sha256(verifier.encode("ascii")).digest()
    challenge = base64.urlsafe_b64encode(digest).rstrip(b"=").decode("ascii")
    return verifier, challenge


def http_json(
    url: str,
    data: dict | None = None,
    headers: dict | None = None,
    timeout: int = 30,
) -> dict:
    body = urllib.parse.urlencode(data).encode() if data is not None else None
    request = urllib.request.Request(url, data=body, headers=headers or {})
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            return json.loads(response.read().decode("utf-8"))
    except urllib.error.HTTPError as error:
        detail = error.read().decode("utf-8", "replace")[:500]
        raise LoginError(f"request to {url} failed: HTTP {error.code}: {detail}")
    except urllib.error.URLError as error:
        raise LoginError(f"request to {url} failed: {error.reason}")


def discover(issuer_url: str) -> dict:
    metadata = http_json(issuer_url + "/.well-known/openid-configuration")
    for key in ("authorization_endpoint", "token_endpoint"):
        if key not in metadata:
            raise LoginError(f"issuer metadata lacks {key}")
    return metadata


def app_dir() -> str:
    if sys.platform == "win32":
        base = os.environ.get("APPDATA") or os.path.expanduser("~")
        return os.path.join(base, "fortemi-login")
    if sys.platform == "darwin":
        return os.path.join(os.path.expanduser("~"), "Library", "Application Support", "fortemi-login")
    base = os.environ.get("XDG_CONFIG_HOME") or os.path.join(os.path.expanduser("~"), ".config")
    return os.path.join(base, "fortemi-login")


def token_file() -> str:
    return os.path.join(app_dir(), "tokens.json")


def ensure_private_dir(path: str) -> None:
    os.makedirs(path, mode=DIR_MODE, exist_ok=True)
    if os.name == "posix":
        mode = stat.S_IMODE(os.stat(path).st_mode)
        if mode != DIR_MODE:
            os.chmod(path, DIR_MODE)
            mode = stat.S_IMODE(os.stat(path).st_mode)
        if mode != DIR_MODE:
            raise LoginError(f"refusing to use directory with unsafe mode: {path}")


def check_private_file(path: str) -> None:
    if os.name != "posix":
        return
    mode = stat.S_IMODE(os.stat(path).st_mode)
    if mode != FILE_MODE:
        raise LoginError(
            f"refusing to read {path} with mode {mode:04o}: expected 0600 "
            "(remove it or run: chmod 600)"
        )


def read_file_store() -> dict:
    path = token_file()
    if not os.path.exists(path):
        return {}
    check_private_file(path)
    with open(path, encoding="utf-8") as handle:
        data = json.load(handle)
    if not isinstance(data, dict):
        raise LoginError(f"token file is corrupt: {path}")
    return data


def write_file_store(data: dict) -> None:
    directory = app_dir()
    ensure_private_dir(directory)
    path = token_file()
    tmp = path + ".tmp"
    with open(tmp, "w", encoding="utf-8") as handle:
        json.dump(data, handle)
    os.chmod(tmp, FILE_MODE)
    os.replace(tmp, path)


def file_sessions(data: dict) -> dict:
    """Read a scoped store or a legacy session with verifiable identity."""
    if not data:
        return {}
    if data.get("version") == FILE_STORE_VERSION:
        sessions = data.get("sessions")
        if set(data) != {"version", "sessions"} or not isinstance(sessions, dict):
            raise LoginError("token file has an invalid session format")
        for issuer_url, clients in sessions.items():
            if not isinstance(issuer_url, str) or not isinstance(clients, dict):
                raise LoginError("token file has an invalid session format")
            for client, session in clients.items():
                if not isinstance(client, str) or not isinstance(session, dict):
                    raise LoginError("token file has an invalid session format")
                if session.get("issuer") != issuer_url or session.get("client_id") != client:
                    raise LoginError("token file contains a session with mismatched identity")
        return sessions
    issuer_url, client = data.get("issuer"), data.get("client_id")
    if isinstance(issuer_url, str) and issuer_url and isinstance(client, str) and client:
        return {issuer_url: {client: data}}
    raise LoginError("legacy token file has no verifiable issuer and client; move it aside and sign in again")


def checked_session(session: dict, issuer_url: str, client: str) -> dict:
    if not isinstance(session, dict) or session.get("issuer") != issuer_url or session.get("client_id") != client:
        raise LoginError("stored session does not match the requested issuer and client")
    return session


def keyring_session(raw: str, issuer_url: str, client: str, *, legacy: bool = False) -> dict | None:
    try:
        session = json.loads(raw)
    except (TypeError, ValueError):
        raise LoginError("OS keychain session is invalid") from None
    if not isinstance(session, dict):
        raise LoginError("OS keychain session is invalid")
    stored_issuer, stored_client = session.get("issuer"), session.get("client_id")
    if not isinstance(stored_issuer, str) or not stored_issuer or not isinstance(stored_client, str) or not stored_client:
        raise LoginError("OS keychain session has no verifiable issuer and client")
    if legacy and (stored_issuer != issuer_url or stored_client != client):
        return None  # A different realm may own the old host-only account.
    return checked_session(session, issuer_url, client)


def require_file_store(args: argparse.Namespace) -> None:
    if not file_store_allowed(args):
        raise LoginError(
            "no usable OS keychain is available; re-run with --allow-file-store "
            "for a permission-restricted plaintext file fallback (mode 0600)"
        )


def load_file_tokens(issuer_url: str, client: str) -> dict:
    session = file_sessions(read_file_store()).get(issuer_url, {}).get(client)
    return checked_session(session, issuer_url, client) if session is not None else {}


def save_file_tokens(issuer_url: str, client: str, data: dict) -> None:
    checked_session(data, issuer_url, client)
    sessions = file_sessions(read_file_store())
    sessions.setdefault(issuer_url, {})[client] = data
    write_file_store({"version": FILE_STORE_VERSION, "sessions": sessions})


def delete_file_tokens(issuer_url: str, client: str) -> None:
    sessions = file_sessions(read_file_store())
    clients = sessions.get(issuer_url, {})
    if client not in clients:
        return
    del clients[client]
    if not clients:
        del sessions[issuer_url]
    write_file_store({"version": FILE_STORE_VERSION, "sessions": sessions})


def load_tokens(issuer_url: str, client: str, args: argparse.Namespace) -> dict:
    if _keyring is not None:
        try:
            raw = _keyring.get_password(KEYRING_SERVICE, account_name(issuer_url, client))
            if raw is not None:
                return keyring_session(raw, issuer_url, client)
            legacy_raw = _keyring.get_password(KEYRING_SERVICE, legacy_account_name(issuer_url, client))
            if legacy_raw is None:
                return {}  # A healthy keyring miss must not select an old file store.
            legacy = keyring_session(legacy_raw, issuer_url, client, legacy=True)
            if legacy is None:
                return {}
            # Copy only a verified matching legacy session. Leave the old entry until
            # this same identity logs out, so another realm's entry is never deleted.
            _keyring.set_password(KEYRING_SERVICE, account_name(issuer_url, client), json.dumps(legacy))
            return legacy
        except _NoKeyringError:
            pass
        except LoginError:
            raise
        except Exception:
            raise LoginError("OS keychain read failed") from None
    require_file_store(args)
    return load_file_tokens(issuer_url, client)


def save_tokens(issuer_url: str, client: str, data: dict, args: argparse.Namespace) -> None:
    checked_session(data, issuer_url, client)
    if _keyring is not None:
        try:
            _keyring.set_password(KEYRING_SERVICE, account_name(issuer_url, client), json.dumps(data))
        except _NoKeyringError:
            pass
        except Exception:
            raise LoginError("OS keychain write failed") from None
        else:
            return
    require_file_store(args)
    save_file_tokens(issuer_url, client, data)


def delete_tokens(issuer_url: str, client: str, args: argparse.Namespace) -> None:
    if _keyring is not None:
        try:
            exact_account = account_name(issuer_url, client)
            legacy_account = legacy_account_name(issuer_url, client)
            exact_raw = _keyring.get_password(KEYRING_SERVICE, exact_account)
            legacy_raw = _keyring.get_password(KEYRING_SERVICE, legacy_account)
            if exact_raw is not None:
                keyring_session(exact_raw, issuer_url, client)
            legacy = (keyring_session(legacy_raw, issuer_url, client, legacy=True)
                      if legacy_raw is not None else None)
            if exact_raw is not None:
                _keyring.delete_password(KEYRING_SERVICE, exact_account)
            if legacy is not None:
                _keyring.delete_password(KEYRING_SERVICE, legacy_account)
            return
        except _NoKeyringError:
            pass
        except LoginError:
            raise
        except Exception:
            raise LoginError("OS keychain deletion failed") from None
    require_file_store(args)
    delete_file_tokens(issuer_url, client)


def store_session(issuer_url: str, client: str, response: dict, args: argparse.Namespace) -> dict:
    if "access_token" not in response:
        raise LoginError("token endpoint returned no access token")
    session = {
        "access_token": response["access_token"],
        "refresh_token": response.get("refresh_token", ""),
        "scope": response.get("scope", ""),
        "expires_at": int(time.time()) + int(response.get("expires_in", 300)),
        "issuer": issuer_url,
        "client_id": client,
    }
    save_tokens(issuer_url, client, session, args)
    return session


class _CallbackHandler(BaseHTTPRequestHandler):
    code: str | None = None
    state: str | None = None
    error: str | None = None

    def do_GET(self) -> None:  # noqa: N802
        query = urllib.parse.parse_qs(urllib.parse.urlsplit(self.path).query)
        _CallbackHandler.code = (query.get("code") or [None])[0]
        _CallbackHandler.state = (query.get("state") or [None])[0]
        _CallbackHandler.error = (query.get("error") or [None])[0]
        self.send_response(200)
        self.send_header("Content-Type", "text/html")
        self.end_headers()
        self.wfile.write(
            b"<html><body><h1>Signed in.</h1><p>Return to your terminal.</p></body></html>"
        )

    def log_message(self, *args: object) -> None:
        pass


def login_loopback(metadata: dict, issuer_url: str, client: str, args: argparse.Namespace) -> dict:
    verifier, challenge = pkce_pair()
    state = secrets.token_urlsafe(16)
    server = HTTPServer(("127.0.0.1", 0), _CallbackHandler)
    redirect_uri = f"http://127.0.0.1:{server.server_port}/callback"
    params = {
        "client_id": client,
        "redirect_uri": redirect_uri,
        "response_type": "code",
        "scope": "openid profile email fortemi-api fortemi-mcp",
        "code_challenge": challenge,
        "code_challenge_method": "S256",
        "state": state,
    }
    url = metadata["authorization_endpoint"] + "?" + urllib.parse.urlencode(params)
    print("Opening the browser to sign in...", file=sys.stderr)
    print(url, file=sys.stderr)
    webbrowser.open(url)
    _CallbackHandler.code = _CallbackHandler.state = _CallbackHandler.error = None
    deadline = time.time() + 300
    server.timeout = 1
    while time.time() < deadline:
        server.handle_request()
        if _CallbackHandler.code or _CallbackHandler.error:
            break
    server.server_close()
    if _CallbackHandler.error:
        raise LoginError(f"authorization failed: {_CallbackHandler.error}")
    if not _CallbackHandler.code:
        raise LoginError("timed out waiting for the browser callback")
    if _CallbackHandler.state != state:
        raise LoginError("state mismatch in the browser callback")
    response = http_json(
        metadata["token_endpoint"],
        {
            "grant_type": "authorization_code",
            "code": _CallbackHandler.code,
            "redirect_uri": redirect_uri,
            "client_id": client,
            "code_verifier": verifier,
        },
    )
    return store_session(issuer_url, client, response, args)


def login_device(metadata: dict, issuer_url: str, client: str, args: argparse.Namespace) -> dict:
    endpoint = metadata.get("device_authorization_endpoint")
    if not endpoint:
        raise LoginError("issuer does not advertise the device authorization endpoint")
    # PKCE also binds the device flow (Keycloak enforces it for clients that
    # require S256), so a stolen device_code cannot be redeemed alone.
    verifier, challenge = pkce_pair()
    device = http_json(
        endpoint,
        {
            "client_id": client,
            "scope": "openid profile email fortemi-api fortemi-mcp",
            "code_challenge": challenge,
            "code_challenge_method": "S256",
        },
    )
    print(f"Open {device['verification_uri']}", file=sys.stderr)
    print(f"and enter code: {device['user_code']}", file=sys.stderr)
    if device.get("verification_uri_complete"):
        print(device["verification_uri_complete"], file=sys.stderr)
    interval = int(device.get("interval", 5))
    deadline = time.time() + int(device.get("expires_in", 600))
    while time.time() < deadline:
        time.sleep(interval)
        try:
            response = http_json(
                metadata["token_endpoint"],
                {
                    "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
                    "device_code": device["device_code"],
                    "client_id": client,
                    "code_verifier": verifier,
                },
            )
        except LoginError as error:
            message = str(error)
            if "slow_down" in message:
                interval += 5
                continue
            if "authorization_pending" in message:
                continue
            # access_denied, expired_token and any other error end the flow.
            raise
        return store_session(issuer_url, client, response, args)
    raise LoginError("device authorization expired before approval")


def refresh_session(
    metadata: dict, issuer_url: str, client: str, session: dict, args: argparse.Namespace
) -> dict:
    if not session.get("refresh_token"):
        raise LoginError("no refresh token stored; run 'fortemi-login login' again")
    response = http_json(
        metadata["token_endpoint"],
        {
            "grant_type": "refresh_token",
            "refresh_token": session["refresh_token"],
            "client_id": client,
        },
    )
    if "refresh_token" not in response:
        response["refresh_token"] = session["refresh_token"]  # rotation not applied
    return store_session(issuer_url, client, response, args)


def fresh_access_token(metadata: dict, issuer_url: str, client: str, args: argparse.Namespace) -> str:
    session = load_tokens(issuer_url, client, args)
    if not session.get("access_token"):
        raise LoginError("not signed in; run 'fortemi-login login' first")
    if session.get("expires_at", 0) - 30 > time.time():
        return str(session["access_token"])
    return str(refresh_session(metadata, issuer_url, client, session, args)["access_token"])


def cmd_login(args: argparse.Namespace) -> int:
    issuer_url = issuer(args)
    metadata = discover(issuer_url)
    if args.device:
        login_device(metadata, issuer_url, client_id(args), args)
    else:
        login_loopback(metadata, issuer_url, client_id(args), args)
    print("Signed in.", file=sys.stderr)
    return 0


def cmd_token(args: argparse.Namespace) -> int:
    issuer_url = issuer(args)
    print(fresh_access_token(discover(issuer_url), issuer_url, client_id(args), args))
    return 0


def cmd_whoami(args: argparse.Namespace) -> int:
    issuer_url = issuer(args)
    token = fresh_access_token(discover(issuer_url), issuer_url, client_id(args), args)
    request = urllib.request.Request(
        fortemi_url(args) + "/api/v1/me", headers={"Authorization": f"Bearer {token}"}
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            print(response.read().decode("utf-8"))
    except urllib.error.HTTPError as error:
        raise LoginError(f"GET /api/v1/me failed: HTTP {error.code}")
    return 0


def cmd_logout(args: argparse.Namespace) -> int:
    issuer_url = issuer(args)
    client = client_id(args)
    session = load_tokens(issuer_url, client, args)
    failures = 0
    try:
        revocation = discover(issuer_url).get("revocation_endpoint")
    except LoginError:
        revocation = None
    if revocation:
        for token in (session.get("refresh_token"), session.get("access_token")):
            if not token:
                continue
            try:
                http_json(revocation, {"token": token, "client_id": client})
            except LoginError as error:
                print(f"warning: {error}", file=sys.stderr)
                failures += 1
    else:
        print("warning: issuer advertises no revocation endpoint", file=sys.stderr)
    delete_tokens(issuer_url, client, args)
    print("Signed out.", file=sys.stderr)
    return 1 if failures else 0


def mcp_config(client: str, base_url: str) -> dict:
    mcp_url = base_url + "/mcp"
    if client == "claude-code":
        return {
            "mcpServers": {
                "fortemi": {
                    "type": "http",
                    "url": mcp_url,
                    "headersHelper": "fortemi-login token",
                }
            }
        }
    if client == "claude-desktop":
        return {
            "mcpServers": {
                "fortemi": {
                    "command": "fortemi-login",
                    "args": ["token"],
                    "env": {"FORTEMI_MCP_URL": mcp_url},
                }
            }
        }
    if client == "cursor":
        return {"mcpServers": {"fortemi": {"url": mcp_url}}}
    if client == "vscode":
        return {"servers": {"fortemi": {"url": mcp_url, "type": "http"}}}
    raise LoginError(f"unknown MCP client '{client}'; choose from {', '.join(MCP_CLIENTS)}")


def cmd_mcp_config(args: argparse.Namespace) -> int:
    print(json.dumps(mcp_config(args.client, fortemi_url(args)), indent=2))
    print(
        "Paste the JSON above into the client's MCP configuration. These clients "
        "discover OAuth from "
        + fortemi_url(args)
        + "/mcp and sign in with the 'fortemi-cli' public client; "
        "where OAuth is impossible, use a personal access token instead.",
        file=sys.stderr,
    )
    return 0


def build_parser() -> argparse.ArgumentParser:
    def common_options(*, subcommand: bool) -> argparse.ArgumentParser:
        common = argparse.ArgumentParser(add_help=False)
        value_default = argparse.SUPPRESS if subcommand else ""
        flag_default = argparse.SUPPRESS if subcommand else False
        common.add_argument("--fortemi-url", default=value_default)
        common.add_argument("--issuer", default=value_default)
        common.add_argument("--client-id", default=value_default)
        common.add_argument("--allow-file-store", action="store_true", default=flag_default)
        return common

    parser = argparse.ArgumentParser(prog="fortemi-login", parents=[common_options(subcommand=False)])
    sub = parser.add_subparsers(dest="command", required=True)
    sub_common = common_options(subcommand=True)
    login = sub.add_parser("login", parents=[sub_common], help="sign in and store tokens")
    login.add_argument("--device", action="store_true", help="use the device grant")
    login.set_defaults(func=cmd_login)
    token = sub.add_parser("token", parents=[sub_common], help="print a fresh access token")
    token.set_defaults(func=cmd_token)
    whoami = sub.add_parser("whoami", parents=[sub_common], help="call GET /api/v1/me")
    whoami.set_defaults(func=cmd_whoami)
    logout = sub.add_parser("logout", parents=[sub_common], help="revoke and delete local tokens")
    logout.set_defaults(func=cmd_logout)
    config = sub.add_parser(
        "mcp-config", parents=[sub_common], help="print MCP client configuration"
    )
    config.add_argument("--client", required=True, choices=MCP_CLIENTS)
    config.set_defaults(func=cmd_mcp_config)
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        return int(args.func(args))
    except LoginError as error:
        print(f"fortemi-login: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
