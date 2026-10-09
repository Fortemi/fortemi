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
except ImportError:
    _keyring = None

KEYRING_SERVICE = "fortemi-login"
DEFAULT_CLIENT_ID = "fortemi-cli"
DEFAULT_FORTEMI_URL = "https://fortemi.example.org"
MCP_CLIENTS = ("claude-code", "claude-desktop", "cursor", "vscode")
FILE_MODE = 0o600
DIR_MODE = 0o700


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


def load_tokens(issuer_url: str, client: str, args: argparse.Namespace) -> dict:
    if _keyring is not None:
        raw = _keyring.get_password(KEYRING_SERVICE, account_name(issuer_url, client))
        return json.loads(raw) if raw else {}
    if not file_store_allowed(args):
        raise LoginError(
            "no OS keychain is available (install 'keyring' or re-run with "
            "--allow-file-store for an encrypted-at-rest file fallback)"
        )
    return read_file_store()


def save_tokens(issuer_url: str, client: str, data: dict, args: argparse.Namespace) -> None:
    if _keyring is not None:
        _keyring.set_password(KEYRING_SERVICE, account_name(issuer_url, client), json.dumps(data))
        return
    if not file_store_allowed(args):
        raise LoginError(
            "no OS keychain is available (install 'keyring' or re-run with "
            "--allow-file-store for an encrypted-at-rest file fallback)"
        )
    write_file_store(data)


def delete_tokens(issuer_url: str, client: str, args: argparse.Namespace) -> None:
    if _keyring is not None:
        try:
            _keyring.delete_password(KEYRING_SERVICE, account_name(issuer_url, client))
        except Exception:
            pass
        return
    if not file_store_allowed(args):
        raise LoginError("no OS keychain is available; re-run with --allow-file-store")
    try:
        os.remove(token_file())
    except FileNotFoundError:
        pass


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
    common = argparse.ArgumentParser(add_help=False)
    common.add_argument("--fortemi-url", default="")
    common.add_argument("--issuer", default="")
    common.add_argument("--client-id", default="")
    common.add_argument("--allow-file-store", action="store_true")
    parser = argparse.ArgumentParser(prog="fortemi-login", parents=[common])
    sub = parser.add_subparsers(dest="command", required=True)
    login = sub.add_parser("login", parents=[common], help="sign in and store tokens")
    login.add_argument("--device", action="store_true", help="use the device grant")
    login.set_defaults(func=cmd_login)
    token = sub.add_parser("token", parents=[common], help="print a fresh access token")
    token.set_defaults(func=cmd_token)
    whoami = sub.add_parser("whoami", parents=[common], help="call GET /api/v1/me")
    whoami.set_defaults(func=cmd_whoami)
    logout = sub.add_parser("logout", parents=[common], help="revoke and delete local tokens")
    logout.set_defaults(func=cmd_logout)
    config = sub.add_parser(
        "mcp-config", parents=[common], help="print MCP client configuration"
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
