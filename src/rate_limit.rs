use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
    time::{Duration, Instant},
};

use tokio::sync::Mutex;

use crate::{
    error::{AppError, AppResult},
};

#[derive(Clone)]
pub struct AuthRateLimiter {
    inner: Arc<Mutex<HashMap<String, VecDeque<Instant>>>>,
    max_requests: usize,
    window: Duration,
}

impl AuthRateLimiter {
    pub fn new(max_requests: usize, window: Duration) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            max_requests,
            window,
        }
    }

    pub async fn check(&self, key: &str, operation: &str) -> AppResult<()> {
        let now = Instant::now();
        let mut buckets = self.inner.lock().await;
        let bucket = buckets.entry(key.to_string()).or_default();

        while matches!(bucket.front(), Some(timestamp) if now.duration_since(*timestamp) >= self.window)
        {
            bucket.pop_front();
        }

        if bucket.len() >= self.max_requests {
            let retry_after_secs = bucket
                .front()
                .map(|timestamp| {
                    self.window
                        .saturating_sub(now.duration_since(*timestamp))
                        .as_secs()
                        .max(1)
                })
                .unwrap_or(1);

            return Err(AppError::RateLimited {
                service: "auth",
                operation: operation.to_string(),
                retry_after_secs,
            });
        }

        bucket.push_back(now);
        Ok(())
    }
}
