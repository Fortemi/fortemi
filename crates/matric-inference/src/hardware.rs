//! Hardware tier detection and model recommendations.
//!
//! This module provides hardware detection and tier-based model recommendations
//! for matric-memory knowledge management workloads.
//!
//! # Hardware Tiers
//!
//! | Tier | VRAM | Example GPUs | Target Models |
//! |------|------|--------------|---------------|
//! | Budget | <8GB | RTX 3060, integrated | 7B quantized (Q4) |
//! | Mainstream | 8-16GB | RTX 4070, M1/M2 | 7B-14B models |
//! | Performance | 24GB | RTX 4090, M2 Ultra | 14B-32B models |
//! | Professional | 48GB+ | A6000, dual GPU | 70B+ models |

use serde::{Deserialize, Serialize};
use std::fmt;
use std::process::Command;

/// Hardware tier classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HardwareTier {
    /// <8GB VRAM - budget systems
    Budget,
    /// 8-16GB VRAM - mainstream systems
    Mainstream,
    /// 24GB VRAM - performance systems
    Performance,
    /// 48GB+ VRAM - professional systems
    Professional,
}

impl HardwareTier {
    /// Determine tier from VRAM in MB.
    pub fn from_vram_mb(vram_mb: u64) -> Self {
        match vram_mb {
            v if v >= 48_000 => HardwareTier::Professional,
            v if v >= 20_000 => HardwareTier::Performance,
            v if v >= 8_000 => HardwareTier::Mainstream,
            _ => HardwareTier::Budget,
        }
    }

    /// Human-readable description.
    pub fn description(&self) -> &'static str {
        match self {
            HardwareTier::Budget => "Budget (<8GB VRAM)",
            HardwareTier::Mainstream => "Mainstream (8-16GB VRAM)",
            HardwareTier::Performance => "Performance (24GB VRAM)",
            HardwareTier::Professional => "Professional (48GB+ VRAM)",
        }
    }

    /// Example GPUs in this tier.
    pub fn example_gpus(&self) -> &'static [&'static str] {
        match self {
            HardwareTier::Budget => &["RTX 3060", "GTX 1660", "Intel Arc A770", "Integrated"],
            HardwareTier::Mainstream => &["RTX 4070", "RTX 3080", "M1 Pro", "M2"],
            HardwareTier::Performance => &["RTX 4090", "RTX 3090", "M2 Ultra", "A5000"],
            HardwareTier::Professional => &["A6000", "A100", "H100", "Dual GPU"],
        }
    }
}

/// Detected system capabilities.
#[derive(Clone, Serialize, Deserialize)]
pub struct SystemCapabilities {
    /// GPU VRAM in MB (if detected).
    pub gpu_vram_mb: Option<u64>,
    /// GPU name (if detected).
    pub gpu_name: Option<String>,
    /// System RAM in MB.
    pub system_ram_mb: u64,
    /// Number of CPU cores.
    pub cpu_cores: usize,
    /// Detected hardware tier.
    pub detected_tier: HardwareTier,
    /// Whether CUDA is available.
    pub cuda_available: bool,
    /// Whether ROCm is available.
    pub rocm_available: bool,
    /// Whether Metal is available (macOS).
    pub metal_available: bool,
}

impl fmt::Debug for SystemCapabilities {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SystemCapabilities")
            .field("gpu_vram_mb", &self.gpu_vram_mb)
            .field(
                "gpu_name_len",
                &self.gpu_name.as_ref().map(|value| value.chars().count()),
            )
            .field("system_ram_mb", &self.system_ram_mb)
            .field("cpu_cores", &self.cpu_cores)
            .field("detected_tier", &self.detected_tier)
            .field("cuda_available", &self.cuda_available)
            .field("rocm_available", &self.rocm_available)
            .field("metal_available", &self.metal_available)
            .finish()
    }
}

impl Default for SystemCapabilities {
    fn default() -> Self {
        Self {
            gpu_vram_mb: None,
            gpu_name: None,
            system_ram_mb: 0,
            cpu_cores: 1,
            detected_tier: HardwareTier::Budget,
            cuda_available: false,
            rocm_available: false,
            metal_available: false,
        }
    }
}

impl SystemCapabilities {
    /// Detect system capabilities.
    pub fn detect() -> Self {
        let mut caps = Self {
            cpu_cores: num_cpus(),
            system_ram_mb: system_ram_mb(),
            ..Default::default()
        };

        // Try NVIDIA first
        if let Some((name, vram)) = detect_nvidia_gpu() {
            caps.gpu_name = Some(name);
            caps.gpu_vram_mb = Some(vram);
            caps.cuda_available = true;
        }
        // Try AMD ROCm
        else if let Some((name, vram)) = detect_amd_gpu() {
            caps.gpu_name = Some(name);
            caps.gpu_vram_mb = Some(vram);
            caps.rocm_available = true;
        }
        // Try macOS Metal
        else if cfg!(target_os = "macos") {
            if let Some((name, vram)) = detect_metal_gpu() {
                caps.gpu_name = Some(name);
                caps.gpu_vram_mb = Some(vram);
                caps.metal_available = true;
            }
        }

        // Determine tier
        caps.detected_tier = if let Some(vram) = caps.gpu_vram_mb {
            HardwareTier::from_vram_mb(vram)
        } else {
            // CPU-only: use RAM to estimate (very limited)
            if caps.system_ram_mb >= 64_000 {
                HardwareTier::Mainstream
            } else {
                HardwareTier::Budget
            }
        };

        caps
    }
}

/// Model recommendation for a hardware tier.
#[derive(Clone, Serialize, Deserialize)]
pub struct ModelRecommendation {
    /// Model name (Ollama format).
    pub model: String,
    /// Role: "embedding" or "generation".
    pub role: String,
    /// Why this model is recommended.
    pub rationale: String,
}

impl fmt::Debug for ModelRecommendation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ModelRecommendation")
            .field("model_len", &self.model.chars().count())
            .field("role_len", &self.role.chars().count())
            .field("rationale_len", &self.rationale.chars().count())
            .finish()
    }
}

impl ModelRecommendation {
    /// Create a new recommendation.
    pub fn new(
        model: impl Into<String>,
        role: impl Into<String>,
        rationale: impl Into<String>,
    ) -> Self {
        Self {
            model: model.into(),
            role: role.into(),
            rationale: rationale.into(),
        }
    }
}

/// Quality expectations for a hardware tier.
#[derive(Clone, Serialize, Deserialize)]
pub struct TierQualityExpectations {
    /// Hardware tier.
    pub tier: HardwareTier,
    /// Expected title generation quality (0-100).
    pub title_quality_range: (f32, f32),
    /// Expected semantic similarity accuracy (0-100).
    pub semantic_accuracy_range: (f32, f32),
    /// Revision quality description.
    pub revision_quality: String,
    /// Expected latency per generation (ms).
    pub latency_range_ms: (u64, u64),
}

impl fmt::Debug for TierQualityExpectations {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TierQualityExpectations")
            .field("tier", &self.tier)
            .field("title_quality_range", &self.title_quality_range)
            .field("semantic_accuracy_range", &self.semantic_accuracy_range)
            .field(
                "revision_quality_len",
                &self.revision_quality.chars().count(),
            )
            .field("latency_range_ms", &self.latency_range_ms)
            .finish()
    }
}

/// Get quality expectations for a hardware tier.
pub fn tier_quality_expectations(tier: HardwareTier) -> TierQualityExpectations {
    match tier {
        HardwareTier::Budget => TierQualityExpectations {
            tier,
            title_quality_range: (75.0, 85.0),
            semantic_accuracy_range: (80.0, 85.0),
            revision_quality: "Good for simple notes".to_string(),
            latency_range_ms: (200, 400),
        },
        HardwareTier::Mainstream => TierQualityExpectations {
            tier,
            title_quality_range: (88.0, 94.0),
            semantic_accuracy_range: (85.0, 91.0),
            revision_quality: "Excellent".to_string(),
            latency_range_ms: (300, 600),
        },
        HardwareTier::Performance => TierQualityExpectations {
            tier,
            title_quality_range: (92.0, 96.0),
            semantic_accuracy_range: (90.0, 94.0),
            revision_quality: "Near-human".to_string(),
            latency_range_ms: (500, 1500),
        },
        HardwareTier::Professional => TierQualityExpectations {
            tier,
            title_quality_range: (95.0, 99.0),
            semantic_accuracy_range: (94.0, 98.0),
            revision_quality: "Best for bulk historical processing".to_string(),
            latency_range_ms: (1000, 3000),
        },
    }
}

/// Get recommended models for a hardware tier.
pub fn tier_model_recommendations(tier: HardwareTier) -> Vec<ModelRecommendation> {
    match tier {
        HardwareTier::Budget => vec![
            ModelRecommendation::new(
                "nomic-embed-text",
                "embedding",
                "Low memory footprint, good quality",
            ),
            ModelRecommendation::new("qwen2.5:7b-q4", "generation", "Best quality at 7B size"),
            ModelRecommendation::new("llama3.2:3b", "generation", "Fastest for interactive use"),
        ],
        HardwareTier::Mainstream => vec![
            ModelRecommendation::new("nomic-embed-text", "embedding", "Optimal balance"),
            ModelRecommendation::new("qwen2.5:14b", "generation", "Best quality (93.8% titles)"),
            ModelRecommendation::new(
                "qwen2.5:7b",
                "generation",
                "Fast alternative (88.9% titles)",
            ),
            ModelRecommendation::new("llama3.1:8b", "generation", "Fastest (258ms latency)"),
        ],
        HardwareTier::Performance => vec![
            ModelRecommendation::new(
                "nomic-embed-text",
                "embedding",
                "Optimal balance (upgrade to qwen3-embedding in Phase 2)",
            ),
            ModelRecommendation::new(
                "qwen3.5:27b",
                "generation",
                "Primary: 256K context, multimodal, fits 24GB (17GB VRAM)",
            ),
            ModelRecommendation::new(
                "qwen3.5:9b",
                "generation",
                "Fast: 256K context, multimodal, unified gen+vision",
            ),
            ModelRecommendation::new(
                "gpt-oss:20b",
                "generation",
                "Fallback: production stable, 98K context",
            ),
        ],
        HardwareTier::Professional => vec![
            ModelRecommendation::new("mxbai-embed-large", "embedding", "Best embedding quality"),
            ModelRecommendation::new(
                "qwen3.5:27b",
                "generation",
                "Primary: 256K context, multimodal, excellent quality",
            ),
            ModelRecommendation::new("qwen2.5:32b", "generation", "Highest quality dense model"),
            ModelRecommendation::new(
                "llama3.1:70b",
                "generation",
                "Maximum quality for batch processing",
            ),
        ],
    }
}

/// Ollama optimization settings for a hardware tier.
#[derive(Clone, Serialize, Deserialize)]
pub struct OllamaSettings {
    /// OLLAMA_FLASH_ATTENTION (always 1).
    pub flash_attention: bool,
    /// OLLAMA_KV_CACHE_TYPE.
    pub kv_cache_type: String,
    /// OLLAMA_NUM_PARALLEL.
    pub num_parallel: u32,
    /// OLLAMA_MAX_LOADED_MODELS.
    pub max_loaded_models: u32,
}

impl fmt::Debug for OllamaSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OllamaSettings")
            .field("flash_attention", &self.flash_attention)
            .field("kv_cache_type_len", &self.kv_cache_type.chars().count())
            .field("num_parallel", &self.num_parallel)
            .field("max_loaded_models", &self.max_loaded_models)
            .finish()
    }
}

impl OllamaSettings {
    /// Get recommended Ollama settings for a hardware tier.
    pub fn for_tier(tier: HardwareTier) -> Self {
        match tier {
            HardwareTier::Budget => Self {
                flash_attention: true,
                kv_cache_type: "q4_0".to_string(),
                num_parallel: 1,
                max_loaded_models: 1,
            },
            HardwareTier::Mainstream => Self {
                flash_attention: true,
                kv_cache_type: "q8_0".to_string(),
                num_parallel: 2,
                max_loaded_models: 2,
            },
            HardwareTier::Performance => Self {
                flash_attention: true,
                kv_cache_type: "q8_0".to_string(),
                num_parallel: 3,
                max_loaded_models: 3,
            },
            HardwareTier::Professional => Self {
                flash_attention: true,
                kv_cache_type: "f16".to_string(),
                num_parallel: 4,
                max_loaded_models: 4,
            },
        }
    }

    /// Generate environment variable export commands.
    pub fn to_env_exports(&self) -> String {
        format!(
            "OLLAMA_FLASH_ATTENTION={}\nOLLAMA_KV_CACHE_TYPE={}\nOLLAMA_NUM_PARALLEL={}\nOLLAMA_MAX_LOADED_MODELS={}",
            if self.flash_attention { "1" } else { "0" },
            self.kv_cache_type,
            self.num_parallel,
            self.max_loaded_models
        )
    }
}

/// Cloud/API provider comparison.
#[derive(Clone, Serialize, Deserialize)]
pub struct CloudComparison {
    /// Provider name.
    pub provider: String,
    /// Equivalent local tier.
    pub equivalent_tier: HardwareTier,
    /// Cost per 1K notes processed.
    pub cost_per_1k_notes: String,
    /// Typical latency.
    pub latency: String,
}

impl fmt::Debug for CloudComparison {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CloudComparison")
            .field("provider_len", &self.provider.chars().count())
            .field("equivalent_tier", &self.equivalent_tier)
            .field(
                "cost_per_1k_notes_len",
                &self.cost_per_1k_notes.chars().count(),
            )
            .field("latency_len", &self.latency.chars().count())
            .finish()
    }
}

/// Get cloud provider comparisons.
pub fn cloud_comparisons() -> Vec<CloudComparison> {
    vec![
        CloudComparison {
            provider: "OpenAI GPT-4o".to_string(),
            equivalent_tier: HardwareTier::Professional,
            cost_per_1k_notes: "$0.50-1.00".to_string(),
            latency: "500ms-2s".to_string(),
        },
        CloudComparison {
            provider: "Claude Sonnet".to_string(),
            equivalent_tier: HardwareTier::Performance,
            cost_per_1k_notes: "$0.30-0.60".to_string(),
            latency: "800ms-2s".to_string(),
        },
        CloudComparison {
            provider: "Groq (Llama 70B)".to_string(),
            equivalent_tier: HardwareTier::Professional,
            cost_per_1k_notes: "$0.05-0.10".to_string(),
            latency: "100-300ms".to_string(),
        },
        CloudComparison {
            provider: "Local 14B".to_string(),
            equivalent_tier: HardwareTier::Mainstream,
            cost_per_1k_notes: "$0 (amortized)".to_string(),
            latency: "300-600ms".to_string(),
        },
    ]
}

// =============================================================================
// Hardware Detection Helpers
// =============================================================================

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(1)
}

fn system_ram_mb() -> u64 {
    // Try Linux /proc/meminfo first
    if let Ok(content) = std::fs::read_to_string("/proc/meminfo") {
        for line in content.lines() {
            if line.starts_with("MemTotal:") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    if let Ok(kb) = parts[1].parse::<u64>() {
                        return kb / 1024;
                    }
                }
            }
        }
    }

    // Fallback: try sysctl on macOS
    #[cfg(target_os = "macos")]
    {
        if let Ok(output) = Command::new("sysctl").args(["-n", "hw.memsize"]).output() {
            if output.status.success() {
                if let Ok(bytes) = String::from_utf8_lossy(&output.stdout)
                    .trim()
                    .parse::<u64>()
                {
                    return bytes / (1024 * 1024);
                }
            }
        }
    }

    0
}

fn detect_nvidia_gpu() -> Option<(String, u64)> {
    let output = Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,memory.total",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout.lines().next()?;
    let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();

    if parts.len() >= 2 {
        let name = parts[0].to_string();
        let vram_mb = parts[1].parse::<u64>().ok()?;
        Some((name, vram_mb))
    } else {
        None
    }
}

fn detect_amd_gpu() -> Option<(String, u64)> {
    let output = Command::new("rocm-smi")
        .args(["--showmeminfo", "vram"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    // Parse rocm-smi output (simplified)
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.contains("Total Memory") {
            // Extract memory value
            let parts: Vec<&str> = line.split_whitespace().collect();
            for (i, part) in parts.iter().enumerate() {
                if part.contains("MB") && i > 0 {
                    if let Ok(vram) = parts[i - 1].parse::<u64>() {
                        return Some(("AMD GPU".to_string(), vram));
                    }
                }
            }
        }
    }

    None
}

#[cfg(target_os = "macos")]
fn detect_metal_gpu() -> Option<(String, u64)> {
    // Use system_profiler to get GPU info
    let output = Command::new("system_profiler")
        .args(["SPDisplaysDataType", "-json"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    // Parse JSON output
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let displays = json.get("SPDisplaysDataType")?.as_array()?;

    for display in displays {
        if let Some(name) = display.get("sppci_model").and_then(|v| v.as_str()) {
            // For Apple Silicon, unified memory is shared
            // Try to get VRAM or estimate from chip
            if let Some(vram) = display.get("spdisplays_vram").and_then(|v| v.as_str()) {
                // Parse VRAM string like "16 GB"
                let parts: Vec<&str> = vram.split_whitespace().collect();
                if let Some(size) = parts.first() {
                    if let Ok(gb) = size.parse::<u64>() {
                        return Some((name.to_string(), gb * 1024));
                    }
                }
            }

            // Estimate for Apple Silicon (shared memory)
            if name.contains("Apple M") {
                // Use system RAM as approximation (70% usable for GPU)
                let ram = system_ram_mb();
                return Some((name.to_string(), ram * 70 / 100));
            }
        }
    }

    None
}

#[cfg(not(target_os = "macos"))]
fn detect_metal_gpu() -> Option<(String, u64)> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hardware_tier_from_vram() {
        assert_eq!(HardwareTier::from_vram_mb(4000), HardwareTier::Budget);
        assert_eq!(HardwareTier::from_vram_mb(8000), HardwareTier::Mainstream);
        assert_eq!(HardwareTier::from_vram_mb(16000), HardwareTier::Mainstream);
        assert_eq!(HardwareTier::from_vram_mb(24000), HardwareTier::Performance);
        assert_eq!(
            HardwareTier::from_vram_mb(48000),
            HardwareTier::Professional
        );
        assert_eq!(
            HardwareTier::from_vram_mb(80000),
            HardwareTier::Professional
        );
    }

    #[test]
    fn test_tier_ordering() {
        assert!(HardwareTier::Budget < HardwareTier::Mainstream);
        assert!(HardwareTier::Mainstream < HardwareTier::Performance);
        assert!(HardwareTier::Performance < HardwareTier::Professional);
    }

    #[test]
    fn test_tier_model_recommendations() {
        let budget = tier_model_recommendations(HardwareTier::Budget);
        assert!(!budget.is_empty());
        assert!(budget.iter().any(|r| r.role == "embedding"));
        assert!(budget.iter().any(|r| r.role == "generation"));

        let professional = tier_model_recommendations(HardwareTier::Professional);
        assert!(professional.len() >= budget.len());
    }

    #[test]
    fn test_tier_quality_expectations() {
        let budget = tier_quality_expectations(HardwareTier::Budget);
        let professional = tier_quality_expectations(HardwareTier::Professional);

        assert!(professional.title_quality_range.0 > budget.title_quality_range.0);
    }

    #[test]
    fn test_ollama_settings() {
        let budget = OllamaSettings::for_tier(HardwareTier::Budget);
        assert_eq!(budget.num_parallel, 1);
        assert_eq!(budget.kv_cache_type, "q4_0");

        let professional = OllamaSettings::for_tier(HardwareTier::Professional);
        assert_eq!(professional.num_parallel, 4);
        assert_eq!(professional.kv_cache_type, "f16");
    }

    #[test]
    fn test_ollama_settings_exports() {
        let settings = OllamaSettings::for_tier(HardwareTier::Mainstream);
        let exports = settings.to_env_exports();

        assert!(exports.contains("OLLAMA_FLASH_ATTENTION=1"));
        assert!(exports.contains("OLLAMA_KV_CACHE_TYPE=q8_0"));
    }

    #[test]
    fn test_cloud_comparisons() {
        let comparisons = cloud_comparisons();
        assert!(!comparisons.is_empty());
        assert!(comparisons.iter().any(|c| c.provider.contains("OpenAI")));
    }

    #[test]
    fn test_system_capabilities_default() {
        let caps = SystemCapabilities::default();
        assert!(caps.gpu_vram_mb.is_none());
        assert_eq!(caps.detected_tier, HardwareTier::Budget);
    }

    #[test]
    fn hardware_debug_redacts_topology_models_and_recommendations() {
        let caps = SystemCapabilities {
            gpu_vram_mb: Some(24_576),
            gpu_name: Some("NVIDIA Tenant GPU jane@example.com sk-private".to_string()),
            system_ram_mb: 131_072,
            cpu_cores: 32,
            detected_tier: HardwareTier::Performance,
            cuda_available: true,
            rocm_available: false,
            metal_available: false,
        };
        let recommendation = ModelRecommendation::new(
            "private-model-jane@example.com:sk-private",
            "generation",
            "Use https://tenant.example with token sk-private",
        );
        let quality = TierQualityExpectations {
            tier: HardwareTier::Performance,
            title_quality_range: (90.0, 95.0),
            semantic_accuracy_range: (88.0, 92.0),
            revision_quality: "Best for /srv/private notes from jane@example.com".to_string(),
            latency_range_ms: (100, 200),
        };
        let settings = OllamaSettings {
            flash_attention: true,
            kv_cache_type: "kv-secret@example.com".to_string(),
            num_parallel: 2,
            max_loaded_models: 2,
        };
        let cloud = CloudComparison {
            provider: "Private provider https://tenant.example".to_string(),
            equivalent_tier: HardwareTier::Professional,
            cost_per_1k_notes: "$100 for account jane@example.com".to_string(),
            latency: "token sk-private latency".to_string(),
        };

        let debug = format!("{caps:?}\n{recommendation:?}\n{quality:?}\n{settings:?}\n{cloud:?}");

        for raw in [
            "NVIDIA Tenant GPU",
            "jane@example.com",
            "sk-private",
            "private-model",
            "https://tenant.example",
            "/srv/private",
            "kv-secret",
            "Private provider",
        ] {
            assert!(!debug.contains(raw), "debug output leaked {raw}: {debug}");
        }

        assert!(debug.contains("gpu_name_len"));
        assert!(debug.contains("model_len"));
        assert!(debug.contains("rationale_len"));
        assert!(debug.contains("revision_quality_len"));
        assert!(debug.contains("kv_cache_type_len"));
        assert!(debug.contains("provider_len"));
    }
}
