//! Endpoints, retry policy, and shared constants.
//!
//! Ported from `alpaca/common/constants.py`.

use std::time::Duration;

/// Maximum number of items the market data API returns per page.
pub const DATA_MAX_LIMIT: u32 = 10_000;

/// Default page size for the broker account-activities endpoint.
pub const ACCOUNT_ACTIVITIES_DEFAULT_PAGE_SIZE: u32 = 100;

/// Maximum number of documents accepted by a single broker upload request.
pub const BROKER_DOCUMENT_UPLOAD_LIMIT: usize = 10;

/// Retries attempted after the initial request, matching `DEFAULT_RETRY_ATTEMPTS`.
pub const DEFAULT_RETRY_ATTEMPTS: u32 = 3;

/// Delay between retries, matching `DEFAULT_RETRY_WAIT_SECONDS`.
pub const DEFAULT_RETRY_WAIT: Duration = Duration::from_secs(3);

/// HTTP statuses that trigger a retry, matching `DEFAULT_RETRY_EXCEPTION_CODES`.
pub const DEFAULT_RETRY_STATUS_CODES: &[u16] = &[429, 504];

/// The `User-Agent` sent with every request.
///
/// Mirrors the `APCA-PY/<sdk> Python/<runtime>` shape alpaca-py sends, as
/// `APCA-RS/<sdk> Rust/<rustc>`; the compiler version is captured in `build.rs`.
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

/// How the client retries requests that fail with a retryable status.
///
/// The defaults are up to 3 retries after the initial request — 4 requests total
/// — with a flat 3-second wait, on HTTP 429 and 504.
///
/// **These do not follow Alpaca's own advice.** [The rate-limit
/// documentation][rate-limits] says to "stop, wait, and retry using exponential
/// backoff", doubling from about a second and adding jitter. This flat wait is
/// inherited from alpaca-py, and is kept only because changing a retry policy
/// silently changes how a caller's application behaves under load. A caller who
/// wants Alpaca's shape should build a [`RetryConfig`] for it; the crate should
/// grow one and default to it.
///
/// It is `#[non_exhaustive]`, so build one from [`RetryConfig::default`] or
/// [`RetryConfig::none`] and adjust it with the methods below rather than with a
/// struct literal. That is what lets the wait strategy above change without
/// breaking every caller.
///
/// ```
/// use std::time::Duration;
/// use alpaca_sdk::RetryConfig;
///
/// let retry = RetryConfig::default().attempts(5).wait(Duration::from_secs(1));
/// ```
///
/// [rate-limits]: https://docs.alpaca.markets/us/docs/broker-api-rate-limits
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct RetryConfig {
    /// Number of retries after the first attempt.
    pub attempts: u32,
    /// Flat delay between attempts.
    pub wait: Duration,
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
            status_codes: Vec::new(),
        }
    }

    /// Sets the number of retries attempted after the first request.
    #[must_use]
    pub fn attempts(mut self, attempts: u32) -> Self {
        self.attempts = attempts;
        self
    }

    /// Sets the delay between attempts.
    #[must_use]
    pub fn wait(mut self, wait: Duration) -> Self {
        self.wait = wait;
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
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            attempts: DEFAULT_RETRY_ATTEMPTS,
            wait: DEFAULT_RETRY_WAIT,
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
    fn endpoints_match_alpaca_py() {
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

    #[test]
    fn retry_defaults_match_alpaca_py() {
        let cfg = RetryConfig::default();
        assert_eq!(cfg.attempts, 3);
        assert_eq!(cfg.wait, Duration::from_secs(3));
        assert!(cfg.should_retry(429));
        assert!(cfg.should_retry(504));
        assert!(!cfg.should_retry(500));
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
