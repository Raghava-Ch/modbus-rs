//! Retry scheduling: backoff strategies and jitter.

/// Application-provided callback used to generate randomness for retry jitter.
///
/// The callback returns a raw `u32` value that is consumed by jitter logic.
/// The distribution does not need to be cryptographically secure. A simple
/// pseudo-random source from the target platform is sufficient.
pub type RetryRandomFn = fn() -> u32;

/// Retry delay strategy used after a request times out.
///
/// The delay is computed per retry attempt in a poll-driven manner. No internal
/// sleeping or blocking waits are performed by the library.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BackoffStrategy {
    /// Retry immediately after timeout detection.
    #[default]
    Immediate,
    /// Retry using a constant delay in milliseconds.
    Fixed {
        /// Delay applied before each retry.
        delay_ms: u32,
    },
    /// Retry with an exponential sequence: `base_delay_ms * 2^(attempt-1)`.
    Exponential {
        /// Base delay for the first retry attempt.
        base_delay_ms: u32,
        /// Upper bound used to clamp growth.
        max_delay_ms: u32,
    },
    /// Retry with a linear sequence: `initial_delay_ms + (attempt-1) * increment_ms`.
    Linear {
        /// Delay for the first retry attempt.
        initial_delay_ms: u32,
        /// Increment added on every subsequent retry.
        increment_ms: u32,
        /// Upper bound used to clamp growth.
        max_delay_ms: u32,
    },
}

impl BackoffStrategy {
    /// Computes the base retry delay in milliseconds for a 1-based retry attempt index.
    ///
    /// `retry_attempt` is expected to start at `1` for the first retry after the
    /// initial request timeout.
    pub fn delay_ms_for_retry(&self, retry_attempt: u8) -> u32 {
        let attempt = retry_attempt.max(1);
        match self {
            BackoffStrategy::Immediate => 0,
            BackoffStrategy::Fixed { delay_ms } => *delay_ms,
            BackoffStrategy::Exponential {
                base_delay_ms,
                max_delay_ms,
            } => {
                let shift = (attempt.saturating_sub(1)).min(31);
                let factor = 1u32 << shift;
                base_delay_ms.saturating_mul(factor).min(*max_delay_ms)
            }
            BackoffStrategy::Linear {
                initial_delay_ms,
                increment_ms,
                max_delay_ms,
            } => {
                let growth = increment_ms.saturating_mul((attempt.saturating_sub(1)) as u32);
                initial_delay_ms.saturating_add(growth).min(*max_delay_ms)
            }
        }
    }
}

/// Jitter strategy applied on top of computed backoff delay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JitterStrategy {
    /// Do not apply jitter.
    #[default]
    None,
    /// Apply symmetric percentage jitter around the base delay.
    ///
    /// For example, with `percent = 20` and base `100ms`, the final delay is
    /// in the range `[80ms, 120ms]`.
    Percentage {
        /// Maximum percentage variation from the base delay.
        percent: u8,
    },
    /// Apply symmetric bounded jitter in milliseconds around the base delay.
    ///
    /// For example, with `max_jitter_ms = 15` and base `100ms`, the final delay is
    /// in the range `[85ms, 115ms]`.
    BoundedMs {
        /// Maximum absolute jitter in milliseconds.
        max_jitter_ms: u32,
    },
}

impl JitterStrategy {
    /// Applies jitter to `base_delay_ms` using an application-provided random callback.
    ///
    /// If jitter is disabled or no callback is provided, this method returns `base_delay_ms`.
    pub fn apply(self, base_delay_ms: u32, random_fn: Option<RetryRandomFn>) -> u32 {
        let delta = match self {
            JitterStrategy::None => return base_delay_ms,
            JitterStrategy::Percentage { percent } => {
                if percent == 0 || base_delay_ms == 0 {
                    return base_delay_ms;
                }
                base_delay_ms.saturating_mul((percent.min(100)) as u32) / 100
            }
            JitterStrategy::BoundedMs { max_jitter_ms } => {
                if max_jitter_ms == 0 {
                    return base_delay_ms;
                }
                max_jitter_ms
            }
        };

        let random = match random_fn {
            Some(cb) => cb(),
            None => return base_delay_ms,
        };

        let span = delta.saturating_mul(2).saturating_add(1);
        if span == 0 {
            return base_delay_ms;
        }

        let offset = (random % span) as i64 - delta as i64;
        let jittered = base_delay_ms as i64 + offset;
        jittered.max(0) as u32
    }
}
