#!/usr/bin/env python3
"""Lint the reference Keycloak realm and claim policy for client onboarding.

Covers the reference-realm requirements from the OIDC identity threat model:
SR-17 (client-role claim path), SR-19 (admin-managed tenant attribute),
SR-24 (service client without impersonation/token exchange), SR-41 (locked-down
anonymous client registration), SR-42 (device grant + public-client ceiling),
SR-45 (refresh rotation, no default offline_access, session lifetimes), plus
the fortemi-auth claim-policy contract 3.0.0 schema.

Usage:
  python3 scripts/ci/verify-reference-realm.py [--root .]
  python3 scripts/ci/verify-reference-realm.py --self-test
"""

from __future__ import annotations

import argparse
import copy
import json
import sys
import tempfile
from pathlib import Path
from urllib.parse import urlsplit

API_AUDIENCE = "https://fortemi.example.org"
MCP_AUDIENCE = "https://fortemi.example.org/mcp"
EXPECTED_ROLE_SCOPES = {
    "fortemi-read": {"read"},
    "fortemi-write": {"read", "write"},
    "fortemi-admin": {"read", "write", "admin"},
    "fortemi-mcp": {"mcp"},
}
PUBLIC_CLIENTS = ("fortemi-cli", "claude-code", "claude-desktop", "cursor", "vscode")
SERVICE_CLIENT = "fortemi-service-example"
RESOURCE_CLIENT = "fortemi"
# IdP user attributes a member can edit themselves; the tenant attribute must
# never be one of these (SR-19). The reference realm uses a custom attribute
# whose edit permission the operator restricts to realm admins.
USER_EDITABLE_ATTRIBUTES = {
    "username",
    "email",
    "firstName",
    "first_name",
    "lastName",
    "last_name",
}
LOOPBACK_HOSTS = {"localhost", "127.0.0.1", "[::1]"}


def load_json(path: Path) -> tuple[dict | None, str | None]:
    try:
        return json.loads(path.read_text(encoding="utf-8")), None
    except FileNotFoundError:
        return None, f"missing file: {path}"
    except (OSError, ValueError) as error:
        return None, f"cannot parse {path}: {error}"


def find_client(realm: dict, client_id: str) -> dict | None:
    for client in realm.get("clients", []):
        if isinstance(client, dict) and client.get("clientId") == client_id:
            return client
    return None


def audience_mappers(realm: dict) -> list[dict]:
    mappers = []
    for scope in realm.get("clientScopes", []):
        if not isinstance(scope, dict):
            continue
        for mapper in scope.get("protocolMappers", []):
            if isinstance(mapper, dict) and mapper.get("protocolMapper") == "oidc-audience-mapper":
                mappers.append({"scope": scope.get("name"), **mapper})
    return mappers


def anonymous_policies(realm: dict) -> list[dict]:
    components = realm.get("components", {})
    policies = components.get(
        "org.keycloak.services.clientregistration.policy.ClientRegistrationPolicy", []
    )
    return [p for p in policies if isinstance(p, dict) and p.get("subType") == "anonymous"]


def policy_values(policy: dict) -> list[str]:
    values: list[str] = []

    def walk(node: object, in_description: bool = False) -> None:
        if isinstance(node, dict):
            for key, value in node.items():
                walk(value, in_description or key in ("description", "name"))
        elif isinstance(node, list):
            for item in node:
                walk(item, in_description)
        elif isinstance(node, str) and not in_description:
            values.append(node)

    walk(policy.get("config", {}))
    return values


def check_lifetimes(realm: dict) -> list[str]:
    errors = []
    lifespan = realm.get("accessTokenLifespan")
    if not isinstance(lifespan, int) or not 300 <= lifespan <= 900:
        errors.append(f"SR-45/RR-3: accessTokenLifespan must be 300..900 s, got {lifespan!r}")
    idle = realm.get("ssoSessionIdleTimeout")
    if not isinstance(idle, int) or idle > 28800:
        errors.append(f"SR-45: ssoSessionIdleTimeout must be at most 8 h, got {idle!r}")
    device = realm.get("oauth2DeviceCodeLifespan")
    if not isinstance(device, int) or device > 600:
        errors.append(f"SR-42: oauth2DeviceCodeLifespan must be at most 600 s, got {device!r}")
    if realm.get("revokeRefreshToken") is not True:
        errors.append("SR-45: revokeRefreshToken must be true (rotation with reuse detection)")
    defaults = realm.get("defaultDefaultClientScopes", [])
    if "offline_access" in defaults:
        errors.append("SR-45: offline_access must not be a default client scope")
    return errors


def check_resource_client(realm: dict) -> list[str]:
    errors = []
    client = find_client(realm, RESOURCE_CLIENT)
    if client is None:
        return ["SR-17: resource client 'fortemi' is missing"]
    if client.get("publicClient") is not False:
        errors.append("SR-17: resource client 'fortemi' must not be public (bearer-only)")
    for flag in ("standardFlowEnabled", "implicitFlowEnabled", "directAccessGrantsEnabled"):
        if client.get(flag) is not False:
            errors.append(f"SR-17: resource client 'fortemi' must disable {flag}")
    if client.get("serviceAccountsEnabled") is not False:
        errors.append("SR-17: resource client 'fortemi' must not enable service accounts")
    roles = (realm.get("roles", {}).get("client", {}) or {}).get(RESOURCE_CLIENT, [])
    names = {r.get("name") for r in roles if isinstance(r, dict)}
    for role in EXPECTED_ROLE_SCOPES:
        if role not in names:
            errors.append(f"SR-17: client role '{role}' is missing on resource client 'fortemi'")
    return errors


def check_audience_scopes(realm: dict) -> list[str]:
    errors = []
    scopes = {s.get("name"): s for s in realm.get("clientScopes", []) if isinstance(s, dict)}
    for scope_name, audience in (("fortemi-api", API_AUDIENCE), ("fortemi-mcp", MCP_AUDIENCE)):
        scope = scopes.get(scope_name)
        if scope is None:
            errors.append(f"SR-41/MCP: client scope '{scope_name}' is missing")
            continue
        audiences = [
            m.get("config", {}).get("included.custom.audience")
            for m in scope.get("protocolMappers", [])
            if isinstance(m, dict) and m.get("protocolMapper") == "oidc-audience-mapper"
        ]
        if audience not in audiences:
            errors.append(
                f"SR-41/MCP: scope '{scope_name}' lacks an Audience mapper for {audience}"
            )
    return errors


def check_groups(realm: dict) -> list[str]:
    errors = []
    groups = {g.get("path"): g for g in realm.get("groups", []) if isinstance(g, dict)}
    users = groups.get("/fortemi-users")
    if users is None:
        errors.append("SR-17: group '/fortemi-users' is missing")
    else:
        roles = set((users.get("clientRoles", {}) or {}).get(RESOURCE_CLIENT, []))
        for role in ("fortemi-read", "fortemi-write", "fortemi-mcp"):
            if role not in roles:
                errors.append(f"SR-17: group '/fortemi-users' must map '{role}'")
        if "fortemi-admin" in roles:
            errors.append("SR-17: group '/fortemi-users' must not map 'fortemi-admin'")
    admins = groups.get("/fortemi-admins")
    if admins is None:
        errors.append("SR-17: group '/fortemi-admins' is missing")
    elif "fortemi-admin" not in set((admins.get("clientRoles", {}) or {}).get(RESOURCE_CLIENT, [])):
        errors.append("SR-17: group '/fortemi-admins' must map 'fortemi-admin'")
    return errors


def check_tenant_mapper(realm: dict) -> list[str]:
    errors = []
    found = False
    for scope in realm.get("clientScopes", []):
        if not isinstance(scope, dict):
            continue
        for mapper in scope.get("protocolMappers", []):
            if not isinstance(mapper, dict):
                continue
            kind = mapper.get("protocolMapper")
            config = mapper.get("config", {})
            if kind == "oidc-hardcoded-claim-mapper" and config.get("claim.name") == "fortemi_tenant_id":
                found = True
            if kind == "oidc-usermodel-attribute-mapper" and (
                config.get("claim.name") == "fortemi_tenant_id"
            ):
                found = True
                attribute = config.get("user.attribute", "")
                if attribute in USER_EDITABLE_ATTRIBUTES:
                    errors.append(
                        f"SR-19: tenant attribute '{attribute}' is user-editable; "
                        "source it from an admin-managed attribute"
                    )
                if config.get("access.token.claim") != "true":
                    errors.append("SR-19: tenant mapper must add the claim to access tokens")
    if not found:
        errors.append("SR-19: no tenant-claim mapper for 'fortemi_tenant_id' found")
    return errors


def is_loopback_redirect(uri: str) -> bool:
    try:
        parts = urlsplit(uri.replace("*", "wildcard", 1) if "*" in uri else uri)
    except ValueError:
        return False
    if parts.scheme != "http" or "@" in uri.split("://", 1)[1].split("/", 1)[0]:
        return False
    host = parts.hostname or ""
    return host in LOOPBACK_HOSTS and parts.fragment == ""


def check_public_client(realm: dict, client_id: str) -> list[str]:
    errors = []
    client = find_client(realm, client_id)
    if client is None:
        return [f"SR-41/SR-42: pre-registered client '{client_id}' is missing"]
    if client.get("publicClient") is not True:
        errors.append(f"SR-42: client '{client_id}' must be public")
    if client.get("standardFlowEnabled") is not True:
        errors.append(f"SR-42: client '{client_id}' must enable the standard flow")
    for flag in ("implicitFlowEnabled", "directAccessGrantsEnabled", "serviceAccountsEnabled"):
        if client.get(flag) is not False:
            errors.append(f"SR-42: client '{client_id}' must disable {flag}")
    if client.get("consentRequired") is not True:
        errors.append(f"SR-42: client '{client_id}' must require consent")
    if client.get("fullScopeAllowed") is not False:
        errors.append(f"SR-42: client '{client_id}' must not allow full scope")
    if client.get("attributes", {}).get("pkce.code.challenge.method") != "S256":
        errors.append(f"SR-42: client '{client_id}' must enforce PKCE S256")
    defaults = client.get("defaultClientScopes", [])
    if "offline_access" in defaults:
        errors.append(f"SR-45: client '{client_id}' must not default to offline_access")
    if any("admin" in str(scope).lower() for scope in defaults):
        errors.append(f"SR-42: client '{client_id}' must not default to an admin scope")
    return errors


def check_cli_client(realm: dict) -> list[str]:
    errors = check_public_client(realm, "fortemi-cli")
    client = find_client(realm, "fortemi-cli")
    if client is None:
        return errors
    grant = client.get("oauth2DeviceAuthorizationGrantEnabled", client.get("attributes", {}).get(
        "oauth2.device.authorization.grant.enabled"))
    if grant not in (True, "true"):
        errors.append("SR-42: client 'fortemi-cli' must enable the device authorization grant")
    for uri in client.get("redirectUris", []):
        if not is_loopback_redirect(uri):
            errors.append(f"SR-42: client 'fortemi-cli' redirect must be loopback, got {uri!r}")
    if not client.get("redirectUris"):
        errors.append("SR-42: client 'fortemi-cli' must register a loopback redirect")
    device = realm.get("oauth2DeviceCodeLifespan")
    if not isinstance(device, int) or device > 600:
        errors.append("SR-42: device user-code lifespan must be at most 600 s")
    return errors


def check_pre_registered_clients(realm: dict) -> list[str]:
    errors = []
    for client_id in ("claude-code", "vscode"):
        client = find_client(realm, client_id)
        for error in check_public_client(realm, client_id):
            errors.append(error)
        if client is not None:
            for uri in client.get("redirectUris", []):
                if not is_loopback_redirect(uri):
                    errors.append(f"SR-41: client '{client_id}' redirect must be loopback, got {uri!r}")
    desktop = find_client(realm, "claude-desktop")
    errors.extend(check_public_client(realm, "claude-desktop"))
    if desktop is not None and desktop.get("redirectUris") != [
        "https://claude.ai/api/mcp/auth_callback"
    ]:
        errors.append(
            "SR-41: client 'claude-desktop' must redirect to "
            "https://claude.ai/api/mcp/auth_callback"
        )
    cursor = find_client(realm, "cursor")
    errors.extend(check_public_client(realm, "cursor"))
    if cursor is not None and "CONFIRM" not in cursor.get("description", ""):
        errors.append(
            "SR-41: client 'cursor' redirect is unverified; keep the CONFIRM marker "
            "until the vendor redirect is confirmed"
        )
    return errors


# Keycloak 26.7 ClientRepresentation fields. Anything else fails the realm
# import outright ("Unrecognized field"); settings such as PKCE, the device
# grant and consent-screen display are client *attributes*.
CLIENT_FIELDS = set("""enabled clientAuthenticatorType redirectUris clientId
authenticationFlowBindingOverrides authorizationServicesEnabled name implicitFlowEnabled
registeredNodes nodeReRegistrationTimeout publicClient attributes protocol webOrigins
protocolMappers id baseUrl surrogateAuthRequired adminUrl fullScopeAllowed frontchannelLogout
clientTemplate origin defaultClientScopes directGrantsOnly rootUrl secret useTemplateMappers
notBefore useTemplateScope standardFlowEnabled type description directAccessGrantsEnabled
alwaysDisplayInConsole useTemplateConfig serviceAccountsEnabled optionalClientScopes
consentRequired access bearerOnly registrationAccessToken defaultRoles
authorizationSettings""".split())
BUILTIN_SCOPES_REQUIRED = {"basic", "roles"}


def check_keycloak_representation(realm: dict) -> list[str]:
    """Checks learned from importing the realm into Keycloak 26.7.3."""
    errors = []
    for client in realm.get("clients", []):
        unknown = sorted(set(client) - CLIENT_FIELDS)
        if unknown:
            errors.append(f"import: client '{client.get('clientId')}' has unknown fields {unknown}")
    scope_names = {scope.get("name") for scope in realm.get("clientScopes", [])}
    missing = sorted(BUILTIN_SCOPES_REQUIRED - scope_names)
    if missing:
        errors.append(f"import: built-in client scopes {missing} must be defined (basic emits sub, roles emits resource_access)")
    for client in realm.get("clients", []):
        if client.get("bearerOnly"):
            continue
        defaults = set(client.get("defaultClientScopes", []))
        if not BUILTIN_SCOPES_REQUIRED <= defaults:
            errors.append(f"tokens: client '{client.get('clientId')}' must default to {sorted(BUILTIN_SCOPES_REQUIRED)}")
    for scope in realm.get("clientScopes", []):
        for mapper in scope.get("protocolMappers", []):
            config = mapper.get("config", {})
            if mapper.get("protocolMapper") == "oidc-audience-mapper" and config.get("included.client.audience"):
                errors.append(f"audience: scope '{scope.get('name')}' sets included.client.audience, which overrides the resource-URI audience")
            if mapper.get("protocolMapper") == "oidc-audience-resolve-mapper":
                errors.append(f"audience: scope '{scope.get('name')}' has an audience-resolve mapper; it adds client ids to aud, which Fortemi rejects (SR-3)")
    mapped = {entry.get("clientScope") for entries in realm.get("clientScopeMappings", {}).values() for entry in entries}
    for name in ("fortemi-api", "fortemi-mcp"):
        if name not in mapped:
            errors.append(f"roles: client scope '{name}' needs Fortemi role scope mappings, or roles never reach tokens when fullScopeAllowed is false")
    return errors


def check_registration_policies(realm: dict) -> list[str]:
    errors = []
    policies = anonymous_policies(realm)
    if not policies:
        return errors  # anonymous DCR disabled satisfies SR-41
    by_provider = {p.get("providerId"): p for p in policies}
    trusted = by_provider.get("trusted-hosts")
    if trusted is None:
        errors.append("SR-41: anonymous DCR lacks a trusted-hosts policy")
    else:
        config = trusted.get("config", {})
        if config.get("host-sending-registration-request-must-match") != ["true"]:
            errors.append("SR-41: trusted-hosts policy must require host match")
        if config.get("trusted-hosts") not in ([], [""]):
            errors.append("SR-41: trusted-hosts policy must leave trusted hosts empty")
    if "consent-required" not in by_provider:
        errors.append("SR-41: anonymous DCR must require consent")
    max_clients = by_provider.get("max-clients")
    if max_clients is None:
        errors.append("SR-41: anonymous DCR lacks a max-clients policy")
    else:
        try:
            limit = int(max_clients.get("config", {}).get("max-clients", [""])[0])
        except (ValueError, IndexError):
            limit = -1
        if limit <= 0 or limit > 100:
            errors.append(f"SR-41: max-clients must bound anonymous registration, got {limit!r}")
    for policy in policies:
        for value in policy_values(policy):
            if "admin" in value.lower():
                errors.append(
                    f"SR-41: anonymous DCR policy '{policy.get('name')}' must exclude "
                    f"admin scopes (found {value!r})"
                )
    return errors


def check_service_client(realm: dict) -> list[str]:
    errors = []
    client = find_client(realm, SERVICE_CLIENT)
    if client is None:
        return ["SR-24: service client 'fortemi-service-example' is missing"]
    if client.get("publicClient") is not False:
        errors.append("SR-24: service client must be confidential")
    if client.get("clientAuthenticatorType") not in ("client-secret", "private-key-jwt"):
        errors.append("SR-24: service client needs a secret or private-key-jwt authenticator")
    if client.get("serviceAccountsEnabled") is not True:
        errors.append("SR-24: service client must enable service accounts")
    for flag in ("standardFlowEnabled", "implicitFlowEnabled", "directAccessGrantsEnabled"):
        if client.get(flag) is not False:
            errors.append(f"SR-24: service client must disable {flag}")
    if client.get("authorizationServicesEnabled") is not False:
        errors.append("SR-24: service client must not enable authorization services")
    if "authorizationSettings" in client:
        errors.append("SR-24: service client must not carry authorization (exchange) settings")
    text = json.dumps(
        {k: v for k, v in client.items() if k not in ("description", "name")},
        sort_keys=True,
    ).lower()
    if "token-exchange" in text or "impersonat" in text:
        errors.append("SR-24: service client enables token exchange or impersonation")
    return errors


def claim_path_segments(claim: object) -> list[str] | None:
    if not isinstance(claim, str) or not claim:
        return None
    if claim.startswith("/"):
        return [segment for segment in claim.split("/") if segment != ""]
    return claim.split(".")


def check_claim_policy(policy: dict, realm: dict) -> list[str]:
    errors = []
    if policy.get("scope_source") != "mapping":
        errors.append("claim-policy: scope_source must be 'mapping'")
    if policy.get("allow_token_scopes") is not False:
        errors.append("claim-policy: allow_token_scopes must be false with scope_source=mapping")
    if set(policy.get("scope_vocabulary", [])) != {"read", "write", "admin", "mcp"}:
        errors.append("claim-policy: scope_vocabulary must be exactly read/write/admin/mcp")
    mapping = policy.get("scope_mapping", {})
    segments = claim_path_segments(mapping.get("claim"))
    if segments != ["resource_access", RESOURCE_CLIENT, "roles"]:
        errors.append(
            "SR-17: scope_mapping.claim must be resource_access.fortemi.roles "
            "(client roles, never bare group names)"
        )
    rules = {(r.get("value")): set(r.get("scopes", [])) for r in mapping.get("rules", [])}
    for value, scopes in EXPECTED_ROLE_SCOPES.items():
        if rules.get(value) != scopes:
            errors.append(
                f"SR-17: rule for '{value}' must grant exactly {sorted(scopes)}, "
                f"got {sorted(rules.get(value, set()))}"
            )
    clients = policy.get("clients", {})
    allowed = set(clients.get("allowed", []))
    for client_id in PUBLIC_CLIENTS:
        if client_id not in allowed:
            errors.append(f"claim-policy: clients.allowed must list '{client_id}'")
    service = set(clients.get("service", []))
    if SERVICE_CLIENT not in service:
        errors.append(f"SR-24: clients.service must list '{SERVICE_CLIENT}'")
    ceilings = clients.get("ceilings", {})
    cli_ceiling = set(ceilings.get("fortemi-cli", []))
    if cli_ceiling != {"read", "write", "mcp"}:
        errors.append(
            "SR-42/SR-43: clients.ceilings['fortemi-cli'] must be exactly read/write/mcp "
            f"(no admin), got {sorted(cli_ceiling)}"
        )
    for client_id in PUBLIC_CLIENTS:
        if "admin" in set(ceilings.get(client_id, [])):
            errors.append(f"SR-42: clients.ceilings['{client_id}'] must exclude admin")
    for name in allowed | service:
        if find_client(realm, name) is None:
            errors.append(f"claim-policy: client '{name}' is not defined in the realm")
    return errors


def verify(realm_path: Path, policy_path: Path) -> list[str]:
    errors = []
    realm, error = load_json(realm_path)
    if error:
        return [error]
    policy, error = load_json(policy_path)
    if error:
        return [error]
    if not isinstance(realm, dict) or not isinstance(policy, dict):
        return ["realm and policy files must contain JSON objects"]
    if realm.get("realm") != "example" or not realm.get("enabled"):
        errors.append("reference realm must be the enabled 'example' realm")
    if realm.get("sslRequired") != "all":
        errors.append("reference realm must require SSL (sslRequired=all)")
    errors.extend(check_lifetimes(realm))
    errors.extend(check_resource_client(realm))
    errors.extend(check_audience_scopes(realm))
    errors.extend(check_groups(realm))
    errors.extend(check_tenant_mapper(realm))
    errors.extend(check_cli_client(realm))
    errors.extend(check_pre_registered_clients(realm))
    errors.extend(check_keycloak_representation(realm))
    errors.extend(check_registration_policies(realm))
    errors.extend(check_service_client(realm))
    errors.extend(check_claim_policy(policy, realm))
    return errors


SELF_TESTS: list[tuple[str, str]] = [
    ("cli ceiling grants admin", "ceilings['fortemi-cli']"),
    ("default offline_access", "offline_access"),
    ("long device code", "oauth2DeviceCodeLifespan"),
    ("missing MCP audience", "fortemi-mcp"),
    ("bare group claim path", "resource_access.fortemi.roles"),
    ("user-editable tenant attribute", "SR-19"),
    ("admin scope via anonymous DCR", "anonymous DCR"),
    ("service client with browser flow", "service client must disable"),
    ("non-loopback CLI redirect", "must be loopback"),
    ("hour access-token lifespan", "accessTokenLifespan"),
    ("refresh rotation off", "revokeRefreshToken"),
    ("token scope source", "scope_source"),
    ("cursor marker removed", "CONFIRM"),
]


def mutate(case: str, realm: dict, policy: dict) -> None:
    cli = find_client(realm, "fortemi-cli")
    assert cli is not None
    if case.startswith("cli ceiling"):
        policy["clients"]["ceilings"]["fortemi-cli"].append("admin")
    elif case.startswith("default offline"):
        realm["defaultDefaultClientScopes"].append("offline_access")
    elif case.startswith("long device"):
        realm["oauth2DeviceCodeLifespan"] = 3600
    elif case.startswith("missing MCP"):
        for scope in realm["clientScopes"]:
            if scope["name"] == "fortemi-mcp":
                scope["protocolMappers"] = []
    elif case.startswith("bare group"):
        policy["scope_mapping"]["claim"] = "groups"
    elif case.startswith("user-editable"):
        for scope in realm["clientScopes"]:
            for mapper in scope.get("protocolMappers", []):
                if mapper.get("protocolMapper") == "oidc-usermodel-attribute-mapper":
                    mapper["config"]["user.attribute"] = "email"
    elif case.startswith("admin scope via"):
        for component in realm["components"][
            "org.keycloak.services.clientregistration.policy.ClientRegistrationPolicy"
        ]:
            if component.get("providerId") == "allowed-client-templates" and component.get("subType") == "anonymous":
                component["config"].setdefault("allowed-client-scopes", []).append("fortemi-admin")
    elif case.startswith("service client"):
        find_client(realm, SERVICE_CLIENT)["standardFlowEnabled"] = True
    elif case.startswith("non-loopback"):
        cli["redirectUris"] = ["https://attacker.example/callback"]
    elif case.startswith("hour access"):
        realm["accessTokenLifespan"] = 3600
    elif case.startswith("refresh rotation"):
        realm["revokeRefreshToken"] = False
    elif case.startswith("token scope"):
        policy["scope_source"] = "token"
    elif case.startswith("cursor marker"):
        find_client(realm, "cursor")["description"] = "Cursor client."


def self_test() -> list[str]:
    failures = []
    root = Path(__file__).resolve().parents[2]
    realm_path = root / "deploy" / "identity" / "keycloak" / "realm-example.json"
    policy_path = root / "deploy" / "identity" / "claim-policy.example.json"
    realm, error = load_json(realm_path)
    if error:
        return [error]
    policy, error = load_json(policy_path)
    if error:
        return [error]
    assert isinstance(realm, dict) and isinstance(policy, dict)
    with_titles = verify(realm_path, policy_path)
    if with_titles:
        failures.append(f"known-good fixtures must pass, got: {with_titles}")
    for case, marker in SELF_TESTS:
        bad_realm = copy.deepcopy(realm)
        bad_policy = copy.deepcopy(policy)
        mutate(case, bad_realm, bad_policy)
        with tempfile.TemporaryDirectory() as tmp:
            tmp_realm = Path(tmp) / "realm.json"
            tmp_policy = Path(tmp) / "policy.json"
            tmp_realm.write_text(json.dumps(bad_realm), encoding="utf-8")
            tmp_policy.write_text(json.dumps(bad_policy), encoding="utf-8")
            errors = verify(tmp_realm, tmp_policy)
        if not errors:
            failures.append(f"self-test '{case}' passed but must fail")
        elif not any(marker in error for error in errors):
            failures.append(f"self-test '{case}' missed marker {marker!r}: {errors}")
    return failures


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", default=".")
    parser.add_argument("--realm", default="deploy/identity/keycloak/realm-example.json")
    parser.add_argument("--policy", default="deploy/identity/claim-policy.example.json")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        failures = self_test()
        if failures:
            print("Reference-realm self-tests failed:", file=sys.stderr)
            for failure in failures:
                print(f"- {failure}", file=sys.stderr)
            return 1
        print("Reference-realm self-tests passed (13 known-bad fixtures rejected)")
        return 0

    root = Path(args.root).resolve()
    errors = verify(root / args.realm, root / args.policy)
    if errors:
        print("Reference-realm verification failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print("Reference-realm verification passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
