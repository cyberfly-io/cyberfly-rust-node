use crate::error::DbError;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tracing::{debug, warn};

type Result<T> = std::result::Result<T, DbError>;

#[derive(Debug, Clone)]
pub enum RetryStrategy {
    None,
    Linear { max_attempts: u32, delay_ms: u64 },
    Exponential { max_attempts: u32, base_delay_ms: u64, max_delay_ms: u64 },
    Custom { intervals_ms: Vec<u64> },
}

/// Retry executor for handling recoverable operations
pub struct RetryExecutor {
    strategy: RetryStrategy,
}

impl RetryExecutor {
    pub fn new(strategy: RetryStrategy) -> Self {
        Self { strategy }
    }

    /// Execute an operation with retry logic
    pub async fn execute<F, Fut, T>(&self, operation: F) -> Result<T>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        match &self.strategy {
            RetryStrategy::None => operation().await,
            RetryStrategy::Linear { max_attempts, delay_ms } => {
                self.execute_linear(operation, *max_attempts, *delay_ms).await
            }
            RetryStrategy::Exponential { max_attempts, base_delay_ms, max_delay_ms } => {
                self.execute_exponential(operation, *max_attempts, *base_delay_ms, *max_delay_ms).await
            }
            RetryStrategy::Custom { intervals_ms } => {
                self.execute_custom(operation, intervals_ms).await
            }
        }
    }

    async fn execute_linear<F, Fut, T>(&self, operation: F, max_attempts: u32, delay_ms: u64) -> Result<T>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        let mut last_error = None;
        
        for attempt in 1..=max_attempts {
            debug!("Attempt {} of {}", attempt, max_attempts);
            
            match operation().await {
                Ok(result) => return Ok(result),
                Err(err) => {
                    if !err.is_recoverable() || attempt == max_attempts {
                        return Err(err);
                    }
                    
                    warn!("Attempt {} failed: {}, retrying in {}ms", attempt, err, delay_ms);
                    last_error = Some(err);
                    sleep(Duration::from_millis(delay_ms)).await;
                }
            }
        }
        
        Err(last_error.unwrap())
    }

    async fn execute_exponential<F, Fut, T>(
        &self,
        operation: F,
        max_attempts: u32,
        base_delay_ms: u64,
        max_delay_ms: u64,
    ) -> Result<T>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        let mut last_error = None;
        let mut delay = base_delay_ms;
        
        for attempt in 1..=max_attempts {
            debug!("Attempt {} of {}", attempt, max_attempts);
            
            match operation().await {
                Ok(result) => return Ok(result),
                Err(err) => {
                    if !err.is_recoverable() || attempt == max_attempts {
                        return Err(err);
                    }
                    
                    warn!("Attempt {} failed: {}, retrying in {}ms", attempt, err, delay);
                    last_error = Some(err);
                    sleep(Duration::from_millis(delay)).await;
                    
                    // Exponential backoff with cap
                    delay = std::cmp::min(delay * 2, max_delay_ms);
                }
            }
        }
        
        Err(last_error.unwrap())
    }

    async fn execute_custom<F, Fut, T>(&self, operation: F, intervals_ms: &[u64]) -> Result<T>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        let max_attempts = intervals_ms.len() + 1;
        let mut last_error = None;
        
        for (attempt, &delay_ms) in std::iter::once(&0u64).chain(intervals_ms.iter()).enumerate() {
            let attempt = attempt + 1;
            debug!("Attempt {} of {}", attempt, max_attempts);
            
            if attempt > 1 {
                sleep(Duration::from_millis(delay_ms)).await;
            }
            
            match operation().await {
                Ok(result) => return Ok(result),
                Err(err) => {
                    if !err.is_recoverable() || attempt == max_attempts {
                        return Err(err);
                    }
                    
                    warn!("Attempt {} failed: {}, retrying in {}ms", attempt, err, delay_ms);
                    last_error = Some(err);
                }
            }
        }
        
        Err(last_error.unwrap())
    }
}

/// Circuit breaker for preventing cascading failures
pub struct CircuitBreaker {
    failure_threshold: u32,
    recovery_timeout: Duration,
    failure_count: std::sync::Arc<std::sync::Mutex<u32>>,
    last_failure_time: std::sync::Arc<std::sync::Mutex<Option<Instant>>>,
    state: std::sync::Arc<std::sync::Mutex<CircuitBreakerState>>,
}

#[derive(Debug, Clone, PartialEq)]
enum CircuitBreakerState {
    Closed,
    Open,
    HalfOpen,
}

impl CircuitBreaker {
    pub fn new(failure_threshold: u32, recovery_timeout: Duration) -> Self {
        Self {
            failure_threshold,
            recovery_timeout,
            failure_count: std::sync::Arc::new(std::sync::Mutex::new(0)),
            last_failure_time: std::sync::Arc::new(std::sync::Mutex::new(None)),
            state: std::sync::Arc::new(std::sync::Mutex::new(CircuitBreakerState::Closed)),
        }
    }

    pub async fn execute<F, Fut, T>(&self, operation: F) -> Result<T>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        // Check if circuit breaker should allow the operation
        if !self.should_allow_request() {
            return Err(DbError::ResourceExhausted("Circuit breaker is open".to_string()));
        }

        match operation().await {
            Ok(result) => {
                self.on_success();
                Ok(result)
            }
            Err(err) => {
                self.on_failure();
                Err(err)
            }
        }
    }

    fn should_allow_request(&self) -> bool {
        let mut state = self.state.lock().unwrap();
        
        match *state {
            CircuitBreakerState::Closed => true,
            CircuitBreakerState::Open => {
                let last_failure = self.last_failure_time.lock().unwrap();
                if let Some(last_time) = *last_failure {
                    if last_time.elapsed() >= self.recovery_timeout {
                        *state = CircuitBreakerState::HalfOpen;
                        true
                    } else {
                        false
                    }
                } else {
                    true
                }
            }
            CircuitBreakerState::HalfOpen => true,
        }
    }

    fn on_success(&self) {
        let mut failure_count = self.failure_count.lock().unwrap();
        let mut state = self.state.lock().unwrap();
        
        *failure_count = 0;
        *state = CircuitBreakerState::Closed;
    }

    fn on_failure(&self) {
        let mut failure_count = self.failure_count.lock().unwrap();
        let mut last_failure_time = self.last_failure_time.lock().unwrap();
        let mut state = self.state.lock().unwrap();
        
        *failure_count += 1;
        *last_failure_time = Some(Instant::now());
        
        if *failure_count >= self.failure_threshold {
            *state = CircuitBreakerState::Open;
        }
    }
}

/// Convenience functions for common retry patterns
pub async fn retry_with_exponential_backoff<F, Fut, T>(
    operation: F,
    max_attempts: u32,
    base_delay_ms: u64,
) -> Result<T>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let executor = RetryExecutor::new(
        RetryStrategy::Exponential {
            max_attempts,
            base_delay_ms,
            max_delay_ms: base_delay_ms * 16, // Cap at 16x base delay
        },
    );
    
    executor.execute(operation).await
}

pub async fn retry_with_linear_backoff<F, Fut, T>(
    operation: F,
    max_attempts: u32,
    delay_ms: u64,
) -> Result<T>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let executor = RetryExecutor::new(
        RetryStrategy::Linear { max_attempts, delay_ms },
    );
    
    executor.execute(operation).await
}