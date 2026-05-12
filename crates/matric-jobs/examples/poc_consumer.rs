use std::collections::HashSet;
use std::env;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rand::Rng;
use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;
use tokio::time::sleep;

const STREAM: &str = "poc-consumer-stream";
const GROUP: &str = "poc-cg-1";
const PAYLOAD_BYTES: usize = 1024;

fn redis_url() -> String {
    env::var("POC_REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6399".to_string())
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
}

async fn connect() -> redis::RedisResult<ConnectionManager> {
    let client = redis::Client::open(redis_url())?;
    ConnectionManager::new(client).await
}

async fn ensure_group(conn: &mut ConnectionManager) -> redis::RedisResult<()> {
    let _: redis::RedisResult<()> = redis::cmd("XGROUP")
        .arg("CREATE")
        .arg(STREAM)
        .arg(GROUP)
        .arg("0")
        .arg("MKSTREAM")
        .query_async(conn)
        .await;
    Ok(())
}

async fn cmd_seed(n: u64) -> anyhow::Result<()> {
    let mut conn = connect().await?;
    let _: redis::RedisResult<()> = redis::cmd("DEL").arg(STREAM).query_async(&mut conn).await;
    ensure_group(&mut conn).await?;
    let payload: String = "x".repeat(PAYLOAD_BYTES);
    let start = Instant::now();
    let batch: u64 = 500;
    let mut sent: u64 = 0;
    while sent < n {
        let mut pipe = redis::pipe();
        let end = (sent + batch).min(n);
        for i in sent..end {
            let id = format!("ev-{:010}", i);
            pipe.cmd("XADD")
                .arg(STREAM)
                .arg("*")
                .arg("eid")
                .arg(&id)
                .arg("p")
                .arg(&payload)
                .ignore();
        }
        let _: () = pipe.query_async(&mut conn).await?;
        sent = end;
        if sent.is_multiple_of(10_000) {
            eprintln!(
                "seeded {} ({:.0} ev/s)",
                sent,
                sent as f64 / start.elapsed().as_secs_f64().max(0.001)
            );
        }
    }
    let len: i64 = redis::cmd("XLEN")
        .arg(STREAM)
        .query_async(&mut conn)
        .await?;
    eprintln!("seed complete: XLEN={}", len);
    Ok(())
}

async fn cmd_consume(
    consumer: String,
    sleep_ms: u64,
    run_seconds: u64,
    metrics_path: Option<String>,
    dedup_enabled: bool,
) -> anyhow::Result<()> {
    let mut conn = connect().await?;
    ensure_group(&mut conn).await?;
    let read_conn = conn.clone();
    let ack_conn = conn.clone();

    let (tx, mut rx) = mpsc::channel::<(String, String)>(64);

    let stop = Arc::new(AtomicBool::new(false));
    let stop_r = stop.clone();
    let stop_a = stop.clone();

    let delivered = Arc::new(AtomicU64::new(0));
    let acked = Arc::new(AtomicU64::new(0));
    let duplicates = Arc::new(AtomicU64::new(0));
    let channel_full_signals = Arc::new(AtomicU64::new(0));
    let pel_samples: Arc<tokio::sync::Mutex<Vec<(u128, i64)>>> =
        Arc::new(tokio::sync::Mutex::new(Vec::new()));

    let delivered_r = delivered.clone();
    let duplicates_r = duplicates.clone();
    let channel_full_r = channel_full_signals.clone();
    let consumer_r = consumer.clone();

    let reader = tokio::spawn(async move {
        let mut conn = read_conn;
        let mut seen: HashSet<String> = HashSet::new();
        let mut cursor: String = "0".to_string();
        let mut draining_pel = true;
        while !stop_r.load(Ordering::Relaxed) {
            let read_id = if draining_pel { cursor.as_str() } else { ">" };
            let res: redis::RedisResult<redis::Value> = redis::cmd("XREADGROUP")
                .arg("GROUP")
                .arg(GROUP)
                .arg(&consumer_r)
                .arg("COUNT")
                .arg(32)
                .arg("BLOCK")
                .arg(500)
                .arg("STREAMS")
                .arg(STREAM)
                .arg(read_id)
                .query_async(&mut conn)
                .await;
            let value = match res {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("[{}] XREADGROUP err: {}", consumer_r, e);
                    sleep(Duration::from_millis(200)).await;
                    continue;
                }
            };
            let entries = parse_xreadgroup(&value);
            if draining_pel && entries.is_empty() {
                draining_pel = false;
                continue;
            }
            if draining_pel {
                if let Some((last_sid, _)) = entries.last() {
                    cursor = last_sid.clone();
                }
            }
            for (sid, eid) in entries {
                delivered_r.fetch_add(1, Ordering::Relaxed);
                if dedup_enabled && !seen.insert(eid.clone()) {
                    duplicates_r.fetch_add(1, Ordering::Relaxed);
                    let mut c = conn.clone();
                    let _: redis::RedisResult<i64> = c.xack(STREAM, GROUP, &[&sid]).await;
                    continue;
                }
                match tx.try_send((sid.clone(), eid.clone())) {
                    Ok(_) => {}
                    Err(mpsc::error::TrySendError::Full(item)) => {
                        channel_full_r.fetch_add(1, Ordering::Relaxed);
                        if tx.send(item).await.is_err() {
                            return;
                        }
                    }
                    Err(_) => return,
                }
            }
        }
    });

    let acked_a = acked.clone();
    let acker = tokio::spawn(async move {
        let mut conn = ack_conn;
        while let Some((sid, _eid)) = rx.recv().await {
            if sleep_ms > 0 {
                sleep(Duration::from_millis(sleep_ms)).await;
            }
            let _: redis::RedisResult<i64> = conn.xack(STREAM, GROUP, &[&sid]).await;
            acked_a.fetch_add(1, Ordering::Relaxed);
            if stop_a.load(Ordering::Relaxed) {
                break;
            }
        }
    });

    let pel_samples_p = pel_samples.clone();
    let consumer_p = consumer.clone();
    let stop_p = stop.clone();
    let probe = tokio::spawn(async move {
        let mut conn = match connect().await {
            Ok(c) => c,
            Err(_) => return,
        };
        while !stop_p.load(Ordering::Relaxed) {
            let res: redis::RedisResult<redis::Value> = redis::cmd("XPENDING")
                .arg(STREAM)
                .arg(GROUP)
                .arg("IDLE")
                .arg(0)
                .arg("-")
                .arg("+")
                .arg(10000)
                .arg(&consumer_p)
                .query_async(&mut conn)
                .await;
            let pel = match res {
                Ok(redis::Value::Array(items)) => items.len() as i64,
                _ => -1,
            };
            pel_samples_p.lock().await.push((now_ms(), pel));
            sleep(Duration::from_millis(1000)).await;
        }
    });

    let start = Instant::now();
    let deadline = start + Duration::from_secs(run_seconds);
    while Instant::now() < deadline {
        sleep(Duration::from_millis(200)).await;
    }
    stop.store(true, Ordering::Relaxed);
    let _ = reader.await;
    let _ = acker.await;
    let _ = probe.await;

    let d = delivered.load(Ordering::Relaxed);
    let a = acked.load(Ordering::Relaxed);
    let dup = duplicates.load(Ordering::Relaxed);
    let cf = channel_full_signals.load(Ordering::Relaxed);
    let pel = pel_samples.lock().await.clone();
    let report = format!(
        "consumer={} delivered={} acked={} duplicates={} channel_full_events={} pel_samples={}\n{}\n",
        consumer,
        d,
        a,
        dup,
        cf,
        pel.len(),
        pel.iter()
            .map(|(t, n)| format!("{},{}", t, n))
            .collect::<Vec<_>>()
            .join(";")
    );
    if let Some(p) = metrics_path {
        tokio::fs::write(&p, report.as_bytes()).await?;
    } else {
        println!("{}", report);
    }
    Ok(())
}

fn parse_xreadgroup(v: &redis::Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let streams = match v {
        redis::Value::Array(a) => a,
        _ => return out,
    };
    for stream in streams {
        let parts = match stream {
            redis::Value::Array(a) if a.len() == 2 => a,
            _ => continue,
        };
        let entries = match &parts[1] {
            redis::Value::Array(a) => a,
            _ => continue,
        };
        for entry in entries {
            let pair = match entry {
                redis::Value::Array(a) if a.len() == 2 => a,
                _ => continue,
            };
            let sid = match &pair[0] {
                redis::Value::BulkString(b) => String::from_utf8_lossy(b).to_string(),
                redis::Value::SimpleString(s) => s.clone(),
                _ => continue,
            };
            let fields = match &pair[1] {
                redis::Value::Array(a) => a,
                _ => continue,
            };
            let mut eid = String::new();
            let mut i = 0;
            while i + 1 < fields.len() {
                let k = match &fields[i] {
                    redis::Value::BulkString(b) => String::from_utf8_lossy(b).to_string(),
                    _ => String::new(),
                };
                if k == "eid" {
                    if let redis::Value::BulkString(b) = &fields[i + 1] {
                        eid = String::from_utf8_lossy(b).to_string();
                    }
                }
                i += 2;
            }
            if !eid.is_empty() {
                out.push((sid, eid));
            }
        }
    }
    out
}

fn self_exe() -> String {
    env::current_exe().unwrap().to_string_lossy().to_string()
}

fn spawn_consumer(name: &str, sleep_ms: u64, secs: u64, out: &str, dedup: bool) -> Child {
    let mut cmd = Command::new(self_exe());
    cmd.arg("consume")
        .arg(name)
        .arg(sleep_ms.to_string())
        .arg(secs.to_string())
        .arg(out)
        .arg(if dedup { "dedup" } else { "nodedup" })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    cmd.spawn().expect("spawn consumer")
}

async fn drain(mut child: Child, label: String) -> std::io::Result<()> {
    if let Some(out) = child.stdout.take() {
        let mut r = BufReader::new(out).lines();
        let l = label.clone();
        tokio::spawn(async move {
            while let Ok(Some(line)) = r.next_line().await {
                println!("[{}] {}", l, line);
            }
        });
    }
    if let Some(out) = child.stderr.take() {
        let mut r = BufReader::new(out).lines();
        let l = label.clone();
        tokio::spawn(async move {
            while let Ok(Some(line)) = r.next_line().await {
                eprintln!("[{}] {}", l, line);
            }
        });
    }
    let _ = child.wait().await?;
    Ok(())
}

async fn reset() -> anyhow::Result<()> {
    let mut conn = connect().await?;
    let _: redis::RedisResult<()> = redis::cmd("DEL").arg(STREAM).query_async(&mut conn).await;
    ensure_group(&mut conn).await?;
    Ok(())
}

fn read_metrics(path: &str) -> (u64, u64, u64, u64, Vec<(u128, i64)>) {
    let s = std::fs::read_to_string(path).unwrap_or_default();
    let mut delivered = 0u64;
    let mut acked = 0u64;
    let mut dup = 0u64;
    let mut cf = 0u64;
    let mut pel = Vec::new();
    for line in s.lines() {
        if line.starts_with("consumer=") {
            for kv in line.split_whitespace() {
                let mut it = kv.splitn(2, '=');
                let k = it.next().unwrap_or("");
                let v = it.next().unwrap_or("0");
                match k {
                    "delivered" => delivered = v.parse().unwrap_or(0),
                    "acked" => acked = v.parse().unwrap_or(0),
                    "duplicates" => dup = v.parse().unwrap_or(0),
                    "channel_full_events" => cf = v.parse().unwrap_or(0),
                    _ => {}
                }
            }
        } else if line.contains(',') && line.contains(';') {
            for tok in line.split(';') {
                let mut it = tok.split(',');
                if let (Some(a), Some(b)) = (it.next(), it.next()) {
                    if let (Ok(t), Ok(n)) = (a.parse::<u128>(), b.parse::<i64>()) {
                        pel.push((t, n));
                    }
                }
            }
        }
    }
    (delivered, acked, dup, cf, pel)
}

async fn cmd_f1() -> anyhow::Result<()> {
    println!("== F1: kill consumer mid-batch ==");
    reset().await?;
    cmd_seed(20_000).await?;
    let out1 = "/tmp/poc_consumer_f1_c1.txt";
    let out2 = "/tmp/poc_consumer_f1_c2.txt";
    let _ = std::fs::remove_file(out1);
    let _ = std::fs::remove_file(out2);

    let mut c1 = spawn_consumer("c1", 0, 60, out1, false);
    let c2 = spawn_consumer("c2", 0, 60, out2, false);

    sleep(Duration::from_secs(5)).await;
    let kill_ts = now_ms();
    eprintln!("F1 SIGKILL c1 at {}", kill_ts);
    let pid = c1.id().expect("pid");
    let _ = Command::new("kill")
        .arg("-9")
        .arg(pid.to_string())
        .status()
        .await;
    let _ = c1.wait().await;

    let restart_ts = now_ms();
    eprintln!(
        "F1 restarting c1 at {} (kill->restart {} ms)",
        restart_ts,
        restart_ts - kill_ts
    );
    let out1b = "/tmp/poc_consumer_f1_c1b.txt";
    let _ = std::fs::remove_file(out1b);
    let c1b = spawn_consumer("c1", 0, 30, out1b, false);

    let first_ack_ts = Arc::new(tokio::sync::Mutex::new(0u128));
    let mut probe = connect().await?;
    let first_ack_ts_p = first_ack_ts.clone();
    let stop_probe = Arc::new(AtomicBool::new(false));
    let stop_probe_p = stop_probe.clone();
    let probe_task = tokio::spawn(async move {
        let mut last_pel: i64 = -1;
        while !stop_probe_p.load(Ordering::Relaxed) {
            let res: redis::RedisResult<redis::Value> = redis::cmd("XPENDING")
                .arg(STREAM)
                .arg(GROUP)
                .arg("IDLE")
                .arg(0)
                .arg("-")
                .arg("+")
                .arg(1)
                .arg("c1")
                .query_async(&mut probe)
                .await;
            let pel = match res {
                Ok(redis::Value::Array(a)) => a.len() as i64,
                _ => -1,
            };
            if last_pel == -1 {
                last_pel = pel;
            } else if pel != last_pel && pel >= 0 {
                let mut g = first_ack_ts_p.lock().await;
                if *g == 0 {
                    *g = now_ms();
                }
                break;
            }
            sleep(Duration::from_millis(50)).await;
        }
    });

    let _ = drain(c2, "c2".into()).await;
    let _ = drain(c1b, "c1b".into()).await;
    stop_probe.store(true, Ordering::Relaxed);
    let _ = probe_task.await;

    let first_ack = *first_ack_ts.lock().await;
    let recovery_ms = if first_ack > 0 {
        first_ack as i128 - restart_ts as i128
    } else {
        0
    };

    let mut conn = connect().await?;
    let xlen: i64 = redis::cmd("XLEN")
        .arg(STREAM)
        .query_async(&mut conn)
        .await?;
    let final_pel: redis::Value = redis::cmd("XPENDING")
        .arg(STREAM)
        .arg(GROUP)
        .query_async(&mut conn)
        .await?;
    let pel_total = match &final_pel {
        redis::Value::Array(a) => match a.first() {
            Some(redis::Value::Int(n)) => *n,
            _ => -1,
        },
        _ => -1,
    };

    let (d1, a1, _, _, _) = read_metrics(out1);
    let (d2, a2, _, _, _) = read_metrics(out2);
    let (d1b, a1b, _, _, _) = read_metrics(out1b);

    let total_delivered = d1 + d2 + d1b;
    let total_acked = a1 + a2 + a1b;
    println!(
        "F1 RESULT: xlen={} c1_pre(d={},a={}) c2(d={},a={}) c1_post(d={},a={}) total_delivered={} total_acked={} unique_seeded=20000 final_pel={} kill_to_restart_ms={} restart_to_first_ack_ms={}",
        xlen,
        d1,
        a1,
        d2,
        a2,
        d1b,
        a1b,
        total_delivered,
        total_acked,
        pel_total,
        restart_ts - kill_ts,
        recovery_ms
    );
    Ok(())
}

async fn measure_xadd_latency() -> u128 {
    let mut conn = match connect().await {
        Ok(c) => c,
        Err(_) => return u128::MAX,
    };
    let t = Instant::now();
    let _: redis::RedisResult<String> = redis::cmd("XADD")
        .arg(STREAM)
        .arg("*")
        .arg("probe")
        .arg("1")
        .query_async(&mut conn)
        .await;
    t.elapsed().as_micros()
}

async fn cmd_f2() -> anyhow::Result<()> {
    println!("== F2: Redis DEBUG SLEEP ==");
    reset().await?;
    cmd_seed(20_000).await?;
    let out1 = "/tmp/poc_consumer_f2_c1.txt";
    let _ = std::fs::remove_file(out1);
    let c1 = spawn_consumer("c1", 0, 25, out1, false);

    sleep(Duration::from_secs(3)).await;
    let xadd_lat_pre = measure_xadd_latency().await;
    eprintln!("F2 pre-stall XADD latency: {} us", xadd_lat_pre);

    let mut bg_conn = connect().await?;
    let stall_start = now_ms();
    eprintln!("F2 issuing DEBUG SLEEP 5 inline at {}", stall_start);
    let bg = tokio::spawn(async move {
        let _: redis::RedisResult<redis::Value> = redis::cmd("DEBUG")
            .arg("SLEEP")
            .arg("5")
            .query_async(&mut bg_conn)
            .await;
    });

    sleep(Duration::from_millis(200)).await;
    let probe_t0 = now_ms();
    let probe_a = measure_xadd_latency().await;
    let probe_a_done = now_ms();
    eprintln!(
        "F2 during-stall XADD: {} us (issued +{} ms, returned +{} ms)",
        probe_a,
        probe_t0 - stall_start,
        probe_a_done - stall_start
    );

    let _ = bg.await;
    let recovery_t0 = now_ms();
    let probe_b = measure_xadd_latency().await;
    let recovery_ms = now_ms() - recovery_t0;
    eprintln!(
        "F2 post-stall XADD latency: {} us (recovery probe took {} ms)",
        probe_b, recovery_ms
    );
    let xadd_during = probe_a;
    let xadd_post = probe_b;

    let _ = drain(c1, "c1".into()).await;
    let (d, a, _, _, _) = read_metrics(out1);
    println!(
        "F2 RESULT: xadd_pre_us={} xadd_during_us={} xadd_post_us={} delivered={} acked={}",
        xadd_lat_pre, xadd_during, xadd_post, d, a
    );
    Ok(())
}

async fn cmd_f3() -> anyhow::Result<()> {
    println!("== F3: slow consumer isolation ==");
    reset().await?;
    cmd_seed(60_000).await?;
    let out_slow = "/tmp/poc_consumer_f3_slow.txt";
    let out_fast = "/tmp/poc_consumer_f3_fast.txt";
    let _ = std::fs::remove_file(out_slow);
    let _ = std::fs::remove_file(out_fast);

    let slow = spawn_consumer("slow", 100, 60, out_slow, false);
    let fast = spawn_consumer("fast", 0, 60, out_fast, false);

    let _ = tokio::join!(drain(slow, "slow".into()), drain(fast, "fast".into()));

    let (ds, a_s, _, cf_s, pel_s) = read_metrics(out_slow);
    let (df, af, _, cf_f, _) = read_metrics(out_fast);

    let pel_growth: String = if pel_s.len() >= 2 {
        let head_end = pel_s.len().min(5);
        let head = &pel_s[..head_end];
        let tail_start = pel_s.len().saturating_sub(5);
        let tail = &pel_s[tail_start..];
        format!(
            "head=[{}] tail=[{}]",
            head.iter()
                .map(|(t, n)| format!("t+{}s:{}", (*t - pel_s[0].0) / 1000, n))
                .collect::<Vec<_>>()
                .join(","),
            tail.iter()
                .map(|(t, n)| format!("t+{}s:{}", (*t - pel_s[0].0) / 1000, n))
                .collect::<Vec<_>>()
                .join(",")
        )
    } else {
        "insufficient samples".into()
    };

    println!(
        "F3 RESULT: slow(delivered={} acked={} channel_full={}) fast(delivered={} acked={} channel_full={}) slow_pel: {}",
        ds, a_s, cf_s, df, af, cf_f, pel_growth
    );
    Ok(())
}

async fn cmd_dedup_compare() -> anyhow::Result<()> {
    println!("== Dedup comparison ==");
    let mut rng = rand::thread_rng();
    let payload: String = "y".repeat(PAYLOAD_BYTES);

    reset().await?;
    cmd_seed(10_000).await?;
    {
        let mut conn = connect().await?;
        for _ in 0..500 {
            let i: u64 = rng.gen_range(0..10_000);
            let id = format!("ev-{:010}", i);
            let _: redis::RedisResult<String> = redis::cmd("XADD")
                .arg(STREAM)
                .arg("*")
                .arg("eid")
                .arg(&id)
                .arg("p")
                .arg(&payload)
                .query_async(&mut conn)
                .await;
        }
    }
    let out_no = "/tmp/poc_consumer_dedup_off.txt";
    let _ = std::fs::remove_file(out_no);
    let c = spawn_consumer("dedup-off", 0, 15, out_no, false);
    let _ = drain(c, "dedup-off".into()).await;
    let (d_off, _, dup_off, _, _) = read_metrics(out_no);

    reset().await?;
    cmd_seed(10_000).await?;
    {
        let mut conn = connect().await?;
        for _ in 0..500 {
            let i: u64 = rng.gen_range(0..10_000);
            let id = format!("ev-{:010}", i);
            let _: redis::RedisResult<String> = redis::cmd("XADD")
                .arg(STREAM)
                .arg("*")
                .arg("eid")
                .arg(&id)
                .arg("p")
                .arg(&payload)
                .query_async(&mut conn)
                .await;
        }
    }
    let out_on = "/tmp/poc_consumer_dedup_on.txt";
    let _ = std::fs::remove_file(out_on);
    let c = spawn_consumer("dedup-on", 0, 15, out_on, true);
    let _ = drain(c, "dedup-on".into()).await;
    let (d_on, _, dup_on, _, _) = read_metrics(out_on);

    println!(
        "DEDUP RESULT: off(delivered={} duplicates_detected_via_set={}) on(delivered={} duplicates_detected_and_skipped={})",
        d_off, dup_off, d_on, dup_on
    );
    Ok(())
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    let sub = args.get(1).cloned().unwrap_or_else(|| "help".into());
    match sub.as_str() {
        "seed" => {
            let n: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(100_000);
            cmd_seed(n).await
        }
        "consume" => {
            let name = args.get(2).cloned().unwrap_or_else(|| "c1".into());
            let sleep_ms: u64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
            let secs: u64 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(60);
            let out = args.get(5).cloned();
            let dedup = args.get(6).map(|s| s == "dedup").unwrap_or(false);
            cmd_consume(name, sleep_ms, secs, out, dedup).await
        }
        "f1" => cmd_f1().await,
        "f2" => cmd_f2().await,
        "f3" => cmd_f3().await,
        "dedup" => cmd_dedup_compare().await,
        "all" => {
            cmd_f1().await?;
            cmd_f2().await?;
            cmd_f3().await?;
            cmd_dedup_compare().await?;
            Ok(())
        }
        _ => {
            eprintln!("usage: poc_consumer <seed N|consume name sleep_ms secs out [dedup|nodedup]|f1|f2|f3|dedup|all>");
            Ok(())
        }
    }
}
