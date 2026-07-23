#!/usr/bin/env python3
"""Verify the Knowledge Shard 2.0 presence inventory and canonical vectors."""

from __future__ import annotations

import copy
import hashlib
import json
import sys
from pathlib import Path
from typing import Any

from jsonschema import Draft202012Validator, FormatChecker


ROOT = Path(__file__).resolve().parents[2]
INVENTORY = ROOT / "contracts/knowledge-shard/2.0.0/field-semantics.json"
FIXTURES = ROOT / "tests/fixtures/shards/presence-semantics-v2.0.json"
CONTRACT = ROOT / "contracts/knowledge-shard/2.0.0/contract.json"


def pointer_parts(pointer: str) -> list[str]:
    if not pointer.startswith("/"):
        raise ValueError(f"invalid JSON Pointer: {pointer}")
    return [part.replace("~1", "/").replace("~0", "~") for part in pointer[1:].split("/")]


def parent_and_key(document: dict[str, Any], pointer: str) -> tuple[dict[str, Any], str]:
    parts = pointer_parts(pointer)
    parent: Any = document
    for part in parts[:-1]:
        parent = parent[part]
    if not isinstance(parent, dict):
        raise ValueError(f"fixture pointer parent is not an object: {pointer}")
    return parent, parts[-1]


def classify(document: dict[str, Any], pointer: str) -> tuple[str, Any]:
    parent, key = parent_and_key(document, pointer)
    if key not in parent:
        return "absent", None
    value = parent[key]
    if value is None:
        return "null", None
    if value == "" or value == [] or value == {}:
        return "empty", value
    return "value", value


def apply_case(prototype: dict[str, Any], case: dict[str, Any]) -> dict[str, Any]:
    document = copy.deepcopy(prototype)
    parent, key = parent_and_key(document, case["pointer"])
    if case["operation"] == "delete":
        parent.pop(key, None)
    elif case["operation"] == "set":
        parent[key] = copy.deepcopy(case["value"])
    else:
        raise ValueError(f"unsupported fixture operation: {case['operation']}")
    return document


def inventory_key(case: dict[str, Any]) -> tuple[str, str, str]:
    path = Path(case["schema"])
    profile = path.parent.name
    component = path.name.removesuffix(".schema.json")
    aliases = {
        "embedding": "embeddings",
        "manifest": "manifest",
        "note": "notes",
    }
    return profile, aliases[component], case["pointer"]


def inventory_allows(field: dict[str, Any], state: str, value: Any) -> bool:
    states = field["states"]
    if state == "empty":
        kind = "empty-string" if value == "" else "empty-array" if value == [] else "empty-object"
        return kind in states["empty"]
    return states[state] == "preserve"


def main() -> int:
    inventory = json.loads(INVENTORY.read_text())
    fixtures = json.loads(FIXTURES.read_text())
    contract = json.loads(CONTRACT.read_text())
    previous_contract = json.loads((ROOT / "contracts/knowledge-shard/contract.json").read_text())
    fields = {
        (field["profile"], field["component"], field["pointer"]): field
        for field in inventory["fields"]
    }
    failures: list[str] = []
    observed = {"absent": 0, "null": 0, "empty": 0, "value": 0}

    for case in fixtures["cases"]:
        document = apply_case(fixtures["prototypes"][case["prototype"]], case)
        state, value = classify(document, case["pointer"])
        observed[state] += 1
        if state != case["state"]:
            failures.append(f"{case['id']}: classified {state}, expected {case['state']}")

        round_tripped = json.loads(json.dumps(document, separators=(",", ":"), sort_keys=True))
        round_state, round_value = classify(round_tripped, case["pointer"])
        if (round_state, round_value) != (state, value):
            failures.append(f"{case['id']}: JSON round trip changed own-property state or value")

        schema = json.loads((ROOT / case["schema"]).read_text())
        errors = list(Draft202012Validator(schema, format_checker=FormatChecker()).iter_errors(document))
        actual_valid = not errors
        if actual_valid != case["valid"]:
            detail = "valid" if actual_valid else errors[0].message
            failures.append(f"{case['id']}: schema result {detail}, expected valid={case['valid']}")

        field = fields.get(inventory_key(case))
        if field is None:
            failures.append(f"{case['id']}: field missing from field-semantics inventory")
        elif inventory_allows(field, state, value) != case["valid"]:
            failures.append(f"{case['id']}: field inventory and fixture validity disagree")

    if not all(observed.values()):
        failures.append(f"fixture corpus does not cover all four states: {observed}")
    if len(fields) != len(inventory["fields"]):
        failures.append("field-semantics inventory contains duplicate profile/component/pointer rows")

    for document, schema_path, label in (
        (inventory, ROOT / "contracts/knowledge-shard/2.0.0/field-semantics.schema.json", "field inventory"),
        (contract, ROOT / "contracts/knowledge-shard/2.0.0/contract.schema.json", "authority descriptor"),
    ):
        schema = json.loads(schema_path.read_text())
        errors = list(Draft202012Validator(schema, format_checker=FormatChecker()).iter_errors(document))
        if errors:
            failures.append(f"{label} does not match its schema: {errors[0].message}")

    aggregate = hashlib.sha256()
    for name in sorted(contract["schemaBundle"]["files"]):
        expected = contract["schemaBundle"]["files"][name]
        payload = (ROOT / name).read_bytes()
        aggregate.update(payload)
        actual = hashlib.sha256(payload).hexdigest()
        if actual != expected:
            failures.append(f"schema bundle digest mismatch: {name}")
    if aggregate.hexdigest() != contract["schemaBundle"]["sha256"]:
        failures.append("schema bundle aggregate digest mismatch")
    fixture_digest = hashlib.sha256(FIXTURES.read_bytes()).hexdigest()
    if fixture_digest != contract["canonicalCorpus"]["sha256"]:
        failures.append("canonical presence corpus digest mismatch")

    previous_aggregate = hashlib.sha256()
    for name in sorted(previous_contract["schemaBundle"]["files"]):
        expected = previous_contract["schemaBundle"]["files"][name]
        payload = (ROOT / name).read_bytes()
        previous_aggregate.update(payload)
        if hashlib.sha256(payload).hexdigest() != expected:
            failures.append(f"released 1.x schema changed: {name}")
    if previous_aggregate.hexdigest() != previous_contract["schemaBundle"]["sha256"]:
        failures.append("released 1.x schema bundle aggregate changed")
    if contract["previousAuthority"]["schemaBundleSha256"] != previous_contract["schemaBundle"]["sha256"]:
        failures.append("2.0 descriptor does not pin the immutable previous schema bundle")

    for schema_path in sorted((ROOT / "contracts/knowledge-shard/2.0.0").rglob("*.schema.json")):
        try:
            Draft202012Validator.check_schema(json.loads(schema_path.read_text()))
        except Exception as error:
            failures.append(f"schema does not compile: {schema_path.relative_to(ROOT)}: {error}")

    for profile in ("core-v1", "record-v1", "full-v1"):
        manifest = json.loads((ROOT / f"contracts/knowledge-shard/2.0.0/{profile}/manifest.schema.json").read_text())
        if manifest["properties"]["version"].get("const") != "2.0.0":
            failures.append(f"{profile} manifest does not require version 2.0.0")
        if manifest["properties"]["profile"].get("const") != profile:
            failures.append(f"{profile} manifest profile identity changed")
        if manifest["properties"]["min_reader_version"].get("const") != "2.0.0":
            failures.append(f"{profile} manifest does not require minimum reader 2.0.0")
    if failures:
        for failure in failures:
            print(f"FAIL: {failure}", file=sys.stderr)
        return 1
    print(
        f"Knowledge Shard presence contract verified: {len(fields)} inventoried fields, "
        f"{len(fixtures['cases'])} canonical cases ({observed})."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
