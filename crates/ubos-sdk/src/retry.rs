//! The retry policy (DSN-SDK-003): only errors the registry classifies retryable (and
//! transport failures) are retried, with exponential back-off and full jitter, within the
//! caller's deadline.

use crate::error::{Error, Result};
use std::time::{Duration, Instant};

/// How a call is retried.
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Attempts in all (1: no retry).
    pub max_attempts: u32,
    /// The first back-off.
    pub base: Duration,
    /// The longest back-off.
    pub cap: Duration,
    /// The deadline of a call when the caller gives none.
    pub deadline: Duration,
}

impl Default for RetryPolicy {
    fn default() -> RetryPolicy {
        RetryPolicy {
            max_attempts: 6,
            base: Duration::from_millis(100),
            cap: Duration::from_secs(5),
            deadline: Duration::from_secs(30),
        }
    }
}

impl RetryPolicy {
    /// No retries.
    pub fn none() -> RetryPolicy {
        RetryPolicy {
            max_attempts: 1,
            ..RetryPolicy::default()
        }
    }

    /// The back-off before attempt `n` (from 1): a uniform draw in
    /// `[0, min(cap, base · 2^n)]`.
    pub fn backoff(&self, n: u32) -> Duration {
        let ceiling = self
            .base
            .saturating_mul(1u32.checked_shl(n.min(20)).unwrap_or(u32::MAX))
            .min(self.cap);
        Duration::from_millis(crate::util::jitter(ceiling.as_millis() as u64 + 1))
    }

    /// Run `op` until it succeeds, fails with an error that is not retryable, runs out of
    /// attempts or would pass `deadline`. `head_moved` says whether `TXN.HEAD_MOVED` is
    /// retryable for this operation (re-executable: no `base` given).
    pub async fn run<T, F, Fut>(&self, deadline: Instant, head_moved: bool, mut op: F) -> Result<T>
    where
        F: FnMut(u32) -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        let mut attempt = 0;
        loop {
            attempt += 1;
            let e = match op(attempt).await {
                Ok(v) => return Ok(v),
                Err(e) => e,
            };
            let retry = if e.code_text == "TXN.HEAD_MOVED" {
                head_moved
            } else {
                e.retryable()
            };
            let wait = self.backoff(attempt);
            if !retry || attempt >= self.max_attempts || Instant::now() + wait >= deadline {
                return Err(e);
            }
            tokio::time::sleep(wait).await;
        }
    }
}

/// An error for a call whose deadline passed.
pub fn deadline_passed() -> Error {
    Error::transport("the deadline passed")
}
