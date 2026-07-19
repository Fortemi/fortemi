#!/usr/bin/env python3
"""Validate the security boundary of a rendered Fortemi bundle."""

from __future__ import annotations

import argparse
import ipaddress
import json
import sys
from pathlib import Path
from typing import Any
from urllib.parse import urlsplit


LOOPBACK_PROFILE = "local"
SHARED_PROFILE = "shared"
KNOWN_INSECURE_PASSWORDS = {
    "",
    "matric",
    "fortemi-local-dev",
    "password",
    "changeme",
    "<postgres_password>",
    "<operator_supplied_database_password>",
}
PRIVATE_DNS_SUFFIXES = (".localhost", ".local", ".internal", ".lan")
AUTOHEAL_PROFILE = "ops-autoheal"
AUTOHEAL_SERVICE = "autoheal"
AUTOHEAL_IMAGE = (
    "willfarrell/autoheal:1.2.0"
    "@sha256:31f580ef0279eaced5b38d631b08c474d70d8403c1c2fdd6ddcf2e879d5f3f7c"
)
THIRD_PARTY_BUNDLE_SERVICES = {
    "autoheal",
    "redis",
    "whisper",
    "whisper-gpu",
}


def parse_bool(value: Any) -> bool | None:
    normalized = str(value).strip().lower()
    if normalized in {"true", "1"}:
        return True
    if normalized in {"false", "0"}:
        return False
    return None


def is_insecure_password(value: Any) -> bool:
    normalized = str(value).strip()
    if (
        len(normalized) >= 2
        and normalized[0] == normalized[-1]
        and normalized[0] in {"'", '"'}
    ):
        normalized = normalized[1:-1]
    return normalized.strip().casefold() in KNOWN_INSECURE_PASSWORDS


def is_loopback_bind(value: str | None) -> bool:
    if not value:
        return False
    try:
        return ipaddress.ip_address(value).is_loopback
    except ValueError:
        return False


def display_binding(host_ip: str | None, published: str) -> str:
    host = host_ip or "all-interfaces"
    if ":" in host:
        host = f"[{host}]"
    return f"{host}:{published}"


def is_public_https_url(value: str, *, path_policy: str) -> bool:
    try:
        parsed = urlsplit(value)
        parsed.port
    except ValueError:
        return False
    if (
        parsed.scheme != "https"
        or not parsed.hostname
        or parsed.username is not None
        or parsed.password is not None
        or parsed.query
        or parsed.fragment
    ):
        return False
    if path_policy in {"issuer", "origin"} and parsed.path not in {"", "/"}:
        return False

    hostname = parsed.hostname.rstrip(".").lower()
    if (
        "*" in hostname
        or hostname == "localhost"
        or hostname.endswith(PRIVATE_DNS_SUFFIXES)
    ):
        return False
    try:
        address = ipaddress.ip_address(hostname)
    except ValueError:
        return True
    return address.is_global


def published_ports(service: dict[str, Any]) -> dict[int, list[dict[str, Any]]]:
    result: dict[int, list[dict[str, Any]]] = {}
    for item in service.get("ports", []):
        if not isinstance(item, dict):
            continue
        try:
            target = int(item.get("target"))
        except (TypeError, ValueError):
            continue
        if target in {3000, 3001}:
            result.setdefault(target, []).append(item)
    return result


def socket_services(services: dict[str, Any]) -> list[str]:
    found: list[str] = []
    for name, service in services.items():
        for volume in service.get("volumes", []):
            if not isinstance(volume, dict):
                continue
            if volume.get("source") in {"/var/run/docker.sock", "/run/docker.sock"}:
                found.append(name)
                break
    return sorted(found)


def mutable_image_services(services: dict[str, Any]) -> list[str]:
    return sorted(
        name
        for name, service in services.items()
        if service.get("image") and "@sha256:" not in service["image"]
    )


def mutable_third_party_services(services: dict[str, Any]) -> list[str]:
    return sorted(
        name
        for name in THIRD_PARTY_BUNDLE_SERVICES
        if isinstance(services.get(name), dict)
        and services[name].get("image")
        and "@sha256:" not in services[name]["image"]
    )


def validate(
    rendered: dict[str, Any],
    active_profiles: set[str] | None = None,
) -> tuple[list[str], list[str], dict[str, str]]:
    errors: list[str] = []
    warnings: list[str] = []
    active_profiles = active_profiles or set()
    services = rendered.get("services")
    if not isinstance(services, dict) or not isinstance(services.get("fortemi"), dict):
        return ["rendered config is missing services.fortemi"], warnings, {}

    fortemi = services["fortemi"]
    environment = fortemi.get("environment")
    if not isinstance(environment, dict):
        return ["rendered fortemi service is missing a structured environment"], warnings, {}

    profile = str(environment.get("FORTEMI_EXPOSURE_PROFILE", "")).strip().lower()
    if profile not in {LOOPBACK_PROFILE, SHARED_PROFILE}:
        errors.append(
            "FORTEMI_EXPOSURE_PROFILE must be exactly 'local' or 'shared'"
        )

    ports = published_ports(fortemi)
    port_report: dict[int, str] = {}
    for target, label in ((3000, "API"), (3001, "MCP")):
        matches = ports.get(target, [])
        if not matches:
            errors.append(f"{label} target port {target} is not published")
            continue
        if len(matches) != 1:
            errors.append(f"{label} target port {target} must have one host mapping")
        rendered_bindings: list[str] = []
        for port in matches:
            host_ip = port.get("host_ip")
            published = str(port.get("published", "unknown"))
            rendered_bindings.append(display_binding(host_ip, published))
            if profile == LOOPBACK_PROFILE and not is_loopback_bind(host_ip):
                errors.append(
                    f"{label} must publish to a loopback host IP in the local profile"
                )
        port_report[target] = ",".join(rendered_bindings)

    auth = parse_bool(environment.get("REQUIRE_AUTH"))
    if auth is None:
        errors.append("REQUIRE_AUTH must render as a strict boolean")

    issuer = str(environment.get("ISSUER_URL", "")).strip()
    mcp_base = str(environment.get("MCP_BASE_URL", "")).strip()
    origins = [
        origin.strip()
        for origin in str(environment.get("ALLOWED_ORIGINS", "")).split(",")
        if origin.strip()
    ]
    password = str(environment.get("POSTGRES_PASSWORD", ""))
    uses_insecure_password = is_insecure_password(password)
    if uses_insecure_password:
        errors.append(
            "bundle profiles require a generated or operator-supplied "
            "POSTGRES_PASSWORD"
        )

    if profile == SHARED_PROFILE:
        if auth is not True:
            errors.append("shared exposure requires REQUIRE_AUTH=true")
        if not is_public_https_url(issuer, path_policy="issuer"):
            errors.append("shared exposure requires a public HTTPS ISSUER_URL")
        if not is_public_https_url(mcp_base, path_policy="resource"):
            errors.append("shared exposure requires a public HTTPS MCP_BASE_URL")
        if not origins or any(
            not is_public_https_url(origin, path_policy="origin") for origin in origins
        ):
            errors.append(
                "shared exposure requires explicit public HTTPS ALLOWED_ORIGINS"
            )

    mutable = mutable_image_services(services)
    mutable_third_party = mutable_third_party_services(services)
    sockets = socket_services(services)
    if mutable_third_party:
        errors.append(
            "third-party bundle images require immutable digest references: "
            + ", ".join(mutable_third_party)
        )
    mutable_fortemi = sorted(set(mutable) - set(mutable_third_party))
    if mutable_fortemi:
        warnings.append(
            "#888 Fortemi release image evidence remains for services: "
            + ", ".join(mutable_fortemi)
        )

    autoheal = services.get(AUTOHEAL_SERVICE)
    autoheal_enabled = AUTOHEAL_PROFILE in active_profiles
    if sockets and not autoheal_enabled:
        errors.append(
            "Docker socket access requires the explicit ops-autoheal profile"
        )
    unexpected_sockets = [name for name in sockets if name != AUTOHEAL_SERVICE]
    if unexpected_sockets:
        errors.append(
            "Docker socket access is forbidden outside the autoheal service: "
            + ", ".join(unexpected_sockets)
        )
    if autoheal_enabled:
        if not isinstance(autoheal, dict):
            errors.append(
                "ops-autoheal profile must render the reviewed autoheal service"
            )
        else:
            if AUTOHEAL_SERVICE not in sockets:
                errors.append(
                    "ops-autoheal service must render its reviewed Docker socket mount"
                )
            if autoheal.get("image") != AUTOHEAL_IMAGE:
                errors.append(
                    f"ops-autoheal image must be pinned to {AUTOHEAL_IMAGE}"
                )
    elif autoheal is not None:
        errors.append(
            "autoheal service must not render without the explicit ops-autoheal profile"
        )

    report = {
        "profile": profile or "invalid",
        "api_bind": port_report.get(3000, "missing"),
        "mcp_bind": port_report.get(3001, "missing"),
        "auth_mode": "required" if auth is True else "not-required",
        "issuer_mode": "public_https"
        if is_public_https_url(issuer, path_policy="issuer")
        else "local_or_invalid",
        "db_secret_source": (
            "missing_or_insecure"
            if uses_insecure_password
            else "generated_or_operator_supplied"
        ),
        "image_trust": (
            "third_party_digest_locked"
            if not mutable_third_party
            else "third_party_mutable"
        ),
        "docker_socket_profile": (
            AUTOHEAL_PROFILE if autoheal_enabled and sockets else "absent"
        ),
    }
    return errors, warnings, report


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("rendered_config", type=Path)
    parser.add_argument(
        "--active-profiles",
        default="",
        help="comma-separated effective Docker Compose profiles",
    )
    args = parser.parse_args()

    try:
        rendered = json.loads(args.rendered_config.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        print(f"ERROR: cannot read rendered Compose JSON: {error}", file=sys.stderr)
        return 2

    active_profiles = {
        profile.strip()
        for profile in args.active_profiles.split(",")
        if profile.strip()
    }
    errors, warnings, report = validate(rendered, active_profiles)
    for warning in warnings:
        print(f"WARNING: {warning}", file=sys.stderr)
    for key, value in report.items():
        print(f"bundle_exposure {key}={value}")
    if errors:
        print("Bundle exposure validation failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1

    print("Bundle exposure validation passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
