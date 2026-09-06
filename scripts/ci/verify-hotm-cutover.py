#!/usr/bin/env python3
"""Read-only topology and browser-origin HTTP checks after bundle cutover.

Exit 0 proves topology/HTTP reachability only. A real fresh-browser smoke is
separate; this script never treats health as authenticated session acceptance.
"""
import argparse
import json
import subprocess
import sys
import urllib.error
import urllib.parse
import urllib.request


def check_topology(containers, alias="fortemi"):
    """Require each HotM consumer to share the server's upstream alias network."""
    server, *consumers = containers
    problems = []
    for item in containers:
        if not item.get("State", {}).get("Running"):
            problems.append(f"container not running: {item.get('Name', 'unknown')}")
    server_networks = server.get("NetworkSettings", {}).get("Networks", {})
    for consumer in consumers:
        networks = consumer.get("NetworkSettings", {}).get("Networks", {})
        shared = [name for name, net in server_networks.items()
                  if name in networks and net.get("NetworkID")
                  and net["NetworkID"] == networks[name].get("NetworkID")
                  and alias in (net.get("Aliases") or [])]
        if not shared:
            problems.append(f"no shared network with upstream alias {alias}: "
                            f"{consumer.get('Name', 'unknown')}")
    return problems


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


def probe(base_url, path):
    opener = urllib.request.build_opener(NoRedirect)
    try:
        with opener.open(base_url.rstrip("/") + path, timeout=10) as response:
            data = response.read(1024 * 1024 + 1)
            if len(data) > 1024 * 1024:
                return "invalid_response", response.status
            try:
                payload = json.loads(data)
            except (ValueError, UnicodeError):
                return "invalid_response", response.status
            expected = (isinstance(payload, dict) and
                        (payload.get("status") in ("healthy", "ok", "ready")
                         if path == "/health"
                         else bool(payload.get("schema_version")) and
                         isinstance(payload.get("api"), dict)))
            return ("reachable" if expected else "invalid_response"), response.status
    except urllib.error.HTTPError as exc:
        code = exc.code
        exc.close()
        return ("auth_required" if code in (401, 403) else "http_error"), code
    except (urllib.error.URLError, TimeoutError, OSError):
        return "unreachable", None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--server", default="fortemi-fortemi-1")
    parser.add_argument("--ui", default="hotm-ui-manual")
    parser.add_argument("--proxy", default="hotm-agent-proxy-manual")
    parser.add_argument("--base-url", default="http://localhost:4180")
    args = parser.parse_args()
    url = urllib.parse.urlsplit(args.base_url)
    if url.scheme not in ("http", "https") or not url.netloc or url.username or url.password or url.query or url.fragment or url.path not in ("", "/"):
        parser.error("base URL must be an HTTP(S) origin without credentials or query")
    try:
        result = subprocess.run(["docker", "inspect", args.server, args.ui, args.proxy],
                                capture_output=True, text=True, check=True, timeout=15)
        containers = json.loads(result.stdout)
        if len(containers) != 3:
            raise ValueError("unexpected inspect result")
        problems = check_topology(containers)
    except (OSError, subprocess.SubprocessError, ValueError):
        print(json.dumps({"status": "failed", "reason": "container inspection failed"}))
        return 1
    # Report only selected public checks, never docker inspect's environment.
    checks = {path: dict(zip(("state", "http_status"), probe(args.base_url, path)))
              for path in ("/health", "/api/v1/system/compatibility")}
    states = {check["state"] for check in checks.values()}
    status = "failed" if problems or states - {"reachable", "auth_required"} else (
        "auth_required" if "auth_required" in states else "reachable")
    print(json.dumps({"status": status, "topology_errors": problems, "http": checks,
                      "browser_session_verified": False}, indent=2))
    return {"reachable": 0, "auth_required": 2, "failed": 1}[status]


if __name__ == "__main__":
    sys.exit(main())
