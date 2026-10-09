from __future__ import annotations

import copy
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "ci" / "verify-reference-realm.py"
SPEC = importlib.util.spec_from_file_location("verify_reference_realm", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)

REALM_PATH = ROOT / "deploy" / "identity" / "keycloak" / "realm-example.json"
POLICY_PATH = ROOT / "deploy" / "identity" / "claim-policy.example.json"


class ReferenceRealmVerifierTests(unittest.TestCase):
    def setUp(self) -> None:
        realm = json.loads(REALM_PATH.read_text(encoding="utf-8"))
        policy = json.loads(POLICY_PATH.read_text(encoding="utf-8"))
        assert isinstance(realm, dict) and isinstance(policy, dict)
        self.realm = realm
        self.policy = policy

    def verify_mutated(self, case: str) -> list[str]:
        bad_realm = copy.deepcopy(self.realm)
        bad_policy = copy.deepcopy(self.policy)
        MODULE.mutate(case, bad_realm, bad_policy)
        with tempfile.TemporaryDirectory() as tmp:
            realm_path = Path(tmp) / "realm.json"
            policy_path = Path(tmp) / "policy.json"
            realm_path.write_text(json.dumps(bad_realm), encoding="utf-8")
            policy_path.write_text(json.dumps(bad_policy), encoding="utf-8")
            return MODULE.verify(realm_path, policy_path)

    def test_reference_fixtures_pass(self) -> None:
        self.assertEqual(MODULE.verify(REALM_PATH, POLICY_PATH), [])

    def test_cli_ceiling_must_exclude_admin(self) -> None:
        errors = self.verify_mutated("cli ceiling grants admin")
        self.assertTrue(any("fortemi-cli" in error for error in errors))

    def test_bare_group_claim_path_rejected(self) -> None:
        errors = self.verify_mutated("bare group claim path")
        self.assertTrue(any("SR-17" in error for error in errors))

    def test_user_editable_tenant_attribute_rejected(self) -> None:
        errors = self.verify_mutated("user-editable tenant attribute")
        self.assertTrue(any("SR-19" in error for error in errors))

    def test_service_client_browser_flow_rejected(self) -> None:
        errors = self.verify_mutated("service client with browser flow")
        self.assertTrue(any("SR-24" in error for error in errors))

    def test_admin_scope_via_anonymous_dcr_rejected(self) -> None:
        errors = self.verify_mutated("admin scope via anonymous DCR")
        self.assertTrue(any("SR-41" in error for error in errors))

    def test_missing_files_reported(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            missing = Path(tmp) / "absent.json"
            self.assertTrue(any("missing" in error for error in MODULE.verify(missing, POLICY_PATH)))
            self.assertTrue(any("missing" in error for error in MODULE.verify(REALM_PATH, missing)))

    def test_self_test_suite_passes(self) -> None:
        self.assertEqual(MODULE.self_test(), [])


if __name__ == "__main__":
    unittest.main()
