use std::future::Future;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, warn};
use rand::Rng;

/// Configuration for retry behavior with exponential backoff
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_attempts: u32,
    /// Initial backoff duration
    pub initial_backoff: Duration,
    /// Maximum backoff duration
    pub max_backoff: Duration,
    /// Multiplier for exponential backoff
    pub backoff_multiplier: f64,
    /// Add jitter to prevent thundering herd
    pub jitter: bool,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 5,
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_secs(30),
            backoff_multiplier: 2.0,
            jitter: true,
        }
    }
}

impl RetryConfig {
    /// Create a configuration for quick operations (less retries)
    pub fn quick() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(50),
            max_backoff: Duration::from_secs(5),
            ..Default::default()
        }
    }

    /// Create a configuration for important operations (more retries)
    pub fn persistent() -> Self {
        Self {
            max_attempts: 10,
            initial_backoff: Duration::from_millis(200),
            max_backoff: Duration::from_secs(60),
            ..Default::default()
        }
    }

    /// Calculate backoff duration for a given attempt
    fn backoff_duration(&self, attempt: u32) -> Duration {
        let base_duration = self.initial_backoff.as_millis() as f64
            * self.backoff_multiplier.powi(attempt as i32);
        
        let duration_ms = base_duration.min(self.max_backoff.as_millis() as f64) as u64;
        let mut duration = Duration::from_millis(duration_ms);

        // Add jitter: random value between 0% and 25% of duration
        if self.jitter {
            let jitter_ms = rand::thread_rng().gen_range(0..=(duration_ms / 4));
            duration += Duration::from_millis(jitter_ms);
        }

        duration
    }
}

/// Trait to determine if an error is retryable
pub trait Retryable {
    fn is_retryable(&self) -> bool;
}

impl Retryable for ghostsnap_core::Error {
    fn is_retryable(&self) -> bool {
        match self {
            // Network errors are generally retryable
            ghostsnap_core::Error::Io(_) => true,
            // Backend errors might be retryable (rate limits, temporary failures)
            ghostsnap_core::Error::Backend(msg) => {
                // Retry on common transient errors
                msg.contains("timeout")
                    || msg.contains("rate limit")
                    || msg.contains("throttle")
                    || msg.contains("temporarily unavailable")
                    || msg.contains("try again")
                    || msg.contains("503")
                    || msg.contains("429")
            }
            // Don't retry on authentication, validation, or corruption errors
            ghostsnap_core::Error::InvalidPassword
            | ghostsnap_core::Error::RepositoryNotFound { .. }
            | ghostsnap_core::Error::RepositoryExists { .. }
            | ghostsnap_core::Error::InvalidFormatVersion { .. }
            | ghostsnap_core::Error::CorruptedPack { .. } => false,
            // Other errors - default to not retrying
            _ => false,
        }
    }
}

/// Retry a future operation with exponential backoff
pub async fn retry_with_backoff<F, Fut, T, E>(
    config: &RetryConfig,
    operation_name: &str,
    mut operation: F,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: Retryable + std::fmt::Display,
{
    let mut last_error = None;

    for attempt in 0..config.max_attempts {
        match operation().await {
            Ok(result) => {
                if attempt > 0 {
                    debug!(
                        operation = operation_name,
                        attempt = attempt + 1,
                        "Operation succeeded after retry"
                    );
                }
                return Ok(result);
            }
            Err(error) => {
                if !error.is_retryable() {
                    debug!(
                        operation = operation_name,
                        error = %error,
                        "Error is not retryable, failing immediately"
                    );
                    return Err(error);
                }

                last_error = Some(error);

                // Don't sleep after the last attempt
                if attempt < config.max_attempts - 1 {
                    let backoff = config.backoff_duration(attempt);
                    warn!(
                        operation = operation_name,
                        attempt = attempt + 1,
                        max_attempts = config.max_attempts,
                        backoff_ms = backoff.as_millis(),
                        error = %last_error.as_ref().unwrap(),
                        "Operation failed, retrying after backoff"
                    );
                    sleep(backoff).await;
                }
            }
        }
    }

    // All attempts exhausted
    let error = last_error.expect("Should have at least one error");
    warn!(
        operation = operation_name,
        max_attempts = config.max_attempts,
        error = %error,
        "Operation failed after all retry attempts"
    );
    Err(error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_retry_succeeds_eventually() {
        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = attempts.clone();

        let config = RetryConfig {
            max_attempts: 5,
            initial_backoff: Duration::from_millis(10),
            max_backoff: Duration::from_millis(50),
            backoff_multiplier: 2.0,
            jitter: false,
        };

        let result = retry_with_backoff(&config, "test_operation", || {
            let attempts = attempts_clone.clone();
            async move {
                let count = attempts.fetch_add(1, Ordering::SeqCst);
                if count < 2 {
                    Err(ghostsnap_core::Error::Io(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "Temporary failure",
                    )))
                } else {
                    Ok(42)
                }
            }
        })
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_fails_after_max_attempts() {
        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = attempts.clone();

        let config = RetryConfig {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(10),
            max_backoff: Duration::from_millis(50),
            backoff_multiplier: 2.0,
            jitter: false,
        };

        let result = retry_with_backoff(&config, "test_operation", || {
            let attempts = attempts_clone.clone();
            async move {
                attempts.fetch_add(1, Ordering::SeqCst);
                Err::<i32, _>(ghostsnap_core::Error::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "Persistent failure",
                )))
            }
        })
        .await;

        assert!(result.is_err());
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_non_retryable_error_fails_immediately() {
        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = attempts.clone();

        let config = RetryConfig::default();

        let result = retry_with_backoff(&config, "test_operation", || {
            let attempts = attempts_clone.clone();
            async move {
                attempts.fetch_add(1, Ordering::SeqCst);
                Err::<i32, _>(ghostsnap_core::Error::InvalidPassword)
            }
        })
        .await;

        assert!(result.is_err());
        assert_eq!(attempts.load(Ordering::SeqCst), 1); // Should not retry
    }

    #[test]
    fn test_backoff_duration_calculation() {
        let config = RetryConfig {
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_secs(10),
            backoff_multiplier: 2.0,
            jitter: false,
            ..Default::default()
        };

        // Attempt 0: 100ms * 2^0 = 100ms
        assert_eq!(config.backoff_duration(0), Duration::from_millis(100));

        // Attempt 1: 100ms * 2^1 = 200ms
        assert_eq!(config.backoff_duration(1), Duration::from_millis(200));

        // Attempt 2: 100ms * 2^2 = 400ms
        assert_eq!(config.backoff_duration(2), Duration::from_millis(400));

        // Should cap at max_backoff
        assert_eq!(config.backoff_duration(10), Duration::from_secs(10));
    }
}
