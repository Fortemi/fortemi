//! Sidecar lifecycle management for GPU-exclusive mode (#576).
//!
//! On single-GPU systems (e.g., RTX 4090 24GB), always-on sidecars (whisper: 4.1GB,
//! pyannote: 2.5GB) permanently occupy VRAM that Ollama needs for KV cache during
//! text generation. This module manages sidecar container lifecycle at tier boundaries:
//!
//! - **Audio tier start**: start whisper + pyannote containers, wait for health
//! - **Audio tier end**: stop containers, free ~6.6 GB VRAM before Ollama tiers
//!
//! When `GPU_EXCLUSIVE_MODE=false` (multi-GPU systems), sidecars run continuously.

use std::time::Duration;
use tracing::{info, warn};

/// Sidecar services that consume GPU resources.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sidecar {
    Whisper,
    Pyannote,
}

impl Sidecar {
    /// Docker compose service name for this sidecar.
    pub fn service_name(&self) -> &'static str {
        match self {
            Sidecar::Whisper => "whisper",
            Sidecar::Pyannote => "pyannote",
        }
    }

    /// Health check URL for this sidecar (relative to its base URL).
    pub fn health_url(&self) -> Option<&'static str> {
        match self {
            Sidecar::Whisper => Some("/health"),
            Sidecar::Pyannote => None, // pyannote doesn't have a dedicated health endpoint
        }
    }
}

/// All GPU-consuming sidecars.
pub const ALL_SIDECARS: &[Sidecar] = &[Sidecar::Whisper, Sidecar::Pyannote];

/// Controls external GPU-consuming services at tier boundaries.
#[async_trait::async_trait]
pub trait SidecarController: Send + Sync {
    /// Start a sidecar and wait for health check.
    async fn start(&self, sidecar: Sidecar) -> Result<(), String>;
    /// Stop a sidecar and free GPU memory.
    async fn stop(&self, sidecar: Sidecar) -> Result<(), String>;
    /// Check if a sidecar is running.
    async fn is_running(&self, sidecar: Sidecar) -> bool;
}

/// Docker compose-based sidecar controller.
///
/// Uses `docker compose stop/start` to manage individual services within
/// the bundle compose file. Containers are stopped (not removed) for fast restart.
pub struct DockerSidecarController {
    compose_file: String,
    project_name: Option<String>,
    health_timeout: Duration,
    stop_timeout: Duration,
}

impl Default for DockerSidecarController {
    fn default() -> Self {
        Self::new()
    }
}

impl DockerSidecarController {
    pub fn new() -> Self {
        let compose_file = std::env::var(matric_core::defaults::ENV_COMPOSE_FILE)
            .unwrap_or_else(|_| "docker-compose.bundle.yml".to_string());
        let project_name = std::env::var(matric_core::defaults::ENV_COMPOSE_PROJECT).ok();
        let health_timeout =
            Duration::from_secs(matric_core::defaults::sidecar_health_timeout_secs());
        let stop_timeout = Duration::from_secs(matric_core::defaults::sidecar_stop_timeout_secs());

        Self {
            compose_file,
            project_name,
            health_timeout,
            stop_timeout,
        }
    }

    fn compose_args(&self) -> Vec<String> {
        let mut args = vec![
            "compose".to_string(),
            "-f".to_string(),
            self.compose_file.clone(),
        ];
        if let Some(ref project) = self.project_name {
            args.push("-p".to_string());
            args.push(project.clone());
        }
        args
    }

    async fn run_compose(&self, action: &str, service: &str) -> Result<(), String> {
        let mut args = self.compose_args();
        args.push(action.to_string());
        args.push(service.to_string());

        let output = tokio::process::Command::new("docker")
            .args(&args)
            .output()
            .await
            .map_err(|e| format!("Failed to run docker compose {}: {}", action, e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!(
                "docker compose {} {} failed: {}",
                action, service, stderr
            ));
        }
        Ok(())
    }

    async fn wait_for_health(&self, sidecar: Sidecar) -> Result<(), String> {
        // pyannote doesn't have a health endpoint — just wait briefly
        let url = match sidecar.health_url() {
            Some(path) => {
                let base = match sidecar {
                    Sidecar::Whisper => std::env::var("WHISPER_BASE_URL")
                        .unwrap_or_else(|_| "http://localhost:8000".to_string()),
                    _ => return Ok(()),
                };
                format!("{}{}", base, path)
            }
            None => {
                // No health endpoint — wait a fixed duration
                tokio::time::sleep(Duration::from_secs(5)).await;
                return Ok(());
            }
        };

        let client = reqwest::Client::new();
        let start = std::time::Instant::now();
        let poll_interval = Duration::from_secs(2);

        while start.elapsed() < self.health_timeout {
            match client.get(&url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    info!(
                        sidecar = ?sidecar,
                        elapsed_ms = start.elapsed().as_millis(),
                        "Sidecar health check passed"
                    );
                    return Ok(());
                }
                _ => {
                    tokio::time::sleep(poll_interval).await;
                }
            }
        }

        Err(format!(
            "Sidecar {:?} health check timed out after {}s",
            sidecar,
            self.health_timeout.as_secs()
        ))
    }
}

#[async_trait::async_trait]
impl SidecarController for DockerSidecarController {
    async fn start(&self, sidecar: Sidecar) -> Result<(), String> {
        let service = sidecar.service_name();
        info!(sidecar = service, "Starting sidecar for audio tier");

        self.run_compose("start", service).await?;
        self.wait_for_health(sidecar).await?;

        info!(sidecar = service, "Sidecar started and healthy");
        Ok(())
    }

    async fn stop(&self, sidecar: Sidecar) -> Result<(), String> {
        let service = sidecar.service_name();
        info!(sidecar = service, "Stopping sidecar to free VRAM");

        // Use timeout flag for graceful shutdown
        let mut args = self.compose_args();
        args.push("stop".to_string());
        args.push("-t".to_string());
        args.push(self.stop_timeout.as_secs().to_string());
        args.push(service.to_string());

        let output = tokio::process::Command::new("docker")
            .args(&args)
            .output()
            .await
            .map_err(|e| format!("Failed to stop sidecar {}: {}", service, e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!(
                sidecar = service,
                stderr = %stderr,
                "Sidecar stop returned non-zero (may already be stopped)"
            );
        }

        info!(sidecar = service, "Sidecar stopped");
        Ok(())
    }

    async fn is_running(&self, sidecar: Sidecar) -> bool {
        let service = sidecar.service_name();
        let mut args = self.compose_args();
        args.push("ps".to_string());
        args.push("--status".to_string());
        args.push("running".to_string());
        args.push("-q".to_string());
        args.push(service.to_string());

        match tokio::process::Command::new("docker")
            .args(&args)
            .output()
            .await
        {
            Ok(output) => !output.stdout.is_empty(),
            Err(_) => false,
        }
    }
}

/// No-op sidecar controller for when GPU_EXCLUSIVE_MODE is disabled.
pub struct NoOpSidecarController;

#[async_trait::async_trait]
impl SidecarController for NoOpSidecarController {
    async fn start(&self, _sidecar: Sidecar) -> Result<(), String> {
        Ok(())
    }
    async fn stop(&self, _sidecar: Sidecar) -> Result<(), String> {
        Ok(())
    }
    async fn is_running(&self, _sidecar: Sidecar) -> bool {
        true // Assume always running when not managed
    }
}

/// Start all GPU-consuming sidecars.
pub async fn start_all_sidecars(controller: &dyn SidecarController) {
    for sidecar in ALL_SIDECARS {
        if let Err(e) = controller.start(*sidecar).await {
            warn!(sidecar = ?sidecar, error = %e, "Failed to start sidecar — audio jobs may fail");
        }
    }
}

/// Stop all GPU-consuming sidecars to free VRAM.
pub async fn stop_all_sidecars(controller: &dyn SidecarController) {
    for sidecar in ALL_SIDECARS {
        if let Err(e) = controller.stop(*sidecar).await {
            warn!(sidecar = ?sidecar, error = %e, "Failed to stop sidecar — VRAM may not be freed");
        }
    }
}
