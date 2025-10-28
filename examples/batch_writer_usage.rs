// Example: Using BatchWriter for High-Throughput Writes
// 
// This example demonstrates how to use the BatchWriter for parallel write processing
// with bounded concurrency control, achieving 5-10x write throughput improvement.

use cyberfly_rust_node::{
    BatchWriter, RedisStorage, StoreType, SignatureMetadata,
};
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize storage
    let store = /* initialize FsStore */;
    let storage = RedisStorage::new(store, None).await?;
    
    // Create BatchWriter with max 10 concurrent writes
    let batch_writer = storage.batch_writer(Some(10));
    
    // Example 1: Write single item with concurrency control
    single_write_example(&batch_writer).await?;
    
    // Example 2: Batch write multiple items in parallel
    batch_write_example(&batch_writer).await?;
    
    // Example 3: High-throughput streaming writes
    streaming_write_example(&batch_writer).await?;
    
    // Example 4: Monitor batch writer stats
    stats_example(&batch_writer).await?;
    
    Ok(())
}

/// Example 1: Write a single item with concurrency control
async fn single_write_example(batch_writer: &BatchWriter) -> Result<()> {
    println!("=== Example 1: Single Write ===");
    
    let key = "sensor:temperature:001".to_string();
    let value = serde_json::json!({
        "value": 23.5,
        "timestamp": 1698504000,
        "unit": "celsius"
    });
    
    // Create StoredValue (simplified - actual implementation varies)
    let stored_value = /* convert to StoredValue */;
    
    // Write with automatic concurrency control
    batch_writer.write_one(key, stored_value, StoreType::Json).await?;
    
    println!("✅ Single write completed");
    Ok(())
}

/// Example 2: Batch write multiple items in parallel
async fn batch_write_example(batch_writer: &BatchWriter) -> Result<()> {
    println!("=== Example 2: Batch Write ===");
    
    // Prepare batch of items to write
    let mut items = Vec::new();
    
    for i in 0..100 {
        let key = format!("sensor:temp:{:03}", i);
        let value = serde_json::json!({
            "sensor_id": i,
            "temperature": 20.0 + (i as f64 * 0.1),
            "timestamp": 1698504000 + i,
        });
        
        // Convert to StoredValue
        let stored_value = /* convert */;
        
        items.push((key, stored_value, StoreType::Json));
    }
    
    // Write all items in parallel (max 10 concurrent)
    let results = batch_writer.write_batch(items).await;
    
    // Check results
    let success_count = results.iter().filter(|r| r.is_ok()).count();
    let error_count = results.iter().filter(|r| r.is_err()).count();
    
    println!("✅ Batch write completed: {} success, {} errors", success_count, error_count);
    
    Ok(())
}

/// Example 3: High-throughput streaming writes
async fn streaming_write_example(batch_writer: &BatchWriter) -> Result<()> {
    println!("=== Example 3: Streaming Write ===");
    
    use tokio::time::{Duration, Instant};
    
    let start = Instant::now();
    let mut handles = Vec::new();
    
    // Simulate 1000 incoming write requests
    for i in 0..1000 {
        let writer = batch_writer.clone();
        let key = format!("stream:data:{:04}", i);
        
        let handle = tokio::spawn(async move {
            let value = /* create value */;
            writer.write_one(key, value, StoreType::Json).await
        });
        
        handles.push(handle);
    }
    
    // Wait for all writes
    for handle in handles {
        handle.await??;
    }
    
    let elapsed = start.elapsed();
    let throughput = 1000.0 / elapsed.as_secs_f64();
    
    println!("✅ Streaming write completed");
    println!("   Total time: {:?}", elapsed);
    println!("   Throughput: {:.2} writes/sec", throughput);
    
    Ok(())
}

/// Example 4: Monitor batch writer statistics
async fn stats_example(batch_writer: &BatchWriter) -> Result<()> {
    println!("=== Example 4: Batch Writer Stats ===");
    
    let stats = batch_writer.stats();
    
    println!("Max concurrent writes: {}", stats.max_concurrent);
    println!("Available permits: {}", stats.available_permits);
    println!("Active writes: {}", stats.max_concurrent - stats.available_permits);
    
    Ok(())
}

/// Performance comparison: Sequential vs Batch
async fn performance_comparison() -> Result<()> {
    println!("=== Performance Comparison ===");
    
    let storage = /* initialize */;
    let items_count = 1000;
    
    // Sequential writes (baseline)
    let start = Instant::now();
    for i in 0..items_count {
        let key = format!("seq:{}", i);
        storage.set_json(&key, "/", &serde_json::json!({"value": i}).to_string()).await?;
    }
    let sequential_time = start.elapsed();
    
    // Batch writes (optimized)
    let batch_writer = storage.batch_writer(Some(10));
    let start = Instant::now();
    
    let mut batch = Vec::new();
    for i in 0..items_count {
        let key = format!("batch:{}", i);
        let value = /* create value */;
        batch.push((key, value, StoreType::Json));
    }
    
    batch_writer.write_batch(batch).await;
    let batch_time = start.elapsed();
    
    // Results
    let speedup = sequential_time.as_secs_f64() / batch_time.as_secs_f64();
    
    println!("Sequential: {:?} ({:.2} writes/sec)", 
        sequential_time, 
        items_count as f64 / sequential_time.as_secs_f64()
    );
    println!("Batch:      {:?} ({:.2} writes/sec)", 
        batch_time,
        items_count as f64 / batch_time.as_secs_f64()
    );
    println!("Speedup:    {:.2}x", speedup);
    
    Ok(())
}

/// IoT sensor data streaming example
async fn iot_streaming_example() -> Result<()> {
    println!("=== IoT Sensor Streaming ===");
    
    let storage = /* initialize */;
    let batch_writer = storage.batch_writer(Some(20)); // Higher concurrency for IoT
    
    // Simulate 10 sensors sending data every second
    let sensor_count = 10;
    let duration_seconds = 60;
    
    let start = Instant::now();
    let mut total_writes = 0;
    
    for second in 0..duration_seconds {
        let mut batch = Vec::new();
        
        for sensor_id in 0..sensor_count {
            let key = format!("iot:sensor:{}:ts:{}", sensor_id, second);
            let value = serde_json::json!({
                "sensor_id": sensor_id,
                "timestamp": 1698504000 + second,
                "temperature": 20.0 + (sensor_id as f64 * 0.5),
                "humidity": 50.0 + (second as f64 * 0.1),
            });
            
            let stored_value = /* convert */;
            batch.push((key, stored_value, StoreType::Json));
        }
        
        // Write batch for this second
        let results = batch_writer.write_batch(batch).await;
        total_writes += results.iter().filter(|r| r.is_ok()).count();
        
        // Simulate 1 second interval (in production, this would be real-time)
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    
    let elapsed = start.elapsed();
    
    println!("✅ IoT streaming completed");
    println!("   Total writes: {}", total_writes);
    println!("   Duration: {:?}", elapsed);
    println!("   Throughput: {:.2} writes/sec", total_writes as f64 / elapsed.as_secs_f64());
    
    Ok(())
}

/// Best practices for using BatchWriter
fn best_practices() {
    println!(r#"
=== BatchWriter Best Practices ===

1. Concurrency Tuning:
   - Start with max_concurrent=10 for typical workloads
   - Increase to 20-50 for high-throughput IoT/sensor data
   - Decrease to 5 for low-memory or disk-constrained systems
   - Monitor with batch_writer.stats() and adjust

2. Batch Size:
   - Optimal batch size: 100-1000 items
   - Larger batches (>1000): Split into multiple calls
   - Smaller batches (<10): Consider single writes instead
   - Trade-off: latency vs throughput

3. Error Handling:
   - write_batch() returns Vec<Result<()>>
   - Check each result for partial failures
   - Implement retry logic for failed items
   - Log errors with context (key, timestamp)

4. Resource Management:
   - BatchWriter uses semaphore for backpressure
   - If all permits taken, new writes wait
   - Monitor available_permits to detect bottlenecks
   - Use metrics to track write latency

5. Performance Monitoring:
   - Use Prometheus metrics: storage_writes_total
   - Track write latency: storage_write_duration_seconds
   - Monitor batch_writer.stats() for saturation
   - Alert on high latency or low throughput

6. When to Use:
   - ✅ Bulk data imports
   - ✅ High-frequency sensor data
   - ✅ Event log writes
   - ✅ Cache warming
   - ❌ Single writes (use write_one)
   - ❌ Ordered writes (semaphore doesn't guarantee order)

7. Memory Considerations:
   - Each concurrent write holds memory
   - max_concurrent=50 with 1MB items = 50MB RAM
   - Monitor memory usage under load
   - Adjust concurrency if OOM occurs

8. Integration with Metrics:
   - All writes tracked automatically
   - View metrics at /metrics endpoint
   - Calculate batch throughput:
     rate(storage_writes_total[1m])
   - Alert on p95 latency > 200ms
"#);
}
