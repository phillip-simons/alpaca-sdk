//! Endpoints, retry policy, and shared constants.

use std::time::Duration;

/// Maximum number of items the market data API returns per page.
pub const DATA_MAX_LIMIT: u32 = 10_000;

/// Default page size for the broker account-activities endpoint.
pub const ACCOUNT_ACTIVITIES_DEFAULT_PAGE_SIZE: u32 = 100;

/// Maximum number of documents accepted by a single broker upload request.
pub const BROKER_DOCUMENT_UPLOAD_LIMIT: usize = 10;

/// Retries attempted after the initial request, matching `DEFAULT_RETRY_ATTEMPTS`.
pub const DEFAULT_RETRY_ATTEMPTS: u32 = 3;

/// Base delay before the first retry, doubling from there.
///
/// This is the ~1 second the [rate-limit page][rate-limits] asks for, and the
/// same base the stream reconnect uses ([`crate::backoff::DEFAULT_MIN_BACKOFF`]).
/// See [`RetryConfig`] for how to wait a flat interval instead.
///
/// [rate-limits]: https://docs.alpaca.markets/us/docs/broker-api-rate-limits
pub const DEFAULT_RETRY_WAIT: Duration = crate::backoff::DEFAULT_MIN_BACKOFF;

/// Ceiling the retry delay is capped at, shared with the stream reconnect.
pub const DEFAULT_RETRY_MAX_WAIT: Duration = crate::backoff::DEFAULT_MAX_BACKOFF;

/// HTTP statuses that trigger a retry, matching `DEFAULT_RETRY_EXCEPTION_CODES`.
pub const DEFAULT_RETRY_STATUS_CODES: &[u16] = &[429, 504];

/// The `User-Agent` sent with every request.
///
/// `APCA-RS/<sdk> Rust/<rustc>`, following the `APCA-<lang>` convention Alpaca's
/// clients use. The compiler version is captured in `build.rs`.
#[must_use]
pub fn user_agent() -> &'static str {
    concat!(
        "APCA-RS/",
        env!("CARGO_PKG_VERSION"),
        " Rust/",
        env!("ALPACA_RUSTC_VERSION"),
    )
}

/// The Alpaca API endpoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum BaseUrl {
    /// Broker API sandbox.
    BrokerSandbox,
    /// Broker API production.
    BrokerProduction,
    /// Trading API paper environment.
    TradingPaper,
    /// Trading API live environment.
    TradingLive,
    /// Market data API.
    Data,
    /// Market data API sandbox.
    DataSandbox,
    /// Market data websocket stream.
    MarketDataStream,
    /// Trading updates websocket stream, paper environment.
    TradingStreamPaper,
    /// Trading updates websocket stream, live environment.
    TradingStreamLive,
}

impl BaseUrl {
    /// The URL as a string, without a trailing slash.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BrokerSandbox => "https://broker-api.sandbox.alpaca.markets",
            Self::BrokerProduction => "https://broker-api.alpaca.markets",
            Self::TradingPaper => "https://paper-api.alpaca.markets",
            Self::TradingLive => "https://api.alpaca.markets",
            Self::Data => "https://data.alpaca.markets",
            Self::DataSandbox => "https://data.sandbox.alpaca.markets",
            Self::MarketDataStream => "wss://stream.data.alpaca.markets",
            Self::TradingStreamPaper => "wss://paper-api.alpaca.markets/stream",
            Self::TradingStreamLive => "wss://api.alpaca.markets/stream",
        }
    }

    /// Selects the trading endpoint for the paper or live environment.
    #[must_use]
    pub const fn trading(paper: bool) -> Self {
        if paper {
            Self::TradingPaper
        } else {
            Self::TradingLive
        }
    }

    /// Selects the trading stream endpoint for the paper or live environment.
    #[must_use]
    pub const fn trading_stream(paper: bool) -> Self {
        if paper {
            Self::TradingStreamPaper
        } else {
            Self::TradingStreamLive
        }
    }

    /// Selects the broker endpoint for the sandbox or production environment.
    #[must_use]
    pub const fn broker(sandbox: bool) -> Self {
        if sandbox {
            Self::BrokerSandbox
        } else {
            Self::BrokerProduction
        }
    }

    /// Selects the market data endpoint for the sandbox or production environment.
    #[must_use]
    pub const fn data(sandbox: bool) -> Self {
        if sandbox {
            Self::DataSandbox
        } else {
            Self::Data
        }
    }
}

impl std::fmt::Display for BaseUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<BaseUrl> for String {
    fn from(url: BaseUrl) -> Self {
        url.as_str().to_owned()
    }
}

/// How the delay between retries grows.
///
/// The default is [`Exponential`][Self::Exponential], which is what [Alpaca's
/// rate-limit page][rate-limits] asks for: "stop, wait, and retry using
/// exponential backoff". [`Flat`][Self::Flat] waits the same interval every
/// time, for callers who want a predictable one.
///
/// [rate-limits]: https://docs.alpaca.markets/us/docs/broker-api-rate-limits
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RetryBackoff {
    /// Wait [`RetryConfig::wait`] before every retry, however many have failed.
    Flat,
    /// Double [`RetryConfig::wait`] per consecutive failure, up to `max`, and
    /// draw the actual delay uniformly from the top half of that window.
    ///
    /// The jitter is not decoration. Alpaca rate-limits per account, so a
    /// process that fans out concurrent requests answers one 429 with a burst of
    /// retries that arrive together and 429 again; spreading them is what breaks
    /// the cycle. This is [`crate::backoff::reconnect_delay`], the same curve the
    /// stream reconnect uses.
    Exponential {
        /// Ceiling the doubling is capped at.
        max: Duration,
    },
}

/// How the client retries requests that fail with a retryable status.
///
/// The defaults are up to 3 retries after the initial request — 4 requests total
/// — on HTTP 429 and 504, waiting about a second before the first and doubling
/// from there, capped at 30 seconds and jittered.
///
/// **A response carrying `Retry-After` overrides all of that.** The server is
/// stating the answer this configuration is otherwise guessing at, so its value
/// is used in place of the computed delay — clamped to the backoff's own ceiling,
/// since it arrives from the other end. Only the delta-seconds form is read; an
/// HTTP-date is treated as absent and the delay below applies.
///
/// A flat wait is available for callers who want one, and has to be asked for
/// by name — it does not follow [Alpaca's own rate-limit
/// documentation][rate-limits], which is why it is not the default:
///
/// ```
/// use std::time::Duration;
/// use alpaca_sdk::{RetryBackoff, RetryConfig};
///
/// let flat = RetryConfig::default()
///     .backoff(RetryBackoff::Flat)
///     .wait(Duration::from_secs(3));
/// ```
///
/// It is `#[non_exhaustive]`, so build one from [`RetryConfig::default`] or
/// [`RetryConfig::none`] and adjust it with the builder methods rather than with a
/// struct literal. That is what let the backoff strategy arrive as a new field
/// instead of as a breaking change.
///
/// [rate-limits]: https://docs.alpaca.markets/us/docs/broker-api-rate-limits
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct RetryConfig {
    /// Number of retries after the first attempt.
    pub attempts: u32,
    /// Delay before the first retry, and the base the growth doubles from.
    pub wait: Duration,
    /// How the delay grows across consecutive failures.
    pub backoff: RetryBackoff,
    /// Statuses that trigger a retry.
    pub status_codes: Vec<u16>,
}

impl RetryConfig {
    /// A configuration that never retries.
    #[must_use]
    pub fn none() -> Self {
        Self {
            attempts: 0,
            wait: Duration::ZERO,
            backoff: RetryBackoff::Flat,
            status_codes: Vec::new(),
        }
    }

    /// Sets the number of retries attempted after the first request.
    #[must_use]
    pub fn attempts(mut self, attempts: u32) -> Self {
        self.attempts = attempts;
        self
    }

    /// Sets the delay before the first retry.
    #[must_use]
    pub fn wait(mut self, wait: Duration) -> Self {
        self.wait = wait;
        self
    }

    /// Sets how the delay grows across consecutive failures.
    #[must_use]
    pub fn backoff(mut self, backoff: RetryBackoff) -> Self {
        self.backoff = backoff;
        self
    }

    /// Sets the statuses that trigger a retry, replacing the current list.
    #[must_use]
    pub fn status_codes(mut self, status_codes: impl Into<Vec<u16>>) -> Self {
        self.status_codes = status_codes.into();
        self
    }

    /// Whether `status` should be retried under this configuration.
    #[must_use]
    pub fn should_retry(&self, status: u16) -> bool {
        self.status_codes.contains(&status)
    }

    /// The ceiling a server-supplied `Retry-After` is clamped to.
    ///
    /// A rate limiter — or a proxy in front of one — answering
    /// `Retry-After: 86400` must not put the caller to sleep for a day. Under
    /// [`RetryBackoff::Exponential`] the curve already has a ceiling and it is
    /// the natural bound; a flat policy has none of its own, so the default
    /// applies.
    pub(crate) fn retry_after_cap(&self) -> Duration {
        match self.backoff {
            RetryBackoff::Exponential { max } => max,
            RetryBackoff::Flat => DEFAULT_RETRY_MAX_WAIT,
        }
    }

    /// How long to wait after `failures` consecutive failures, 1-based.
    ///
    /// Under [`RetryBackoff::Exponential`] the result is sampled, so two calls
    /// with the same argument do not agree.
    ///
    /// This is the curve. A response carrying `Retry-After` overrides it — the
    /// server is stating the answer the curve is guessing at.
    #[must_use]
    pub fn delay(&self, failures: u32) -> Duration {
        match self.backoff {
            RetryBackoff::Flat => self.wait,
            RetryBackoff::Exponential { max } => {
                crate::backoff::reconnect_delay(failures, self.wait, max)
            }
        }
    }
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            attempts: DEFAULT_RETRY_ATTEMPTS,
            wait: DEFAULT_RETRY_WAIT,
            backoff: RetryBackoff::Exponential {
                max: DEFAULT_RETRY_MAX_WAIT,
            },
            status_codes: DEFAULT_RETRY_STATUS_CODES.to_vec(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_agent_has_the_apca_rs_shape() {
        let ua = user_agent();
        assert!(ua.starts_with("APCA-RS/"), "{ua}");
        assert!(ua.contains(" Rust/"), "{ua}");
    }

    #[test]
    fn endpoints_are_the_documented_hosts() {
        assert_eq!(
            BaseUrl::TradingPaper.as_str(),
            "https://paper-api.alpaca.markets"
        );
        assert_eq!(BaseUrl::TradingLive.as_str(), "https://api.alpaca.markets");
        assert_eq!(BaseUrl::Data.as_str(), "https://data.alpaca.markets");
        assert_eq!(
            BaseUrl::MarketDataStream.as_str(),
            "wss://stream.data.alpaca.markets"
        );
        assert_eq!(
            BaseUrl::BrokerSandbox.as_str(),
            "https://broker-api.sandbox.alpaca.markets"
        );
    }

    #[test]
    fn environment_selectors_pick_the_right_endpoint() {
        assert_eq!(BaseUrl::trading(true), BaseUrl::TradingPaper);
        assert_eq!(BaseUrl::trading(false), BaseUrl::TradingLive);
        assert_eq!(BaseUrl::trading_stream(true), BaseUrl::TradingStreamPaper);
        assert_eq!(BaseUrl::broker(true), BaseUrl::BrokerSandbox);
        assert_eq!(BaseUrl::data(false), BaseUrl::Data);
    }

    /// The exponential curve already has a ceiling, so it is the one a
    /// server-supplied `Retry-After` is held to.
    #[test]
    fn retry_after_is_capped_by_the_exponential_ceiling() {
        let cfg = RetryConfig::default().backoff(RetryBackoff::Exponential {
            max: Duration::from_secs(5),
        });
        assert_eq!(cfg.retry_after_cap(), Duration::from_secs(5));
    }

    /// A flat policy has no ceiling of its own, so the default one applies —
    /// otherwise a caller who asked for a flat 1s would clamp every
    /// `Retry-After` to 1s, which honours the header in name only.
    #[test]
    fn a_flat_policy_caps_retry_after_at_the_default_ceiling() {
        let cfg = RetryConfig::default().backoff(RetryBackoff::Flat);
        assert_eq!(cfg.retry_after_cap(), DEFAULT_RETRY_MAX_WAIT);
    }

    /// The wait is the rate-limit page's, not a flat interval — see
    /// [`RetryConfig`].
    #[test]
    fn retry_defaults_follow_the_rate_limit_page() {
        let cfg = RetryConfig::default();
        assert_eq!(cfg.attempts, 3);
        assert_eq!(cfg.wait, Duration::from_secs(1));
        assert_eq!(
            cfg.backoff,
            RetryBackoff::Exponential {
                max: Duration::from_secs(30)
            }
        );
        assert!(cfg.should_retry(429));
        assert!(cfg.should_retry(504));
        assert!(!cfg.should_retry(500));
    }

    #[test]
    fn flat_backoff_waits_the_same_amount_every_time() {
        let cfg = RetryConfig::default()
            .backoff(RetryBackoff::Flat)
            .wait(Duration::from_secs(3));

        for failures in 1..=5 {
            assert_eq!(cfg.delay(failures), Duration::from_secs(3));
        }
    }

    /// Bounds rather than values, because the delay is jittered. The lower bound
    /// is what proves the growth: by the fourth failure even the unluckiest draw
    /// exceeds the flat 3 seconds this used to wait.
    #[test]
    fn exponential_backoff_grows_and_stays_under_the_cap() {
        let cfg = RetryConfig::default();

        for failures in 1..=10 {
            let capped = crate::backoff::capped_delay(failures, cfg.wait, Duration::from_secs(30));
            let delay = cfg.delay(failures);
            assert!(
                delay >= capped / 2 && delay <= capped,
                "failures={failures}: {delay:?} outside [{:?}, {capped:?}]",
                capped / 2
            );
            assert!(delay <= Duration::from_secs(30));
        }

        assert!(cfg.delay(4) > Duration::from_secs(3));
    }

    /// The zero-wait configuration the transport tests use must not start
    /// sleeping just because the default strategy changed.
    #[test]
    fn a_zero_wait_stays_instant_under_either_strategy() {
        assert_eq!(RetryConfig::none().delay(1), Duration::ZERO);
        assert_eq!(
            RetryConfig::default().wait(Duration::ZERO).delay(7),
            Duration::ZERO
        );
    }

    /// The struct is `#[non_exhaustive]`, so these methods are the only way a
    /// caller outside the crate can change one field and keep the others.
    #[test]
    fn retry_builders_replace_one_field_at_a_time() {
        let cfg = RetryConfig::default()
            .attempts(5)
            .wait(Duration::ZERO)
            .status_codes([500, 502]);

        assert_eq!(cfg.attempts, 5);
        assert_eq!(cfg.wait, Duration::ZERO);
        assert!(cfg.should_retry(500));
        assert!(!cfg.should_retry(429));

        let none = RetryConfig::none();
        assert_eq!(none.attempts, 0);
        assert!(!none.should_retry(429));
    }
}
