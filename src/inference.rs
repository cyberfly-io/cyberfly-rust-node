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
use tracing::{debug, error, info, warn, trace};

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
    
    // Image Classification - MobileNet V4 (ONNX) - Latest and most accurate
    (
        "mobilenet_v4",
        "https://raw.githubusercontent.com/cyberfly-io/cv_models/refs/heads/main/mobilenetv4.onnx",
        20_000_000, // ~20MB
    ),
    
    // Object Detection - YOLOv11 Nano (ONNX)
    (
        "yolo11n",
        "https://raw.githubusercontent.com/cyberfly-io/cv_models/refs/heads/main/yolo11n.onnx",
        8_000_000, // ~8MB (approximate)
    ),
    
    // Image Segmentation - SegFormer B0 (Cityscapes)
    (
        "segformer",
        "https://raw.githubusercontent.com/cyberfly-io/cv_models/refs/heads/main/segformer.onnx",
        15_000_000, // ~15MB
    ),
    
    // PaddleOCR v5 Text Detection (MNN format for ocr-rs)
    (
        "paddleocr_det",
        "https://raw.githubusercontent.com/zibo-chen/rust-paddle-ocr/refs/heads/next/models/PP-OCRv5_mobile_det.mnn",
        3_000_000, // ~3MB
    ),
    
    // PaddleOCR v5 English Recognition (MNN format for ocr-rs)
    (
        "paddleocr_rec_en",
        "https://raw.githubusercontent.com/zibo-chen/rust-paddle-ocr/refs/heads/next/models/en_PP-OCRv5_mobile_rec_infer.mnn",
        5_000_000, // ~5MB
    ),
    
    // PaddleOCR v5 Character Dictionary (English)
    (
        "paddleocr_keys_en",
        "https://raw.githubusercontent.com/zibo-chen/rust-paddle-ocr/refs/heads/next/models/ppocr_keys_en.txt",
        100_000, // ~100KB
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
pub const MAX_AUTO_DOWNLOAD_SIZE: u64 = 100 * 1024 * 1024;

/// Download default models to the specified directory if not already present.
/// Returns a list of (model_name, success, message) tuples.
pub async fn ensure_models_downloaded(
    models_dir: &std::path::Path,
) -> Vec<(String, bool, String)> {
    let mut results = Vec::new();
    
    for (model_name, url, expected_size) in DEFAULT_MODELS {
        // Extract file extension from URL (supports .onnx, .mnn, .txt, etc.)
        let extension = url.rsplit('.').next().unwrap_or("onnx");
        let model_path = models_dir.join(format!("{}.{}", model_name, extension));
        
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
    
    // Download ImageNet labels JSON if not present
    let labels_path = models_dir.join("imagenet_labels.json");
    if !labels_path.exists() {
        let labels_url = "https://raw.githubusercontent.com/anishathalye/imagenet-simple-labels/master/imagenet-simple-labels.json";
        info!("📥 Downloading ImageNet labels from {}...", labels_url);
        match download_model(labels_url, &labels_path).await {
            Ok(size) => {
                info!("✅ Downloaded ImageNet labels ({} bytes) to {:?}", size, labels_path);
            }
            Err(e) => {
                error!("❌ Failed to download ImageNet labels: {}", e);
            }
        }
    } else {
        info!("✓ ImageNet labels already exist at {:?}", labels_path);
    }
    
    // Download PaddleOCR v5 English dictionary for CTC decoding
    // Use the official PaddleOCR ppocrv5 English dict from the PaddlePaddle repo.
    // If an existing file is present but looks suspiciously small (likely a different dict),
    // re-download and overwrite it.
    let dict_path = models_dir.join("paddleocr_dict_en.txt");
    let dict_url = "https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.4.0/paddle/PP-OCRv5/rec/en_PP-OCRv5_rec_mobile_infer/ppocrv5_en_dict.txt";

    let mut need_download = false;
    if dict_path.exists() {
        // Check approximate line count to detect stale/small dicts
        match tokio::fs::read_to_string(&dict_path).await {
            Ok(content) => {
                let lines = content.lines().count();
                if lines < 1000 {
                    warn!("Existing PaddleOCR keys at {:?} looks small ({} lines); re-downloading.", dict_path, lines);
                    need_download = true;
                } else {
                    info!("✓ PaddleOCR keys already exist at {:?} ({} lines)", dict_path, lines);
                }
            }
            Err(e) => {
                warn!("Failed to read existing PaddleOCR keys at {:?}: {} — will attempt download", dict_path, e);
                need_download = true;
            }
        }
    } else {
        need_download = true;
    }

    if need_download {
        info!("📥 Downloading PaddleOCR English keys (ppocr_keys_v1) from {}...", dict_url);
        match download_model(dict_url, &dict_path).await {
            Ok(size) => {
                info!("✅ Downloaded PaddleOCR keys ({} bytes) to {:?}", size, dict_path);
            }
            Err(e) => {
                error!("❌ Failed to download PaddleOCR keys: {}", e);
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
// ImageNet Labels and Post-Processing
// ============================================================================

/// ImageNet 1000 class labels loaded from JSON file
/// Source: https://github.com/anishathalye/imagenet-simple-labels
/// The labels are loaded lazily from data/iroh/models/imagenet_labels.json
fn get_imagenet_label(class_id: usize) -> String {
    use std::sync::OnceLock;
    static LABELS: OnceLock<Vec<String>> = OnceLock::new();
    
    let labels = LABELS.get_or_init(|| {
        let labels_path = std::path::Path::new("./data/iroh/models/imagenet_labels.json");
        if labels_path.exists() {
            if let Ok(content) = std::fs::read_to_string(labels_path) {
                if let Ok(parsed) = serde_json::from_str::<Vec<String>>(&content) {
                    return parsed;
                }
            }
        }
        // Fallback: empty vector (will return "unknown" for all classes)
        vec![]
    });
    
    labels.get(class_id).cloned().unwrap_or_else(|| format!("class_{}", class_id))
}

/// Apply softmax to convert logits to probabilities
fn softmax(logits: &[f32]) -> Vec<f32> {
    // Find max for numerical stability
    let max_logit = logits.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    
    // Compute exp(x - max) for each element
    let exp_values: Vec<f32> = logits.iter()
        .map(|&x| (x - max_logit).exp())
        .collect();
    
    // Sum of all exp values
    let sum: f32 = exp_values.iter().sum();
    
    // Normalize to get probabilities
    exp_values.iter().map(|&x| x / sum).collect()
}

/// Sigmoid activation
fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

/// Check if output shape matches ImageNet classification (1000 or 1001 classes)
fn is_imagenet_classification(shape: &[usize]) -> bool {
    // Shape should be [1, N] or [N] where N is 1000 or 1001
    match shape {
        [1, n] | [n] if *n == 1000 || *n == 1001 => true,
        _ => false,
    }
}

/// Get top-K predictions from probabilities
fn get_top_k_predictions(probabilities: &[f32], k: usize) -> Vec<(usize, f32)> {
    let mut indexed: Vec<(usize, f32)> = probabilities.iter()
        .enumerate()
        .map(|(i, &p)| (i, p))
        .collect();
    
    // Sort by probability descending
    indexed.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    
    // Take top K
    indexed.into_iter().take(k).collect()
}

// ============================================================================
// COCO Labels and Object Detection Post-Processing
// ============================================================================

/// COCO 80 class labels for object detection
const COCO_LABELS: &[&str] = &[
    "person", "bicycle", "car", "motorcycle", "airplane", "bus", "train", "truck", "boat",
    "traffic light", "fire hydrant", "stop sign", "parking meter", "bench", "bird", "cat", "dog",
    "horse", "sheep", "cow", "elephant", "bear", "zebra", "giraffe", "backpack", "umbrella",
    "handbag", "tie", "suitcase", "frisbee", "skis", "snowboard", "sports ball", "kite",
    "baseball bat", "baseball glove", "skateboard", "surfboard", "tennis racket", "bottle",
    "wine glass", "cup", "fork", "knife", "spoon", "bowl", "banana", "apple", "sandwich", "orange",
    "broccoli", "carrot", "hot dog", "pizza", "donut", "cake", "chair", "couch", "potted plant",
    "bed", "dining table", "toilet", "tv", "laptop", "mouse", "remote", "keyboard", "cell phone",
    "microwave", "oven", "toaster", "sink", "refrigerator", "book", "clock", "vase", "scissors",
    "teddy bear", "hair drier", "toothbrush",
];

/// Check if output shape matches YOLOv8 detection format
fn is_yolo_detection(shape: &[usize]) -> bool {
    // YOLOv8 outputs: [1, 84, 8400] or [1, num_detections, 85]
    match shape {
        [1, 84, _] | [1, _, 85] | [1, _, 84] => true,
        _ => false,
    }
}

/// Bounding box with class and confidence
#[derive(Debug, Clone, serde::Serialize)]
struct Detection {
    class: String,
    class_id: usize,
    confidence: f32,
    bbox: [f32; 4], // [x1, y1, x2, y2] (pixel coordinates)
}

/// Non-Maximum Suppression (Class-Aware)
/// Processes each class separately to avoid cross-class suppression
fn nms(detections: &mut Vec<Detection>, iou_threshold: f32) -> Vec<Detection> {
    if detections.is_empty() {
        return vec![];
    }
    
    // Group detections by class
    let mut by_class: std::collections::HashMap<usize, Vec<Detection>> = std::collections::HashMap::new();
    for det in detections.drain(..) {
        by_class.entry(det.class_id).or_default().push(det);
    }
    
    let mut keep = Vec::new();
    
    // Apply NMS per class
    for (_, mut class_dets) in by_class {
        // Sort by confidence descending
        class_dets.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));
        
        let mut suppressed = vec![false; class_dets.len()];
        
        for i in 0..class_dets.len() {
            if suppressed[i] {
                continue;
            }
            keep.push(class_dets[i].clone());
            
            for j in (i + 1)..class_dets.len() {
                if suppressed[j] {
                    continue;
                }
                
                let iou = calculate_iou(&class_dets[i].bbox, &class_dets[j].bbox);
                if iou > iou_threshold {
                    suppressed[j] = true;
                }
            }
        }
    }
    
    // Sort final results by confidence
    keep.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));
    keep
}

/// Calculate Intersection over Union
fn calculate_iou(box1: &[f32; 4], box2: &[f32; 4]) -> f32 {
    // Boxes are [x1, y1, x2, y2]
    let x1 = box1[0].max(box2[0]);
    let y1 = box1[1].max(box2[1]);
    let x2 = box1[2].min(box2[2]);
    let y2 = box1[3].min(box2[3]);

    let intersection = (x2 - x1).max(0.0) * (y2 - y1).max(0.0);
    let area1 = (box1[2] - box1[0]).max(0.0) * (box1[3] - box1[1]).max(0.0);
    let area2 = (box2[2] - box2[0]).max(0.0) * (box2[3] - box2[1]).max(0.0);
    let union = area1 + area2 - intersection;

    if union > 0.0 {
        intersection / union
    } else {
        0.0
    }
}

/// Process YOLOv8/v11 output into detections
/// 
/// YOLOv11 Ultralytics ONNX export format:
/// - Shape: [1, 84, 8400] where 84 = 4 (bbox) + 80 (classes)
/// - Bbox format: (cx, cy, w, h) in PIXEL coordinates relative to input size
/// - Class scores: raw logits, need sigmoid
/// - No separate objectness score in YOLOv11 (unlike v5)
fn process_yolo_output(output: &[f32], shape: &[usize], conf_threshold: f32, input_w: usize, input_h: usize) -> Vec<Detection> {
    let mut detections = Vec::with_capacity(500);

    if shape.len() != 3 {
        return detections;
    }

    // YOLOv8/v11 channel-first format: [1, 84, 8400]
    // 84 = 4 (cx, cy, w, h) + 80 (class scores)
    if shape[0] == 1 && shape[1] >= 4 {
        let features = shape[1];
        let num_boxes = shape[2];
        
        // YOLOv11 has 84 features: 4 bbox + 80 classes (no objectness)
        // YOLOv5 has 85 features: 4 bbox + 1 obj + 80 classes
        let has_objectness = features == 85;
        let classes_offset = if has_objectness { 5 } else { 4 };
        let num_classes = features.saturating_sub(classes_offset).min(COCO_LABELS.len());

        for i in 0..num_boxes {
            // Channel-first indexing: output[feature_idx * num_boxes + box_idx]
            let get = |f: usize| -> f32 { output[f * num_boxes + i] };
            
            // For YOLOv5 with objectness: apply sigmoid and threshold early
            let objectness = if has_objectness {
                let obj_raw = get(4);
                if obj_raw < -1.1 { continue; } // Early skip: sigmoid(-1.1) ≈ 0.25
                sigmoid(obj_raw)
            } else {
                1.0 // YOLOv11 has no objectness, so treat as 1.0
            };

            // Find BEST class only (argmax over class scores)
            let mut best_class = 0;
            let mut best_class_logit = f32::NEG_INFINITY;
            
            for c in 0..num_classes {
                let class_logit = get(classes_offset + c);
                if class_logit > best_class_logit {
                    best_class_logit = class_logit;
                    best_class = c;
                }
            }
            
            // Apply sigmoid to get probability
            let best_class_prob = sigmoid(best_class_logit);
            let score = objectness * best_class_prob;
            
            // Early threshold - skip low confidence
            if score <= conf_threshold || score.is_nan() {
                continue;
            }
            
            // Get bbox coordinates
            // YOLOv11 Ultralytics export: (cx, cy, w, h) in PIXELS relative to input size
            let cx = get(0);
            let cy = get(1);
            let w = get(2);
            let h = get(3);
            
            // Convert center-format to corner-format (x1, y1, x2, y2)
            // Values are already in pixels, NO multiplication needed!
            let x1 = (cx - w / 2.0).max(0.0);
            let y1 = (cy - h / 2.0).max(0.0);
            let x2 = (cx + w / 2.0).min(input_w as f32);
            let y2 = (cy + h / 2.0).min(input_h as f32);
            
            // Sanity check: skip invalid boxes
            if x2 <= x1 || y2 <= y1 || w <= 0.0 || h <= 0.0 {
                continue;
            }

            let class_name = COCO_LABELS.get(best_class).unwrap_or(&"unknown").to_string();
            detections.push(Detection {
                class: class_name,
                class_id: best_class,
                confidence: score.clamp(0.0, 1.0),
                bbox: [x1, y1, x2, y2],
            });
        }
    }

    // Case B: channel-last [1, N, features] (some exported models)
    if shape[0] == 1 && shape[2] >= 4 && detections.is_empty() {
        let num_boxes = shape[1];
        let features = shape[2];
        let has_objectness = features == 85;
        let classes_offset = if has_objectness { 5 } else { 4 };
        let num_classes = features.saturating_sub(classes_offset).min(COCO_LABELS.len());

        for i in 0..num_boxes {
            let base = i * features;
            
            let objectness = if has_objectness {
                let obj_raw = output[base + 4];
                if obj_raw < -1.1 { continue; }
                sigmoid(obj_raw)
            } else {
                1.0
            };

            // Find BEST class only
            let mut best_class = 0;
            let mut best_class_logit = f32::NEG_INFINITY;
            
            for c in 0..num_classes {
                let class_logit = output[base + classes_offset + c];
                if class_logit > best_class_logit {
                    best_class_logit = class_logit;
                    best_class = c;
                }
            }
            
            let best_class_prob = sigmoid(best_class_logit);
            let score = objectness * best_class_prob;
            
            if score <= conf_threshold || score.is_nan() {
                continue;
            }

            let cx = output[base];
            let cy = output[base + 1];
            let w = output[base + 2];
            let h = output[base + 3];
            
            // Already in pixels
            let x1 = (cx - w / 2.0).max(0.0);
            let y1 = (cy - h / 2.0).max(0.0);
            let x2 = (cx + w / 2.0).min(input_w as f32);
            let y2 = (cy + h / 2.0).min(input_h as f32);
            
            if x2 <= x1 || y2 <= y1 || w <= 0.0 || h <= 0.0 {
                continue;
            }

            let class_name = COCO_LABELS.get(best_class).unwrap_or(&"unknown").to_string();
            detections.push(Detection {
                class: class_name,
                class_id: best_class,
                confidence: score.clamp(0.0, 1.0),
                bbox: [x1, y1, x2, y2],
            });
        }
    }

    detections
}

// ============================================================================
// Segmentation and OCR Post-Processing
// ============================================================================

/// Common segmentation class labels (simplified ADE20K subset)
const SEGMENTATION_LABELS: &[&str] = &[
    "background", "wall", "building", "sky", "floor", "tree", "ceiling", "road", "bed", "window",
    "grass", "cabinet", "sidewalk", "person", "earth", "door", "table", "mountain", "plant",
    "curtain", "chair", "car", "water", "painting", "sofa", "shelf", "house", "sea", "mirror",
    "rug", "field", "armchair", "seat", "fence", "desk", "rock", "wardrobe", "lamp", "bathtub",
];

/// Check if output shape matches segmentation format
fn is_segmentation(shape: &[usize]) -> bool {
    // Segmentation outputs: [1, num_classes, H, W] or [1, H, W, num_classes]
    // Accept models with larger class counts (e.g., 256, 512) and modest H/W
    match shape {
        // Channel-first: [1, classes, H, W]
        // Require at least 2 classes and a reasonable spatial size
        [1, c, h, w] if *c > 1 && *h >= 4 && *w >= 4 => true,
        // Channel-last: [1, H, W, classes]
        [1, h, w, c] if *c > 1 && *h >= 4 && *w >= 4 => true,
        _ => false,
    }
}

/// Check if output looks like OCR/text embeddings
fn is_ocr_output(shape: &[usize]) -> bool {
    // OCR outputs are typically [1, seq_len, vocab_size] or [batch, seq_len]
    // Exclude ImageNet classification shapes [1, 1000] or [1, 1001]
    match shape {
        [1, seq_len, vocab] if *seq_len > 1 && *vocab > 30 && *vocab != 1000 && *vocab != 1001 => true,
        [1, seq_len] if *seq_len > 1 && *seq_len != 1000 && *seq_len != 1001 => true,
        _ => false,
    }
}

/// Process segmentation output
fn process_segmentation_output(output: &[f32], shape: &[usize]) -> serde_json::Value {
    // Determine layout and dims
    let (num_classes, height, width, layout) = match shape {
        [1, c, h, w] => (*c, *h, *w, "channel_first"),
        [1, h, w, c] => (*c, *h, *w, "channel_last"),
        _ => return serde_json::json!({"error": "Invalid segmentation shape"}),
    };

    // For segmentation logits, compute argmax per-pixel across classes
    let hw = height * width;
    if output.len() < num_classes * hw {
        return serde_json::json!({"error": "Output length does not match expected segmentation size"});
    }

    let mut class_counts = vec![0usize; num_classes];

    if layout == "channel_first" {
        // output layout: [1, C, H, W] flattened as C major then H then W
        for y in 0..height {
            for x in 0..width {
                let mut best_class = 0usize;
                let mut best_score = f32::NEG_INFINITY;
                let pixel_offset = y * width + x;
                for c in 0..num_classes {
                    let idx = c * hw + pixel_offset;
                    let v = output[idx];
                    if v > best_score {
                        best_score = v;
                        best_class = c;
                    }
                }
                class_counts[best_class] += 1;
            }
        }
    } else {
        // channel_last: [1, H, W, C] flattened as (H*W)*C with classes fastest
        for y in 0..height {
            for x in 0..width {
                let mut best_class = 0usize;
                let mut best_score = f32::NEG_INFINITY;
                let base = (y * width + x) * num_classes;
                for c in 0..num_classes {
                    let idx = base + c;
                    let v = output[idx];
                    if v > best_score {
                        best_score = v;
                        best_class = c;
                    }
                }
                class_counts[best_class] += 1;
            }
        }
    }

    let classes_detected: Vec<String> = class_counts.iter().enumerate()
        .filter(|(_, &count)| count > 0)
        .map(|(id, _)| SEGMENTATION_LABELS.get(id).unwrap_or(&"unknown").to_string())
        .collect();

    // Provide counts for detected classes
    let mut counts_map = serde_json::Map::new();
    for (id, &count) in class_counts.iter().enumerate() {
        if count > 0 {
            let label = SEGMENTATION_LABELS.get(id).unwrap_or(&"unknown").to_string();
            counts_map.insert(label, serde_json::json!(count));
        }
    }

    serde_json::json!({
        "type": "segmentation",
        "num_classes": num_classes,
        "mask_shape": [height, width],
        "classes_detected": classes_detected,
        "class_counts": counts_map,
        "note": "Mask is low-resolution logits (per-pixel argmax). Upsample to input resolution for visualization"
    })
}

/// Process OCR output (simplified - assumes character-level decoding)
fn process_ocr_output(output: &[f32], shape: &[usize]) -> serde_json::Value {
    // Simplified OCR decoding - in production, use proper CTC/attention decoding
    let text = match shape {
        [1, seq_len, _vocab] => {
            // Take argmax over vocab dimension
            let mut chars = Vec::new();
            for i in 0..*seq_len {
                let start_idx = i * shape[2];
                let end_idx = start_idx + shape[2];
                if end_idx <= output.len() {
                    let slice = &output[start_idx..end_idx];
                    let max_idx = slice.iter()
                        .enumerate()
                        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
                        .map(|(idx, _)| idx)
                        .unwrap_or(0);
                    
                    // Simple ASCII mapping (very simplified)
                    if max_idx > 0 && max_idx < 128 {
                        chars.push(max_idx as u8 as char);
                    }
                }
            }
            chars.into_iter().collect::<String>().trim().to_string()
        }
        _ => "Unable to decode text".to_string(),
    };
    
    serde_json::json!({
        "type": "ocr",
        "text": text,
        "note": "Simplified decoding - production should use CTC/attention decoder"
    })
}

// ============================================================================
// PaddleOCR Detection and Recognition
// ============================================================================

/// Load PaddleOCR dictionary for CTC decoding
/// PaddleOCR dict.txt format: each line is one character
/// Index 0 maps to first character, blank token is at the LAST position (vocab_size - 1)
pub fn get_paddleocr_dictionary() -> Vec<String> {
    use std::sync::OnceLock;
    static DICT: OnceLock<Vec<String>> = OnceLock::new();
    
    DICT.get_or_init(|| {
        let dict_path = std::path::Path::new("./data/iroh/models/paddleocr_dict_en.txt");
        if dict_path.exists() {
            if let Ok(content) = std::fs::read_to_string(dict_path) {
                let chars: Vec<String> = content.lines()
                    .map(|s| s.to_string())
                    .collect();
                info!("📖 Loaded PaddleOCR dictionary with {} characters", chars.len());
                return chars;
            }
        }
        // Fallback: basic ASCII characters (95 printable chars)
        let chars: Vec<String> = (' '..='~').map(|c| c.to_string()).collect();
        warn!("⚠️ PaddleOCR dictionary not found, using ASCII fallback ({} chars)", chars.len());
        chars
    }).clone()
}

/// Check if output shape matches PaddleOCR detection format
/// Detection outputs a probability map: [1, 1, H, W]
fn is_paddleocr_detection(shape: &[usize]) -> bool {
    match shape {
        // Probability map output: [1, 1, H, W]
        [1, 1, h, w] if *h >= 8 && *w >= 8 => true,
        _ => false,
    }
}

/// Check if output shape matches PaddleOCR recognition format
/// Recognition outputs CTC logits: [1, seq_len, vocab_size] or [seq_len, vocab_size]
fn is_paddleocr_recognition(shape: &[usize]) -> bool {
    match shape {
        // PaddleOCR v5: [1, vocab_size, seq_len] where vocab_size is typically ~6000-7000 for English
        [1, vocab, seq_len] if *vocab > 50 && *vocab < 10000 && *seq_len > 10 => true,
        // Some models output: [vocab_size, seq_len]
        [vocab, seq_len] if *vocab > 50 && *vocab < 10000 && *seq_len > 10 => true,
        _ => false,
    }
}

/// Process PaddleOCR detection output to extract text bounding boxes
/// 
/// Input: probability map [1, 1, H, W] where each pixel indicates text probability
/// Output: list of bounding boxes [x1, y1, x2, y2] in normalized coordinates
/// 
/// Proper DBNet pipeline:
/// 1. Apply sigmoid to logits, then threshold (0.3-0.4)
/// 2. Apply morphological dilation (3x3) to connect fragmented text
/// 3. Find connected components (contours)
/// 4. Extract bounding boxes with expansion
fn process_paddleocr_detection(
    output: &[f32], 
    shape: &[usize], 
    threshold: f32,
    scaled_w: usize,  // Actual content width (before padding)
    scaled_h: usize,  // Actual content height (before padding)
) -> Vec<[f32; 4]> {
    let mut boxes = Vec::new();
    
    let (height, width) = match shape {
        [1, 1, h, w] => (*h, *w),
        _ => return boxes,
    };
    
    // NOTE: Some PaddleOCR DBNet ONNX exports (e.g. monkt/paddleocr-onnx v5)
    // already output PROBABILITY MAPS (values in [0,1]). Do NOT re-sigmoid
    // those outputs or probabilities will collapse toward 0.6 and be
    // thresholded away. We'll treat the raw `output` as probabilities.
    // For safety, log the observed min/max before thresholding.
    let (min_p, max_p) = output.iter().take(height * width).fold(
        (f32::INFINITY, f32::NEG_INFINITY),
        |(min, max), &v| (min.min(v), max.max(v)),
    );
    info!(min_p, max_p, "DBNet prob range");

    // Step 1: Create binary mask by thresholding probability map
    let mut binary = vec![false; height * width];
    for i in 0..output.len().min(height * width) {
        let prob = output[i]; // already a probability for v5 models
        binary[i] = prob >= threshold;
    }
    
    // CRITICAL FIX: Mask padding region BEFORE dilation
    // DBNet sees padded pixels as valid input, causing full-width boxes
    // We must zero out probability in padded region before contour extraction
    let valid_w = scaled_w as f32 / width as f32;
    let valid_h = scaled_h as f32 / height as f32;
    
    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            
            // Normalized pixel center
            let nx = (x as f32 + 0.5) / width as f32;
            let ny = (y as f32 + 0.5) / height as f32;
            
            // MASK padding region (beyond actual content)
            if nx > valid_w || ny > valid_h {
                binary[idx] = false;
            }
        }
    }
    
    // Step 2: Morphological dilation (3x3 kernel) - DISABLED for printed text
    // Dilation hurts printed text by merging across lines
    // Re-enable only for handwritten/broken fonts
    let dilated = binary.clone(); // DISABLED: was morphological dilation
    
    // OLD CODE (kept for reference):
    // let mut dilated = vec![false; height * width];
    // for y in 0..height {
    //     for x in 0..width {
    //         let mut any_set = false;
    //         for dy in -1i32..=1 {
    //             for dx in -1i32..=1 {
    //                 let ny = y as i32 + dy;
    //                 let nx = x as i32 + dx;
    //                 if ny >= 0 && ny < height as i32 && nx >= 0 && nx < width as i32 {
    //                     if binary[ny as usize * width + nx as usize] {
    //                         any_set = true;
    //                         break;
    //                     }
    //                 }
    //             }
    //             if any_set { break; }
    //         }
    //         dilated[y * width + x] = any_set;
    //     }
    // }
    
    // Step 3: Find connected components (contours) using flood-fill on dilated mask
    let mut visited = vec![false; height * width];
    
    for start_y in 0..height {
        for start_x in 0..width {
            let idx = start_y * width + start_x;
            if visited[idx] || !dilated[idx] {
                continue;
            }
            
            // Flood fill to find connected region
            let mut min_x = start_x;
            let mut max_x = start_x;
            let mut min_y = start_y;
            let mut max_y = start_y;
            let mut pixel_count = 0usize;
            let mut stack = vec![(start_x, start_y)];
            
            while let Some((x, y)) = stack.pop() {
                let idx = y * width + x;
                if visited[idx] || !dilated[idx] {
                    continue;
                }
                visited[idx] = true;
                pixel_count += 1;
                
                min_x = min_x.min(x);
                max_x = max_x.max(x);
                min_y = min_y.min(y);
                max_y = max_y.max(y);
                
                // 8-connectivity for better text grouping
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        if dx == 0 && dy == 0 { continue; }
                        let nx = x as i32 + dx;
                        let ny = y as i32 + dy;
                        if nx >= 0 && nx < width as i32 && ny >= 0 && ny < height as i32 {
                            stack.push((nx as usize, ny as usize));
                        }
                    }
                }
            }
            
            // Step 4: Filter and expand boxes
            let box_w = max_x - min_x + 1;
            let box_h = max_y - min_y + 1;
            
            // Filter: minimum size and minimum fill ratio (avoid noise)
            let fill_ratio = pixel_count as f32 / (box_w * box_h) as f32;
            let min_width = (width as f32 * 0.01) as usize; // At least 1% of image width
            let min_height = (height as f32 * 0.005) as usize; // At least 0.5% of image height
            
            // Text boxes should be wider than tall (aspect ratio > 1.2 for text lines)
            let aspect_ratio = box_w as f32 / box_h.max(1) as f32;
            
            // Relaxed filtering for printed book text:
            // Book characteristics: thin serif fonts, large margins, low stroke density
            // - Minimum pixel count of 30 (thin serif text can be small)
            // - Aspect ratio > 1.0 (text lines are horizontal)
            // - Reject boxes that span nearly full width (background noise)
            // - Lower fill ratio threshold for thin fonts
                // box filtering logic
                let is_near_full_width = box_w as f32 / width as f32 > 0.97;
                let is_valid_size = pixel_count >= 30;
                let is_valid_aspect = aspect_ratio > 1.0 || (box_w > 30 && box_h < 80);
                let is_valid_fill = fill_ratio > 0.02;
                
                let accepted = is_valid_size && is_valid_aspect && is_valid_fill && !is_near_full_width;

                if !accepted {
                    // Log rejection reason for debugging
                    debug!(
                        "⛔ Rejecting box at {},{}: {}x{} px={} aspect={:.2} fill={:.3} (size={}, asp={}, fill={}, !full={})",
                        min_x, min_y, box_w, box_h, pixel_count, aspect_ratio, fill_ratio,
                        is_valid_size, is_valid_aspect, is_valid_fill, !is_near_full_width
                    );
                } else {
                    debug!(
                        "✅ Accepting box at {},{}: {}x{} px={} aspect={:.2} fill={:.3}",
                        min_x, min_y, box_w, box_h, pixel_count, aspect_ratio, fill_ratio
                    );
                }

                if accepted {
                    
                    // Expand box by 2-3 pixels in each direction (DBNet recommended)
                    let expand_x = (width as f32 * 0.003).max(3.0) as usize;
                    let expand_y = (height as f32 * 0.002).max(2.0) as usize;
                    
                    let x1 = min_x.saturating_sub(expand_x);
                    let y1 = min_y.saturating_sub(expand_y);
                    let x2 = (max_x + expand_x + 1).min(width);
                    let y2 = (max_y + expand_y + 1).min(height);
                    
                    // Normalize coordinates
                    boxes.push([
                        x1 as f32 / width as f32,
                        y1 as f32 / height as f32,
                        x2 as f32 / width as f32,
                        y2 as f32 / height as f32,
                    ]);
                }
        }
    }
    
    // Step 5: Merge overlapping boxes (text lines that got split)
    boxes = merge_overlapping_boxes(boxes);
    
    // Step 6: Sort boxes by y position (top to bottom), then x (left to right)
    boxes.sort_by(|a, b| {
        // Group by lines - if y centers are within 2% of each other, treat as same line
        let a_y_center = (a[1] + a[3]) / 2.0;
        let b_y_center = (b[1] + b[3]) / 2.0;
        if (a_y_center - b_y_center).abs() < 0.02 {
            a[0].partial_cmp(&b[0]).unwrap_or(std::cmp::Ordering::Equal)
        } else {
            a[1].partial_cmp(&b[1]).unwrap_or(std::cmp::Ordering::Equal)
        }
    });
    
    boxes
}

/// Merge overlapping bounding boxes
fn merge_overlapping_boxes(boxes: Vec<[f32; 4]>) -> Vec<[f32; 4]> {
    if boxes.len() <= 1 {
        return boxes;
    }
    
    let mut merged = boxes.clone();
    let mut changed = true;
    
    while changed {
        changed = false;
        let mut new_merged = Vec::new();
        let mut used = vec![false; merged.len()];
        
        for i in 0..merged.len() {
            if used[i] { continue; }
            
            let mut current = merged[i];
            used[i] = true;
            
            for j in (i + 1)..merged.len() {
                if used[j] { continue; }
                
                let other = merged[j];
                
                // FIXED: Tighter thresholds for printed text (was too aggressive)
                // Old: gap_threshold=0.02, y_diff=0.03 would merge paragraphs
                // New: gap_threshold=0.005, y_diff=0.012 for printed books
                let gap_threshold = 0.005;
                let x_overlap = current[2] + gap_threshold >= other[0] && other[2] + gap_threshold >= current[0];
                let y_overlap = current[3] + gap_threshold >= other[1] && other[3] + gap_threshold >= current[1];
                
                // Check if on same line (tighter vertical threshold)
                let current_y_center = (current[1] + current[3]) / 2.0;
                let other_y_center = (other[1] + other[3]) / 2.0;
                let same_line = (current_y_center - other_y_center).abs() < 0.012;
                
                if x_overlap && y_overlap && same_line {
                    // Merge boxes
                    current[0] = current[0].min(other[0]);
                    current[1] = current[1].min(other[1]);
                    current[2] = current[2].max(other[2]);
                    current[3] = current[3].max(other[3]);
                    used[j] = true;
                    changed = true;
                }
            }
            
            new_merged.push(current);
        }
        
        merged = new_merged;
    }
    
    merged
}

/// Rotate an RGB image by specified angle (0, 90, 180, 270 degrees)
/// Returns rotated image buffer and new dimensions
fn rotate_image(rgb_data: &[u8], width: u32, height: u32, angle: u32) -> Option<(Vec<u8>, u32, u32)> {
    if angle == 0 {
        return Some((rgb_data.to_vec(), width, height));
    }
    
    let img = image::RgbImage::from_raw(width, height, rgb_data.to_vec())?;
    let dyn_img = image::DynamicImage::ImageRgb8(img);
    
    let rotated = match angle {
        90 => dyn_img.rotate90(),
        180 => dyn_img.rotate180(),
        270 => dyn_img.rotate270(),
        _ => return None,
    };
    
    let rotated_rgb = rotated.to_rgb8();
    let new_width = rotated_rgb.width();
    let new_height = rotated_rgb.height();
    
    Some((rotated_rgb.into_raw(), new_width, new_height))
}

/// Detect document orientation using PP-LCNet model (0°, 90°, 180°, 270°)
/// Returns the rotation angle needed to correct the document
fn detect_document_orientation(
    img: &image::DynamicImage,
    ori_session: &std::sync::Arc<std::sync::Mutex<ort::session::Session>>,
) -> Option<u32> {
    use image::imageops::FilterType;
    
    // Model expects 192x192 input
    let input_size = 192u32;
    let resized = img.resize_exact(input_size, input_size, FilterType::Triangle);
    let rgb = resized.to_rgb8();
    
    // Normalize to [0, 1] range (standard ImageNet preprocessing)
    let mut float_data = vec![0.0f32; 3 * input_size as usize * input_size as usize];
    for y in 0..input_size {
        for x in 0..input_size {
            let pixel = rgb.get_pixel(x, y);
            for c in 0..3 {
                let val = pixel[c] as f32 / 255.0;
                let idx = c * (input_size * input_size) as usize + (y * input_size + x) as usize;
                float_data[idx] = val;
            }
        }
    }
    
    // Create input tensor (NCHW format)
    let input_array = ndarray::Array4::<f32>::from_shape_vec(
        (1, 3, input_size as usize, input_size as usize),
        float_data,
    ).ok()?;
    
    // Run inference
    let output = {
        let mut session_lock = ori_session.lock().ok()?;
        let input_name = session_lock.inputs.first()
            .map(|i| i.name.clone())
            .unwrap_or_else(|| "input".to_string());
        let tensor_ref = ort::value::TensorRef::from_array_view(&input_array).ok()?;
        let outs = session_lock.run(ort::inputs![input_name.as_str() => tensor_ref]).ok()?;
        
        let (_, output_value) = outs.iter().next()?;
        let array = output_value.try_extract_array::<f32>().ok()?;
        array.iter().cloned().collect::<Vec<f32>>()
    };
    
    // Output shape is (1, 4) for 4 classes: [0°, 90°, 180°, 270°]
    if output.len() < 4 {
        return None;
    }
    
    // Find argmax
    let (predicted_class, _) = output.iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))?;
    
    // Map class to rotation angle needed to correct
    // If image is detected as 90° CW, we need to rotate 270° CW (or 90° CCW) to correct it
    let correction_angle = match predicted_class {
        0 => 0,   // Image is correct (0°)
        1 => 270, // Image is 90° CW, rotate 270° CW (or 90° CCW) to correct
        2 => 180, // Image is upside down, rotate 180° to correct
        3 => 90,  // Image is 270° CW (90° CCW), rotate 90° CW to correct
        _ => 0,
    };
    
    Some(correction_angle)
}

/// Detect text line orientation using PP-LCNet model (0° vs 180°)
/// Returns true if 180° rotation is needed
fn detect_textline_orientation(
    crop_rgb: &[u8],
    width: u32,
    height: u32,
    ori_session: &std::sync::Arc<std::sync::Mutex<ort::session::Session>>,
) -> Option<bool> {
    use image::imageops::FilterType;
    
    info!("DEBUG: detect_textline_orientation called with crop {}x{}, rgb_len={}", width, height, crop_rgb.len());
    
    // Model expects 80x160 input (height x width) - verified from ONNX model
    let input_h = 80u32;
    let input_w = 160u32;
    
    let img = match image::RgbImage::from_raw(width, height, crop_rgb.to_vec()) {
        Some(i) => {
            info!("DEBUG: Image created successfully");
            i
        }
        None => {
            warn!("DEBUG: Failed to create RgbImage from raw bytes");
            return None;
        }
    };
    let dyn_img = image::DynamicImage::ImageRgb8(img);
    let resized = dyn_img.resize_exact(input_w, input_h, FilterType::Triangle);
    let rgb = resized.to_rgb8();
    info!("DEBUG: Image resized to {}x{}", input_w, input_h);
    
    // Normalize to [0, 1] range
    let mut float_data = vec![0.0f32; 3 * input_h as usize * input_w as usize];
    for y in 0..input_h {
        for x in 0..input_w {
            let pixel = rgb.get_pixel(x, y);
            for c in 0..3 {
                let val = pixel[c] as f32 / 255.0;
                let idx = c * (input_h * input_w) as usize + (y * input_w + x) as usize;
                float_data[idx] = val;
            }
        }
    }
    
    // Create input tensor (NCHW format)
    let input_array = match ndarray::Array4::<f32>::from_shape_vec(
        (1, 3, input_h as usize, input_w as usize),
        float_data,
    ) {
        Ok(arr) => {
            info!("DEBUG: Input tensor created");
            arr
        }
        Err(e) => {
            warn!("DEBUG: Failed to create input tensor: {}", e);
            return None;
        }
    };
    
    // Run inference
    let output = {
        let mut session_lock = match ori_session.lock() {
            Ok(s) => s,
            Err(e) => {
                warn!("DEBUG: Failed to lock session: {}", e);
                return None;
            }
        };
        let input_name = session_lock.inputs.first()
            .map(|i| i.name.clone())
            .unwrap_or_else(|| "input".to_string());
        let tensor_ref = match ort::value::TensorRef::from_array_view(&input_array) {
            Ok(t) => t,
            Err(e) => {
                warn!("DEBUG: Failed to create tensor ref: {}", e);
                return None;
            }
        };
        let outs = match session_lock.run(ort::inputs![input_name.as_str() => tensor_ref]) {
            Ok(o) => {
                info!("DEBUG: ONNX inference succeeded");
                o
            }
            Err(e) => {
                warn!("DEBUG: ONNX inference failed: {}", e);
                return None;
            }
        };
        
        let (_, output_value) = match outs.iter().next() {
            Some(v) => v,
            None => {
                warn!("DEBUG: No output from ONNX");
                return None;
            }
        };
        let array = match output_value.try_extract_array::<f32>() {
            Ok(a) => a,
            Err(e) => {
                warn!("DEBUG: Failed to extract output array: {}", e);
                return None;
            }
        };
        array.iter().cloned().collect::<Vec<f32>>()
    };
    
    info!("DEBUG: Output extracted, len={}", output.len());
    
    // Output shape is (1, 2) for 2 classes: [0°, 180°]
    if output.len() < 2 {
        return None;
    }
    
    // Find argmax
    let (predicted_class, _) = output.iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))?;
    
    // Return true if 180° rotation is needed
    Some(predicted_class == 1)
}

/// CTC greedy decoding for PaddleOCR recognition output
/// 
/// CRITICAL: PaddleOCR v5 ONNX outputs [1, vocab_size, seq_len] (CHANNEL-MAJOR)
/// NOT [1, seq_len, vocab_size] (time-major)
/// 
/// This means we must iterate vocab dimension for each timestep, not slice contiguous memory.
/// Correct order: Argmax -> Collapse Repeats -> Skip Blank -> Map
fn ctc_decode(output: &[f32], shape: &[usize], dictionary: &[String]) -> (String, f32) {
    // FIXED: Correct axis interpretation for PaddleOCR v5
    // Shape is [1, vocab_size, seq_len] or [vocab_size, seq_len]
    let (vocab_size, seq_len) = match shape {
        [1, v, s] => (*v, *s),
        [v, s] => (*v, *s),
        _ => {
            warn!("Unexpected CTC output shape: {:?}", shape);
            return (String::new(), 0.0);
        }
    };
    
    info!("🔍 CTC decode: vocab_size={}, seq_len={}, output_len={}", vocab_size, seq_len, output.len());
    
    // In PaddleOCR, blank token is at the LAST index
    let blank_idx = vocab_size.saturating_sub(1);

    let mut result = String::new();
    let mut prev_idx: Option<usize> = None;
    let mut confidence_sum = 0.0f32;
    let mut count = 0;
    
    // Iterate over time steps
    for t in 0..seq_len {
        let mut max_idx = blank_idx;
        let mut max_logit = f32::NEG_INFINITY;
        
        // FIXED: Iterate vocab dimension for channel-major layout
        // Index formula: v * seq_len + t (NOT t * vocab_size + v)
        for v in 0..vocab_size {
            let idx = v * seq_len + t;
            if idx >= output.len() {
                warn!("CTC decode: index {} out of bounds (len={})", idx, output.len());
                break;
            }
            
            let logit = output[idx];
            if logit > max_logit {
                max_logit = logit;
                max_idx = v;
            }
        }
        
        // 1. Collapse repeats FIRST
        if Some(max_idx) == prev_idx {
            continue;
        }
        prev_idx = Some(max_idx);
        
        // 2. Skip blank
        if max_idx == blank_idx {
            continue;
        }
        
        // 3. Map index to dictionary
        if max_idx < dictionary.len() {
            result.push_str(&dictionary[max_idx]);
            
            // Softmax probability calculation
            let mut exp_sum = 0.0f32;
            for v in 0..vocab_size {
                let idx = v * seq_len + t;
                if idx < output.len() {
                    exp_sum += output[idx].exp();
                }
            }
            let prob = if exp_sum > 0.0 { max_logit.exp() / exp_sum } else { 0.0 };
            
            confidence_sum += prob;
            count += 1;
        }
    }
    
    let avg_conf = if count > 0 { confidence_sum / count as f32 } else { 0.0 };
    (result.trim().to_string(), avg_conf)
}

/// Text line with position and text content
#[derive(Debug, Clone, serde::Serialize)]
struct TextLine {
    text: String,
    confidence: f32,
    bbox: [f32; 4], // [x1, y1, x2, y2] normalized
}

/// Run recognition on a single cropped text line
/// 
/// Takes raw RGB pixel data of a text crop, resizes to recognition input size,
/// creates ONNX input tensor, runs inference, and decodes CTC output.
fn recognize_text_crop(
    crop_rgb: &[u8],
    crop_width: u32,
    crop_height: u32,
    recognition_session: &std::sync::Arc<std::sync::Mutex<ort::session::Session>>,
    textline_ori_session: &Option<std::sync::Arc<std::sync::Mutex<ort::session::Session>>>,
    job_id: &str,
    box_idx: usize,
) -> Option<(String, f32)> {
    use image::imageops::FilterType;
    
    // Recognition model expects height=48, width varies (we use 320 with padding)
    let rec_height = 48u32;
    let rec_width = 320u32;
    
    // Create image from raw bytes
    let crop_img = image::RgbImage::from_raw(crop_width, crop_height, crop_rgb.to_vec())?;
    let mut crop_dyn = image::DynamicImage::ImageRgb8(crop_img);
    
    // DEBUG: Save raw crop to disk to verify coordinates
    if box_idx == 0 {
        let _ = crop_dyn.save(format!("debug_crops/job_{}_box_{}_raw.png", job_id, box_idx));
    }
    
    // --- CRITICAL: Textline orientation detection (0° vs 180°) ---
    // This fixes garbage output for rotated text by detecting and correcting 180° rotation
    // Skip for very small crops where orientation is meaningless
    if let Some(ori_session) = textline_ori_session {
        if crop_width >= 32 && crop_height >= 16 {
            info!("DEBUG: Calling detect_textline_orientation for box {}", box_idx);
            if let Some(needs_rotation) = detect_textline_orientation(crop_rgb, crop_width, crop_height, ori_session) {
                if needs_rotation {
                    info!("🔄 Box {}: Detected 180° rotation, correcting", box_idx);
                    crop_dyn = crop_dyn.rotate180();
                } else {
                    info!("✓ Box {}: Orientation correct (0°)", box_idx);
                }
            } else {
                warn!("Box {}: Orientation detection returned None", box_idx);
            }
        } else {
            // Skip orientation for very small crops
            info!("⏭️  Box {}: Skipping orientation (crop too small: {}x{})", box_idx, crop_width, crop_height);
        }
    } else {
        warn!("Box {}: No textline orientation session available", box_idx);
    }
    
    // Resize preserving aspect ratio
    let scale = rec_height as f32 / crop_height as f32;
    let new_width = ((crop_width as f32 * scale).round() as u32).min(rec_width);
    let resized = crop_dyn.resize(new_width, rec_height, FilterType::Triangle);
    
    // Create padded canvas (black padding for standard tensor)
    // CRITICAL FIX #3: PaddleOCR expects GRAY padding (128), not black (0)
    // Black padding shifts activation statistics and causes CTC collapse
    let mut canvas = image::RgbImage::from_pixel(rec_width, rec_height, image::Rgb([128, 128, 128]));
    let offset_x = 0i64; // Left-align for text
    image::imageops::overlay(&mut canvas, &resized.to_rgb8(), offset_x, 0);

    // DEBUG: Save processed input to disk
    if box_idx == 0 {
        let _ = canvas.save(format!("debug_crops/job_{}_box_{}_input.png", job_id, box_idx));
    }
    
    // Convert to RGB-normalized tensor [-1, 1]
    // IMPORTANT: PaddleOCR v5 expects RGB normalization, NOT grayscale!
    // Do NOT broadcast grayscale to 3 channels.
    let mut float_data = vec![0.0f32; 3 * rec_height as usize * rec_width as usize];
    for y in 0..rec_height as usize {
        for x in 0..rec_width as usize {
            let pixel = canvas.get_pixel(x as u32, y as u32);
            // Per-channel RGB normalization: (pixel/255 - 0.5) / 0.5
            for c in 0..3 {
                let val = pixel[c] as f32 / 255.0;
                let normalized = (val - 0.5) / 0.5;
                let idx = c * (rec_height as usize * rec_width as usize) + y * rec_width as usize + x;
                float_data[idx] = normalized;
            }
        }
    }
    
    // Create input tensor (NCHW format)
    let input_array = ndarray::Array4::<f32>::from_shape_vec(
        (1, 3, rec_height as usize, rec_width as usize),
        float_data,
    ).ok()?;
    
    // Run recognition inference
    let (shape, values) = {
        let mut session_lock = recognition_session.lock().ok()?;
        let input_name = session_lock.inputs.first()
            .map(|i| i.name.clone())
            .unwrap_or_else(|| "input".to_string());
        let tensor_ref = ort::value::TensorRef::from_array_view(&input_array).ok()?;
        let outs = session_lock.run(ort::inputs![input_name.as_str() => tensor_ref]).ok()?;
        
        let (_, output_value) = outs.iter().next()?;
        let array = output_value.try_extract_array::<f32>().ok()?;
        (array.shape().to_vec(), array.iter().cloned().collect::<Vec<f32>>())
    };
    
    // CTC decode
    let dictionary = get_paddleocr_dictionary();

    // Vocab check for debugging: log seq/vocab/dict sizes
    // PaddleOCR v5 outputs [1, vocab_size, seq_len], NOT [1, seq_len, vocab_size]
    let (vocab_size, seq_len) = match shape.as_slice() {
        [1, v, s] => (*v, *s),
        [v, s] => (*v, *s),
        _ => return Some((String::new(), 0.0)),
    };
    info!(vocab_size, seq_len, dict_len = dictionary.len(), "OCR vocab check (recognize_text_crop)");

    let (text, avg_conf) = ctc_decode(&values, &shape, &dictionary);

    Some((text, avg_conf))
}

// ============================================================================
// Audio Post-Processing (VAD and Denoising)
// ============================================================================

/// Check if output is Voice Activity Detection (VAD)
fn is_vad_output(shape: &[usize]) -> bool {
    // VAD outputs: [1, T] where T is time steps
    // Exclude ImageNet classification shapes [1, 1000] or [1, 1001]
    // Exclude YOLO shapes [1, 84, N]
    match shape {
        [1, t] if *t > 10 && *t != 1000 && *t != 1001 && *t < 10000 => true,
        [t] if *t > 10 && *t != 1000 && *t != 1001 && *t < 10000 => true,
        _ => false,
    }
}

/// Check if output is audio waveform (denoising)
fn is_audio_output(shape: &[usize]) -> bool {
    // Audio outputs: [1, samples] or [samples] where samples is large (> 10000)
    // Must be significantly larger than ImageNet (1000/1001) to avoid false positives
    match shape {
        [1, s] if *s > 10000 => true,
        [s] if *s > 10000 => true,
        _ => false,
    }
}

/// Process VAD output
fn process_vad_output(output: &[f32], shape: &[usize]) -> serde_json::Value {
    let num_frames = match shape {
        [1, t] => *t,
        [t] => *t,
        _ => return serde_json::json!({"error": "Invalid VAD shape"}),
    };
    
    // Calculate average speech probability
    let avg_prob: f32 = output.iter().sum::<f32>() / num_frames as f32;
    let speech_detected = avg_prob > 0.5;
    
    // Find speech segments (simplified - threshold at 0.5)
    let mut segments = Vec::new();
    let mut in_speech = false;
    let mut start_frame = 0;
    
    for (i, &prob) in output.iter().enumerate() {
        if prob > 0.5 && !in_speech {
            start_frame = i;
            in_speech = true;
        } else if prob <= 0.5 && in_speech {
            // Assume 10ms per frame (typical for VAD)
            let start_time = start_frame as f32 * 0.01;
            let end_time = i as f32 * 0.01;
            segments.push([start_time, end_time]);
            in_speech = false;
        }
    }
    
    // Close final segment if still in speech
    if in_speech {
        let start_time = start_frame as f32 * 0.01;
        let end_time = num_frames as f32 * 0.01;
        segments.push([start_time, end_time]);
    }
    
    serde_json::json!({
        "type": "vad",
        "speech_probability": avg_prob,
        "speech_detected": speech_detected,
        "num_segments": segments.len(),
        "segments": segments,
        "duration_seconds": num_frames as f32 * 0.01
    })
}

/// Process audio denoising output
fn process_audio_output(output: &[f32], shape: &[usize]) -> serde_json::Value {
    let num_samples = match shape {
        [1, s] => *s,
        [s] => *s,
        _ => return serde_json::json!({"error": "Invalid audio shape"}),
    };
    
    // Assume 16kHz sample rate (common for speech)
    let sample_rate = 16000;
    let duration_seconds = num_samples as f32 / sample_rate as f32;
    
    // Calculate RMS energy
    let rms = (output.iter().map(|&x| x * x).sum::<f32>() / num_samples as f32).sqrt();
    
    serde_json::json!({
        "type": "audio_denoising",
        "num_samples": num_samples,
        "sample_rate": sample_rate,
        "duration_seconds": duration_seconds,
        "rms_energy": rms,
        "note": "Denoised audio available in raw output"
    })
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

/// File metadata for inference outputs that are files
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadata {
    /// Original filename or generated name
    pub filename: String,
    /// MIME type of the file
    pub mime_type: String,
    /// Size in bytes
    pub size_bytes: u64,
}

/// Metadata for stored inference results
///
/// Stored in blob storage with key `result:{job_id}` for persistent retrieval
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceResultMetadata {
    /// Job ID
    pub job_id: String,
    /// Type of output: "json" or "file"
    pub output_type: String,
    /// Blob hash of the output data
    pub output_blob_hash: String,
    /// File metadata if output_type is "file"
    pub file_metadata: Option<FileMetadata>,
    /// Unix timestamp when completed
    pub completed_at: i64,
    /// Whether execution was successful
    pub success: bool,
    /// Error message if failed
    pub error: Option<String>,
    /// Execution latency in milliseconds
    pub latency_ms: u64,
    /// Node that executed the job
    pub node_id: String,
}

impl InferenceResultMetadata {
    /// Create metadata from InferenceResult
    pub fn from_result(result: &InferenceResult, output_type: String, output_blob_hash: String) -> Self {
        Self {
            job_id: result.job_id.clone(),
            output_type,
            output_blob_hash,
            file_metadata: None,
            completed_at: result.completed_at,
            success: result.success,
            error: result.error.clone(),
            latency_ms: result.latency_ms,
            node_id: result.node_id.clone(),
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
            whitelist.insert("mobilenet_v4".to_string());
            whitelist.insert("paddleocr_det".to_string());      // PaddleOCR detection
            whitelist.insert("paddleocr_rec_en".to_string());   // PaddleOCR English recognition
            whitelist.insert("paddleocr_en".to_string());       // Combined OCR pipeline
            whitelist.insert("yolo11n".to_string());
            whitelist.insert("segformer".to_string());
            whitelist.insert("silero_vad".to_string());
            whitelist.insert("dtln_denoise".to_string());
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

        /// Get a job by ID (checking running then pending)
        pub fn get_job(&self, job_id: &str) -> Option<InferenceJob> {
            if let Some(entry) = self.running_jobs.get(job_id) {
                return Some(entry.value().0.clone());
            }
            if let Some(entry) = self.pending_jobs.get(job_id) {
                return Some(entry.value().clone());
            }
            None
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
        /// Per-model ONNX Runtime session cache (model_name -> Session)
        session_cache: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, std::sync::Arc<std::sync::Mutex<ort::session::Session>>>>>,
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
                session_cache: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
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

            // Route combined OCR models to the full pipeline (skip model file check for virtual models)
            // Also route standalone recognition models to combined pipeline to ensure textline orientation is applied
            if job.model_name == "paddleocr_en" || job.model_name.starts_with("paddleocr_rec") {
                let execution_result = self.execute_combined_ocr(&job.input_uri, job).await;
                let latency_ms = start.elapsed().as_millis() as u64;
                return match execution_result {
                    Ok(output_uri) => {
                        info!(job_id = %job.job_id, latency_ms, "OCR pipeline completed successfully");
                        crate::metrics::INFERENCE_LATENCY_MS.observe(latency_ms as f64);
                        Ok(InferenceResult::success(job.job_id.clone(), self.node_id, output_uri, latency_ms))
                    }
                    Err(e) => {
                        error!(job_id = %job.job_id, error = %e, "OCR pipeline failed");
                        Ok(InferenceResult::failure(job.job_id.clone(), self.node_id, e.to_string(), latency_ms))
                    }
                };
            }

            // Check model exists (for regular models)
            let model_path = self.model_dir.join(format!("{}.onnx", job.model_name));
            if !model_path.exists() {
                return Err(InferenceError::ModelNotFound(job.model_name.clone()));
            }

            // Execute regular single-model inference
            let execution_result = self.execute_tflite_inference(&model_path, &job.input_uri, job).await;

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

        /// Execute full OCR pipeline using ocr-rs (MNN-based PaddleOCR v5)
        /// 
        /// Uses ocr-rs which provides:
        /// 1. Det - DBNet text detection (MNN)
        /// 2. Rec - CRNN text recognition with CTC decoding (MNN)
        async fn execute_combined_ocr(
            &self,
            input_uri: &str,
            job: &InferenceJob,
        ) -> Result<String> {
            use ocr_rs::OcrEngine;
            
            info!(job_id = %job.job_id, "🔤 Starting OCR pipeline using ocr-rs (MNN)");
            
            let models_dir = std::path::PathBuf::from("./data/iroh/models");
            let det_path = models_dir.join("paddleocr_det.mnn");
            let rec_path = models_dir.join("paddleocr_rec_en.mnn");
            let dict_path = models_dir.join("paddleocr_keys_en.txt");
            
            // Check models exist
            if !det_path.exists() {
                return Err(InferenceError::ModelNotFound(format!("paddleocr_det.mnn not found at {:?}", det_path)));
            }
            if !rec_path.exists() {
                return Err(InferenceError::ModelNotFound(format!("paddleocr_rec_en.mnn not found at {:?}", rec_path)));
            }
            if !dict_path.exists() {
                return Err(InferenceError::ModelNotFound(format!("paddleocr_keys_en.txt not found at {:?}", dict_path)));
            }
            
            // Load input image
            let orig_bytes = if input_uri.starts_with("http://") || input_uri.starts_with("https://") {
                let response = reqwest::get(input_uri).await
                    .map_err(|e| InferenceError::InputLoadFailed(format!("HTTP request failed: {}", e)))?;
                response.bytes().await
                    .map_err(|e| InferenceError::InputLoadFailed(format!("Failed to read response: {}", e)))?
                    .to_vec()
            } else if input_uri.starts_with("file://") {
                let path = input_uri.strip_prefix("file://").unwrap();
                tokio::fs::read(path).await
                    .map_err(|e| InferenceError::InputLoadFailed(format!("Failed to read file: {}", e)))?
            } else {
                // Assume base64
                base64::Engine::decode(&base64::engine::general_purpose::STANDARD, input_uri)
                    .map_err(|e| InferenceError::InputLoadFailed(format!("Base64 decode failed: {}", e)))?
            };
            
            let img = image::load_from_memory(&orig_bytes)
                .map_err(|e| InferenceError::InputLoadFailed(format!("Image decode failed: {}", e)))?;
            
            let img_width = img.width();
            let img_height = img.height();
            info!(job_id = %job.job_id, width = img_width, height = img_height, "📷 Image loaded");
            
            // Run OCR in blocking thread (MNN inference is CPU-bound)
            let det_path_str = det_path.to_string_lossy().to_string();
            let rec_path_str = rec_path.to_string_lossy().to_string();
            let dict_path_str = dict_path.to_string_lossy().to_string();
            let job_id = job.job_id.clone();
            
            let ocr_result = tokio::task::spawn_blocking(move || -> std::result::Result<(Vec<(String, f32, [f32; 4])>, u32, u32), InferenceError> {
                // Create high-level OCR engine (handles detection + recognition in one pass)
                let engine = OcrEngine::new(
                    &det_path_str,
                    &rec_path_str,
                    &dict_path_str,
                    None, // Use default config
                ).map_err(|e| InferenceError::ExecutionFailed(format!("Failed to create OCR engine: {:?}", e)))?;
                
                tracing::info!(job_id = %job_id, "✅ OCR engine initialized (MNN)");
                
                // Run end-to-end OCR (detection + recognition)
                let ocr_results = engine.recognize(&img)
                    .map_err(|e| InferenceError::ExecutionFailed(format!("OCR recognition failed: {:?}", e)))?;
                
                tracing::info!(job_id = %job_id, num_results = ocr_results.len(), "📝 OCR completed");
                
                // Convert results to our tuple format
                let results: Vec<(String, f32, [f32; 4])> = ocr_results.iter().map(|r| {
                    let rect = &r.bbox.rect;
                    let bbox = [
                        rect.left() as f32 / img.width() as f32,
                        rect.top() as f32 / img.height() as f32,
                        rect.right() as f32 / img.width() as f32,
                        rect.bottom() as f32 / img.height() as f32,
                    ];
                    (r.text.clone(), r.confidence, bbox)
                }).collect();
                
                Ok((results, img.width(), img.height()))
            }).await.map_err(|e| InferenceError::ExecutionFailed(format!("OCR thread join error: {}", e)))??;
            
            let (text_results, width, height) = ocr_result;
            
            // Convert results to our format
            let text_lines: Vec<serde_json::Value> = text_results.iter().map(|(text, confidence, bbox)| {
                serde_json::json!({
                    "text": text,
                    "confidence": confidence,
                    "bbox": bbox
                })
            }).collect();
            
            let full_text: String = text_results.iter()
                .map(|(text, _, _)| text.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            
            let avg_conf: f32 = if text_results.is_empty() {
                0.0
            } else {
                text_results.iter().map(|(_, conf, _)| conf).sum::<f32>() / text_results.len() as f32
            };
            
            info!(
                job_id = %job.job_id,
                num_lines = text_results.len(),
                total_chars = full_text.len(),
                avg_confidence = avg_conf,
                image_size = format!("{}x{}", width, height),
                "✅ OCR pipeline complete (ocr-rs/MNN)"
            );
            
            let output = serde_json::json!({
                "type": "ocr",
                "text": full_text,
                "lines": text_lines,
                "confidence": avg_conf,
                "num_lines": text_results.len(),
                "model": "ocr-rs (PP-OCRv5 MNN)"
            });
            
            let output_json = serde_json::to_string(&output)
                .map_err(|e| InferenceError::ExecutionFailed(format!("JSON serialization failed: {}", e)))?;
            
            let temp_result = InferenceResult::success(job.job_id.clone(), self.node_id, String::new(), 0);
            self.store_blob_with_metadata(&job.job_id, output_json, &temp_result).await
        }


        /// Execute ONNX inference using ONNX Runtime (ort)
        ///
        /// Loads the model, processes input, runs inference, and returns output URI.
        async fn execute_tflite_inference(
            &self,
            model_path: &PathBuf,
            input_uri: &str,
            job: &InferenceJob,
        ) -> Result<String> {
            use ort::session::Session;
            use ndarray::Array4;
            
            info!(
                model_path = %model_path.display(),
                input_uri = %input_uri,
                job_id = %job.job_id,
                "🚀 Starting ONNX inference execution"
            );

            
            // Determine input shape based on model requirements
            let (batch, channels, height, width) = if job.model_name.starts_with("mobilenet") {
                // MobileNetV4 expects 256x256
                (1usize, 3usize, 256usize, 256usize)
            } else if job.model_name.starts_with("yolo") {
                // YOLOv8/v11 expects 640x640
                (1usize, 3usize, 640usize, 640usize)
            } else if job.model_name.starts_with("segformer") {
                // SegFormer model in this repo expects 512x512 (HxW)
                (1usize, 3usize, 512usize, 512usize)
            } else if job.model_name == "paddleocr_det" || job.model_name.starts_with("paddleocr_det") {
                // PaddleOCR detection: 1280x1280 for better printed text quality
                (1usize, 3usize, 1280usize, 1280usize)
            } else if job.model_name.starts_with("paddleocr_rec") || job.model_name == "paddleocr_en" {
                // PaddleOCR v5 recognition: fixed height 48, width 320 (typical)
                // PP-OCRv5 uses height 48, v3/v4 used 32
                (1usize, 3usize, 48usize, 320usize)
            } else {
                // Default: 224x224 for most classification models
                (1usize, 3usize, 224usize, 224usize)
            };

            // Load input data from URI and decode/resize to required dimensions
            let input_data = self.load_input_data(input_uri, width, height).await?;
            
            // Prepare input tensor with proper normalization
            let mut float_data = vec![0.0f32; batch * channels * height * width];
            
            if job.model_name.starts_with("mobilenet") {
                // MobileNet normalization: pixel = (pixel / 127.5) - 1.0 → range [-1, 1]
                // NOT ImageNet mean/std normalization!
                for h in 0..height {
                    for w in 0..width {
                        let pixel_idx = (h * width + w) * 3;
                        for c in 0..channels {
                            let input_val = if pixel_idx + c < input_data.len() {
                                input_data[pixel_idx + c]
                            } else {
                                0
                            };
                            // Correct MobileNet normalization: [-1, 1] range
                            let normalized = (input_val as f32 / 127.5) - 1.0;
                            // NCHW format: [batch, channel, height, width]
                            let output_idx = c * height * width + h * width + w;
                            float_data[output_idx] = normalized;
                        }
                    }
                }
            } else if job.model_name.starts_with("yolo") {
                // YOLO expects BGR order (Ultralytics default), NOT RGB!
                // Input data is RGB from image decoder, need to swap R and B channels
                for h in 0..height {
                    for w in 0..width {
                        let pixel_idx = (h * width + w) * 3;
                        
                        // Read RGB from input
                        let r = if pixel_idx < input_data.len() { input_data[pixel_idx] } else { 0 };
                        let g = if pixel_idx + 1 < input_data.len() { input_data[pixel_idx + 1] } else { 0 };
                        let b = if pixel_idx + 2 < input_data.len() { input_data[pixel_idx + 2] } else { 0 };
                        
                        // Normalize to [0, 1]
                        let r_norm = r as f32 / 255.0;
                        let g_norm = g as f32 / 255.0;
                        let b_norm = b as f32 / 255.0;
                        
                        // Write as BGR in NCHW format
                        // Channel 0 = B, Channel 1 = G, Channel 2 = R
                        float_data[0 * height * width + h * width + w] = b_norm;
                        float_data[1 * height * width + h * width + w] = g_norm;
                        float_data[2 * height * width + h * width + w] = r_norm;
                    }
                }
            } else if job.model_name.starts_with("paddleocr_rec") || job.model_name.starts_with("paddleocr_det") || job.model_name == "paddleocr_en" {
                // PaddleOCR v5 recognition: use RGB per-channel normalization
                // DO NOT use grayscale - PaddleOCR v5 expects true RGB
                // Normalization: (pixel/255 - 0.5) / 0.5 for [-1, 1] range
                for h in 0..height {
                    for w in 0..width {
                        let pixel_idx = (h * width + w) * 3;
                        
                        let r = if pixel_idx < input_data.len() { input_data[pixel_idx] } else { 128 };
                        let g = if pixel_idx + 1 < input_data.len() { input_data[pixel_idx + 1] } else { 128 };
                        let b = if pixel_idx + 2 < input_data.len() { input_data[pixel_idx + 2] } else { 128 };
                        
                        // Per-channel RGB normalization: (pixel/255 - 0.5) / 0.5
                        let r_norm = (r as f32 / 255.0 - 0.5) / 0.5;
                        let g_norm = (g as f32 / 255.0 - 0.5) / 0.5;
                        let b_norm = (b as f32 / 255.0 - 0.5) / 0.5;
                        
                        // RGB in NCHW format (Channel 0=R, 1=G, 2=B)
                        float_data[0 * height * width + h * width + w] = r_norm;
                        float_data[1 * height * width + h * width + w] = g_norm;
                        float_data[2 * height * width + h * width + w] = b_norm;
                    }
                }
            } else {
                // Other models: simple [0, 1] normalization in RGB NCHW format
                for h in 0..height {
                    for w in 0..width {
                        let pixel_idx = (h * width + w) * 3;
                        for c in 0..channels {
                            let input_idx = if pixel_idx + c < input_data.len() {
                                input_data[pixel_idx + c]
                            } else {
                                0
                            };
                            let normalized = input_idx as f32 / 255.0;
                            let output_idx = c * height * width + h * width + w;
                            float_data[output_idx] = normalized;
                        }
                    }
                }
            }
            
            info!(
                job_id = %job.job_id,
                model = %job.model_name,
                input_size = format!("{}x{}", width, height),
                "📦 Input preprocessed, starting model inference"
            );

            // Run ONNX Runtime in a blocking thread to avoid blocking the async runtime
            let model_path_buf = model_path.clone();
            let job_model_name = job.model_name.clone();
            let float_data = float_data; // move into closure
            // Clone cache for blocking thread
            let cache = self.session_cache.clone();
            let run_result = tokio::task::spawn_blocking(move || -> std::result::Result<String, InferenceError> {
                // Try to get cached session for this model
                let mut init_time_ms = 0u128;
                let mut run_time_ms = 0u128;
                let session_arc = {
                    let mut cache_lock = cache.lock().unwrap();
                    if let Some(sess) = cache_lock.get(&job_model_name) {
                        info!(model = %job_model_name, "✅ Using cached ONNX session");
                        sess.clone()
                    } else {
                        // Initialize session once and cache it
                        info!(model = %job_model_name, "🔄 Loading ONNX model (first run, may take 2-5s)...");
                        let t0 = std::time::Instant::now();
                        // Enable graph optimizations and threading for 5-10x speedup
                        let num_threads = std::thread::available_parallelism()
                            .map(|p| p.get())
                            .unwrap_or(4);
                        let sess = Session::builder()
                            .map_err(|e| InferenceError::ExecutionFailed(format!("Failed to create session builder: {}", e)))?
                            .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)
                            .map_err(|e| InferenceError::ExecutionFailed(format!("Failed to set optimization level: {}", e)))?
                            .with_intra_threads(num_threads)
                            .map_err(|e| InferenceError::ExecutionFailed(format!("Failed to set intra threads: {}", e)))?
                            .with_inter_threads(1)
                            .map_err(|e| InferenceError::ExecutionFailed(format!("Failed to set inter threads: {}", e)))?
                            .commit_from_file(&model_path_buf)
                            .map_err(|e| InferenceError::ModelNotFound(format!("Failed to load ONNX model: {}", e)))?;
                        init_time_ms = t0.elapsed().as_millis();
                        info!(model = %job_model_name, init_time_ms = init_time_ms, "✅ ONNX session initialized");
                        let sess_arc = std::sync::Arc::new(std::sync::Mutex::new(sess));
                        cache_lock.insert(job_model_name.clone(), sess_arc.clone());
                        sess_arc
                    }
                };

                // Build input tensor
                let input_array = Array4::<f32>::from_shape_vec((batch, channels, height, width), float_data)
                    .map_err(|e| InferenceError::InputLoadFailed(format!("Failed to create tensor: {}", e)))?;

                // Lock session and run inference, extracting output data while lock held
                let t_run_start = std::time::Instant::now();
                let (shape, values) = {
                    let mut session_lock = session_arc.lock().unwrap();
                    let input_name = session_lock.inputs.first()
                        .map(|i| i.name.clone())
                        .unwrap_or_else(|| "input".to_string());
                    let tensor_ref = ort::value::TensorRef::from_array_view(&input_array)
                        .map_err(|e| InferenceError::ExecutionFailed(format!("Failed to create tensor ref: {}", e)))?;
                    let outs = session_lock.run(ort::inputs![input_name.as_str() => tensor_ref])
                        .map_err(|e| InferenceError::ExecutionFailed(format!("Inference failed: {}", e)))?;

                    // Extract first output tensor as owned Vec<f32> while session_lock is held
                    let (_, output_value) = outs.iter().next()
                        .ok_or_else(|| InferenceError::ExecutionFailed("No output tensors".to_string()))?;
                    let array = output_value.try_extract_array::<f32>()
                        .map_err(|e| InferenceError::ExecutionFailed(format!("Failed to extract array: {}", e)))?;

                    let shape: Vec<usize> = array.shape().to_vec();
                    let values: Vec<f32> = array.iter().cloned().collect();
                    (shape, values)
                };
                run_time_ms = t_run_start.elapsed().as_millis();

                // Log timings at info level for visibility
                info!(model = %job_model_name, session_init_ms = init_time_ms, run_ms = run_time_ms, "🧠 ONNX inference completed");

                // Check if this is YOLO object detection output
                if is_yolo_detection(&shape) {
                    info!(model = %job_model_name, shape = ?shape, "📊 Processing YOLO detections...");
                    let t_post = std::time::Instant::now();
                    
                    // Use threshold 0.6 for clean inference results
                    // sigmoid(0) = 0.5, so 0.6 filters out noise
                    let mut detections = process_yolo_output(&values, &shape, 0.6, if job_model_name.starts_with("yolo") {640} else {256}, if job_model_name.starts_with("yolo") {640} else {256});
                    
                    // Debug: log score distribution
                    if !detections.is_empty() {
                        let max_conf = detections.iter().map(|d| d.confidence).fold(0.0f32, f32::max);
                        let min_conf = detections.iter().map(|d| d.confidence).fold(1.0f32, f32::min);
                        info!(model = %job_model_name, count = detections.len(), min_conf = min_conf, max_conf = max_conf, "📈 Detection score distribution");
                    }
                    
                    let pre_nms_count = detections.len();
                    // Cap to 50 before NMS (standard YOLO practice)
                    if detections.len() > 50 {
                        detections.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));
                        detections.truncate(50);
                    }
                    let detections = nms(&mut detections, 0.45);
                    let post_time_ms = t_post.elapsed().as_millis();
                    info!(model = %job_model_name, pre_nms = pre_nms_count, post_nms = detections.len(), post_processing_ms = post_time_ms, "✅ YOLO post-processing complete");
                    let mut result = serde_json::Map::new();
                    result.insert("type".to_string(), serde_json::json!("object_detection"));
                    result.insert("num_detections".to_string(), serde_json::json!(detections.len()));
                    result.insert("detections".to_string(), serde_json::json!(detections));
                    return serde_json::to_string(&result)
                        .map_err(|e| InferenceError::ExecutionFailed(format!("JSON serialization failed: {}", e)));
                }

                // Check if this is segmentation output (SegFormer, etc.)
                if is_segmentation(&shape) {
                    info!(model = %job_model_name, shape = ?shape, "🎨 Processing segmentation output...");
                    let seg_result = process_segmentation_output(&values, &shape);
                    return serde_json::to_string(&seg_result)
                        .map_err(|e| InferenceError::ExecutionFailed(format!("JSON serialization failed: {}", e)));
                }

                // Check if this is PaddleOCR detection output
                if is_paddleocr_detection(&shape) || job_model_name.starts_with("paddleocr_det") {
                    info!(model = %job_model_name, shape = ?shape, "📝 Processing PaddleOCR detection output...");
                    // For standalone detection, use the input dimensions as content size
                    // (no padding in this path since we don't control the input)
                    let (det_h, det_w) = match shape.as_slice() {
                        [1, 1, h, w] => (*h, *w),
                        _ => (height, width),
                    };
                    let boxes = process_paddleocr_detection(&values, &shape, 0.3, det_w, det_h);
                    let result = serde_json::json!({
                        "type": "text_detection",
                        "num_boxes": boxes.len(),
                        "boxes": boxes,
                        "note": "Bounding boxes in normalized [x1, y1, x2, y2] format"
                    });
                    return serde_json::to_string(&result)
                        .map_err(|e| InferenceError::ExecutionFailed(format!("JSON serialization failed: {}", e)));
                }

                // Check if this is ImageNet classification output
                if is_imagenet_classification(&shape) {
                    let sum: f32 = values.iter().sum();
                    let is_already_softmaxed = (sum - 1.0).abs() < 0.1;
                    let probabilities = if is_already_softmaxed { values.clone() } else { softmax(&values) };
                    let top_5 = get_top_k_predictions(&probabilities, 5);
                    let mut result = serde_json::Map::new();
                    result.insert("type".to_string(), serde_json::json!("classification"));
                    if let Some((class_id, confidence)) = top_5.first() {
                        let label = get_imagenet_label(*class_id);
                        result.insert("top_1".to_string(), serde_json::json!({"label": label, "class_id": class_id, "confidence": confidence}));
                    }
                    let top_5_json: Vec<serde_json::Value> = top_5.iter().map(|(class_id, confidence)| {
                        let label = get_imagenet_label(*class_id);
                        serde_json::json!({"label": label, "class_id": class_id, "confidence": confidence})
                    }).collect();
                    result.insert("top_5".to_string(), serde_json::json!(top_5_json));
                    return serde_json::to_string(&result)
                        .map_err(|e| InferenceError::ExecutionFailed(format!("JSON serialization failed: {}", e)));
                }

                // Default: return raw output
                // Log the shape to help debug why other matchers didn't trigger
                info!(model = %job_model_name, shape = ?shape, "⚠️ No specific handler matched, returning raw output");
                let result = serde_json::json!({"type": "raw", "shape": shape, "values_count": values.len()});
                serde_json::to_string(&result).map_err(|e| InferenceError::ExecutionFailed(format!("JSON serialization failed: {}", e)))
            }).await.map_err(|e| InferenceError::ExecutionFailed(format!("Inference thread join error: {}", e)))?;

            let output_json = run_result?;

            // Create a temporary result for metadata storage
            let temp_result = InferenceResult::success(
                job.job_id.clone(),
                self.node_id,
                String::new(), // Will be filled with actual URI
                0, // Latency will be updated by caller
            );

            // Store result with metadata
            let output_uri = self.store_blob_with_metadata(&job.job_id, output_json, &temp_result).await?;
            
            info!(
                model_path = %model_path.display(),
                output_uri = %output_uri,
                "Inference completed successfully"
            );

            Ok(output_uri)
        }

        /// Load input data from URI, decode image and resize to target dimensions
        /// Uses aspect-ratio preserving resize with center padding to avoid distortion
        async fn load_input_data(&self, input_uri: &str, target_width: usize, target_height: usize) -> Result<Vec<u8>> {
            use image::imageops::FilterType;
            use image::GenericImageView;

            // Helper to decode image bytes with aspect-ratio preserving resize and center padding
            // This prevents character distortion in OCR recognition
            fn decode_and_resize_preserve_aspect(bytes: &[u8], w: u32, h: u32) -> std::result::Result<Vec<u8>, String> {
                let img = image::load_from_memory(bytes).map_err(|e| format!("Image decode failed: {}", e))?;
                
                let (orig_w, orig_h) = img.dimensions();
                
                // Calculate scale to fit within target dimensions while preserving aspect ratio
                let scale_w = w as f32 / orig_w as f32;
                let scale_h = h as f32 / orig_h as f32;
                let scale = scale_w.min(scale_h);
                
                let new_w = (orig_w as f32 * scale).round() as u32;
                let new_h = (orig_h as f32 * scale).round() as u32;
                
                // Resize preserving aspect ratio
                let resized = img.resize(new_w, new_h, FilterType::Triangle);
                
                // Create canvas with padding (gray background for OCR)
                let mut canvas = image::RgbImage::from_pixel(w, h, image::Rgb([128, 128, 128]));
                
                // Center the resized image on the canvas
                let offset_x = ((w - new_w) / 2) as i64;
                let offset_y = ((h - new_h) / 2) as i64;
                
                image::imageops::overlay(&mut canvas, &resized.to_rgb8(), offset_x, offset_y);
                
                Ok(canvas.into_raw())
            }

            if input_uri.starts_with("blob://") {
                // Load from blob storage (placeholder - would use iroh blobs)
                let hash = input_uri.strip_prefix("blob://").unwrap();
                debug!(hash = %hash, "Loading input from blob storage (placeholder)");
                // For now, return a zeroed RGB image of requested size
                Ok(vec![0u8; target_width * target_height * 3])
            } else if input_uri.starts_with("file://") {
                let path = input_uri.strip_prefix("file://").unwrap();
                let bytes = tokio::fs::read(path).await
                    .map_err(|e| InferenceError::InputLoadFailed(format!("Failed to read file: {}", e)))?;
                decode_and_resize_preserve_aspect(&bytes, target_width as u32, target_height as u32)
                    .map_err(|e| InferenceError::InputLoadFailed(e))
            } else if input_uri.starts_with("http://") || input_uri.starts_with("https://") {
                // Download from URL
                let response = reqwest::get(input_uri).await
                    .map_err(|e| InferenceError::InputLoadFailed(format!("HTTP request failed: {}", e)))?;
                let bytes = response.bytes().await
                    .map_err(|e| InferenceError::InputLoadFailed(format!("Failed to read response: {}", e)))?;
                decode_and_resize_preserve_aspect(&bytes, target_width as u32, target_height as u32)
                    .map_err(|e| InferenceError::InputLoadFailed(e))
            } else {
                // Assume base64 encoded image data
                let decoded = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, input_uri)
                    .map_err(|e| InferenceError::InputLoadFailed(format!("Base64 decode failed: {}", e)))?;
                decode_and_resize_preserve_aspect(&decoded, target_width as u32, target_height as u32)
                    .map_err(|e| InferenceError::InputLoadFailed(e))
            }
        }

        /// Process ONNX Runtime outputs and convert to JSON
        fn process_ort_outputs(
            &self,
            outputs: &ort::session::output::SessionOutputs<'_>,
            _job: &InferenceJob,
        ) -> Result<String> {
            // Get first output value
            let (_, output_value) = outputs.iter().next()
                .ok_or_else(|| InferenceError::ExecutionFailed("No output tensors".to_string()))?;
            
            // Extract as f32 ndarray
            let array = output_value.try_extract_array::<f32>()
                .map_err(|e| InferenceError::ExecutionFailed(format!("Failed to extract array: {}", e)))?;
            
            let shape: Vec<usize> = array.shape().to_vec();
            let values: Vec<f32> = array.iter().cloned().collect();
                
                // Check if this is YOLO object detection output
                if is_yolo_detection(&shape) {
                    // Use input image size from shape inference or job defaults (assume normalized coords)
                    // We'll assume pixel scaling based on common input sizes (224/256/640)
                    let input_w = if _job.model_name.starts_with("yolo") { 640usize } else { 256usize };
                    let input_h = input_w;

                    let mut detections = process_yolo_output(&values, &shape, 0.25, input_w, input_h);
                    let detections = nms(&mut detections, 0.45);

                    let mut result = serde_json::Map::new();
                    result.insert("type".to_string(), serde_json::json!("object_detection"));
                    result.insert("num_detections".to_string(), serde_json::json!(detections.len()));
                    result.insert("detections".to_string(), serde_json::json!(detections));

                    return serde_json::to_string(&result)
                        .map_err(|e| InferenceError::ExecutionFailed(format!("JSON serialization failed: {}", e)));
                }
                
                // Check if this is ImageNet classification output
                if is_imagenet_classification(&shape) {
                    // Check if output is already softmaxed
                    let sum: f32 = values.iter().sum();
                    let is_already_softmaxed = (sum - 1.0).abs() < 0.1;
                    
                    let probabilities = if is_already_softmaxed {
                        values.clone()
                    } else {
                        softmax(&values)
                    };
                    
                    // Get top-5 predictions
                    let top_5 = get_top_k_predictions(&probabilities, 5);
                    
                    // Build structured classification result
                    let mut result = serde_json::Map::new();
                    result.insert("type".to_string(), serde_json::json!("classification"));
                    
                    // Top-1 prediction
                    if let Some((class_id, confidence)) = top_5.first() {
                        let label = get_imagenet_label(*class_id);
                        result.insert("top_1".to_string(), serde_json::json!({
                            "label": label,
                            "class_id": class_id,
                            "confidence": confidence
                        }));
                    }
                    
                    // Top-5 predictions
                    let top_5_json: Vec<serde_json::Value> = top_5.iter().map(|(class_id, confidence)| {
                        let label = get_imagenet_label(*class_id);
                        serde_json::json!({
                            "label": label,
                            "class_id": class_id,
                            "confidence": confidence
                        })
                    }).collect();
                    result.insert("top_5".to_string(), serde_json::json!(top_5_json));
                    
                    return serde_json::to_string(&result)
                        .map_err(|e| InferenceError::ExecutionFailed(format!("JSON serialization failed: {}", e)));
                }
                
                // Default: return raw output
                let result = serde_json::json!({
                    "type": "raw",
                    "shape": shape,
                    "values": values
                });
                serde_json::to_string(&result)
                    .map_err(|e| InferenceError::ExecutionFailed(format!("JSON serialization failed: {}", e)))
        }

        /// Store output JSON as blob and metadata (Async)
        ///
        /// Stores both the output data and metadata for retrieval by job ID
        async fn store_blob_with_metadata(
            &self,
            job_id: &str,
            output_json: String,
            result: &InferenceResult,
        ) -> Result<String> {
            // Store output as Iroh blob and return hash URI for download
            let output_bytes = output_json.as_bytes().to_vec();
            let blobs = self.blob_store.blobs();
            let tag = blobs.add_bytes(output_bytes.clone()).await
                .map_err(|e| InferenceError::ExecutionFailed(format!("Failed to store output blob: {}", e)))?;
            
            let blob_hash = tag.hash.to_string();
            let output_uri = format!("blob://{}", blob_hash);
            
            // Create metadata
            let metadata = InferenceResultMetadata::from_result(
                result,
                "json".to_string(),
                blob_hash.clone(),
            );
            
            // Store metadata with key result:{job_id}
            let metadata_json = serde_json::to_string(&metadata)
                .map_err(|e| InferenceError::ExecutionFailed(format!("Failed to serialize metadata: {}", e)))?;
            let metadata_bytes = metadata_json.as_bytes().to_vec();
            
            // Store metadata blob
            let metadata_tag = blobs.add_bytes(metadata_bytes).await
                .map_err(|e| InferenceError::ExecutionFailed(format!("Failed to store metadata blob: {}", e)))?;
            
            info!(
                job_id = %job_id,
                output_size = output_bytes.len(),
                blob_hash = %blob_hash,
                metadata_hash = %metadata_tag.hash,
                output_uri = %output_uri,
                "Inference output and metadata stored as blobs"
            );
            
            Ok(output_uri)
        }

        /// Store output JSON as blob and return URI (Async)
        /// Legacy method - prefer store_blob_with_metadata
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

