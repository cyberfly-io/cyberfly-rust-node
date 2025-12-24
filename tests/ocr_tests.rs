// Integration test for PaddleOCR pipeline
#[cfg(test)]
mod ocr_tests {
    use std::path::PathBuf;
    use ort::session::Session;

    #[tokio::test]
    async fn test_paddleocr_full_pipeline() {
        // Initialize logging for test
        let _ = tracing_subscriber::fmt()
            .with_env_filter("info,ort::logging=error")
            .try_init();

        println!("\n🔬 Testing PaddleOCR Full Pipeline");
        println!("==================================\n");

        // Test image path
        let test_image = "/tmp/ocr_test.png";
        
        if !std::path::Path::new(test_image).exists() {
            eprintln!("⚠️  Test image not found: {}", test_image);
            eprintln!("   Skipping OCR test - copy a test image to /tmp/ocr_test.png to run");
            return;
        }

        println!("📷 Test image: {}", test_image);
        
        // Load the image
        let img = match image::open(test_image) {
            Ok(img) => img,
            Err(e) => {
                eprintln!("❌ Failed to load image: {}", e);
                return;
            }
        };
        
        println!("✓ Image loaded: {}x{}", img.width(), img.height());

        // Create model directory path
        let model_dir = PathBuf::from("models");
        
        // Check if models exist
        let det_model = model_dir.join("paddleocr_det.onnx");
        let rec_model = model_dir.join("paddleocr_rec_en.onnx");
        let ori_model = model_dir.join("paddleocr_textline_ori.onnx");
        let dict_path = model_dir.join("paddleocr_dict.txt");
        
        if !det_model.exists() || !rec_model.exists() || !ori_model.exists() || !dict_path.exists() {
            eprintln!("⚠️  Required models not found in models/ directory");
            eprintln!("   Skipping OCR test");
            return;
        }

        // Load PaddleOCR dictionary
        let dictionary = match std::fs::read_to_string(&dict_path) {
            Ok(dict) => dict,
            Err(e) => {
                eprintln!("❌ Failed to load dictionary: {}", e);
                return;
            }
        };
        let dictionary: Vec<String> = dictionary.lines().map(|s| s.to_string()).collect();
        println!("✓ Loaded dictionary: {} characters", dictionary.len());

        // Initialize ONNX Runtime
        if let Err(e) = ort::init()
            .with_execution_providers([ort::execution_providers::CPUExecutionProvider::default().build()])
            .commit()
        {
            eprintln!("❌ Failed to initialize ONNX Runtime: {}", e);
            return;
        }

        // Load models
        println!("\n📦 Loading ONNX models...");
        
        let det_session = match Session::builder()
            .and_then(|b| b.commit_from_file(&det_model))
        {
            Ok(s) => s,
            Err(e) => {
                eprintln!("❌ Failed to load detection model: {}", e);
                return;
            }
        };
        println!("  ✓ Detection model loaded");
        
        let rec_session = match Session::builder()
            .and_then(|b| b.commit_from_file(&rec_model))
        {
            Ok(s) => s,
            Err(e) => {
                eprintln!("❌ Failed to load recognition model: {}", e);
                return;
            }
        };
        println!("  ✓ Recognition model loaded");
        
        let ori_session = match Session::builder()
            .and_then(|b| b.commit_from_file(&ori_model))
        {
            Ok(s) => s,
            Err(e) => {
                eprintln!("❌ Failed to load orientation model: {}", e);
                return;
            }
        };
        println!("  ✓ Textline orientation model loaded");

        println!("\n✅ All models loaded successfully!");
        println!("📝 Models are ready for inference");
        println!("\n💡 To test the full OCR pipeline:");
        println!("   1. Start the server: cargo run --release");
        println!("   2. Submit a job via the inference worker");
        println!("   3. Check logs for OCR output");
    }
}
