//! Managed attachment malware/content scanning gate (#970).

use async_trait::async_trait;
use matric_core::{
    AttachmentScanStatus, AuditEvent, AuditFailurePolicy, AuditOutcome, AuditSeverity, AuditSink,
    AuditSource, AuditVisibilityClass, JobRepository, JobType, TracingSink,
};
use matric_db::Database;
use serde::Serialize;
use serde_json::{json, Value as JsonValue};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tracing::{info, warn};
use uuid::Uuid;

use crate::handler::{JobContext, JobHandler, JobResult};

const CLAMD_CHUNK_BYTES: usize = 64 * 1024;
const CLAMD_MAX_RESPONSE_BYTES: u64 = 4096;
const DEFAULT_SCAN_TIMEOUT_MS: u64 = 30_000;
const MAX_SCAN_TIMEOUT_MS: u64 = 300_000;

/// Deployment policy for managed attachment scanning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachmentScanMode {
    Required,
    Disabled,
}

impl AttachmentScanMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Required => "required",
            Self::Disabled => "disabled",
        }
    }
}

/// Validated scanner startup configuration.
#[derive(Debug, Clone)]
pub struct AttachmentScanConfig {
    pub mode: AttachmentScanMode,
    pub clamd_addr: Option<SocketAddr>,
    pub timeout: Duration,
    pub max_bytes: usize,
}

impl AttachmentScanConfig {
    pub fn from_env(hosted_multi_tenant: bool, max_upload_bytes: usize) -> anyhow::Result<Self> {
        let optional_env = |name| {
            std::env::var(name)
                .ok()
                .filter(|value| !value.trim().is_empty())
        };
        Self::from_values(
            hosted_multi_tenant,
            max_upload_bytes,
            std::env::var("MATRIC_ATTACHMENT_SCAN_MODE").ok(),
            optional_env("MATRIC_ATTACHMENT_CLAMD_ADDR"),
            optional_env("MATRIC_ATTACHMENT_SCAN_TIMEOUT_MS"),
            optional_env("MATRIC_ATTACHMENT_SCAN_MAX_BYTES"),
        )
    }

    fn from_values(
        hosted_multi_tenant: bool,
        max_upload_bytes: usize,
        mode: Option<String>,
        clamd_addr: Option<String>,
        timeout_ms: Option<String>,
        max_bytes: Option<String>,
    ) -> anyhow::Result<Self> {
        let mode = match mode.as_deref() {
            Some("required") => AttachmentScanMode::Required,
            Some("disabled") => AttachmentScanMode::Disabled,
            Some(_) => anyhow::bail!(
                "MATRIC_ATTACHMENT_SCAN_MODE has an invalid value. Expected required or disabled."
            ),
            None => anyhow::bail!(
                "MATRIC_ATTACHMENT_SCAN_MODE must be set explicitly to required or disabled."
            ),
        };
        if hosted_multi_tenant && mode != AttachmentScanMode::Required {
            anyhow::bail!(
                "FORTEMI_MULTI_TENANT=true requires MATRIC_ATTACHMENT_SCAN_MODE=required."
            );
        }

        let timeout_ms = parse_bounded_usize(
            "MATRIC_ATTACHMENT_SCAN_TIMEOUT_MS",
            timeout_ms.as_deref(),
            DEFAULT_SCAN_TIMEOUT_MS as usize,
            100,
            MAX_SCAN_TIMEOUT_MS as usize,
        )? as u64;
        let max_bytes = parse_bounded_usize(
            "MATRIC_ATTACHMENT_SCAN_MAX_BYTES",
            max_bytes.as_deref(),
            max_upload_bytes,
            1,
            usize::MAX,
        )?;
        if mode == AttachmentScanMode::Required && max_bytes < max_upload_bytes {
            anyhow::bail!(
                "MATRIC_ATTACHMENT_SCAN_MAX_BYTES must be at least MATRIC_MAX_UPLOAD_SIZE_BYTES."
            );
        }

        let clamd_addr = match (mode, clamd_addr) {
            (AttachmentScanMode::Required, Some(value)) => Some(value.parse().map_err(|_| {
                anyhow::anyhow!("MATRIC_ATTACHMENT_CLAMD_ADDR must be a numeric IP socket address.")
            })?),
            (AttachmentScanMode::Required, None) => anyhow::bail!(
                "MATRIC_ATTACHMENT_CLAMD_ADDR is required when attachment scanning is required."
            ),
            (AttachmentScanMode::Disabled, _) => None,
        };

        Ok(Self {
            mode,
            clamd_addr,
            timeout: Duration::from_millis(timeout_ms),
            max_bytes,
        })
    }
}

fn parse_bounded_usize(
    name: &str,
    value: Option<&str>,
    default: usize,
    min: usize,
    max: usize,
) -> anyhow::Result<usize> {
    let parsed = match value {
        Some(value) => value
            .parse::<usize>()
            .map_err(|_| anyhow::anyhow!("{name} must be an integer."))?,
        None => default,
    };
    if !(min..=max).contains(&parsed) {
        anyhow::bail!("{name} is outside its allowed range.");
    }
    Ok(parsed)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachmentScanOutcome {
    Clean,
    Infected,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachmentScanFailureKind {
    Unavailable,
    Timeout,
    Protocol,
}

impl AttachmentScanFailureKind {
    fn reason_code(self) -> &'static str {
        match self {
            Self::Unavailable => "scanner_unavailable",
            Self::Timeout => "scanner_timeout",
            Self::Protocol => "scanner_protocol_error",
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("attachment scanner failure: {kind:?}")]
pub struct AttachmentScanFailure {
    kind: AttachmentScanFailureKind,
}

impl AttachmentScanFailure {
    fn new(kind: AttachmentScanFailureKind) -> Self {
        Self { kind }
    }

    pub fn unavailable() -> Self {
        Self::new(AttachmentScanFailureKind::Unavailable)
    }

    pub fn timed_out() -> Self {
        Self::new(AttachmentScanFailureKind::Timeout)
    }

    pub fn kind(&self) -> AttachmentScanFailureKind {
        self.kind
    }
}

/// Scanner adapter boundary. Implementations receive bounded bytes, never paths.
#[async_trait]
pub trait AttachmentScanner: Send + Sync {
    fn backend_name(&self) -> &'static str;
    fn engine_version(&self) -> Option<&str>;
    fn signature_version(&self) -> Option<&str>;
    fn max_bytes(&self) -> usize;
    async fn health_check(&self) -> Result<(), AttachmentScanFailure>;
    async fn scan(&self, data: &[u8]) -> Result<AttachmentScanOutcome, AttachmentScanFailure>;
}

/// ClamAV daemon adapter using the byte-streaming INSTREAM protocol.
pub struct ClamdScanner {
    addr: SocketAddr,
    timeout: Duration,
    max_bytes: usize,
    engine_version: Option<String>,
    signature_version: Option<String>,
}

impl ClamdScanner {
    pub async fn connect(config: &AttachmentScanConfig) -> anyhow::Result<Self> {
        let addr = config
            .clamd_addr
            .ok_or_else(|| anyhow::anyhow!("clamd address is not configured"))?;
        let mut scanner = Self {
            addr,
            timeout: config.timeout,
            max_bytes: config.max_bytes,
            engine_version: None,
            signature_version: None,
        };
        scanner
            .health_check()
            .await
            .map_err(|error| anyhow::anyhow!(error))?;
        if let Ok(version) = scanner.command(b"zVERSION\0").await {
            let (engine, signatures) = parse_version_metadata(&version);
            scanner.engine_version = engine;
            scanner.signature_version = signatures;
        }
        Ok(scanner)
    }

    async fn command(&self, command: &[u8]) -> Result<Vec<u8>, AttachmentScanFailure> {
        let operation = async {
            let mut stream = TcpStream::connect(self.addr)
                .await
                .map_err(|_| AttachmentScanFailure::new(AttachmentScanFailureKind::Unavailable))?;
            stream
                .write_all(command)
                .await
                .map_err(|_| AttachmentScanFailure::new(AttachmentScanFailureKind::Unavailable))?;
            read_bounded_response(&mut stream).await
        };
        timeout(self.timeout, operation)
            .await
            .map_err(|_| AttachmentScanFailure::new(AttachmentScanFailureKind::Timeout))?
    }
}

#[async_trait]
impl AttachmentScanner for ClamdScanner {
    fn backend_name(&self) -> &'static str {
        "clamd"
    }

    fn engine_version(&self) -> Option<&str> {
        self.engine_version.as_deref()
    }

    fn signature_version(&self) -> Option<&str> {
        self.signature_version.as_deref()
    }

    fn max_bytes(&self) -> usize {
        self.max_bytes
    }

    async fn health_check(&self) -> Result<(), AttachmentScanFailure> {
        let response = self.command(b"zPING\0").await?;
        if response_text(&response) == "PONG" {
            Ok(())
        } else {
            Err(AttachmentScanFailure::new(
                AttachmentScanFailureKind::Protocol,
            ))
        }
    }

    async fn scan(&self, data: &[u8]) -> Result<AttachmentScanOutcome, AttachmentScanFailure> {
        if data.len() > self.max_bytes {
            return Ok(AttachmentScanOutcome::Unsupported);
        }

        let operation = async {
            let mut stream = TcpStream::connect(self.addr)
                .await
                .map_err(|_| AttachmentScanFailure::new(AttachmentScanFailureKind::Unavailable))?;
            stream
                .write_all(b"zINSTREAM\0")
                .await
                .map_err(|_| AttachmentScanFailure::new(AttachmentScanFailureKind::Unavailable))?;
            for chunk in data.chunks(CLAMD_CHUNK_BYTES) {
                let length = u32::try_from(chunk.len())
                    .map_err(|_| AttachmentScanFailure::new(AttachmentScanFailureKind::Protocol))?;
                stream.write_all(&length.to_be_bytes()).await.map_err(|_| {
                    AttachmentScanFailure::new(AttachmentScanFailureKind::Unavailable)
                })?;
                stream.write_all(chunk).await.map_err(|_| {
                    AttachmentScanFailure::new(AttachmentScanFailureKind::Unavailable)
                })?;
            }
            stream
                .write_all(&0_u32.to_be_bytes())
                .await
                .map_err(|_| AttachmentScanFailure::new(AttachmentScanFailureKind::Unavailable))?;
            read_bounded_response(&mut stream).await
        };

        let response = timeout(self.timeout, operation)
            .await
            .map_err(|_| AttachmentScanFailure::new(AttachmentScanFailureKind::Timeout))??;
        parse_scan_response(response_text(&response))
    }
}

async fn read_bounded_response(stream: &mut TcpStream) -> Result<Vec<u8>, AttachmentScanFailure> {
    let mut response = Vec::new();
    let mut buffer = [0_u8; 256];
    loop {
        let read = stream
            .read(&mut buffer)
            .await
            .map_err(|_| AttachmentScanFailure::new(AttachmentScanFailureKind::Unavailable))?;
        if read == 0 {
            break;
        }
        let terminator = buffer[..read].iter().position(|byte| *byte == 0);
        let take = terminator.unwrap_or(read);
        response.extend_from_slice(&buffer[..take]);
        if response.len() as u64 > CLAMD_MAX_RESPONSE_BYTES {
            return Err(AttachmentScanFailure::new(
                AttachmentScanFailureKind::Protocol,
            ));
        }
        if terminator.is_some() {
            break;
        }
    }
    Ok(response)
}

fn response_text(response: &[u8]) -> &str {
    std::str::from_utf8(response)
        .unwrap_or_default()
        .trim_end_matches(['\0', '\r', '\n'])
}

fn parse_scan_response(response: &str) -> Result<AttachmentScanOutcome, AttachmentScanFailure> {
    if response.ends_with(": OK") {
        Ok(AttachmentScanOutcome::Clean)
    } else if response.ends_with(" FOUND") {
        Ok(AttachmentScanOutcome::Infected)
    } else if response.contains("INSTREAM size limit exceeded") {
        Ok(AttachmentScanOutcome::Unsupported)
    } else {
        Err(AttachmentScanFailure::new(
            AttachmentScanFailureKind::Protocol,
        ))
    }
}

fn parse_version_metadata(response: &[u8]) -> (Option<String>, Option<String>) {
    let text = response_text(response);
    let token = text
        .strip_prefix("ClamAV ")
        .and_then(|rest| rest.split_whitespace().next())
        .unwrap_or_default();
    let mut parts = token.split('/');
    (
        safe_version_token(parts.next().unwrap_or_default()),
        safe_version_token(parts.next().unwrap_or_default()),
    )
}

fn safe_version_token(value: &str) -> Option<String> {
    (!value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')))
    .then(|| value.to_string())
}

#[derive(Debug, Default)]
pub struct AttachmentScanMetrics {
    available: AtomicBool,
    active: AtomicUsize,
    total: AtomicU64,
    clean: AtomicU64,
    infected: AtomicU64,
    errors: AtomicU64,
    unsupported: AtomicU64,
    bypassed: AtomicU64,
    duration_ms_total: AtomicU64,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct AttachmentScanMetricsSnapshot {
    pub available: bool,
    pub active: usize,
    pub total: u64,
    pub clean: u64,
    pub infected: u64,
    pub errors: u64,
    pub unsupported: u64,
    pub bypassed: u64,
    pub duration_ms_total: u64,
}

impl AttachmentScanMetrics {
    pub fn snapshot(&self) -> AttachmentScanMetricsSnapshot {
        AttachmentScanMetricsSnapshot {
            available: self.available.load(Ordering::Relaxed),
            active: self.active.load(Ordering::Relaxed),
            total: self.total.load(Ordering::Relaxed),
            clean: self.clean.load(Ordering::Relaxed),
            infected: self.infected.load(Ordering::Relaxed),
            errors: self.errors.load(Ordering::Relaxed),
            unsupported: self.unsupported.load(Ordering::Relaxed),
            bypassed: self.bypassed.load(Ordering::Relaxed),
            duration_ms_total: self.duration_ms_total.load(Ordering::Relaxed),
        }
    }

    pub fn record_bypass(&self) {
        self.total.fetch_add(1, Ordering::Relaxed);
        self.bypassed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn set_available(&self, available: bool) {
        self.available.store(available, Ordering::Relaxed);
    }
}

struct ActiveScan<'a> {
    metrics: &'a AttachmentScanMetrics,
    started: Instant,
}

impl<'a> ActiveScan<'a> {
    fn new(metrics: &'a AttachmentScanMetrics) -> Self {
        metrics.active.fetch_add(1, Ordering::Relaxed);
        metrics.total.fetch_add(1, Ordering::Relaxed);
        Self {
            metrics,
            started: Instant::now(),
        }
    }
}

impl Drop for ActiveScan<'_> {
    fn drop(&mut self) {
        self.metrics.active.fetch_sub(1, Ordering::Relaxed);
        self.metrics.duration_ms_total.fetch_add(
            self.started.elapsed().as_millis().min(u64::MAX as u128) as u64,
            Ordering::Relaxed,
        );
    }
}

pub struct AttachmentScanHandler {
    db: Database,
    scanner: Arc<dyn AttachmentScanner>,
    metrics: Arc<AttachmentScanMetrics>,
}

impl AttachmentScanHandler {
    pub fn new(
        db: Database,
        scanner: Arc<dyn AttachmentScanner>,
        metrics: Arc<AttachmentScanMetrics>,
    ) -> Self {
        Self {
            db,
            scanner,
            metrics,
        }
    }

    async fn persist_verdict(
        &self,
        schema: &str,
        attachment_id: Uuid,
        status: AttachmentScanStatus,
        reason_code: &'static str,
        blob_hash: &str,
    ) -> Result<(), JobResult> {
        let schema_ctx = self
            .db
            .for_schema(schema)
            .map_err(|_| JobResult::Failed("Invalid schema".to_string()))?;
        let mut tx = schema_ctx
            .begin_tx()
            .await
            .map_err(|_| JobResult::Retry("scan_verdict_transaction_failed".to_string()))?;
        let storage = self
            .db
            .file_storage
            .as_ref()
            .ok_or_else(|| JobResult::Failed("File storage not configured".to_string()))?;
        storage
            .set_scan_verdict_tx(
                &mut tx,
                attachment_id,
                status,
                Some(self.scanner.backend_name()),
                self.scanner.engine_version(),
                self.scanner.signature_version(),
                Some(reason_code),
                Some(blob_hash),
            )
            .await
            .map_err(|_| JobResult::Retry("scan_verdict_persist_failed".to_string()))?;
        tx.commit()
            .await
            .map_err(|_| JobResult::Retry("scan_verdict_commit_failed".to_string()))
    }

    async fn queue_downstream(&self, ctx: &JobContext) -> Result<usize, JobResult> {
        let attachment_id = ctx
            .payload()
            .and_then(|payload| payload.get("attachment_id"))
            .and_then(JsonValue::as_str)
            .and_then(|value| value.parse::<Uuid>().ok())
            .ok_or_else(|| JobResult::Failed("Invalid attachment scan id".to_string()))?;
        let schema = ctx
            .payload()
            .and_then(|payload| payload.get("schema"))
            .and_then(JsonValue::as_str)
            .filter(|value| !value.is_empty())
            .unwrap_or("public");
        let jobs = ctx
            .payload()
            .and_then(|payload| payload.get("downstream_jobs"))
            .and_then(JsonValue::as_array)
            .cloned()
            .unwrap_or_default();
        let mut queued = 0;
        for job in jobs {
            let job_type = downstream_job_type(
                job.get("job_type")
                    .and_then(JsonValue::as_str)
                    .unwrap_or_default(),
            )
            .ok_or_else(|| JobResult::Failed("Invalid scan downstream job type".to_string()))?;
            let payload = job.get("payload").cloned();
            let job_id = self
                .db
                .jobs
                .queue_attachment_once(attachment_id, schema, ctx.note_id(), job_type, payload)
                .await
                .map_err(|_| JobResult::Retry("scan_downstream_queue_failed".to_string()))?;
            if let Some(job_id) = job_id {
                ctx.emit_job_queued(job_id, job_type, ctx.note_id());
                queued += 1;
            }
        }
        Ok(queued)
    }

    async fn emit_audit_event(
        &self,
        ctx: &JobContext,
        schema: &str,
        attachment_id: Uuid,
        status: AttachmentScanStatus,
        reason_code: &'static str,
        outcome: AuditOutcome,
    ) {
        let event = attachment_scan_audit_event(
            schema,
            ctx.note_id(),
            attachment_id,
            self.scanner.backend_name(),
            status,
            reason_code,
            outcome,
        );
        if let Err(error) = TracingSink.emit(event.sanitized()).await {
            warn!(
                error_len = error.to_string().chars().count(),
                "Failed to emit attachment scan audit event"
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn attachment_scan_audit_event(
    schema: &str,
    note_id: Option<Uuid>,
    attachment_id: Uuid,
    scanner_backend: &str,
    status: AttachmentScanStatus,
    reason_code: &'static str,
    outcome: AuditOutcome,
) -> AuditEvent {
    let mut event = AuditEvent::new("attachment", "malware_scan", outcome)
        .with_tenant(schema)
        .with_principal("attachment-scan-worker")
        .with_resource("attachment", attachment_id.to_string())
        .with_attr("scanner_backend", scanner_backend)
        .with_attr("verdict", status.to_string())
        .with_attr("reason_code", reason_code);
    if let Some(note_id) = note_id {
        event = event.with_attr("parent_note_id", note_id.to_string());
    }
    event.idempotency_key = Some(format!("attachment-scan:{schema}:{attachment_id}:{status}"));
    event.reason = Some(reason_code.to_string());
    event.source = AuditSource::Worker;
    event.visibility = AuditVisibilityClass::SecurityRestricted;
    event.failure_policy = AuditFailurePolicy::BestEffort;
    event.severity = match outcome {
        AuditOutcome::Success => AuditSeverity::Info,
        AuditOutcome::Denied => AuditSeverity::Warn,
        AuditOutcome::Failure | AuditOutcome::Error => AuditSeverity::Error,
        AuditOutcome::Unknown => AuditSeverity::Warn,
    };
    event.sanitized()
}

#[async_trait]
impl JobHandler for AttachmentScanHandler {
    fn job_type(&self) -> JobType {
        JobType::AttachmentVirusScan
    }

    async fn execute(&self, ctx: JobContext) -> JobResult {
        let _active = ActiveScan::new(&self.metrics);
        let payload = match ctx.payload() {
            Some(payload) => payload,
            None => return JobResult::Failed("Missing attachment scan payload".to_string()),
        };
        let attachment_id = match payload
            .get("attachment_id")
            .and_then(JsonValue::as_str)
            .and_then(|value| value.parse::<Uuid>().ok())
        {
            Some(id) => id,
            None => return JobResult::Failed("Invalid attachment scan id".to_string()),
        };
        let schema = payload
            .get("schema")
            .and_then(JsonValue::as_str)
            .filter(|value| !value.is_empty())
            .unwrap_or("public");
        let schema_ctx = match self.db.for_schema(schema) {
            Ok(context) => context,
            Err(_) => return JobResult::Failed("Invalid schema".to_string()),
        };
        let storage = match self.db.file_storage.as_ref() {
            Some(storage) => storage,
            None => return JobResult::Failed("File storage not configured".to_string()),
        };
        let mut tx = match schema_ctx.begin_tx().await {
            Ok(tx) => tx,
            Err(_) => return JobResult::Retry("scan_read_transaction_failed".to_string()),
        };
        let scan_file = match storage.read_file_for_scan_tx(&mut tx, attachment_id).await {
            Ok(file) => file,
            Err(_) => return JobResult::Failed("Attachment scan read failed".to_string()),
        };
        if tx.commit().await.is_err() {
            return JobResult::Retry("scan_read_commit_failed".to_string());
        }

        let outcome = if scan_file.data.len() > self.scanner.max_bytes() {
            Ok(AttachmentScanOutcome::Unsupported)
        } else {
            self.scanner.scan(&scan_file.data).await
        };
        match outcome {
            Ok(AttachmentScanOutcome::Clean) => {
                if let Err(result) = self
                    .persist_verdict(
                        schema,
                        attachment_id,
                        AttachmentScanStatus::Clean,
                        "scanner_clean",
                        &scan_file.content_hash,
                    )
                    .await
                {
                    return result;
                }
                self.metrics.set_available(true);
                self.metrics.clean.fetch_add(1, Ordering::Relaxed);
                self.emit_audit_event(
                    &ctx,
                    schema,
                    attachment_id,
                    AttachmentScanStatus::Clean,
                    "scanner_clean",
                    AuditOutcome::Success,
                )
                .await;
                match self.queue_downstream(&ctx).await {
                    Ok(queued) => {
                        info!(
                            attachment_present = true,
                            queued,
                            size_bytes = scan_file.size_bytes,
                            "Attachment scan accepted bytes and released downstream jobs"
                        );
                        JobResult::Success(Some(json!({
                            "verdict": "clean",
                            "downstream_jobs_queued": queued,
                        })))
                    }
                    Err(result) => result,
                }
            }
            Ok(AttachmentScanOutcome::Infected) => {
                if let Err(result) = self
                    .persist_verdict(
                        schema,
                        attachment_id,
                        AttachmentScanStatus::Infected,
                        "malware_detected",
                        &scan_file.content_hash,
                    )
                    .await
                {
                    return result;
                }
                self.metrics.set_available(true);
                self.metrics.infected.fetch_add(1, Ordering::Relaxed);
                self.emit_audit_event(
                    &ctx,
                    schema,
                    attachment_id,
                    AttachmentScanStatus::Infected,
                    "malware_detected",
                    AuditOutcome::Denied,
                )
                .await;
                warn!(
                    attachment_present = true,
                    size_bytes = scan_file.size_bytes,
                    "Attachment quarantined by scanner verdict"
                );
                JobResult::Success(Some(json!({"verdict": "infected"})))
            }
            Ok(AttachmentScanOutcome::Unsupported) => {
                if let Err(result) = self
                    .persist_verdict(
                        schema,
                        attachment_id,
                        AttachmentScanStatus::Unsupported,
                        "scanner_content_unsupported",
                        &scan_file.content_hash,
                    )
                    .await
                {
                    return result;
                }
                self.metrics.set_available(true);
                self.metrics.unsupported.fetch_add(1, Ordering::Relaxed);
                self.emit_audit_event(
                    &ctx,
                    schema,
                    attachment_id,
                    AttachmentScanStatus::Unsupported,
                    "scanner_content_unsupported",
                    AuditOutcome::Denied,
                )
                .await;
                JobResult::Success(Some(json!({"verdict": "unsupported"})))
            }
            Err(error) => {
                self.metrics.set_available(false);
                if let Err(result) = self
                    .persist_verdict(
                        schema,
                        attachment_id,
                        AttachmentScanStatus::Error,
                        error.kind.reason_code(),
                        &scan_file.content_hash,
                    )
                    .await
                {
                    return result;
                }
                self.metrics.errors.fetch_add(1, Ordering::Relaxed);
                self.emit_audit_event(
                    &ctx,
                    schema,
                    attachment_id,
                    AttachmentScanStatus::Error,
                    error.kind.reason_code(),
                    AuditOutcome::Error,
                )
                .await;
                JobResult::Retry(error.kind.reason_code().to_string())
            }
        }
    }
}

fn downstream_job_type(value: &str) -> Option<JobType> {
    match value {
        "extraction" => Some(JobType::Extraction),
        "exif_extraction" => Some(JobType::ExifExtraction),
        "media_optimize" => Some(JobType::MediaOptimize),
        "audio_transcription" => Some(JobType::AudioTranscription),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    #[test]
    fn config_requires_explicit_mode_and_hosted_scan() {
        assert!(AttachmentScanConfig::from_values(false, 100, None, None, None, None).is_err());
        assert!(AttachmentScanConfig::from_values(
            true,
            100,
            Some("disabled".to_string()),
            None,
            None,
            None,
        )
        .is_err());
        assert!(AttachmentScanConfig::from_values(
            true,
            100,
            Some("required".to_string()),
            Some("127.0.0.1:3310".to_string()),
            None,
            Some("99".to_string()),
        )
        .is_err());
    }

    #[test]
    fn config_accepts_explicit_local_bypass_and_required_scanner() {
        let disabled = AttachmentScanConfig::from_values(
            false,
            100,
            Some("disabled".to_string()),
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(disabled.mode, AttachmentScanMode::Disabled);

        let required = AttachmentScanConfig::from_values(
            true,
            100,
            Some("required".to_string()),
            Some("127.0.0.1:3310".to_string()),
            Some("5000".to_string()),
            Some("100".to_string()),
        )
        .unwrap();
        assert_eq!(required.mode, AttachmentScanMode::Required);
        assert_eq!(required.timeout, Duration::from_millis(5000));
    }

    #[test]
    fn clamd_responses_map_to_bounded_verdicts() {
        assert_eq!(
            parse_scan_response("stream: OK").unwrap(),
            AttachmentScanOutcome::Clean
        );
        assert_eq!(
            parse_scan_response("stream: Eicar-Test-Signature FOUND").unwrap(),
            AttachmentScanOutcome::Infected
        );
        assert_eq!(
            parse_scan_response("INSTREAM size limit exceeded. ERROR").unwrap(),
            AttachmentScanOutcome::Unsupported
        );
        assert!(parse_scan_response("stream: private/raw scanner detail ERROR").is_err());
    }

    #[test]
    fn version_metadata_keeps_only_safe_tokens() {
        assert_eq!(
            parse_version_metadata(b"ClamAV 1.4.2/27349/Fri Jul 17\0"),
            (Some("1.4.2".to_string()), Some("27349".to_string()))
        );
        assert_eq!(
            parse_version_metadata(b"ClamAV ../../private/secret\0"),
            (None, None)
        );
    }

    #[test]
    fn downstream_job_types_are_strictly_allowlisted() {
        assert_eq!(downstream_job_type("extraction"), Some(JobType::Extraction));
        assert_eq!(downstream_job_type("attachment_virus_scan"), None);
        assert_eq!(downstream_job_type("purge_note"), None);
    }

    fn test_scanner(addr: SocketAddr, max_bytes: usize, timeout_ms: u64) -> ClamdScanner {
        ClamdScanner {
            addr,
            timeout: Duration::from_millis(timeout_ms),
            max_bytes,
            engine_version: Some("test".to_string()),
            signature_version: Some("1".to_string()),
        }
    }

    #[tokio::test]
    async fn clamd_instream_sends_exact_bytes_and_accepts_clean() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let expected = vec![0x41; CLAMD_CHUNK_BYTES + 7];
        let server_expected = expected.clone();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut command = [0_u8; 10];
            socket.read_exact(&mut command).await.unwrap();
            assert_eq!(&command, b"zINSTREAM\0");
            let mut received = Vec::new();
            loop {
                let mut length = [0_u8; 4];
                socket.read_exact(&mut length).await.unwrap();
                let length = u32::from_be_bytes(length) as usize;
                if length == 0 {
                    break;
                }
                let mut chunk = vec![0_u8; length];
                socket.read_exact(&mut chunk).await.unwrap();
                received.extend_from_slice(&chunk);
            }
            assert_eq!(received, server_expected);
            socket.write_all(b"stream: OK\0").await.unwrap();
        });

        let scanner = test_scanner(addr, expected.len(), 1000);
        assert_eq!(
            scanner.scan(&expected).await.unwrap(),
            AttachmentScanOutcome::Clean
        );
        server.await.unwrap();
    }

    #[tokio::test]
    async fn clamd_instream_maps_infected_without_exposing_signature() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut command = [0_u8; 10];
            socket.read_exact(&mut command).await.unwrap();
            loop {
                let mut length = [0_u8; 4];
                socket.read_exact(&mut length).await.unwrap();
                let length = u32::from_be_bytes(length) as usize;
                if length == 0 {
                    break;
                }
                let mut chunk = vec![0_u8; length];
                socket.read_exact(&mut chunk).await.unwrap();
            }
            socket
                .write_all(b"stream: Private-Signature-Name FOUND\0")
                .await
                .unwrap();
        });

        let scanner = test_scanner(addr, 100, 1000);
        assert_eq!(
            scanner.scan(b"test").await.unwrap(),
            AttachmentScanOutcome::Infected
        );
        server.await.unwrap();
    }

    #[tokio::test]
    async fn clamd_timeout_and_unavailable_are_distinct_failures() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (_socket, _) = listener.accept().await.unwrap();
            tokio::time::sleep(Duration::from_millis(100)).await;
        });
        let timeout_scanner = test_scanner(addr, 100, 10);
        assert_eq!(
            timeout_scanner.scan(b"test").await.unwrap_err().kind,
            AttachmentScanFailureKind::Timeout
        );
        server.await.unwrap();

        let unused = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let unavailable_addr = unused.local_addr().unwrap();
        drop(unused);
        let unavailable_scanner = test_scanner(unavailable_addr, 100, 100);
        assert_eq!(
            unavailable_scanner.scan(b"test").await.unwrap_err().kind,
            AttachmentScanFailureKind::Unavailable
        );
    }

    #[tokio::test]
    async fn scanner_max_is_an_unsupported_verdict_without_connecting() {
        let unused = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = unused.local_addr().unwrap();
        drop(unused);
        let scanner = test_scanner(addr, 3, 100);
        assert_eq!(
            scanner.scan(b"four").await.unwrap(),
            AttachmentScanOutcome::Unsupported
        );
    }

    #[tokio::test]
    async fn required_clamd_startup_fails_when_backend_is_unhealthy() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let config = AttachmentScanConfig {
            mode: AttachmentScanMode::Required,
            clamd_addr: Some(addr),
            timeout: Duration::from_millis(100),
            max_bytes: 100,
        };
        assert!(ClamdScanner::connect(&config).await.is_err());
    }

    #[test]
    fn scan_audit_event_carries_bounded_security_context() {
        let note_id = Uuid::now_v7();
        let attachment_id = Uuid::now_v7();
        let event = attachment_scan_audit_event(
            "archive_test",
            Some(note_id),
            attachment_id,
            "clamd",
            AttachmentScanStatus::Infected,
            "malware_detected",
            AuditOutcome::Denied,
        );

        assert_eq!(event.category, "attachment");
        assert_eq!(event.action, "malware_scan");
        assert_eq!(event.tenant_id.as_deref(), Some("archive_test"));
        assert_eq!(
            event.principal_id.as_deref(),
            Some("attachment-scan-worker")
        );
        assert_eq!(
            event.resource_id.as_deref(),
            Some(attachment_id.to_string()).as_deref()
        );
        assert_eq!(event.attrs["parent_note_id"], note_id.to_string());
        assert_eq!(event.attrs["scanner_backend"], "clamd");
        assert_eq!(event.attrs["verdict"], "infected");
        assert_eq!(event.reason.as_deref(), Some("malware_detected"));
        assert_eq!(event.visibility, AuditVisibilityClass::SecurityRestricted);
    }
}
