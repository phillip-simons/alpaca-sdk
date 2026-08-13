//! Exponential backoff with equal jitter, shared by the stream reconnect and
//! the REST retry.
//!
//! One curve serves both on purpose. The reconnect had it first; the REST path
//! waited a flat interval that contradicted [Alpaca's own rate-limit
//! guidance][rate-limits], and now calls [`reconnect_delay`] through
//! [`RetryBackoff::Exponential`](crate::RetryBackoff::Exponential).
//!
//! The jitter matters in both places, for the same reason. Alpaca rate-limits
//! per account and permits a single stream connection, so a fixed delay turns
//! one 429 — or one stale connection being reaped — into a burst of retries that
//! arrive together and collide again. Spreading them is what breaks the cycle.
//!
//! [rate-limits]: https://docs.alpaca.markets/us/docs/broker-api-rate-limits

use std::time::Duration;

use rand::RngExt as _;

/// Base delay for the first retry.
pub const DEFAULT_MIN_BACKOFF: Duration = Duration::from_secs(1);

/// Ceiling the exponential growth is capped at.
pub const DEFAULT_MAX_BACKOFF: Duration = Duration::from_secs(30);

/// Computes how long to wait before reconnect attempt number `retries`.
///
/// `retries` counts consecutive failures and is 1-based; the first failure waits
/// `min_backoff`, and each subsequent one doubles until `max_backoff`. The result
/// is drawn uniformly from `[capped / 2, capped]` — equal jitter — so a fleet of
/// clients reconnecting after the same outage spreads out rather than colliding.
#[must_use]
pub fn reconnect_delay(retries: u32, min_backoff: Duration, max_backoff: Duration) -> Duration {
    let capped = capped_delay(retries, min_backoff, max_backoff).as_secs_f64();
    let half = capped / 2.0;
    let jitter = if half > 0.0 {
        rand::rng().random_range(0.0..half)
    } else {
        0.0
    };
    Duration::from_secs_f64(half + jitter)
}

/// The deterministic, pre-jitter delay. Exposed for tests and for callers that
/// want to reason about the growth curve without sampling it.
#[must_use]
pub fn capped_delay(retries: u32, min_backoff: Duration, max_backoff: Duration) -> Duration {
    let max = max_backoff.as_secs_f64();
    let mut capped = min_backoff.as_secs_f64();

    for _ in 0..retries.saturating_sub(1) {
        if capped >= max / 2.0 {
            capped = max;
            break;
        }
        capped *= 2.0;
    }

    Duration::from_secs_f64(capped.min(max))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: Duration = DEFAULT_MIN_BACKOFF;
    const MAX: Duration = DEFAULT_MAX_BACKOFF;

    #[test]
    fn growth_curve_doubles_to_the_cap() {
        // 1s base, doubling, 30s cap. The tail is the part worth pinning: the
        // loop jumps straight to the ceiling once the value reaches half the
        // maximum, so 16 is followed by 30, not 32. Doubling to 32 and then
        // clamping would give the same answer here and a different one for any
        // other ceiling.
        let expected = [1.0, 1.0, 2.0, 4.0, 8.0, 16.0, 30.0, 30.0, 30.0];
        for (retries, want) in expected.into_iter().enumerate() {
            #[allow(clippy::cast_possible_truncation)]
            let got = capped_delay(retries as u32, MIN, MAX).as_secs_f64();
            assert!(
                (got - want).abs() < f64::EPSILON,
                "retries={retries}: got {got}, want {want}"
            );
        }
    }

    #[test]
    fn jitter_stays_within_half_to_full_window() {
        for retries in 1..=8 {
            let capped = capped_delay(retries, MIN, MAX);
            for _ in 0..64 {
                let delay = reconnect_delay(retries, MIN, MAX);
                assert!(
                    delay >= capped / 2 && delay <= capped,
                    "retries={retries}: {delay:?} outside [{:?}, {capped:?}]",
                    capped / 2
                );
            }
        }
    }

    #[test]
    fn jitter_actually_varies() {
        let samples: Vec<_> = (0..32).map(|_| reconnect_delay(5, MIN, MAX)).collect();
        let first = samples[0];
        assert!(
            samples.iter().any(|d| *d != first),
            "delay is constant, jitter is not being applied"
        );
    }

    #[test]
    fn never_exceeds_the_ceiling() {
        assert_eq!(capped_delay(1_000, MIN, MAX), MAX);
    }
}
