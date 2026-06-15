//! Redis Stream inbound connector (#834) — the first concrete
//! [`InboundEventSource`].
//!
//! Consumes an external Redis Stream via a consumer group (`XREADGROUP`) for
//! at-least-once delivery, committing each entry with `XACK` only after the
//! supervisor has durably written it to the outbox. On (re)start the connector
//! first drains its pending-entries list (read id `0`) — re-delivering anything
//! read-but-not-acked before the last stop — then switches to new entries
//! (read id `>`). This is how "restart resumes from last XACK with no event
//! loss" is achieved.

use async_trait::async_trait;
use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use serde::Deserialize;
use serde_json::{Map, Value};
use std::sync::Mutex as StdMutex;
use tokio::sync::Mutex as AsyncMutex;
use tracing::info;

use super::source::{InboundError, InboundEvent, InboundEventSource, InboundResult, Offset};

/// Connector config, deserialized from the `inbound_source.config` JSONB.
#[derive(Debug, Clone, Deserialize)]
pub struct RedisStreamConfig {
    pub url: String,
    pub stream: String,
    pub group: String,
    #[serde(default = "default_consumer")]
    pub consumer: String,
    /// Where a *new* consumer group starts: `$` (only new) or `0` (from start).
    #[serde(default = "default_start")]
    pub start: String,
    #[serde(default = "default_block_ms")]
    pub block_ms: u64,
    /// Stream field whose value becomes the outbox `event_type`.
    #[serde(default = "default_event_type_field")]
    pub event_type_field: String,
    /// Fallback `event_type` when the field is absent.
    #[serde(default = "default_event_type")]
    pub default_event_type: String,
}

fn default_consumer() -> String {
    "fortemi".to_string()
}
fn default_start() -> String {
    "$".to_string()
}
fn default_block_ms() -> u64 {
    5000
}
fn default_event_type_field() -> String {
    "event_type".to_string()
}
fn default_event_type() -> String {
    "external.redis.v1".to_string()
}

#[derive(Clone, Copy, PartialEq)]
enum ReadMode {
    /// Draining the consumer's pending (read-but-unacked) backlog via id `0`.
    Pending,
    /// Reading newly arriving entries via id `>`.
    New,
}

/// A Redis Stream consumer connector.
pub struct RedisStreamSource {
    name: String,
    config: RedisStreamConfig,
    /// Lazily established — the registry builder is sync, so the connection
    /// opens on first `next_event` and reconnects after a transient error.
    conn: AsyncMutex<Option<ConnectionManager>>,
    mode: StdMutex<ReadMode>,
}

impl RedisStreamSource {
    /// Build from JSON config (sync; used by the connector registry).
    pub fn from_config(name: &str, config: &Value) -> InboundResult<Self> {
        let cfg: RedisStreamConfig = serde_json::from_value(config.clone())
            .map_err(|e| InboundError::Transient(format!("invalid redis-stream config: {e}")))?;
        if cfg.url.trim().is_empty() || cfg.stream.trim().is_empty() || cfg.group.trim().is_empty()
        {
            return Err(InboundError::Transient(
                "redis-stream config requires non-empty url, stream, and group".to_string(),
            ));
        }
        Ok(Self {
            name: name.to_string(),
            config: cfg,
            conn: AsyncMutex::new(None),
            mode: StdMutex::new(ReadMode::Pending),
        })
    }

    async fn ensure_connected(&self, guard: &mut Option<ConnectionManager>) -> InboundResult<()> {
        if guard.is_some() {
            return Ok(());
        }
        let client = redis::Client::open(self.config.url.as_str())
            .map_err(|e| InboundError::Transient(format!("redis open: {e}")))?;
        let mut conn = ConnectionManager::new(client)
            .await
            .map_err(|e| InboundError::Transient(format!("redis connect: {e}")))?;
        // Idempotently create the consumer group (ignores BUSYGROUP when it
        // already exists); MKSTREAM creates the stream if absent.
        let _: redis::RedisResult<()> = redis::cmd("XGROUP")
            .arg("CREATE")
            .arg(&self.config.stream)
            .arg(&self.config.group)
            .arg(&self.config.start)
            .arg("MKSTREAM")
            .query_async(&mut conn)
            .await;
        info!(
            "redis-stream '{}' connected (stream={}, group={}, consumer={})",
            self.name, self.config.stream, self.config.group, self.config.consumer
        );
        *guard = Some(conn);
        Ok(())
    }

    fn read_id(&self) -> &'static str {
        match *self.mode.lock().unwrap() {
            ReadMode::Pending => "0",
            ReadMode::New => ">",
        }
    }
}

#[async_trait]
impl InboundEventSource for RedisStreamSource {
    async fn next_event(&self) -> InboundResult<InboundEvent> {
        loop {
            let read_id = self.read_id();
            let reply: redis::RedisResult<redis::Value> = {
                let mut guard = self.conn.lock().await;
                self.ensure_connected(&mut guard).await?;
                let conn = guard.as_mut().expect("connection established above");
                redis::cmd("XREADGROUP")
                    .arg("GROUP")
                    .arg(&self.config.group)
                    .arg(&self.config.consumer)
                    .arg("COUNT")
                    .arg(1)
                    .arg("BLOCK")
                    .arg(self.config.block_ms)
                    .arg("STREAMS")
                    .arg(&self.config.stream)
                    .arg(read_id)
                    .query_async(conn)
                    .await
            };

            match reply {
                Ok(value) => {
                    if let Some(ev) = parse_first_entry(&value, &self.config) {
                        return Ok(ev);
                    }
                    // Empty reply. While draining pending (`0`), an empty result
                    // means the backlog is clear → switch to new entries (`>`).
                    // In `New` mode it's just a block timeout → loop and block again.
                    let mut mode = self.mode.lock().unwrap();
                    if *mode == ReadMode::Pending {
                        *mode = ReadMode::New;
                    }
                }
                Err(e) => {
                    // Drop the connection so the next attempt reconnects, and
                    // surface a transient error so the supervisor backs off.
                    *self.conn.lock().await = None;
                    return Err(InboundError::Transient(format!("XREADGROUP failed: {e}")));
                }
            }
        }
    }

    async fn commit(&self, offset: Offset) -> InboundResult<()> {
        let mut guard = self.conn.lock().await;
        let conn = guard
            .as_mut()
            .ok_or_else(|| InboundError::Transient("redis-stream not connected".to_string()))?;
        let _: i64 = conn
            .xack(&self.config.stream, &self.config.group, &[offset.as_str()])
            .await
            .map_err(|e| InboundError::Transient(format!("XACK failed: {e}")))?;
        Ok(())
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// Parse the first entry out of an `XREADGROUP` reply into an `InboundEvent`.
/// The Redis reply nests as `[[stream, [[id, [k, v, k, v, ...]], ...]], ...]`.
fn parse_first_entry(v: &redis::Value, cfg: &RedisStreamConfig) -> Option<InboundEvent> {
    for stream in as_array(v)? {
        let parts = match as_array(stream) {
            Some(p) if p.len() == 2 => p,
            _ => continue,
        };
        let entries = match as_array(&parts[1]) {
            Some(e) => e,
            None => continue,
        };
        for entry in entries {
            let pair = match as_array(entry) {
                Some(p) if p.len() == 2 => p,
                _ => continue,
            };
            let id = match as_string(&pair[0]) {
                Some(s) => s,
                None => continue,
            };
            let fields = match as_array(&pair[1]) {
                Some(f) => f,
                None => continue,
            };
            let mut map = Map::new();
            let mut i = 0;
            while i + 1 < fields.len() {
                if let Some(k) = as_string(&fields[i]) {
                    if !k.is_empty() {
                        let val = as_string(&fields[i + 1]).unwrap_or_default();
                        map.insert(k, Value::String(val));
                    }
                }
                i += 2;
            }
            let event_type = map
                .get(&cfg.event_type_field)
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .unwrap_or_else(|| cfg.default_event_type.clone());
            return Some(InboundEvent::new(event_type, Value::Object(map), id));
        }
    }
    None
}

fn as_array(v: &redis::Value) -> Option<&Vec<redis::Value>> {
    match v {
        redis::Value::Array(a) => Some(a),
        _ => None,
    }
}

fn as_string(v: &redis::Value) -> Option<String> {
    match v {
        redis::Value::BulkString(b) => Some(String::from_utf8_lossy(b).to_string()),
        redis::Value::SimpleString(s) => Some(s.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn from_config_requires_core_fields() {
        assert!(RedisStreamSource::from_config(
            "r",
            &json!({"url":"redis://x","stream":"s","group":"g"})
        )
        .is_ok());
        assert!(
            RedisStreamSource::from_config("r", &json!({"url":"redis://x","stream":"s"})).is_err()
        );
        assert!(
            RedisStreamSource::from_config("r", &json!({"url":"","stream":"s","group":"g"}))
                .is_err()
        );
    }

    #[test]
    fn parses_first_entry_with_event_type_and_fields() {
        let cfg: RedisStreamConfig =
            serde_json::from_value(json!({"url":"redis://x","stream":"s","group":"g"})).unwrap();
        // [[ "s", [[ "1-0", ["event_type","metric.v1","v","42"] ]] ]]
        let reply = redis::Value::Array(vec![redis::Value::Array(vec![
            redis::Value::BulkString(b"s".to_vec()),
            redis::Value::Array(vec![redis::Value::Array(vec![
                redis::Value::BulkString(b"1-0".to_vec()),
                redis::Value::Array(vec![
                    redis::Value::BulkString(b"event_type".to_vec()),
                    redis::Value::BulkString(b"metric.v1".to_vec()),
                    redis::Value::BulkString(b"v".to_vec()),
                    redis::Value::BulkString(b"42".to_vec()),
                ]),
            ])]),
        ])]);
        let ev = parse_first_entry(&reply, &cfg).expect("one entry");
        assert_eq!(ev.offset, "1-0");
        assert_eq!(ev.event_type, "metric.v1");
        assert_eq!(ev.payload["v"], "42");
    }

    #[test]
    fn empty_reply_yields_none() {
        let cfg: RedisStreamConfig =
            serde_json::from_value(json!({"url":"redis://x","stream":"s","group":"g"})).unwrap();
        assert!(parse_first_entry(&redis::Value::Array(vec![]), &cfg).is_none());
        assert!(parse_first_entry(&redis::Value::Nil, &cfg).is_none());
    }

    #[test]
    fn falls_back_to_default_event_type() {
        let cfg: RedisStreamConfig = serde_json::from_value(
            json!({"url":"redis://x","stream":"s","group":"g","default_event_type":"fallback.v1"}),
        )
        .unwrap();
        let reply = redis::Value::Array(vec![redis::Value::Array(vec![
            redis::Value::BulkString(b"s".to_vec()),
            redis::Value::Array(vec![redis::Value::Array(vec![
                redis::Value::BulkString(b"9-0".to_vec()),
                redis::Value::Array(vec![
                    redis::Value::BulkString(b"data".to_vec()),
                    redis::Value::BulkString(b"x".to_vec()),
                ]),
            ])]),
        ])]);
        let ev = parse_first_entry(&reply, &cfg).expect("entry");
        assert_eq!(ev.event_type, "fallback.v1");
    }

    /// Live end-to-end test against a real Redis (XADD → XREADGROUP → XACK,
    /// then verify pending drains). Gated: set `INTEGRATION_TEST_REDIS=1` and
    /// optionally `REDIS_URL`. Skips in CI / when no Redis is available.
    #[tokio::test]
    async fn live_redis_consume_commit_drains_pending() {
        if std::env::var("INTEGRATION_TEST_REDIS").as_deref() != Ok("1") {
            eprintln!("skip: set INTEGRATION_TEST_REDIS=1 (and REDIS_URL) to run");
            return;
        }
        let url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
        let suffix: u32 = rand::random();
        let stream = format!("test:inbound:{suffix}");
        let group = format!("g{suffix}");

        let client = redis::Client::open(url.as_str()).expect("redis open");
        let mut conn = client
            .get_multiplexed_async_connection()
            .await
            .expect("redis connect");

        // Seed two entries; group is created at id "0" on first read so it sees them.
        for v in ["1", "2"] {
            let _: String = redis::cmd("XADD")
                .arg(&stream)
                .arg("*")
                .arg("event_type")
                .arg("metric.v1")
                .arg("v")
                .arg(v)
                .query_async(&mut conn)
                .await
                .expect("XADD");
        }

        let cfg = json!({
            "url": url, "stream": stream, "group": group,
            "start": "0", "block_ms": 500, "consumer": "test"
        });
        let src = RedisStreamSource::from_config("redis-it", &cfg).expect("from_config");

        let e1 = src.next_event().await.expect("event 1");
        assert_eq!(e1.event_type, "metric.v1");
        assert_eq!(e1.payload["v"], "1");
        src.commit(e1.offset.clone()).await.expect("ack 1");

        let e2 = src.next_event().await.expect("event 2");
        assert_eq!(e2.payload["v"], "2");
        src.commit(e2.offset.clone()).await.expect("ack 2");

        // XPENDING summary: [count, min, max, consumers]; count == 0 once acked.
        let pending: redis::Value = redis::cmd("XPENDING")
            .arg(&stream)
            .arg(&group)
            .query_async(&mut conn)
            .await
            .expect("XPENDING");
        if let redis::Value::Array(a) = &pending {
            if let Some(redis::Value::Int(n)) = a.first() {
                assert_eq!(*n, 0, "expected 0 pending after both XACKs");
            }
        }

        let _: redis::RedisResult<i64> =
            redis::cmd("DEL").arg(&stream).query_async(&mut conn).await;
    }
}
