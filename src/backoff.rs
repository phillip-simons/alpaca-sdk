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
    seconds_to_duration(half + jitter, max_backoff)
}

/// Converts a delay in seconds back to a [`Duration`], falling back to `ceiling`
/// rather than panicking.
///
/// `Duration::from_secs_f64` panics on a value that will not fit, and this
/// arithmetic can produce one from a perfectly legal configuration:
/// `RetryBackoff::Exponential { max: Duration::MAX }` is public, and
/// `Duration::MAX.as_secs_f64()` rounds up to just past `u64::MAX` seconds. A
/// panic here fires inside an async task, in a crate that forbids unsafe code
/// and never otherwise panics on caller input.
fn seconds_to_duration(seconds: f64, ceiling: Duration) -> Duration {
    Duration::try_from_secs_f64(seconds)
        .unwrap_or(ceiling)
        .min(ceiling)
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

    seconds_to_duration(capped.min(max), max_backoff)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `Duration::MAX` is reachable through the public
    /// `RetryBackoff::Exponential { max }`, and `as_secs_f64` rounds it to just
    /// past what `Duration::from_secs_f64` will accept — so the arithmetic used
    /// to panic inside an async task rather than return a delay.
    #[test]
    fn an_unbounded_ceiling_saturates_instead_of_panicking() {
        assert_eq!(
            capped_delay(65, Duration::from_secs(1), Duration::MAX),
            Duration::MAX
        );
        // And with jitter applied, which does its own conversion. The value is
        // drawn from [capped/2, capped], so with an unbounded ceiling it lands
        // in the top half of the range rather than anywhere at all.
        let delay = reconnect_delay(65, Duration::from_secs(1), Duration::MAX);
        assert!(delay >= Duration::MAX / 2);
    }

    /// The same, at the boundary rather than far past it.
    #[test]
    fn a_ceiling_near_the_representable_limit_is_still_a_duration() {
        let huge = Duration::from_secs(u64::MAX / 2);
        assert_eq!(capped_delay(100, Duration::from_secs(1), huge), huge);
        assert!(reconnect_delay(100, Duration::from_secs(1), huge) >= huge / 2);
    }

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
