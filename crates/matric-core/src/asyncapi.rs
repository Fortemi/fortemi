//! AsyncAPI 3.0 spec builder for the Fortemi SSE event catalog.
//!
//! Generates a complete AsyncAPI 3.0 document from the [`ServerEvent`] enum
//! metadata and `schemars`-derived JSON Schemas. The spec is built at runtime
//! (like the OpenAPI spec via utoipa) so it never drifts from the code.

use schemars::schema_for;
use serde_json::{json, Value};

use crate::events::{EventActor, EventEnvelope, EventPriority, ServerEvent};

/// Build a complete AsyncAPI 3.0.0 specification document.
///
/// # Arguments
/// - `version`: API version string (e.g., from `env!("CARGO_PKG_VERSION")`)
/// - `server_url`: Base URL of the SSE endpoint (e.g., `"https://your-domain.com"`)
pub fn build_asyncapi_spec(version: &str, server_url: &str) -> Value {
    let mut spec = json!({
        "asyncapi": "3.0.0",
        "info": {
            "title": "Fortemi Event Stream",
            "version": version,
            "description": "Real-time server-sent events (SSE) for the Fortemi knowledge base. Connect to `/api/v1/events` to receive domain events as they occur.",
            "license": {
                "name": "BSL-1.1",
                "url": "https://github.com/fortemi/fortemi/blob/main/LICENSE"
            }
        },
        "servers": {
            "production": {
                "host": server_url,
                "protocol": "https",
                "description": "Fortemi API server (SSE over HTTPS)"
            }
        },
        "channels": {
            "events": {
                "address": "/api/v1/events",
                "description": "Server-Sent Events stream delivering real-time domain events. Supports reconnection via `Last-Event-ID` header.",
                "parameters": {
                    "token": {
                        "description": "Bearer token for authentication (alternative to Authorization header)"
                    },
                    "memory": {
                        "description": "Memory archive to scope events to (default: all archives)"
                    },
                    "types": {
                        "description": "Comma-separated list of event types to subscribe to (e.g., `note.created,note.updated`)"
                    },
                    "entity_id": {
                        "description": "Filter events to a specific entity ID"
                    }
                },
                "messages": {}
            }
        },
        "operations": {
            "receiveEvents": {
                "action": "receive",
                "channel": { "$ref": "#/channels/events" },
                "summary": "Receive real-time domain events via SSE",
                "description": "Subscribe to the event stream. Events are delivered as SSE `data:` frames with JSON payloads wrapped in an EventEnvelope."
            }
        },
        "components": {
            "schemas": {}
        }
    });

    // Build messages from variant metadata
    let meta = ServerEvent::all_variants_metadata();
    let messages = spec["channels"]["events"]["messages"]
        .as_object_mut()
        .unwrap();

    let mut message_refs = Vec::new();

    for m in &meta {
        let message_key = m.variant_name.to_string();
        let message = json!({
            "name": m.namespaced_type,
            "title": m.variant_name,
            "summary": m.description,
            "contentType": "application/json",
            "payload": {
                "$ref": format!("#/components/schemas/EventEnvelope")
            },
            "x-event-type": m.namespaced_type,
            "x-entity-type": m.entity_type,
            "x-priority": format!("{:?}", m.priority)
        });
        messages.insert(message_key.clone(), message);
        message_refs.push(json!({
            "$ref": format!("#/channels/events/messages/{}", message_key)
        }));
    }

    // Wire message refs into the operation
    spec["operations"]["receiveEvents"]["messages"] = Value::Array(message_refs);

    // Generate schemas from schemars and remap $ref paths
    let schemas = spec["components"]["schemas"].as_object_mut().unwrap();

    let envelope_schema = remap_refs(schema_for!(EventEnvelope));
    let actor_schema = remap_refs(schema_for!(EventActor));
    let event_schema = remap_refs(schema_for!(ServerEvent));
    let priority_schema = remap_refs(schema_for!(EventPriority));

    // Insert root schemas
    insert_schema(schemas, "EventEnvelope", &envelope_schema);
    insert_schema(schemas, "EventActor", &actor_schema);
    insert_schema(schemas, "ServerEvent", &event_schema);
    insert_schema(schemas, "EventPriority", &priority_schema);

    // Hoist nested definitions from each root schema into components/schemas
    for root in [
        &envelope_schema,
        &actor_schema,
        &event_schema,
        &priority_schema,
    ] {
        let root_val = serde_json::to_value(root).unwrap();
        if let Some(defs) = root_val.get("definitions").and_then(|d| d.as_object()) {
            for (name, def) in defs {
                if !schemas.contains_key(name) {
                    schemas.insert(name.clone(), remap_refs_value(def.clone()));
                }
            }
        }
    }

    spec
}

/// Insert a schemars root schema into the schemas map, stripping `definitions`
/// and `$schema` to keep the components section clean.
fn insert_schema(
    schemas: &mut serde_json::Map<String, Value>,
    name: &str,
    schema: &schemars::schema::RootSchema,
) {
    let mut val = serde_json::to_value(&schema.schema).unwrap();
    // Remove schemars metadata that doesn't belong in AsyncAPI
    if let Some(obj) = val.as_object_mut() {
        obj.remove("$schema");
        obj.remove("definitions");
    }
    remap_refs_in_place(&mut val);
    schemas.insert(name.to_string(), val);
}

/// Remap all `#/definitions/Foo` references to `#/components/schemas/Foo`
/// in a schemars RootSchema.
fn remap_refs(mut schema: schemars::schema::RootSchema) -> schemars::schema::RootSchema {
    let mut val = serde_json::to_value(&schema).unwrap();
    remap_refs_in_place(&mut val);
    schema = serde_json::from_value(val).unwrap();
    schema
}

/// Recursively rewrite `$ref` values from schemars format to AsyncAPI format.
fn remap_refs_in_place(val: &mut Value) {
    match val {
        Value::Object(map) => {
            if let Some(Value::String(r)) = map.get_mut("$ref") {
                if r.starts_with("#/definitions/") {
                    *r = r.replace("#/definitions/", "#/components/schemas/");
                }
            }
            for v in map.values_mut() {
                remap_refs_in_place(v);
            }
        }
        Value::Array(arr) => {
            for v in arr {
                remap_refs_in_place(v);
            }
        }
        _ => {}
    }
}

/// Remap refs in an arbitrary serde_json::Value.
fn remap_refs_value(mut val: Value) -> Value {
    remap_refs_in_place(&mut val);
    val
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_spec_produces_valid_structure() {
        let spec = build_asyncapi_spec("2026.2.9", "https://example.com");

        assert_eq!(spec["asyncapi"], "3.0.0");
        assert_eq!(spec["info"]["title"], "Fortemi Event Stream");
        assert_eq!(spec["info"]["version"], "2026.2.9");
        assert_eq!(spec["info"]["license"]["name"], "BSL-1.1");

        // Server
        assert_eq!(spec["servers"]["production"]["host"], "https://example.com");
        assert_eq!(spec["servers"]["production"]["protocol"], "https");

        // Channel
        assert!(spec["channels"]["events"]["address"].as_str().unwrap() == "/api/v1/events");

        // 44 messages
        let messages = spec["channels"]["events"]["messages"]
            .as_object()
            .expect("messages should be an object");
        assert_eq!(
            messages.len(),
            44,
            "Expected 44 messages, got {}",
            messages.len()
        );

        // Operation references all 44 messages
        let op_msgs = spec["operations"]["receiveEvents"]["messages"]
            .as_array()
            .expect("operation messages should be an array");
        assert_eq!(op_msgs.len(), 44);

        // Schemas present
        let schemas = spec["components"]["schemas"]
            .as_object()
            .expect("schemas should be an object");
        assert!(
            schemas.contains_key("EventEnvelope"),
            "Missing EventEnvelope schema"
        );
        assert!(
            schemas.contains_key("EventActor"),
            "Missing EventActor schema"
        );
        assert!(
            schemas.contains_key("ServerEvent"),
            "Missing ServerEvent schema"
        );
        assert!(
            schemas.contains_key("EventPriority"),
            "Missing EventPriority schema"
        );
    }

    #[test]
    fn schemas_use_asyncapi_refs() {
        let spec = build_asyncapi_spec("1.0.0", "https://example.com");
        let spec_str = serde_json::to_string_pretty(&spec).unwrap();

        // No leftover schemars-style refs
        assert!(
            !spec_str.contains("#/definitions/"),
            "Found leftover #/definitions/ ref in spec:\n{}",
            spec_str
        );
    }

    #[test]
    fn spec_serializes_to_yaml() {
        let spec = build_asyncapi_spec("2026.2.9", "https://example.com");
        let yaml = serde_yaml::to_string(&spec).expect("YAML serialization must succeed");

        assert!(yaml.contains("asyncapi: 3.0.0"));
        assert!(yaml.contains("Fortemi Event Stream"));
        assert!(yaml.contains("/api/v1/events"));
    }

    #[test]
    fn messages_have_extension_fields() {
        let spec = build_asyncapi_spec("1.0.0", "https://example.com");
        let messages = spec["channels"]["events"]["messages"].as_object().unwrap();

        // Check a specific message
        let note_created = &messages["NoteCreated"];
        assert_eq!(note_created["x-event-type"], "note.created");
        assert_eq!(note_created["x-entity-type"], "note");
        assert_eq!(note_created["x-priority"], "Critical");

        let queue_status = &messages["QueueStatus"];
        assert_eq!(queue_status["x-event-type"], "queue.status");
        assert!(queue_status["x-entity-type"].is_null());
        assert_eq!(queue_status["x-priority"], "Low");
    }

    #[test]
    fn channel_has_query_parameters() {
        let spec = build_asyncapi_spec("1.0.0", "https://example.com");
        let params = spec["channels"]["events"]["parameters"]
            .as_object()
            .expect("parameters should be an object");

        assert!(params.contains_key("token"), "Missing token parameter");
        assert!(params.contains_key("memory"), "Missing memory parameter");
        assert!(params.contains_key("types"), "Missing types parameter");
        assert!(
            params.contains_key("entity_id"),
            "Missing entity_id parameter"
        );
    }
}
