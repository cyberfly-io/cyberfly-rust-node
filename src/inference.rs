//! AI Inference Execution Module
//!
//! Provides TFLite-based inference execution with gossip-based job coordination.
//! This module enables cyberfly-rust-node to:
//! - Receive inference jobs via gossip
//! - Execute inference locally on whitelisted models
//! - Publish signed results back via gossip
//!
//! Architecture:
//! - Pull-based job assignment (workers pull jobs, no push)
//! - Ed25519 signed messages for authenticity
//! - In-memory job queue with capability-based scoring

use dashmap::DashMap;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use iroh::EndpointId;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;
use thiserror::Error;
use tracing::{debug, error, info, warn};

// Re-export for external use
pub use worker::InferenceWorker;
pub use scheduler::InferenceScheduler;

// ============================================================================
// Error Types
// ============================================================================

/// Inference module errors
#[derive(Error, Debug)]
pub enum InferenceError {
    #[error("Model not whitelisted: {0}")]
    ModelNotWhitelisted(String),

    #[error("Model not found: {0}")]
    ModelNotFound(String),

    #[error("Input load failed: {0}")]
    InputLoadFailed(String),

    #[error("Inference execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Job timeout exceeded")]
    Timeout,

    #[error("Signature verification failed: {0}")]
    SignatureVerification(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Deserialization error: {0}")]
    Deserialization(String),

    #[error("No suitable worker available")]
    NoWorkerAvailable,

    #[error("Job not found: {0}")]
    JobNotFound(String),

    #[error("Job already claimed")]
    JobAlreadyClaimed,
}

pub type Result<T> = std::result::Result<T, InferenceError>;

// ============================================================================
// Model Download Configuration
// ============================================================================

/// Default ONNX models to download on startup if not found locally.
/// These are optimized for VPS/server deployment (x86 with AVX2/512).
/// Mobile nodes use TFLite in a separate repository.
pub const DEFAULT_MODELS: &[(&str, &str, u64)] = &[
    // (model_name, download_url, expected_size_bytes)
    
    // Image Classification - MobileNet V2 (ONNX)
    (
        "mobilenet_v2",
        "https://github.com/onnx/models/raw/main/validated/vision/classification/mobilenet/model/mobilenetv2-7.onnx",
        14_000_000, // ~14MB
    ),
    
    // Object Detection - YOLOv8 Nano (ONNX)
    (
        "yolov8n",
        "https://github.com/jahongir7174/YOLOv8-onnx/raw/refs/heads/master/weights/v8_n.onnx",
        7_000_000, // ~7MB
    ),
    
    // Image Segmentation - SegFormer (ONNX)
    (
        "segformer",
        "https://github.com/onnx/models/raw/main/validated/vision/body_analysis/ultraface/models/version-RFB-640.onnx",
        2_000_000, // ~2MB (using ultraface as placeholder)
    ),
    
    // Text Recognition - EasyOCR English (ONNX, JPQD quantized)
    (
        "easyocr_en",
        "https://huggingface.co/asmud/EasyOCR-onnx/resolve/main/english_g2_jpqd.onnx",
        9_000_000, // ~8.54MB
    ),
    
    // Speech Processing - Silero VAD (ONNX)
    (
        "silero_vad",
        "https://raw.githubusercontent.com/snakers4/silero-vad/master/src/silero_vad/data/silero_vad.onnx",
        2_000_000, // ~2MB - Voice Activity Detection
    ),
    
    // Audio Denoising - DTLN (ONNX)
    (
        "dtln_denoise",
        "https://github.com/breizhn/DTLN/raw/refs/heads/master/pretrained_model/model_2.onnx",
        2_000_000, // ~2MB - Real-time speech denoising
    ),
];

/// Maximum model size to auto-download (50MB)
pub const MAX_AUTO_DOWNLOAD_SIZE: u64 = 50 * 1024 * 1024;

/// Download default models to the specified directory if not already present.
/// Returns a list of (model_name, success, message) tuples.
pub async fn ensure_models_downloaded(
    models_dir: &std::path::Path,
) -> Vec<(String, bool, String)> {
    let mut results = Vec::new();
    
    for (model_name, url, expected_size) in DEFAULT_MODELS {
        let model_path = models_dir.join(format!("{}.onnx", model_name));
        
        if model_path.exists() {
            info!("✓ Model {} already exists at {:?}", model_name, model_path);
            results.push((model_name.to_string(), true, "Already exists".to_string()));
            continue;
        }
        
        if *expected_size > MAX_AUTO_DOWNLOAD_SIZE {
            warn!("⚠️ Model {} exceeds max auto-download size ({} bytes), skipping", 
                  model_name, expected_size);
            results.push((
                model_name.to_string(),
                false,
                format!("Exceeds max size ({} bytes)", expected_size),
            ));
            continue;
        }
        
        info!("📥 Downloading model {} from {}...", model_name, url);
        
        match download_model(url, &model_path).await {
            Ok(size) => {
                info!("✅ Downloaded {} ({} bytes) to {:?}", model_name, size, model_path);
                results.push((model_name.to_string(), true, format!("Downloaded {} bytes", size)));
            }
            Err(e) => {
                error!("❌ Failed to download {}: {}", model_name, e);
                results.push((model_name.to_string(), false, e.to_string()));
            }
        }
    }
    
    results
}

/// Download a model from URL to the specified path.
async fn download_model(
    url: &str,
    path: &std::path::Path,
) -> std::result::Result<u64, Box<dyn std::error::Error + Send + Sync>> {
    use tokio::io::AsyncWriteExt;
    
    // Create HTTP client with timeout
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()?;
    
    // Download the model
    let response = client.get(url).send().await?;
    
    if !response.status().is_success() {
        return Err(format!("HTTP error: {}", response.status()).into());
    }
    
    let bytes = response.bytes().await?;
    let size = bytes.len() as u64;
    
    // Validate size
    if size > MAX_AUTO_DOWNLOAD_SIZE {
        return Err(format!("Downloaded file too large: {} bytes", size).into());
    }
    
    // Write to file
    let mut file = tokio::fs::File::create(path).await?;
    file.write_all(&bytes).await?;
    file.flush().await?;
    
    Ok(size)
}

// ============================================================================
// Data Models
// ============================================================================

/// Job execution status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum JobStatus {
    /// Job is waiting to be claimed
    Pending,
    /// Job is being executed by a node
    Running {
        node_id: String,
        started_at: i64,
    },
    /// Job completed successfully
    Completed {
        node_id: String,
        latency_ms: u64,
    },
    /// Job failed with an error
    Failed {
        reason: String,
    },
    /// Job exceeded max_latency_ms deadline
    TimedOut,
    /// Job was cancelled
    Cancelled,
}

impl Default for JobStatus {
    fn default() -> Self {
        JobStatus::Pending
    }
}

/// Inference job definition
///
/// Represents a unit of work to be executed by an inference worker.
/// Jobs are posted to the "inference-jobs" gossip topic and claimed by workers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceJob {
    /// Unique job identifier (UUID v4)
    pub job_id: String,
    /// Model name (must be whitelisted)
    pub model_name: String,
    /// URI to input data (blob hash, HTTP URL, or local path)
    pub input_uri: String,
    /// Maximum allowed execution time in milliseconds (SLA)
    pub max_latency_ms: u64,
    /// Current job status
    pub status: JobStatus,
    /// Unix timestamp (milliseconds) when job was created
    pub created_at: i64,
    /// EndpointId of the node that submitted the job
    pub requester: String,
}

impl InferenceJob {
    /// Create a new pending inference job
    pub fn new(
        job_id: String,
        model_name: String,
        input_uri: String,
        max_latency_ms: u64,
        requester: EndpointId,
    ) -> Self {
        Self {
            job_id,
            model_name,
            input_uri,
            max_latency_ms,
            status: JobStatus::Pending,
            created_at: chrono::Utc::now().timestamp_millis(),
            requester: requester.to_string(),
        }
    }

    /// Check if job has exceeded its deadline
    pub fn is_expired(&self) -> bool {
        let now = chrono::Utc::now().timestamp_millis();
        let age_ms = (now - self.created_at) as u64;
        age_ms > self.max_latency_ms
    }

    /// Get age of job in milliseconds
    pub fn age_ms(&self) -> u64 {
        let now = chrono::Utc::now().timestamp_millis();
        (now - self.created_at).max(0) as u64
    }
}

/// Extended node capabilities for inference workloads
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InferenceCapabilities {
    /// Number of available CPU cores
    pub cpu_cores: u32,
    /// Available RAM in megabytes
    pub ram_mb: u64,
    /// Whether this node supports TFLite inference
    pub supports_tflite: bool,
    /// Whether this node supports ONNX Runtime inference
    pub supports_onnx: bool,
    /// Optional GPU memory in megabytes (0 = no GPU)
    pub gpu_mem_mb: u64,
}

impl InferenceCapabilities {
    /// Create capabilities from system info
    pub fn from_system() -> Self {
        Self {
            cpu_cores: num_cpus(),
            ram_mb: available_ram_mb(),
            supports_tflite: true, // Enabled by feature flag
            supports_onnx: false,  // ONNX not yet implemented
            gpu_mem_mb: 0,         // GPU detection not implemented
        }
    }

    /// Score this node's capability for a given job (higher = better fit)
    pub fn score_for_job(&self, job: &InferenceJob) -> f64 {
        if !self.supports_tflite {
            return 0.0;
        }

        let mut score = 1.0;

        // Prefer nodes with more CPU cores for parallel workloads
        score += (self.cpu_cores as f64) * 0.1;

        // Prefer nodes with more RAM
        score += (self.ram_mb as f64 / 1024.0) * 0.05;

        // Penalize jobs that are close to timeout
        let remaining_ms = job.max_latency_ms.saturating_sub(job.age_ms());
        if remaining_ms < 100 {
            score *= 0.1; // Nearly expired, low priority
        } else if remaining_ms < 1000 {
            score *= 0.5; // Under 1 second remaining
        }

        score
    }
}

/// Signed inference result
///
/// Published to the "inference-results" gossip topic after job completion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceResult {
    /// Job ID this result corresponds to
    pub job_id: String,
    /// EndpointId of the node that executed the job
    pub node_id: String,
    /// URI to output data (blob hash or local path)
    pub output_uri: String,
    /// Actual execution latency in milliseconds
    pub latency_ms: u64,
    /// Unix timestamp when result was produced
    pub completed_at: i64,
    /// Whether execution was successful
    pub success: bool,
    /// Error message if success is false
    pub error: Option<String>,
}

impl InferenceResult {
    /// Create a successful result
    pub fn success(job_id: String, node_id: EndpointId, output_uri: String, latency_ms: u64) -> Self {
        Self {
            job_id,
            node_id: node_id.to_string(),
            output_uri,
            latency_ms,
            completed_at: chrono::Utc::now().timestamp_millis(),
            success: true,
            error: None,
        }
    }

    /// Create a failed result
    pub fn failure(job_id: String, node_id: EndpointId, error: String, latency_ms: u64) -> Self {
        Self {
            job_id,
            node_id: node_id.to_string(),
            output_uri: String::new(),
            latency_ms,
            completed_at: chrono::Utc::now().timestamp_millis(),
            success: false,
            error: Some(error),
        }
    }
}

// ============================================================================
// Signed Message Types
// ============================================================================

/// Signed inference message wrapper
///
/// All inference messages are signed with Ed25519 for authenticity.
/// Uses postcard serialization for efficiency (same as gossip_discovery).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedInferenceMessage {
    /// Ed25519 public key of sender (32 bytes)
    pub from: Vec<u8>,
    /// Postcard-serialized payload
    pub data: Vec<u8>,
    /// Ed25519 signature over data (64 bytes)
    pub signature: Vec<u8>,
}

impl SignedInferenceMessage {
    /// Sign and encode an inference payload
    pub fn sign_and_encode<T: Serialize>(secret_key: &SigningKey, payload: &T) -> Result<Vec<u8>> {
        let data = postcard::to_stdvec(payload)
            .map_err(|e| InferenceError::Serialization(e.to_string()))?;

        let signature = secret_key.sign(&data);
        let from = secret_key.verifying_key();

        let signed = Self {
            from: from.to_bytes().to_vec(),
            data,
            signature: signature.to_bytes().to_vec(),
        };

        postcard::to_stdvec(&signed)
            .map_err(|e| InferenceError::Serialization(e.to_string()))
    }

    /// Verify signature and decode payload
    pub fn verify_and_decode<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<(VerifyingKey, T)> {
        let signed: Self = postcard::from_bytes(bytes)
            .map_err(|e| InferenceError::Deserialization(e.to_string()))?;

        // Verify public key
        let from_bytes: [u8; 32] = signed.from.try_into()
            .map_err(|_| InferenceError::SignatureVerification("Invalid public key length".into()))?;
        let key = VerifyingKey::from_bytes(&from_bytes)
            .map_err(|e| InferenceError::SignatureVerification(e.to_string()))?;

        // Verify signature
        let sig_bytes: [u8; 64] = signed.signature.try_into()
            .map_err(|_| InferenceError::SignatureVerification("Invalid signature length".into()))?;
        let signature = Signature::from_bytes(&sig_bytes);

        key.verify(&signed.data, &signature)
            .map_err(|e| InferenceError::SignatureVerification(e.to_string()))?;

        // Decode payload
        let payload: T = postcard::from_bytes(&signed.data)
            .map_err(|e| InferenceError::Deserialization(e.to_string()))?;

        Ok((key, payload))
    }
}

/// Inference gossip message types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InferenceMessage {
    /// New job posted (broadcast to all nodes)
    JobPosted(InferenceJob),
    /// Job claimed by a worker (prevents duplicate execution)
    JobClaimed {
        job_id: String,
        node_id: String,
    },
    /// Job result published
    ResultPublished(InferenceResult),
    /// Job cancelled by requester
    JobCancelled {
        job_id: String,
        reason: String,
    },
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Get number of CPU cores
fn num_cpus() -> u32 {
    std::thread::available_parallelism()
        .map(|p| p.get() as u32)
        .unwrap_or(1)
}

/// Get available RAM in megabytes (placeholder - returns conservative estimate)
fn available_ram_mb() -> u64 {
    // In production, use sysinfo crate or platform-specific APIs
    // For now, return a conservative default
    4096 // 4GB default
}

// ============================================================================
// Scheduler Module
// ============================================================================

pub mod scheduler {
    use super::*;
    use tokio::sync::RwLock;

    /// Inference job scheduler
    ///
    /// Manages job queue and worker assignment using pull-based model.
    /// Workers call `pull_next_job()` to get work.
    pub struct InferenceScheduler {
        /// Pending jobs waiting to be claimed
        pending_jobs: DashMap<String, InferenceJob>,
        /// Jobs currently being executed (job_id -> (job, started_at))
        running_jobs: DashMap<String, (InferenceJob, Instant)>,
        /// Completed job results (kept for retrieval)
        completed_results: DashMap<String, InferenceResult>,
        /// Local node capabilities
        local_capabilities: InferenceCapabilities,
        /// Whitelisted model names
        model_whitelist: RwLock<HashSet<String>>,
        /// Local node ID
        node_id: EndpointId,
    }

    impl InferenceScheduler {
        /// Create a new scheduler with default model whitelist
        pub fn new(node_id: EndpointId, capabilities: InferenceCapabilities) -> Self {
            let mut whitelist = HashSet::new();
            // Default whitelisted models (can be extended via config)
            whitelist.insert("mobilenet_v2".to_string());
            whitelist.insert("efficientnet_lite0".to_string());
            whitelist.insert("ssd_mobilenet".to_string());

            Self {
                pending_jobs: DashMap::new(),
                running_jobs: DashMap::new(),
                completed_results: DashMap::new(),
                local_capabilities: capabilities,
                model_whitelist: RwLock::new(whitelist),
                node_id,
            }
        }

        /// Add models to whitelist
        pub async fn add_whitelisted_models(&self, models: Vec<String>) {
            let mut whitelist = self.model_whitelist.write().await;
            for model in models {
                whitelist.insert(model);
            }
        }

        /// Check if a model is whitelisted
        pub async fn is_model_whitelisted(&self, model_name: &str) -> bool {
            self.model_whitelist.read().await.contains(model_name)
        }

        /// Add a new job to the pending queue
        pub async fn add_job(&self, job: InferenceJob) -> Result<()> {
            // Verify model is whitelisted
            if !self.is_model_whitelisted(&job.model_name).await {
                return Err(InferenceError::ModelNotWhitelisted(job.model_name.clone()));
            }

            // Check if job already exists
            if self.pending_jobs.contains_key(&job.job_id)
                || self.running_jobs.contains_key(&job.job_id)
            {
                debug!(job_id = %job.job_id, "Job already exists, ignoring");
                return Ok(());
            }

            info!(
                job_id = %job.job_id,
                model = %job.model_name,
                max_latency_ms = job.max_latency_ms,
                "Added new inference job to queue"
            );

            self.pending_jobs.insert(job.job_id.clone(), job);
            crate::metrics::INFERENCE_JOBS_PENDING.inc();

            Ok(())
        }

        /// Pull the next suitable job for local execution
        ///
        /// Returns the highest-scored job that this node can handle.
        /// The job is moved from pending to running state.
        pub async fn pull_next_job(&self) -> Option<InferenceJob> {
            // Score all pending jobs
            let mut best_job: Option<(String, f64)> = None;

            for entry in self.pending_jobs.iter() {
                let job = entry.value();

                // Skip expired jobs
                if job.is_expired() {
                    continue;
                }

                let score = self.local_capabilities.score_for_job(job);
                if score > 0.0 {
                    match &best_job {
                        None => best_job = Some((job.job_id.clone(), score)),
                        Some((_, best_score)) if score > *best_score => {
                            best_job = Some((job.job_id.clone(), score))
                        }
                        _ => {}
                    }
                }
            }

            // Claim the best job
            if let Some((job_id, score)) = best_job {
                if let Some((_, mut job)) = self.pending_jobs.remove(&job_id) {
                    job.status = JobStatus::Running {
                        node_id: self.node_id.to_string(),
                        started_at: chrono::Utc::now().timestamp_millis(),
                    };

                    info!(
                        job_id = %job.job_id,
                        score = score,
                        "Claimed inference job"
                    );

                    self.running_jobs.insert(job.job_id.clone(), (job.clone(), Instant::now()));
                    crate::metrics::INFERENCE_JOBS_PENDING.dec();
                    crate::metrics::INFERENCE_JOBS_RUNNING.inc();

                    return Some(job);
                }
            }

            None
        }

        /// Mark a job as claimed by another node
        pub fn mark_claimed(&self, job_id: &str, node_id: &str) {
            if let Some((_, mut job)) = self.pending_jobs.remove(job_id) {
                job.status = JobStatus::Running {
                    node_id: node_id.to_string(),
                    started_at: chrono::Utc::now().timestamp_millis(),
                };
                self.running_jobs.insert(job_id.to_string(), (job, Instant::now()));
                crate::metrics::INFERENCE_JOBS_PENDING.dec();
            }
        }

        /// Record a completed result
        pub fn record_result(&self, result: InferenceResult) {
            let job_id = result.job_id.clone();
            self.running_jobs.remove(&job_id);
            self.completed_results.insert(job_id, result);
            crate::metrics::INFERENCE_JOBS_RUNNING.dec();
            crate::metrics::INFERENCE_JOBS_COMPLETED.inc();
        }

        /// Check for timed-out jobs and return them for retry
        pub fn check_timeouts(&self) -> Vec<InferenceJob> {
            let mut timed_out = Vec::new();
            let now = Instant::now();

            // Collect timed-out job IDs first to avoid holding lock
            let expired_ids: Vec<String> = self
                .running_jobs
                .iter()
                .filter_map(|entry| {
                    let (job, started_at) = entry.value();
                    let elapsed_ms = now.duration_since(*started_at).as_millis() as u64;
                    if elapsed_ms > job.max_latency_ms {
                        Some(entry.key().clone())
                    } else {
                        None
                    }
                })
                .collect();

            // Move expired jobs back to pending for retry
            for job_id in expired_ids {
                if let Some((_, (mut job, _))) = self.running_jobs.remove(&job_id) {
                    warn!(job_id = %job.job_id, "Job timed out, marking for retry");
                    job.status = JobStatus::TimedOut;
                    timed_out.push(job);
                    crate::metrics::INFERENCE_JOBS_RUNNING.dec();
                    crate::metrics::INFERENCE_JOBS_TIMEOUTS.inc();
                }
            }

            timed_out
        }

        /// Get pending job count
        pub fn pending_count(&self) -> usize {
            self.pending_jobs.len()
        }

        /// Get running job count
        pub fn running_count(&self) -> usize {
            self.running_jobs.len()
        }

        /// Get a result by job ID
        pub fn get_result(&self, job_id: &str) -> Option<InferenceResult> {
            self.completed_results.get(job_id).map(|r| r.value().clone())
        }

        /// Get local capabilities
        pub fn capabilities(&self) -> &InferenceCapabilities {
            &self.local_capabilities
        }
    }
}

// ============================================================================
// Worker Module
// ============================================================================

pub mod worker {
    use super::*;
    use std::path::PathBuf;
    use std::time::Duration;
    use tokio::time::sleep;

    /// ONNX inference worker
    ///
    /// Pulls jobs from scheduler, executes inference, and publishes results.
    pub struct InferenceWorker {
        /// Directory containing .onnx model files
        model_dir: PathBuf,
        /// Local node ID
        node_id: EndpointId,
        /// Signing key for results
        secret_key: SigningKey,
        /// Blob store for persisting results
        blob_store: iroh_blobs::store::fs::FsStore,
        /// Whether worker is running
        running: std::sync::atomic::AtomicBool,
    }

    impl InferenceWorker {
        /// Create a new inference worker
        pub fn new(
            model_dir: PathBuf,
            node_id: EndpointId,
            secret_key: SigningKey,
            blob_store: iroh_blobs::store::fs::FsStore,
        ) -> Self {
            Self {
                model_dir,
                node_id,
                secret_key,
                blob_store,
                running: std::sync::atomic::AtomicBool::new(false),
            }
        }

        /// Execute inference for a job
        ///
        /// This is a placeholder implementation. In production:
        /// 1. Load model from disk (cached)
        /// 2. Load input from input_uri
        /// 3. Run TFLite interpreter
        /// 4. Extract output tensors
        /// 5. Upload output to blob storage
        pub async fn execute(&self, job: &InferenceJob) -> Result<InferenceResult> {
            let start = Instant::now();

            info!(
                job_id = %job.job_id,
                model = %job.model_name,
                "Starting inference execution"
            );

            // Check model exists
            let model_path = self.model_dir.join(format!("{}.onnx", job.model_name));
            if !model_path.exists() {
                return Err(InferenceError::ModelNotFound(job.model_name.clone()));
            }

            // Placeholder: In production, use tflite-rs or tract here
            // For now, simulate inference with a small delay
            let execution_result = self.execute_tflite_inference(&model_path, &job.input_uri).await;

            let latency_ms = start.elapsed().as_millis() as u64;

            match execution_result {
                Ok(output_uri) => {
                    info!(
                        job_id = %job.job_id,
                        latency_ms = latency_ms,
                        "Inference completed successfully"
                    );
                    crate::metrics::INFERENCE_LATENCY_MS.observe(latency_ms as f64);
                    Ok(InferenceResult::success(
                        job.job_id.clone(),
                        self.node_id,
                        output_uri,
                        latency_ms,
                    ))
                }
                Err(e) => {
                    error!(
                        job_id = %job.job_id,
                        error = %e,
                        "Inference execution failed"
                    );
                    Ok(InferenceResult::failure(
                        job.job_id.clone(),
                        self.node_id,
                        e.to_string(),
                        latency_ms,
                    ))
                }
            }
        }

        /// Execute TFLite inference using tract
        ///
        /// Loads the model, processes input, runs inference, and returns output URI.
        async fn execute_tflite_inference(
            &self,
            model_path: &PathBuf,
            input_uri: &str,
        ) -> Result<String> {
            use tract_onnx::prelude::*;
            
            debug!(
                model_path = %model_path.display(),
                input_uri = %input_uri,
                "Executing inference with tract"
            );

            // Load input data from URI
            let input_data = self.load_input_data(input_uri).await?;
            
            // Load and optimize the model using tract
            // Tract-onnx can load ONNX models; for TFLite, we read raw file and process
            let extension = model_path.extension()
                .and_then(|e| e.to_str())
                .unwrap_or("tflite");
            
            let model = if extension == "onnx" {
                // Load ONNX model
                tract_onnx::onnx()
                    .model_for_path(model_path)
                    .map_err(|e| InferenceError::ModelNotFound(format!("Failed to load ONNX model: {}", e)))?
                    .into_optimized()
                    .map_err(|e| InferenceError::ExecutionFailed(format!("Model optimization failed: {}", e)))?
                    .into_runnable()
                    .map_err(|e| InferenceError::ExecutionFailed(format!("Model not runnable: {}", e)))?
            } else {
                // For TFLite, use tract-tensorflow or fallback to simple output
                // Note: tract-onnx doesn't natively support TFLite
                // Return a placeholder result for now - full TFLite support requires tract-tensorflow
                warn!(
                    model_path = %model_path.display(),
                    "TFLite native execution not available, returning simulated result"
                );
                
                // Simulate execution and return placeholder
                let output_hash = format!("{:x}", md5::compute(&input_data));
                let output_uri = format!("data:simulated;model={};hash={}", 
                    model_path.file_stem().unwrap_or_default().to_string_lossy(),
                    output_hash
                );
                return Ok(output_uri);
            };

            // Get model input shape from the underlying model
            let input_fact = model.model().input_fact(0)
                .map_err(|e| InferenceError::ExecutionFailed(format!("Failed to get input fact: {}", e)))?
                .clone();
            
            // Create input tensor from data - use default 224x224x3 image shape if dynamic
            let input_shape: Vec<usize> = input_fact.shape.as_concrete()
                .map(|s| s.to_vec())
                .unwrap_or_else(|| vec![1, 224, 224, 3]);
            
            let expected_size: usize = input_shape.iter().product();
            
            // Convert u8 to f32 (normalize to 0-1 range for images)
            let float_data: Vec<f32> = input_data.iter()
                .take(expected_size)
                .map(|&x| x as f32 / 255.0)
                .collect();
            
            // Pad or truncate to expected size
            let mut padded = vec![0.0f32; expected_size];
            let copy_len = float_data.len().min(expected_size);
            padded[..copy_len].copy_from_slice(&float_data[..copy_len]);
            
            let input_tensor: Tensor = Tensor::from_shape(&input_shape, &padded)
                .map_err(|e| InferenceError::InputLoadFailed(format!("Failed to create tensor: {}", e)))?;

            // Run inference and process output within a block to ensure outputs (non-Send) 
            // are dropped before the await point
            let output_json = {
                let outputs = model.run(tvec![input_tensor.into()])
                    .map_err(|e| InferenceError::ExecutionFailed(format!("Inference failed: {}", e)))?;

                // Process output synchronously to get JSON string
                self.process_outputs(&outputs)?
            };

            // Store result async
            let output_uri = self.store_blob(output_json).await?;
            
            info!(
                model_path = %model_path.display(),
                output_uri = %output_uri,
                "Inference completed successfully"
            );

            Ok(output_uri)
        }

        /// Load input data from URI (blob:// or file://)
        async fn load_input_data(&self, input_uri: &str) -> Result<Vec<u8>> {
            if input_uri.starts_with("blob://") {
                // Load from blob storage (placeholder - would use iroh blobs)
                let hash = input_uri.strip_prefix("blob://").unwrap();
                debug!(hash = %hash, "Loading input from blob storage");
                // For now, return empty - in production, fetch from blob store
                Ok(vec![0u8; 224 * 224 * 3]) // Default image size
            } else if input_uri.starts_with("file://") {
                let path = input_uri.strip_prefix("file://").unwrap();
                tokio::fs::read(path).await
                    .map_err(|e| InferenceError::InputLoadFailed(format!("Failed to read file: {}", e)))
            } else if input_uri.starts_with("http://") || input_uri.starts_with("https://") {
                // Download from URL
                let response = reqwest::get(input_uri).await
                    .map_err(|e| InferenceError::InputLoadFailed(format!("HTTP request failed: {}", e)))?;
                response.bytes().await
                    .map(|b| b.to_vec())
                    .map_err(|e| InferenceError::InputLoadFailed(format!("Failed to read response: {}", e)))
            } else {
                // Assume base64 encoded data
                base64::Engine::decode(&base64::engine::general_purpose::STANDARD, input_uri)
                    .map_err(|e| InferenceError::InputLoadFailed(format!("Base64 decode failed: {}", e)))
            }
        }

        /// Prepare input tensor from raw data
        fn prepare_input_tensor(
            &self,
            data: &[u8],
            input_fact: &tract_onnx::prelude::TypedFact,
        ) -> std::result::Result<tract_onnx::prelude::Tensor, String> {
            use tract_onnx::prelude::*;
            
            // Get expected shape from model
            let shape = input_fact.shape.as_concrete()
                .ok_or_else(|| "Dynamic input shape not supported".to_string())?;
            
            // Calculate expected size
            let expected_size: usize = shape.iter().product();
            
            // Create tensor based on data type
            if input_fact.datum_type == f32::datum_type() {
                // Convert u8 to f32 (normalize to 0-1 range for images)
                let float_data: Vec<f32> = data.iter()
                    .take(expected_size)
                    .map(|&x| x as f32 / 255.0)
                    .collect();
                
                // Pad or truncate to expected size
                let mut padded = vec![0.0f32; expected_size];
                let copy_len = float_data.len().min(expected_size);
                padded[..copy_len].copy_from_slice(&float_data[..copy_len]);
                
                Tensor::from_shape(&shape, &padded)
                    .map_err(|e| format!("Failed to create tensor: {}", e))
            } else if input_fact.datum_type == u8::datum_type() {
                // Use raw u8 data
                let mut padded = vec![0u8; expected_size];
                let copy_len = data.len().min(expected_size);
                padded[..copy_len].copy_from_slice(&data[..copy_len]);
                
                Tensor::from_shape(&shape, &padded)
                    .map_err(|e| format!("Failed to create tensor: {}", e))
            } else {
                Err(format!("Unsupported input type: {:?}", input_fact.datum_type))
            }
        }

        /// Convert output tensor(s) to JSON string (Sync)
        ///
        /// This must be synchronous to avoid holding Rc<Tensor> (non-Send) across await points.
        fn process_outputs(
            &self,
            outputs: &tract_onnx::prelude::TVec<tract_onnx::prelude::TValue>,
        ) -> Result<String> {
            // Serialize output tensors to JSON
            let mut output_data = serde_json::Map::new();
            
            for (i, output) in outputs.iter().enumerate() {
                let tensor = output.to_array_view::<f32>()
                    .map(|arr| {
                        let values: Vec<f32> = arr.iter().cloned().collect();
                        serde_json::json!({
                            "shape": arr.shape(),
                            "values": values
                        })
                    })
                    .unwrap_or_else(|_| {
                        // Try as i64
                        output.to_array_view::<i64>()
                            .map(|arr| {
                                let values: Vec<i64> = arr.iter().cloned().collect();
                                serde_json::json!({
                                    "shape": arr.shape(),
                                    "values": values
                                })
                            })
                            .unwrap_or_else(|_| serde_json::json!({"error": "unsupported output type"}))
                    });
                
                output_data.insert(format!("output_{}", i), tensor);
            }
            
            serde_json::to_string(&output_data)
                .map_err(|e| InferenceError::ExecutionFailed(format!("JSON serialization failed: {}", e)))
        }

        /// Store output JSON as blob and return URI (Async)
        async fn store_blob(&self, output_json: String) -> Result<String> {
            // Store output as Iroh blob and return hash URI for download
            let output_bytes = output_json.as_bytes().to_vec();
            let blobs = self.blob_store.blobs();
            let tag = blobs.add_bytes(output_bytes).await
                .map_err(|e| InferenceError::ExecutionFailed(format!("Failed to store output blob: {}", e)))?;
            
            let blob_hash = tag.hash.to_string();
            let output_uri = format!("blob://{}", blob_hash);
            
            info!(
                output_size = output_json.len(),
                blob_hash = %blob_hash,
                output_uri = %output_uri,
                "Inference output stored as blob"
            );
            
            Ok(output_uri)
        }

        /// Sign and encode an inference result
        pub fn sign_result(&self, result: &InferenceResult) -> Result<Vec<u8>> {
            let message = InferenceMessage::ResultPublished(result.clone());
            SignedInferenceMessage::sign_and_encode(&self.secret_key, &message)
        }

        /// Run the worker loop
        ///
        /// Continuously pulls jobs from scheduler, executes them,
        /// and broadcasts results via the provided sender.
        pub async fn run(
            &self,
            scheduler: Arc<scheduler::InferenceScheduler>,
            result_tx: tokio::sync::mpsc::UnboundedSender<InferenceResult>,
        ) -> Result<()> {
            use std::sync::atomic::Ordering;

            self.running.store(true, Ordering::SeqCst);
            info!("🧠 Inference worker started");

            while self.running.load(Ordering::SeqCst) {
                // Check for timed-out jobs
                let timeouts = scheduler.check_timeouts();
                for job in timeouts {
                    let result = InferenceResult::failure(
                        job.job_id.clone(),
                        self.node_id,
                        "Execution timeout".to_string(),
                        job.max_latency_ms,
                    );
                    if result_tx.send(result).is_err() {
                        warn!("Failed to send timeout result - channel closed");
                    }
                }

                // Try to pull next job
                if let Some(job) = scheduler.pull_next_job().await {
                    match self.execute(&job).await {
                        Ok(result) => {
                            scheduler.record_result(result.clone());
                            if result_tx.send(result).is_err() {
                                warn!("Failed to send result - channel closed");
                            }
                        }
                        Err(e) => {
                            error!(job_id = %job.job_id, error = %e, "Job execution failed");
                            let result = InferenceResult::failure(
                                job.job_id,
                                self.node_id,
                                e.to_string(),
                                0,
                            );
                            scheduler.record_result(result.clone());
                            if result_tx.send(result).is_err() {
                                warn!("Failed to send error result - channel closed");
                            }
                        }
                    }
                } else {
                    // No jobs available, wait before polling again
                    sleep(Duration::from_millis(100)).await;
                }
            }

            info!("🧠 Inference worker stopped");
            Ok(())
        }

        /// Stop the worker loop
        pub fn stop(&self) {
            use std::sync::atomic::Ordering;
            self.running.store(false, Ordering::SeqCst);
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Generate a valid EndpointId from a random signing key
    fn generate_valid_endpoint_id() -> EndpointId {
        let mut rng = rand::thread_rng();
        let secret_key = SigningKey::generate(&mut rng);
        let public_key_bytes = secret_key.verifying_key().to_bytes();
        let iroh_public_key = iroh::PublicKey::from_bytes(&public_key_bytes).unwrap();
        EndpointId::from(iroh_public_key)
    }

    #[test]
    fn test_inference_job_creation() {
        let node_id = generate_valid_endpoint_id();
        let job = InferenceJob::new(
            "test-job-123".to_string(),
            "mobilenet_v2".to_string(),
            "blob://abc123".to_string(),
            5000,
            node_id,
        );

        assert_eq!(job.job_id, "test-job-123");
        assert_eq!(job.model_name, "mobilenet_v2");
        assert_eq!(job.max_latency_ms, 5000);
        assert!(matches!(job.status, JobStatus::Pending));
        assert!(!job.is_expired());
    }

    #[test]
    fn test_inference_result_serialization() {
        let node_id = generate_valid_endpoint_id();
        let result = InferenceResult::success(
            "job-456".to_string(),
            node_id,
            "blob://output123".to_string(),
            42,
        );

        // Serialize
        let bytes = postcard::to_stdvec(&result).unwrap();

        // Deserialize
        let decoded: InferenceResult = postcard::from_bytes(&bytes).unwrap();

        assert_eq!(decoded.job_id, result.job_id);
        assert_eq!(decoded.output_uri, result.output_uri);
        assert_eq!(decoded.latency_ms, 42);
        assert!(decoded.success);
    }

    #[test]
    fn test_signed_message_roundtrip() {
        let mut rng = rand::thread_rng();
        let secret_key = SigningKey::generate(&mut rng);

        let job = InferenceJob {
            job_id: "test-123".to_string(),
            model_name: "mobilenet_v2".to_string(),
            input_uri: "blob://input".to_string(),
            max_latency_ms: 1000,
            status: JobStatus::Pending,
            created_at: 0,
            requester: "test-node".to_string(),
        };

        let message = InferenceMessage::JobPosted(job.clone());

        // Sign and encode
        let encoded = SignedInferenceMessage::sign_and_encode(&secret_key, &message).unwrap();

        // Verify and decode
        let (key, decoded): (_, InferenceMessage) =
            SignedInferenceMessage::verify_and_decode(&encoded).unwrap();

        // Check key matches
        assert_eq!(key, secret_key.verifying_key());

        // Check message matches
        if let InferenceMessage::JobPosted(decoded_job) = decoded {
            assert_eq!(decoded_job.job_id, job.job_id);
            assert_eq!(decoded_job.model_name, job.model_name);
        } else {
            panic!("Wrong message type");
        }
    }

    #[test]
    fn test_capabilities_scoring() {
        let caps = InferenceCapabilities {
            cpu_cores: 8,
            ram_mb: 16384,
            supports_tflite: true,
            supports_onnx: false,
            gpu_mem_mb: 0,
        };

        let node_id = generate_valid_endpoint_id();
        let job = InferenceJob::new(
            "test".to_string(),
            "model".to_string(),
            "input".to_string(),
            5000,
            node_id,
        );

        let score = caps.score_for_job(&job);
        assert!(score > 0.0);

        // Node without TFLite support should score 0
        let no_tflite = InferenceCapabilities {
            supports_tflite: false,
            ..caps
        };
        assert_eq!(no_tflite.score_for_job(&job), 0.0);
    }

    #[tokio::test]
    async fn test_scheduler_add_and_pull() {
        let node_id = generate_valid_endpoint_id();
        let caps = InferenceCapabilities::from_system();
        let scheduler = scheduler::InferenceScheduler::new(node_id, caps);

        // Add whitelisted model
        scheduler.add_whitelisted_models(vec!["test_model".to_string()]).await;

        // Create and add job
        let job = InferenceJob::new(
            "job-1".to_string(),
            "test_model".to_string(),
            "blob://input".to_string(),
            5000,
            node_id,
        );

        scheduler.add_job(job).await.unwrap();
        assert_eq!(scheduler.pending_count(), 1);

        // Pull job
        let pulled = scheduler.pull_next_job().await;
        assert!(pulled.is_some());
        assert_eq!(pulled.unwrap().job_id, "job-1");
        assert_eq!(scheduler.pending_count(), 0);
        assert_eq!(scheduler.running_count(), 1);
    }

    #[tokio::test]
    async fn test_scheduler_rejects_non_whitelisted() {
        let node_id = generate_valid_endpoint_id();
        let caps = InferenceCapabilities::from_system();
        let scheduler = scheduler::InferenceScheduler::new(node_id, caps);

        let job = InferenceJob::new(
            "job-2".to_string(),
            "unknown_model".to_string(),
            "blob://input".to_string(),
            5000,
            node_id,
        );

        let result = scheduler.add_job(job).await;
        assert!(matches!(result, Err(InferenceError::ModelNotWhitelisted(_))));
    }
}

