#!/usr/bin/env python3
"""Standard-library Keycloak admin/PKCE driver; never print credential material."""

import base64
import hashlib
import html
import http.cookiejar
import json
import os
from pathlib import Path
import re
import secrets
import socket
import ssl
import sys
import time
import traceback
import urllib.error
import urllib.parse
import urllib.request

WORK = Path(sys.argv[2])
CONFIG = WORK / "credentials.json"
KC_NAME = os.environ["KEYCLOAK_NAME"]
KC_PORT = int(os.environ["KEYCLOAK_PORT"])
KC = f"https://{KC_NAME}:8443"
ISSUER = KC + "/realms/example"
API_RESOURCE = "https://fortemi.example.org"
MCP_RESOURCE = API_RESOURCE + "/mcp"
TLS = ssl.create_default_context(cafile=str(WORK / "tls/ca.pem"))
# Connect through the loopback-published port while retaining the issuer URL,
# TLS SNI/hostname verification and Host header. No host resolver changes.
CONNECT = socket.create_connection


def connect(address, *args, **kwargs):
    if address == (KC_NAME, 8443):
        address = ("127.0.0.1", KC_PORT)
    return CONNECT(address, *args, **kwargs)


socket.create_connection = connect


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        return None


def opener(jar=None):
    handlers = [urllib.request.ProxyHandler({}),
                urllib.request.HTTPSHandler(context=TLS), NoRedirect()]
    if jar is not None:
        handlers.append(urllib.request.HTTPCookieProcessor(jar))
    return urllib.request.build_opener(*handlers)


HTTP = opener()
CREDENTIALS = json.loads(CONFIG.read_text())
SENSITIVE = list(CREDENTIALS.values())
RESULTS = []


def request(url, method="GET", body=None, form=None, token=None, op=HTTP, headers=None):
    headers = dict(headers or {})
    if form is not None:
        body = urllib.parse.urlencode(form).encode()
        headers["Content-Type"] = "application/x-www-form-urlencoded"
    elif isinstance(body, dict) or isinstance(body, list):
        body = json.dumps(body).encode()
        headers["Content-Type"] = "application/json"
    if token:
        headers["Authorization"] = "Bearer " + token
    try:
        response = op.open(urllib.request.Request(url, data=body, method=method,
                                                headers=headers), timeout=20)
    except urllib.error.HTTPError as error:
        response = error
    with response:
        return response.code, response.headers, response.read().decode()


def require(condition, message):
    if not condition:
        raise AssertionError(message)


def tokens(body, case):
    data = json.loads(body)
    require("access_token" in data, f"{case}: no access token")
    for key in ("access_token", "refresh_token", "id_token"):
        if data.get(key):
            SENSITIVE.append(data[key])
    return data["access_token"]


def claims(token):
    payload = token.split(".")[1]
    return json.loads(base64.urlsafe_b64decode(payload + "=" * (-len(payload) % 4)))


def case(name, check):
    try:
        check()
        RESULTS.append({"case": name, "passed": True})
        print(f"[keycloak-it] PASS: {name}", flush=True)
    except Exception as error:
        # Only our controlled assertion messages are safe to print. Network and
        # parser exceptions can contain URLs, codes or response bodies.
        detail = str(error) if isinstance(error, AssertionError) else (
            type(error).__name__ + " at line " + str(traceback.extract_tb(error.__traceback__)[-1].lineno))
        RESULTS.append({"case": name, "passed": False, "detail": detail})
        print(f"[keycloak-it] FAIL: {name}: {detail}", flush=True)


def prepare():
    for _ in range(120):
        try:
            status, _, _ = request(KC + "/realms/master")
            if status == 200:
                break
        except (OSError, urllib.error.URLError):
            pass
        time.sleep(1)
    else:
        raise AssertionError("Keycloak did not become ready over verified TLS")
    status, _, body = request(KC + "/realms/master/protocol/openid-connect/token", "POST",
                             form={"grant_type": "password", "client_id": "admin-cli",
                                   "username": CREDENTIALS["admin_username"],
                                   "password": CREDENTIALS["admin_password"]})
    require(status == 200, f"bootstrap admin token: HTTP {status}")
    admin_token = tokens(body, "bootstrap admin")

    def admin(method, path, body=None, expected=200):
        status, _, data = request(KC + "/admin/realms" + path, method, body=body,
                                  token=admin_token, headers={"Content-Type": "application/json"})
        require(status == expected, f"realm setup {method} {path}: HTTP {status}, expected {expected}")
        return json.loads(data) if data else None

    # Submit the shipped file bytes without rewriting clients, scopes or policies.
    admin("POST", "", Path(sys.argv[3]).read_bytes(), expected=201)
    groups = admin("GET", "/example/groups")
    for username, group in (("tester", "fortemi-users"), ("admin-tester", "fortemi-admins")):
        admin("POST", "/example/users", {"username": username, "enabled": True,
              "email": username + "@example.org", "emailVerified": True,
              "firstName": "Reference", "lastName": "Tester", "requiredActions": [],
              "credentials": [{"type": "password", "temporary": False,
                               "value": CREDENTIALS[username + "_password"]}]}, expected=201)
        user = admin("GET", "/example/users?" + urllib.parse.urlencode({"username": username, "exact": "true"}))[0]
        group_id = next(g["id"] for g in groups if g["name"] == group)
        admin("PUT", f"/example/users/{user['id']}/groups/{group_id}", expected=204)
    clients = admin("GET", "/example/clients")
    service = next(c for c in clients if c["clientId"] == "fortemi-service-example")
    service["secret"] = CREDENTIALS["client_secret"]
    admin("PUT", "/example/clients/" + service["id"], service, expected=204)
    account = admin("GET", "/example/clients/" + service["id"] + "/service-account-user")
    resource = next(c for c in clients if c["clientId"] == "fortemi")
    roles = admin("GET", "/example/clients/" + resource["id"] + "/roles")
    admin("POST", f"/example/users/{account['id']}/role-mappings/clients/{resource['id']}",
          [r for r in roles if r["name"] in ("fortemi-read", "fortemi-write")], expected=204)
    # Preserve all minted secrets for the final log scan, including admin tokens.
    (WORK / "setup-tokens.json").write_text(json.dumps(SENSITIVE))
    print("[keycloak-it] PASS: verified TLS, unmodified realm import, users and service setup", flush=True)


def login(client, username, redirect):
    op = opener(http.cookiejar.CookieJar())
    verifier = secrets.token_urlsafe(40)
    challenge = base64.urlsafe_b64encode(hashlib.sha256(verifier.encode()).digest()).rstrip(b"=").decode()
    state = secrets.token_urlsafe(24)
    params = {"client_id": client, "response_type": "code", "redirect_uri": redirect,
              "scope": "read write admin mcp", "code_challenge": challenge,
              "code_challenge_method": "S256", "state": state}
    status, headers, body = request(ISSUER + "/protocol/openid-connect/auth?" + urllib.parse.urlencode(params), op=op)
    require(status == 200, f"{client} advertised scopes authorization: HTTP {status}, expected login page")
    action = re.search(r'action="([^"]+)"', body)
    require(action is not None, f"{client}: login form missing")
    status, headers, body = request(urllib.parse.urljoin(KC, html.unescape(action[1])), "POST", op=op,
        form={"username": username, "password": CREDENTIALS[username + "_password"]})
    # Follow Keycloak-internal required-action/consent redirects only; never
    # fetch the loopback callback, whose authorization code stays in memory.
    for _ in range(8):
        location = headers.get("Location", "")
        if status in (302, 303) and location.startswith(redirect.split("?", 1)[0]):
            break
        if status in (302, 303) and location.startswith(KC + "/"):
            status, headers, body = request(location, op=op)
            continue
        require(status == 200 and "oauth-grant" in body, f"{client}: expected consent form")
        action = re.search(r'action="([^"]+)"', body)
        require(action is not None, f"{client}: consent action missing")
        form = {"accept": "Yes"}
        code = re.search(r'name="code" value="([^"]+)"', body)
        if code:
            form["code"] = html.unescape(code[1])
        status, headers, body = request(urllib.parse.urljoin(KC, html.unescape(action[1])), "POST", form=form, op=op)
    query = urllib.parse.parse_qs(urllib.parse.urlsplit(headers.get("Location", "")).query)
    require(query.get("state") == [state] and "code" in query,
            f"{client}: authorization-code redirect missing or state mismatch")
    status, _, body = request(ISSUER + "/protocol/openid-connect/token", "POST",
        form={"grant_type": "authorization_code", "code": query["code"][0],
              "redirect_uri": redirect, "client_id": client, "code_verifier": verifier})
    require(status == 200, f"{client}: PKCE token exchange HTTP {status}")
    return tokens(body, client)


def audience(token, expected):
    aud = claims(token).get("aud")
    actual = set(aud if isinstance(aud, list) else [aud])
    require(actual == expected, "token audience set differs from expected resource URIs")


def run():
    api, mcp = sys.argv[3:5]
    SENSITIVE.extend(json.loads((WORK / "setup-tokens.json").read_text()))

    def jwks():
        status, _, body = request(ISSUER + "/protocol/openid-connect/certs")
        require(status == 200, f"JWKS: HTTP {status}")
        require(any(k.get("use") == "enc" for k in json.loads(body)["keys"]),
                "Keycloak JWKS must contain a non-signing encryption key (fortemi-auth#58)")
    case("JWKS contains non-signing key", jwks)

    for path, resource in (("", API_RESOURCE), ("/mcp", MCP_RESOURCE)):
        def metadata(path=path, resource=resource):
            status, _, body = request(api + "/.well-known/oauth-protected-resource" + path)
            require(status == 200, f"metadata: HTTP {status}, expected 200")
            document = json.loads(body)
            require(document["resource"] == resource, "metadata resource mismatch")
            require(document["authorization_servers"] == [ISSUER], "metadata external issuer mismatch")
            require(set(document["scopes_supported"]) == {"read", "write", "admin", "mcp"}, "metadata scope vocabulary mismatch")
        case("public protected-resource metadata " + (path or "/"), metadata)
    for path in ("/.well-known/oauth-authorization-server", "/.well-known/oauth-protected-resource/unknown",
                 "/.well-known/oauth-protected-resource/mcp%2F", "/.well-known/oauth-protected-resource/mcp/"):
        def absent(path=path):
            status, _, _ = request(api + path)
            require(status == 404, f"HTTP {status}, expected 404")
        case("public 404 " + path, absent)

    def mcp_metadata():
        status, _, body = request(mcp + "/.well-known/oauth-protected-resource")
        require(status == 200, f"MCP metadata: HTTP {status}, expected 200")
        document = json.loads(body)
        require(document["resource"] == MCP_RESOURCE, "MCP metadata resource mismatch")
        require(document["authorization_servers"] == [ISSUER], "MCP metadata external issuer mismatch")
    case("MCP public protected-resource metadata", mcp_metadata)

    def mcp_authorization_server_absent():
        status, _, _ = request(mcp + "/.well-known/oauth-authorization-server")
        require(status == 404, f"MCP authorization-server metadata: HTTP {status}, expected 404")
    case("MCP external mode authorization-server metadata 404", mcp_authorization_server_absent)

    def mcp_unauthenticated():
        status, headers, _ = request(mcp + "/", "POST", body={"jsonrpc": "2.0", "id": 1, "method": "initialize"},
                                    headers={"Accept": "application/json, text/event-stream"})
        require(status == 401, f"unauthenticated MCP POST: HTTP {status}, expected 401")
        require('resource_metadata="' + MCP_RESOURCE + '/.well-known/oauth-protected-resource"'
                in headers.get("WWW-Authenticate", ""), "MCP challenge lacks resource_metadata")
    case("MCP unauthenticated POST metadata challenge", mcp_unauthenticated)

    def me(token, kind):
        status, _, body = request(api + "/api/v1/me", token=token)
        require(status == 200, f"/api/v1/me: HTTP {status}, expected 200 (503 indicates the unresolved fortemi-auth JWKS bug)")
        document = json.loads(body)
        require(document["kind"] == kind, "principal kind mismatch")
        scopes = set(document["scopes"])
        expected = {"read", "write", "mcp"} if kind == "user" else {"read", "write"}
        require(expected <= scopes and "admin" not in scopes, "role mapping/client ceiling mismatch")
        if kind == "service":
            require("mcp" not in scopes, "service principal unexpectedly has mcp authority")

    def initialize(token, expected):
        body = {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-03-26", "capabilities": {},
            "clientInfo": {"name": "keycloak-reference-realm-it", "version": "1"}}}
        status, headers, response = request(mcp + "/", "POST", token=token, body=body,
            headers={"Accept": "application/json, text/event-stream"})
        require(status == expected, f"MCP initialize: HTTP {status}, expected {expected}")
        if expected == 403:
            require('error="insufficient_scope"' in headers.get("WWW-Authenticate", ""),
                    "service denial lacks insufficient_scope challenge")
            require(json.loads(response).get("error") == "insufficient_scope", "service denial error mismatch")
        else:
            data = response if response.lstrip().startswith("{") else "\n".join(
                line[5:] for line in response.splitlines() if line.startswith("data:"))
            require("serverInfo" in json.loads(data).get("result", {}), "initialize result missing serverInfo")

    for client, user, redirect, expected in (
        ("claude-code", "tester", "http://localhost:33418/callback", {MCP_RESOURCE}),
        ("fortemi-cli", "tester", "http://127.0.0.1:5000/cb", {API_RESOURCE, MCP_RESOURCE}),
        ("fortemi-cli", "admin-tester", "http://127.0.0.1:5000/cb", {API_RESOURCE, MCP_RESOURCE}),
    ):
        label = client + " " + user
        acquired = []
        def acquire(client=client, user=user, redirect=redirect, expected=expected):
            token = login(client, user, redirect)
            acquired.append(token)
            audience(token, expected)
            require(claims(token)["iss"] == ISSUER, "token issuer mismatch")
        case(label + " PKCE advertised scopes + audiences", acquire)
        # Failure to acquire a token is already a hard failure; dependent cases
        # also fail explicitly so no assertion is silently skipped.
        def acquired_token():
            require(bool(acquired), label + ": PKCE prerequisite failed")
            return acquired[0]
        case(label + " /me role mapping and ceiling", lambda: me(acquired_token(), "user"))
        if client == "claude-code":
            case(label + " MCP initialize", lambda: initialize(acquired_token(), 200))

    service = []
    def client_credentials():
        status, _, body = request(ISSUER + "/protocol/openid-connect/token", "POST", form={
            "grant_type": "client_credentials", "client_id": "fortemi-service-example",
            "client_secret": CREDENTIALS["client_secret"]})
        require(status == 200, f"client credentials: HTTP {status}")
        service.append(tokens(body, "service"))
        audience(service[0], {API_RESOURCE})
    case("service client credentials + audience", client_credentials)
    def service_token():
        require(bool(service), "client credentials prerequisite failed")
        return service[0]
    case("service /me kind and scopes", lambda: me(service_token(), "service"))
    case("service MCP insufficient_scope", lambda: initialize(service_token(), 403))

    def no_leaks():
        fragments = set(SENSITIVE)
        for value in SENSITIVE:
            if value.count(".") == 2:
                fragments.update(part for part in value.split(".") if len(part) >= 16)
        for file in (WORK / "api.log", WORK / "mcp.log", WORK / "output.log"):
            content = file.read_text()
            require(bool(content), f"{file.name} is empty")
            require(not any(fragment in content for fragment in fragments), f"secret material leaked in {file.name}")
    # This always executes, including after authentication assertion failures.
    case("no tokens/passwords/client secrets in API, MCP or script output", no_leaks)
    (WORK / "evidence/cases.json").write_text(json.dumps(RESULTS, indent=2) + "\n")
    failed = sum(not result["passed"] for result in RESULTS)
    print(f"[keycloak-it] {len(RESULTS) - failed} passed, {failed} failed", flush=True)
    return int(failed > 0)


try:
    if sys.argv[1] == "prepare":
        prepare()
    else:
        sys.exit(run())
except Exception as error:
    detail = str(error) if isinstance(error, AssertionError) else type(error).__name__
    print(f"[keycloak-it] FAIL: {sys.argv[1]}: {detail}", file=sys.stderr)
    sys.exit(1)
