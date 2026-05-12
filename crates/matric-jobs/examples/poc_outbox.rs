//! PoC P-01: outbox publisher load test.
//!
//! Standalone benchmark binary — does NOT touch production code.
//!
//! Env:
//!   POC_DB_URL          postgres://matric:matric@localhost:55432/matric
//!   POC_REDIS_URL       redis://localhost:56379
//!   POC_TARGET_RATE     events/sec   (default 10000)
//!   POC_DURATION_SECS   sustained run length (default 60)
//!   POC_WRITERS         concurrent inserter tasks (default 16)
//!   POC_PARTITIONED     "1" to build weekly-partitioned outbox
//!   POC_BATCH           publisher SKIP-LOCKED batch size (default 500)

use std::collections::HashMap;
use std::env;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use redis::AsyncCommands;
use serde_json::json;
use sqlx::postgres::{PgPoolOptions, PgRow};
use sqlx::Row;
use tokio::sync::Mutex;
use tokio::time::sleep;

#[tokio::main(flavor = "multi_thread", worker_threads = 8)]
async fn main() -> anyhow::Result<()> {
    let db_url = env::var("POC_DB_URL")
        .unwrap_or_else(|_| "postgres://matric:matric@localhost:55432/matric".into());
    let redis_url = env::var("POC_REDIS_URL").unwrap_or_else(|_| "redis://localhost:56379".into());
    let target_rate: u64 = env::var("POC_TARGET_RATE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10_000);
    let duration_secs: u64 = env::var("POC_DURATION_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60);
    let writers: u64 = env::var("POC_WRITERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(16);
    let partitioned = env::var("POC_PARTITIONED").ok().as_deref() == Some("1");
    let batch_size: i64 = env::var("POC_BATCH")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(500);

    let variant = if partitioned {
        "partitioned"
    } else {
        "unpartitioned"
    };
    println!("=== PoC P-01 outbox load test ({variant}) ===");
    println!(
        "target_rate={target_rate}/s duration={duration_secs}s writers={writers} batch={batch_size}"
    );

    let pool = PgPoolOptions::new()
        .max_connections(32)
        .connect(&db_url)
        .await?;

    setup_schema(&pool, partitioned).await?;

    let redis_client = redis::Client::open(redis_url.as_str())?;
    let mut mgr_check = redis::aio::ConnectionManager::new(redis_client.clone()).await?;
    let _: () = mgr_check.del("poc:stream").await?;

    let stop = Arc::new(AtomicBool::new(false));
    let total_inserted = Arc::new(AtomicU64::new(0));
    let total_published = Arc::new(AtomicU64::new(0));
    let lat_samples: Arc<Mutex<Vec<u64>>> = Arc::new(Mutex::new(Vec::with_capacity(1_000_000)));
    let batch_sizes: Arc<Mutex<Vec<u64>>> = Arc::new(Mutex::new(Vec::with_capacity(20_000)));

    let pg_stats_before = read_pg_stats(&pool).await?;
    let t0 = Instant::now();

    let mut writer_handles = Vec::new();
    for w in 0..writers {
        let pool = pool.clone();
        let stop = stop.clone();
        let counter = total_inserted.clone();
        let per_writer_rate = target_rate / writers;
        writer_handles.push(tokio::spawn(async move {
            writer_task(pool, stop, counter, per_writer_rate, w).await
        }));
    }

    let publisher_handle = {
        let pool = pool.clone();
        let stop = stop.clone();
        let counter = total_published.clone();
        let lat = lat_samples.clone();
        let bs = batch_sizes.clone();
        let mgr = redis::aio::ConnectionManager::new(redis_client.clone()).await?;
        tokio::spawn(
            async move { publisher_loop(pool, mgr, stop, counter, lat, bs, batch_size).await },
        )
    };

    let stats_handle = {
        let pool = pool.clone();
        let stop = stop.clone();
        tokio::spawn(async move { stats_sampler(pool, stop).await })
    };

    sleep(Duration::from_secs(duration_secs)).await;
    stop.store(true, Ordering::SeqCst);

    for h in writer_handles {
        let _ = h.await;
    }
    let drain_deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < drain_deadline {
        let ins = total_inserted.load(Ordering::Relaxed);
        let pub_ = total_published.load(Ordering::Relaxed);
        if pub_ >= ins {
            break;
        }
        sleep(Duration::from_millis(100)).await;
    }
    let _ = publisher_handle.await;
    let pg_stats_after = stats_handle.await?.unwrap_or_default();

    let elapsed = t0.elapsed().as_secs_f64();
    let inserted = total_inserted.load(Ordering::Relaxed);
    let published = total_published.load(Ordering::Relaxed);

    let xlen: i64 = mgr_check.xlen("poc:stream").await.unwrap_or(-1);

    let lats = lat_samples.lock().await;
    let bsizes = batch_sizes.lock().await;
    let (p50, p95, p99, max) = percentiles_us(&lats);
    let (b_p50, b_p95, b_p99, b_max) = percentiles_u64(&bsizes);

    let n_tup_upd_delta = pg_stats_after
        .get("outbox_n_tup_upd")
        .copied()
        .unwrap_or(0)
        .saturating_sub(
            pg_stats_before
                .get("outbox_n_tup_upd")
                .copied()
                .unwrap_or(0),
        );
    let autovac_count_delta = pg_stats_after
        .get("autovac_count")
        .copied()
        .unwrap_or(0)
        .saturating_sub(pg_stats_before.get("autovac_count").copied().unwrap_or(0));

    println!();
    println!("--- Results ({variant}) ---");
    println!("elapsed_s            : {elapsed:.2}");
    println!("inserted             : {inserted}");
    println!("published            : {published}");
    println!("insert_rate          : {:.0}/s", inserted as f64 / elapsed);
    println!("publish_rate         : {:.0}/s", published as f64 / elapsed);
    println!("publisher_lag_p50_ms : {:.2}", p50 as f64 / 1000.0);
    println!("publisher_lag_p95_ms : {:.2}", p95 as f64 / 1000.0);
    println!("publisher_lag_p99_ms : {:.2}", p99 as f64 / 1000.0);
    println!("publisher_lag_max_ms : {:.2}", max as f64 / 1000.0);
    println!("batch_p50            : {b_p50}");
    println!("batch_p95            : {b_p95}");
    println!("batch_p99            : {b_p99}");
    println!("batch_max            : {b_max}");
    println!("redis_xlen           : {xlen}");
    println!("pg_n_tup_upd_delta   : {n_tup_upd_delta}");
    println!("pg_autovacuum_count  : {autovac_count_delta}");
    let loss = inserted as i64 - xlen;
    println!("event_loss           : {loss} (inserted - xlen)");

    Ok(())
}

async fn setup_schema(pool: &sqlx::PgPool, partitioned: bool) -> anyhow::Result<()> {
    sqlx::query("DROP TABLE IF EXISTS event_outbox CASCADE")
        .execute(pool)
        .await?;

    if partitioned {
        sqlx::query(
            r#"
            CREATE TABLE event_outbox (
                id BIGSERIAL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                event_type TEXT NOT NULL,
                payload JSONB NOT NULL,
                published_at TIMESTAMPTZ,
                PRIMARY KEY (id, created_at)
            ) PARTITION BY RANGE (created_at);
            "#,
        )
        .execute(pool)
        .await?;

        let now: chrono::DateTime<chrono::Utc> = chrono::Utc::now();
        for week_offset in -1..=2 {
            let start = now + chrono::Duration::weeks(week_offset);
            let end = start + chrono::Duration::weeks(1);
            let name = format!("event_outbox_w{}", start.format("%Y%m%d"));
            let ddl = format!(
                "CREATE TABLE {name} PARTITION OF event_outbox FOR VALUES FROM ('{}') TO ('{}')",
                start.format("%Y-%m-%d %H:%M:%S+00"),
                end.format("%Y-%m-%d %H:%M:%S+00")
            );
            let _ = sqlx::query(&ddl).execute(pool).await;
        }
    } else {
        sqlx::query(
            r#"
            CREATE TABLE event_outbox (
                id BIGSERIAL PRIMARY KEY,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                event_type TEXT NOT NULL,
                payload JSONB NOT NULL,
                published_at TIMESTAMPTZ
            );
            "#,
        )
        .execute(pool)
        .await?;
    }

    sqlx::query(
        "CREATE INDEX event_outbox_unpub_idx ON event_outbox (created_at) WHERE published_at IS NULL",
    )
    .execute(pool)
    .await?;

    if !partitioned {
        sqlx::query("ALTER TABLE event_outbox SET (autovacuum_vacuum_scale_factor = 0.02, autovacuum_vacuum_threshold = 1000)")
            .execute(pool)
            .await?;
    }

    Ok(())
}

async fn writer_task(
    pool: sqlx::PgPool,
    stop: Arc<AtomicBool>,
    counter: Arc<AtomicU64>,
    per_writer_rate: u64,
    worker_id: u64,
) -> anyhow::Result<()> {
    let payload_template = make_payload();
    let burst = 100u64;
    let interval = Duration::from_secs_f64(burst as f64 / per_writer_rate.max(1) as f64);

    while !stop.load(Ordering::Relaxed) {
        let tick = Instant::now();
        let mut tx = pool.begin().await?;
        for _ in 0..burst {
            sqlx::query("INSERT INTO event_outbox (event_type, payload) VALUES ($1, $2::jsonb)")
                .bind("note.created")
                .bind(&payload_template)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        counter.fetch_add(burst, Ordering::Relaxed);

        let elapsed = tick.elapsed();
        if elapsed < interval {
            sleep(interval - elapsed).await;
        }
        let _ = worker_id;
    }
    Ok(())
}

fn make_payload() -> String {
    let big = "x".repeat(900);
    json!({
        "note_id": "01J9000000000000000000000A",
        "tenant_id": "default",
        "title": "PoC P-01 event",
        "body": big,
        "ts": chrono::Utc::now().to_rfc3339(),
    })
    .to_string()
}

async fn publisher_loop(
    pool: sqlx::PgPool,
    mut redis_mgr: redis::aio::ConnectionManager,
    stop: Arc<AtomicBool>,
    counter: Arc<AtomicU64>,
    lat_samples: Arc<Mutex<Vec<u64>>>,
    batch_sizes: Arc<Mutex<Vec<u64>>>,
    batch_size: i64,
) -> anyhow::Result<()> {
    loop {
        let stopping = stop.load(Ordering::Relaxed);

        let mut tx = pool.begin().await?;
        let rows = sqlx::query(
            "SELECT id, created_at, event_type, payload \
             FROM event_outbox \
             WHERE published_at IS NULL \
             ORDER BY id \
             FOR UPDATE SKIP LOCKED \
             LIMIT $1",
        )
        .bind(batch_size)
        .fetch_all(&mut *tx)
        .await?;

        if rows.is_empty() {
            tx.rollback().await?;
            if stopping {
                return Ok(());
            }
            sleep(Duration::from_millis(2)).await;
            continue;
        }

        let n = rows.len();
        batch_sizes.lock().await.push(n as u64);

        let mut ids: Vec<i64> = Vec::with_capacity(n);
        let now = chrono::Utc::now();
        let mut max_lag_us: u64 = 0;
        let mut pipe = redis::pipe();
        for row in &rows {
            let id: i64 = row.try_get("id")?;
            let created: chrono::DateTime<chrono::Utc> = row.try_get("created_at")?;
            let event_type: String = row.try_get("event_type")?;
            let payload: serde_json::Value = row.try_get("payload")?;
            let lag_us = (now - created).num_microseconds().unwrap_or(0).max(0) as u64;
            if lag_us > max_lag_us {
                max_lag_us = lag_us;
            }
            ids.push(id);
            pipe.xadd(
                "poc:stream",
                "*",
                &[
                    ("id", id.to_string()),
                    ("type", event_type),
                    ("payload", payload.to_string()),
                ],
            )
            .ignore();
        }
        let xadd_start = Instant::now();
        let _: () = pipe.query_async(&mut redis_mgr).await?;
        let xadd_us = xadd_start.elapsed().as_micros() as u64;

        sqlx::query("UPDATE event_outbox SET published_at = NOW() WHERE id = ANY($1)")
            .bind(&ids)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;

        let total_us = max_lag_us + xadd_us;
        lat_samples.lock().await.push(total_us);
        counter.fetch_add(n as u64, Ordering::Relaxed);
    }
}

async fn stats_sampler(
    pool: sqlx::PgPool,
    stop: Arc<AtomicBool>,
) -> anyhow::Result<HashMap<String, u64>> {
    let mut last = HashMap::new();
    while !stop.load(Ordering::Relaxed) {
        if let Ok(s) = read_pg_stats(&pool).await {
            let upd = s.get("outbox_n_tup_upd").copied().unwrap_or(0);
            let av = s.get("autovac_count").copied().unwrap_or(0);
            let dead = s.get("dead_tuples").copied().unwrap_or(0);
            println!("[stats] n_tup_upd={upd} autovac_count={av} dead_tuples={dead}");
            last = s;
        }
        for _ in 0..50 {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            sleep(Duration::from_millis(100)).await;
        }
    }
    if let Ok(s) = read_pg_stats(&pool).await {
        last = s;
    }
    Ok(last)
}

async fn read_pg_stats(pool: &sqlx::PgPool) -> anyhow::Result<HashMap<String, u64>> {
    let row: PgRow = sqlx::query(
        "SELECT \
            COALESCE(SUM(n_tup_upd),0)::BIGINT AS n_tup_upd, \
            COALESCE(SUM(n_dead_tup),0)::BIGINT AS dead_tup, \
            COALESCE(SUM(autovacuum_count),0)::BIGINT AS autovac \
         FROM pg_stat_user_tables \
         WHERE relname LIKE 'event_outbox%'",
    )
    .fetch_one(pool)
    .await?;
    let mut map = HashMap::new();
    let upd: i64 = row.try_get("n_tup_upd")?;
    let dead: i64 = row.try_get("dead_tup")?;
    let av: i64 = row.try_get("autovac")?;
    map.insert("outbox_n_tup_upd".into(), upd.max(0) as u64);
    map.insert("dead_tuples".into(), dead.max(0) as u64);
    map.insert("autovac_count".into(), av.max(0) as u64);
    Ok(map)
}

fn percentiles_us(samples: &[u64]) -> (u64, u64, u64, u64) {
    percentiles_u64(samples)
}

fn percentiles_u64(samples: &[u64]) -> (u64, u64, u64, u64) {
    if samples.is_empty() {
        return (0, 0, 0, 0);
    }
    let mut s = samples.to_vec();
    s.sort_unstable();
    let pick = |q: f64| s[((s.len() as f64 - 1.0) * q) as usize];
    (pick(0.50), pick(0.95), pick(0.99), *s.last().unwrap())
}
