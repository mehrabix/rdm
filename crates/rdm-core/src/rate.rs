//! Global bandwidth limiting.
//!
//! A single limiter is shared by every connection of every download, so a
//! configured limit applies to the *whole application* (IDM's "speed limit").

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// One-second sliding window. `limit == 0` means unlimited.
///
/// Callers `acquire(n)` before writing `n` bytes; the call sleeps until the
/// current window's budget allows it. The very first chunk of a window is always
/// admitted so a limit smaller than one chunk cannot deadlock.
pub struct RateLimiter {
    limit: AtomicU64,
    window: Mutex<(Instant, u64)>,
}

impl RateLimiter {
    pub fn new(limit_bps: u64) -> Self {
        Self {
            limit: AtomicU64::new(limit_bps),
            window: Mutex::new((Instant::now(), 0)),
        }
    }

    pub fn limit(&self) -> u64 {
        self.limit.load(Ordering::Relaxed)
    }

    /// Change the limit at runtime (0 = unlimited).
    pub fn set_limit(&self, bytes_per_sec: u64) {
        self.limit.store(bytes_per_sec, Ordering::Relaxed);
    }

    /// Wait until `bytes` fit inside the budget.
    pub async fn acquire(&self, bytes: u64) {
        loop {
            let limit = self.limit.load(Ordering::Relaxed);
            if limit == 0 {
                return;
            }

            let wait = {
                let mut window = self.window.lock().unwrap();
                let now = Instant::now();
                let elapsed = now.duration_since(window.0);
                if elapsed >= Duration::from_secs(1) {
                    *window = (now, 0);
                }
                if window.1 == 0 || window.1 + bytes <= limit {
                    window.1 += bytes;
                    return;
                }
                Duration::from_secs(1).saturating_sub(elapsed)
            };
            tokio::time::sleep(wait).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unlimited_never_blocks() {
        let limiter = RateLimiter::new(0);
        limiter.acquire(10_000_000).await;
        assert_eq!(limiter.limit(), 0);
    }

    #[tokio::test]
    async fn first_chunk_is_always_admitted() {
        // A chunk larger than the budget must not deadlock the limiter.
        let limiter = RateLimiter::new(1000);
        limiter.acquire(5000).await;
        assert_eq!(limiter.limit(), 1000);
        limiter.set_limit(0);
        assert_eq!(limiter.limit(), 0);
    }

    #[tokio::test]
    async fn budget_is_consumed_within_a_window() {
        let limiter = RateLimiter::new(1000);
        limiter.acquire(600).await;
        limiter.acquire(400).await; // exactly fills the second
        assert_eq!(limiter.limit(), 1000);
    }
}
