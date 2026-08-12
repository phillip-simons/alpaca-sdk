//! Exponential backoff with equal jitter for websocket reconnects.
//!
//! The jitter matters: Alpaca permits a single stream connection, so a fixed
//! delay turns a stale connection being reaped into a tight reconnect/HTTP 429
//! storm.
//!
//! Ported from `alpaca.common.utils.reconnect_delay`.

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
    fn growth_curve_matches_alpaca_py() {
        // 1s base, doubling, 30s cap. Values captured by running the Python
        // implementation, not derived by hand: the loop jumps straight to the cap
        // once the value reaches half the maximum, so 16 is followed by 30, not 32.
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
