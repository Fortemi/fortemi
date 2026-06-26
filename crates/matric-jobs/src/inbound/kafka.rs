//! Kafka inbound connector (#836) — feature-gated (`kafka`), high-end tier only.
//!
//! Consumes a Kafka topic via a consumer group with manual offset commit
//! (at-least-once): offsets are committed only after the supervisor's durable
//! outbox write. On restart the consumer resumes from the last committed offset
//! (the broker stores per-group offsets); `start_offset` only applies when no
//! committed offset exists.
//!
//! Gating is two-layered per the Phase D cost-gate:
//! - **Compile time:** behind the `kafka` Cargo feature (off by default) so
//!   edge/default builds never compile librdkafka.
//! - **Runtime:** the API registers the `kafka` kind only when
//!   `INBOUND_KAFKA_ENABLED=true`.
//!
//! DLQ is two-pronged: processing-failure poison events are dead-lettered to the
//! shared `inbound_dlq` table by the supervisor (consistent with the other
//! connectors); additionally, a non-UTF-8 (undecodable) message is routed to the
//! optional `dead_letter_topic` (if configured) and skipped at the connector.

use async_trait::async_trait;
use rdkafka::config::ClientConfig;
use rdkafka::consumer::{CommitMode, Consumer, StreamConsumer};
use rdkafka::message::Message;
use rdkafka::producer::{FutureProducer, FutureRecord};
use rdkafka::topic_partition_list::{Offset as RdOffset, TopicPartitionList};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::time::Duration;
use tracing::{info, warn};

use super::source::{
    inbound_error_reason_code, telemetry_destination_class, telemetry_text_len, InboundError,
    InboundEvent, InboundEventSource, InboundResult, Offset,
};

/// Connector config, deserialized from the `inbound_source.config` JSONB.
#[derive(Clone, Deserialize)]
pub struct KafkaConfig {
    pub bootstrap_servers: String,
    pub topic: String,
    pub group_id: String,
    /// `auto.offset.reset` when no committed offset exists: "earliest"/"latest".
    #[serde(default = "default_start_offset")]
    pub start_offset: String,
    /// e.g. "PLAINTEXT", "SSL", "SASL_SSL", "SASL_PLAINTEXT".
    #[serde(default)]
    pub security_protocol: Option<String>,
    /// e.g. "PLAIN", "SCRAM-SHA-256", "SCRAM-SHA-512".
    #[serde(default)]
    pub sasl_mechanism: Option<String>,
    #[serde(default)]
    pub sasl_username: Option<String>,
    #[serde(default)]
    pub sasl_password: Option<String>,
    #[serde(default)]
    pub ssl_ca_location: Option<String>,
    /// Optional Kafka topic for undecodable (non-UTF-8) messages.
    #[serde(default)]
    pub dead_letter_topic: Option<String>,
    /// JSON field (within a parsed object payload) used as the outbox
    /// `event_type`; falls back to `default_event_type`.
    #[serde(default = "default_event_type_field")]
    pub event_type_field: String,
    #[serde(default = "default_event_type")]
    pub default_event_type: String,
    /// Passthrough librdkafka properties (advanced tuning).
    #[serde(default)]
    pub extra: BTreeMap<String, String>,
}

impl std::fmt::Debug for KafkaConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KafkaConfig")
            .field(
                "bootstrap_servers_class",
                &telemetry_destination_class(&self.bootstrap_servers),
            )
            .field(
                "bootstrap_servers_len",
                &telemetry_text_len(&self.bootstrap_servers),
            )
            .field("topic_len", &telemetry_text_len(&self.topic))
            .field("group_id_len", &telemetry_text_len(&self.group_id))
            .field("start_offset_len", &telemetry_text_len(&self.start_offset))
            .field(
                "security_protocol_len",
                &self.security_protocol.as_deref().map(telemetry_text_len),
            )
            .field(
                "sasl_mechanism_len",
                &self.sasl_mechanism.as_deref().map(telemetry_text_len),
            )
            .field("sasl_username_set", &self.sasl_username.is_some())
            .field("sasl_password_set", &self.sasl_password.is_some())
            .field("ssl_ca_location_set", &self.ssl_ca_location.is_some())
            .field(
                "dead_letter_topic_len",
                &self.dead_letter_topic.as_deref().map(telemetry_text_len),
            )
            .field(
                "event_type_field_len",
                &telemetry_text_len(&self.event_type_field),
            )
            .field(
                "default_event_type_len",
                &telemetry_text_len(&self.default_event_type),
            )
            .field("extra_count", &self.extra.len())
            .finish()
    }
}

fn default_start_offset() -> String {
    "latest".to_string()
}
fn default_event_type_field() -> String {
    "event_type".to_string()
}
fn default_event_type() -> String {
    "external.kafka.v1".to_string()
}

/// A Kafka consumer connector.
pub struct KafkaSource {
    name: String,
    config: KafkaConfig,
    consumer: StreamConsumer,
    /// Producer for `dead_letter_topic` (only when configured).
    producer: Option<FutureProducer>,
}

impl KafkaSource {
    /// Build from JSON config (sync; used by the connector registry). Creating
    /// the librdkafka client does not block on the network — it connects lazily.
    pub fn from_config(name: &str, config: &Value) -> InboundResult<Self> {
        let cfg: KafkaConfig = serde_json::from_value(config.clone())
            .map_err(|e| InboundError::Transient(format!("invalid kafka config: {e}")))?;
        if cfg.bootstrap_servers.trim().is_empty()
            || cfg.topic.trim().is_empty()
            || cfg.group_id.trim().is_empty()
        {
            return Err(InboundError::Transient(
                "kafka config requires bootstrap_servers, topic, and group_id".to_string(),
            ));
        }

        let mut cc = ClientConfig::new();
        cc.set("bootstrap.servers", &cfg.bootstrap_servers)
            .set("group.id", &cfg.group_id)
            .set("enable.auto.commit", "false")
            .set("auto.offset.reset", &cfg.start_offset);
        apply_security(&mut cc, &cfg);
        for (k, v) in &cfg.extra {
            cc.set(k, v);
        }
        let consumer: StreamConsumer = cc
            .create()
            .map_err(|e| InboundError::Transient(format!("kafka consumer create: {e}")))?;
        consumer
            .subscribe(&[cfg.topic.as_str()])
            .map_err(|e| InboundError::Transient(format!("kafka subscribe: {e}")))?;

        let producer =
            match &cfg.dead_letter_topic {
                Some(_) => {
                    let mut pc = ClientConfig::new();
                    pc.set("bootstrap.servers", &cfg.bootstrap_servers);
                    apply_security(&mut pc, &cfg);
                    Some(pc.create().map_err(|e| {
                        InboundError::Transient(format!("kafka producer create: {e}"))
                    })?)
                }
                None => None,
            };

        info!(
            source_name_len = telemetry_text_len(name),
            destination_class = telemetry_destination_class(&cfg.bootstrap_servers),
            destination_len = telemetry_text_len(&cfg.bootstrap_servers),
            topic_len = telemetry_text_len(&cfg.topic),
            group_len = telemetry_text_len(&cfg.group_id),
            "kafka connector subscribed"
        );
        Ok(Self {
            name: name.to_string(),
            config: cfg,
            consumer,
            producer,
        })
    }

    async fn commit_offset(&self, topic: &str, partition: i32, offset: i64) -> InboundResult<()> {
        let mut tpl = TopicPartitionList::new();
        // Committed offset is the *next* message to read.
        tpl.add_partition_offset(topic, partition, RdOffset::Offset(offset + 1))
            .map_err(|e| InboundError::Transient(format!("kafka tpl: {e}")))?;
        self.consumer
            .commit(&tpl, CommitMode::Async)
            .map_err(|e| InboundError::Transient(format!("kafka commit: {e}")))
    }

    async fn dead_letter(&self, bytes: &[u8], key: &str) {
        if let (Some(prod), Some(topic)) = (&self.producer, &self.config.dead_letter_topic) {
            let record = FutureRecord::to(topic).payload(bytes).key(key);
            if let Err((e, _)) = prod.send(record, Duration::from_secs(5)).await {
                warn!(
                    source_name_len = telemetry_text_len(&self.name),
                    topic_len = telemetry_text_len(topic),
                    reason_code = inbound_error_reason_code(&e.to_string()),
                    error_len = telemetry_text_len(&e.to_string()),
                    "kafka dead-letter produce failed"
                );
            }
        }
    }
}

#[async_trait]
impl InboundEventSource for KafkaSource {
    async fn next_event(&self) -> InboundResult<InboundEvent> {
        loop {
            let msg = self
                .consumer
                .recv()
                .await
                .map_err(|e| InboundError::Transient(format!("kafka recv: {e}")))?;
            // Copy everything we need to owned values, then drop the borrow so
            // no !Send borrowed message is held across an await.
            let topic = msg.topic().to_string();
            let partition = msg.partition();
            let offset_num = msg.offset();
            let payload: Option<Vec<u8>> = msg.payload().map(|b| b.to_vec());
            drop(msg);

            let key = format!("{topic}:{partition}:{offset_num}");
            let bytes = match payload {
                Some(b) => b,
                None => {
                    // Tombstone / empty payload — skip and advance the offset.
                    self.commit_offset(&topic, partition, offset_num).await.ok();
                    continue;
                }
            };
            let text = match std::str::from_utf8(&bytes) {
                Ok(s) => s,
                Err(_) => {
                    // Undecodable — route to the DLQ topic (if any), skip, advance.
                    self.dead_letter(&bytes, &key).await;
                    self.commit_offset(&topic, partition, offset_num).await.ok();
                    continue;
                }
            };
            let payload = parse_payload(text);
            let event_type = derive_event_type(&payload, &self.config);
            return Ok(InboundEvent::new(event_type, payload, key));
        }
    }

    async fn commit(&self, offset: Offset) -> InboundResult<()> {
        match parse_offset_key(&offset) {
            Some((topic, partition, num)) => self.commit_offset(topic, partition, num).await,
            None => Err(InboundError::Transient(format!(
                "kafka commit: malformed offset key '{offset}'"
            ))),
        }
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// Apply security/SASL/SSL settings shared by the consumer and DLQ producer.
fn apply_security(cc: &mut ClientConfig, cfg: &KafkaConfig) {
    if let Some(v) = &cfg.security_protocol {
        cc.set("security.protocol", v);
    }
    if let Some(v) = &cfg.sasl_mechanism {
        cc.set("sasl.mechanism", v);
    }
    if let Some(v) = &cfg.sasl_username {
        cc.set("sasl.username", v);
    }
    if let Some(v) = &cfg.sasl_password {
        cc.set("sasl.password", v);
    }
    if let Some(v) = &cfg.ssl_ca_location {
        cc.set("ssl.ca.location", v);
    }
}

/// JSON objects pass through; anything else is wrapped as `{"data": <text>}`.
fn parse_payload(text: &str) -> Value {
    match serde_json::from_str::<Value>(text) {
        Ok(v @ Value::Object(_)) => v,
        _ => {
            let mut m = Map::new();
            m.insert("data".to_string(), Value::String(text.to_string()));
            Value::Object(m)
        }
    }
}

fn derive_event_type(payload: &Value, cfg: &KafkaConfig) -> String {
    payload
        .get(&cfg.event_type_field)
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| cfg.default_event_type.clone())
}

/// Parse a `{topic}:{partition}:{offset}` key. Kafka topic names never contain
/// `:`, so a right split of 3 is unambiguous.
fn parse_offset_key(key: &str) -> Option<(&str, i32, i64)> {
    let mut it = key.rsplitn(3, ':');
    let offset = it.next()?.parse::<i64>().ok()?;
    let partition = it.next()?.parse::<i32>().ok()?;
    let topic = it.next()?;
    if topic.is_empty() {
        return None;
    }
    Some((topic, partition, offset))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn from_config_rejects_invalid() {
        // Both reject before any librdkafka client is created, so no runtime
        // is needed: the first fails serde (missing group_id), the second
        // fails validation (empty bootstrap_servers).
        assert!(
            KafkaSource::from_config("k", &json!({"bootstrap_servers":"b:9092","topic":"t"}))
                .is_err()
        );
        assert!(KafkaSource::from_config(
            "k",
            &json!({"bootstrap_servers":"","topic":"t","group_id":"g"})
        )
        .is_err());
    }

    #[test]
    fn config_debug_redacts_credentials_and_topics() {
        let cfg: KafkaConfig = serde_json::from_value(json!({
            "bootstrap_servers": "user:pass@broker.internal:9092",
            "topic": "tenant-secret-topic",
            "group_id": "secret-group",
            "start_offset": "earliest",
            "security_protocol": "SASL_SSL",
            "sasl_mechanism": "PLAIN",
            "sasl_username": "kafka-secret-user",
            "sasl_password": "kafka-secret-password",
            "ssl_ca_location": "/srv/secret/ca.pem",
            "dead_letter_topic": "secret-dlq-topic",
            "event_type_field": "tenant_secret_event_type",
            "default_event_type": "secret.kafka.v1",
            "extra": {
                "sasl.oauthbearer.config": "token=kafka-secret-token",
                "client.id": "secret-client-id"
            }
        }))
        .unwrap();

        let debug = format!("{cfg:?}");
        for forbidden in [
            "user:pass",
            "broker.internal",
            "tenant-secret-topic",
            "secret-group",
            "kafka-secret-user",
            "kafka-secret-password",
            "/srv/secret/ca.pem",
            "secret-dlq-topic",
            "tenant_secret_event_type",
            "secret.kafka.v1",
            "sasl.oauthbearer.config",
            "kafka-secret-token",
            "secret-client-id",
        ] {
            assert!(
                !debug.contains(forbidden),
                "Kafka config Debug leaked {forbidden}: {debug}"
            );
        }
        assert!(debug.contains("bootstrap_servers_class"));
        assert!(debug.contains("sasl_password_set"));
        assert!(debug.contains("extra_count"));
    }

    // Creating a StreamConsumer with the `tokio` feature requires a running
    // reactor (as it does in production, where the registry builder runs inside
    // the API's async runtime). create() does not connect — it only needs the
    // runtime context.
    #[tokio::test]
    async fn from_config_builds_consumer() {
        let ok = json!({
            "bootstrap_servers":"localhost:9092","topic":"t","group_id":"g","start_offset":"earliest"
        });
        assert!(KafkaSource::from_config("k", &ok).is_ok());
    }

    #[test]
    fn offset_key_roundtrip() {
        let key = format!("{}:{}:{}", "orders.v1", 3, 42);
        assert_eq!(parse_offset_key(&key), Some(("orders.v1", 3, 42)));
        assert_eq!(parse_offset_key("a-b_c.d:0:100"), Some(("a-b_c.d", 0, 100)));
        assert!(parse_offset_key("bad").is_none());
        assert!(parse_offset_key(":1:2").is_none());
        assert!(parse_offset_key("t:x:2").is_none());
    }

    #[test]
    fn payload_json_passthrough_else_wrap() {
        assert_eq!(parse_payload("{\"a\":1}")["a"], 1);
        assert_eq!(parse_payload("hello")["data"], "hello");
        assert_eq!(parse_payload("[1,2]")["data"], "[1,2]"); // non-object JSON wrapped
    }

    #[test]
    fn event_type_from_field_else_default() {
        let cfg: KafkaConfig =
            serde_json::from_value(json!({"bootstrap_servers":"b","topic":"t","group_id":"g"}))
                .unwrap();
        assert_eq!(
            derive_event_type(&json!({"event_type":"metric.v1"}), &cfg),
            "metric.v1"
        );
        assert_eq!(
            derive_event_type(&json!({"x":1}), &cfg),
            "external.kafka.v1"
        );
    }
}
