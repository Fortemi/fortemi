#!/usr/bin/env python3
"""Generate the schema 2.0 Knowledge Shard presence-field inventory."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any


COMPONENT_NAMES = {
    "collection": "collections",
    "community-assignment": "community_assignments",
    "community-set": "communities",
    "embedding-config": "embedding_configs",
    "embedding-set-member": "embedding_set_members",
    "embedding-set": "embedding_sets",
    "embedding": "embeddings",
    "graph-edge": "graph_edges",
    "graph-source": "graph_sources",
    "link": "links",
    "named-location": "named_locations",
    "note-original-history": "note_original_history",
    "note-original": "note_originals",
    "note-revised-current": "note_revised_current",
    "note-revision": "note_revisions",
    "note-skos-tag": "note_skos_tags",
    "note": "notes",
    "provenance-activity": "provenance_activities",
    "provenance-device": "provenance_devices",
    "provenance-edge": "provenance_edges",
    "provenance-location": "provenance_locations",
    "provenance-record": "provenance_records",
    "signature": "signature",
    "skos-collection-member": "skos_collection_members",
    "skos-collection": "skos_collections",
    "skos-concept": "skos_concepts",
    "skos-label": "skos_labels",
    "skos-mapping-relation": "skos_mapping_relations",
    "skos-note": "skos_notes",
    "skos-relation": "skos_relations",
    "skos-scheme-membership": "skos_scheme_memberships",
    "skos-scheme": "skos_schemes",
    "tag": "tags",
    "template": "templates",
    "manifest": "manifest",
}


def normalize_schema(schema: Any) -> dict[str, Any]:
    if schema is True:
        return {}
    if schema is False:
        return {"not": {}}
    return schema if isinstance(schema, dict) else {}


def resolve(schema: Any, root: dict[str, Any]) -> dict[str, Any]:
    schema = normalize_schema(schema)
    ref = schema.get("$ref")
    if not isinstance(ref, str) or not ref.startswith("#/"):
        return schema
    node: Any = root
    for part in ref[2:].split("/"):
        node = node[part.replace("~1", "/").replace("~0", "~")]
    return node


def branches(schema: dict[str, Any], root: dict[str, Any]) -> list[dict[str, Any]]:
    schema = resolve(schema, root)
    variants = schema.get("oneOf") or schema.get("anyOf")
    if isinstance(variants, list):
        return [resolve(item, root) for item in variants if isinstance(item, dict)]
    return [schema]


def accepted_types(schema: dict[str, Any], root: dict[str, Any]) -> set[str]:
    def json_type(value: Any) -> str:
        if value is None:
            return "null"
        if isinstance(value, bool):
            return "boolean"
        if isinstance(value, str):
            return "string"
        if isinstance(value, list):
            return "array"
        if isinstance(value, dict):
            return "object"
        return "number"

    found: set[str] = set()
    for item in branches(schema, root):
        value = item.get("type")
        if isinstance(value, str):
            found.add(value)
        elif isinstance(value, list):
            found.update(part for part in value if isinstance(part, str))
        elif "const" in item:
            found.add(json_type(item["const"]))
        elif "enum" in item:
            for enum_value in item["enum"]:
                found.add(json_type(enum_value))
        else:
            found.update({"null", "string", "array", "object", "number", "boolean"})
    return found


def empty_forms(schema: dict[str, Any], root: dict[str, Any]) -> list[str]:
    allowed: set[str] = set()
    for item in branches(schema, root):
        types = accepted_types(item, root)
        if "string" in types and item.get("minLength", 0) == 0 and not item.get("format"):
            if "pattern" not in item and "enum" not in item and "const" not in item:
                allowed.add("empty-string")
        if "array" in types and item.get("minItems", 0) == 0:
            allowed.add("empty-array")
        if "object" in types and item.get("minProperties", 0) == 0:
            allowed.add("empty-object")
    return sorted(allowed)


def escape_pointer(value: str) -> str:
    return value.replace("~", "~0").replace("/", "~1")


def iter_fields(
    schema: dict[str, Any],
    root: dict[str, Any],
    pointer: str = "",
    seen: frozenset[int] = frozenset(),
) -> list[dict[str, Any]]:
    schema = resolve(schema, root)
    identity = id(schema)
    if identity in seen:
        return []
    seen = seen | {identity}
    output: list[dict[str, Any]] = []
    properties = schema.get("properties")
    if isinstance(properties, dict):
        required = set(schema.get("required", []))
        for name, raw_child in sorted(properties.items()):
            child = normalize_schema(raw_child)
            child_pointer = f"{pointer}/{escape_pointer(name)}"
            types = accepted_types(child, root)
            is_required = name in required
            is_nullable = "null" in types
            if not is_required or is_nullable:
                non_null_types = sorted(types - {"null"})
                output.append(
                    {
                        "pointer": child_pointer,
                        "required": is_required,
                        "types": non_null_types,
                        "states": {
                            "absent": "reject" if is_required else "preserve",
                            "null": "preserve" if is_nullable else "reject",
                            "empty": empty_forms(child, root),
                            "value": "preserve" if non_null_types else "reject",
                            "unsupported": "reject-before-write-or-report-profile-loss",
                        },
                        "equality": "json-type-value-and-own-property",
                    }
                )
            for variant in branches(child, root):
                output.extend(iter_fields(variant, root, child_pointer, seen))
                items = variant.get("items")
                if isinstance(items, dict):
                    output.extend(iter_fields(items, root, f"{child_pointer}/*", seen))
    return output


def mapping_status(profile: str) -> dict[str, dict[str, str]]:
    return {
        "server": {
            "status": "authority-specified-runtime-pending",
            "trackingIssue": "https://git.integrolabs.net/Fortemi/fortemi/issues/1083",
        },
        "pglite": {
            "status": "implementation-pending" if profile in {"core-v1", "full-v1"} else "not-profile-owner",
            "trackingIssue": "https://git.integrolabs.net/Fortemi/fortemi-react/issues/379",
        },
        "recordstore": {
            "status": "implementation-pending" if profile == "record-v1" else "not-profile-owner",
            "trackingIssue": "https://git.integrolabs.net/Fortemi/fortemi-react/issues/379",
        },
        "aiwg": {
            "status": "converter-pending",
            "trackingIssue": "https://git.integrolabs.net/Fortemi/fortemi-react/issues/381",
        },
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path("contracts/knowledge-shard/2.0.0"))
    parser.add_argument("--output", type=Path)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    output = args.output or args.root / "field-semantics.json"

    fields: list[dict[str, Any]] = []
    for path in sorted(args.root.glob("*-v1/*.schema.json")):
        schema = json.loads(path.read_text())
        profile = path.parent.name
        component = COMPONENT_NAMES[path.name.removesuffix(".schema.json")]
        for field in iter_fields(schema, schema):
            fields.append(
                {
                    "profile": profile,
                    "component": component,
                    "schema": str(path),
                    **field,
                    "mappings": mapping_status(profile),
                }
            )

    document = {
        "$schema": "./field-semantics.schema.json",
        "schemaVersion": 1,
        "authoritySchemaVersion": "2.0.0",
        "profileIdentity": "(manifest.version, manifest.profile)",
        "wireRepresentation": "direct-json-key-presence",
        "policy": {
            "absent": "The property key is omitted; supported only when the schema does not require it.",
            "null": "The property key exists with JSON null; supported only when the schema admits null.",
            "empty": "The property key exists with an allowed zero-length string, array, or object.",
            "value": "The property key exists with a schema-valid non-empty typed value.",
            "unsupported": "Reject before writes; reduced profiles may convert only with a deterministic machine-readable loss report.",
        },
        "fields": fields,
    }
    rendered = json.dumps(document, indent=2, sort_keys=False) + "\n"
    if args.check:
        if not output.exists() or output.read_text() != rendered:
            raise SystemExit(f"{output} is stale; regenerate it")
    else:
        output.write_text(rendered)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
