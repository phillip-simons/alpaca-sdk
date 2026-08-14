//! The four concrete market data streams.
//!
//! Each wraps a [`DataStream`] with the endpoint its asset class uses and only
//! the channels that class actually carries.

use std::time::Duration;

use futures_util::Stream;

use crate::auth::Credentials;
use crate::config::BaseUrl;
use crate::data::enums::{CryptoFeed, DataFeed, OptionsFeed};
use crate::data::live::{Channel, DataStream, StreamConfig, StreamMessage, SubscriptionSet};
use crate::error::{Error, Result};

/// The feed's wire value, refusing one this crate does not recognise.
///
/// Every `wire_enum!` carries an `Unknown(String)` so an unrecognised value
/// decodes rather than fails. That is right for a response and wrong for a URL:
/// the string ends up as a path segment, where `..` addresses a different
/// endpoint than the caller named. A feed the crate does not know has no live
/// stream behind it either way, so refusing loses nothing.
fn known_feed<T>(feed: &T) -> Result<&str>
where
    T: std::fmt::Display,
    T: AsUnknown,
{
    if feed.is_unknown() {
        return Err(Error::InvalidRequest(format!(
            "unknown feed {feed}: this crate has no live stream endpoint for it"
        )));
    }
    Ok(feed.wire())
}

/// The two `wire_enum!` accessors [`known_feed`] needs, so it can be written
/// once rather than per feed type.
trait AsUnknown {
    fn is_unknown(&self) -> bool;
    fn wire(&self) -> &str;
}

macro_rules! as_unknown {
    ($($ty:ident),+ $(,)?) => {
        $(
            impl AsUnknown for $ty {
                fn is_unknown(&self) -> bool {
                    // Matched on the variant rather than delegating to the
                    // inherent `is_unknown`: `Self::is_unknown(self)` picks the
                    // inherent method only by precedence, so renaming or
                    // removing it would turn this into silent infinite
                    // recursion rather than a compile error.
                    matches!(self, $ty::Unknown(_))
                }
                fn wire(&self) -> &str {
                    self.as_str()
                }
            }
        )+
    };
}

as_unknown!(CryptoFeed, OptionsFeed);

/// Declares the subscribe and unsubscribe pair for one channel.
macro_rules! subscriptions {
    ($( $subscribe:ident / $unsubscribe:ident => $channel:expr, $what:literal ; )+) => {
        $(
            #[doc = concat!("Subscribes to ", $what, " for `symbols`.")]
            ///
            /// Pass `"*"` to receive every symbol on the feed.
            pub fn $subscribe<I, S>(&mut self, symbols: I) -> &mut Self
            where
                I: IntoIterator<Item = S>,
                S: Into<String>,
            {
                self.inner.subscribe($channel, symbols);
                self
            }

            #[doc = concat!("Stops receiving ", $what, " for `symbols`.")]
            pub fn $unsubscribe<I, S>(&mut self, symbols: I) -> &mut Self
            where
                I: IntoIterator<Item = S>,
                S: AsRef<str>,
            {
                self.inner.unsubscribe($channel, symbols);
                self
            }
        )+
    };
}

/// Shared surface: the escape hatches and the run method.
macro_rules! common {
    () => {
        /// The subscriptions registered so far.
        #[must_use]
        pub fn subscriptions(&self) -> &SubscriptionSet {
            self.inner.subscriptions()
        }

        /// Reconnect after this long without market data.
        ///
        /// Off by default: a legitimately quiet subscription would otherwise
        /// reconnect on a timer.
        ///
        /// # Errors
        /// Returns [`Error::InvalidRequest`] if the timeout is not positive.
        pub fn data_timeout(&mut self, timeout: Duration) -> Result<&mut Self> {
            self.inner.config_mut().set_data_timeout(timeout)?;
            Ok(self)
        }

        /// The reconnect backoff window.
        ///
        /// The delay starts at `min`, doubles on each consecutive failure, and
        /// is capped at `max`. See
        /// [`StreamConfig::backoff`](crate::data::StreamConfig::backoff).
        ///
        /// # Errors
        /// Returns [`Error::InvalidRequest`] if `min` is zero, or if `max` is
        /// smaller than `min`.
        pub fn backoff(&mut self, min: Duration, max: Duration) -> Result<&mut Self> {
            self.inner.config_mut().set_backoff(min, max)?;
            Ok(self)
        }

        /// How long a session must stay up before it clears the reconnect
        /// failure count. See
        /// [`StreamConfig::stable_session`](crate::data::StreamConfig::stable_session).
        ///
        /// # Errors
        /// Returns [`Error::InvalidRequest`] if the duration is zero.
        pub fn stable_session(&mut self, after: Duration) -> Result<&mut Self> {
            self.inner.config_mut().set_stable_session(after)?;
            Ok(self)
        }

        /// Connects and yields frames, reconnecting on failure.
        ///
        /// # Errors
        /// Yields an error item per failed attempt; the stream continues unless
        /// the failure was fatal.
        pub fn run(self) -> impl Stream<Item = Result<StreamMessage>> {
            self.inner.run()
        }
    };
}

/// Live market data for US equities.
///
/// ```no_run
/// # use alpaca_sdk::{Credentials, data::{DataFeed, StockDataStream, StreamMessage}};
/// # use futures_util::StreamExt as _;
/// # async fn example() -> alpaca_sdk::Result<()> {
/// let mut stream = StockDataStream::new(Credentials::from_env()?, DataFeed::Iex)?;
/// stream.subscribe_trades(["AAPL", "MSFT"]);
///
/// let mut messages = Box::pin(stream.run());
/// while let Some(message) = messages.next().await {
///     if let Ok(StreamMessage::Trade(trade)) = message {
///         println!("{} {}", trade.symbol, trade.price);
///     }
/// }
/// # Ok(())
/// # }
/// ```
pub struct StockDataStream {
    inner: DataStream,
}

impl StockDataStream {
    /// A stream against the `iex` or `sip` feed.
    ///
    /// # Errors
    /// Returns [`Error::InvalidRequest`] for any other feed; only these two
    /// carry a live stock stream, and a wrong one fails at the handshake rather
    /// than at construction.
    pub fn new(credentials: Credentials, feed: DataFeed) -> Result<Self> {
        if !matches!(feed, DataFeed::Iex | DataFeed::Sip) {
            return Err(Error::InvalidRequest(format!(
                "only the iex and sip feeds have a live stock stream, got {feed}"
            )));
        }
        let endpoint = format!("{}/v2/{feed}", BaseUrl::MarketDataStream);
        Ok(Self {
            inner: DataStream::new(credentials, StreamConfig::new(endpoint)),
        })
    }

    /// A stream against a custom endpoint, for proxies and tests.
    #[must_use]
    pub fn with_endpoint(credentials: Credentials, endpoint: impl Into<String>) -> Self {
        Self {
            inner: DataStream::new(credentials, StreamConfig::new(endpoint)),
        }
    }

    subscriptions! {
        subscribe_trades / unsubscribe_trades => Channel::Trades, "trades";
        subscribe_quotes / unsubscribe_quotes => Channel::Quotes, "quotes";
        subscribe_bars / unsubscribe_bars => Channel::Bars, "minute bars";
        subscribe_updated_bars / unsubscribe_updated_bars => Channel::UpdatedBars, "updated bars";
        subscribe_daily_bars / unsubscribe_daily_bars => Channel::DailyBars, "daily bars";
        subscribe_trading_statuses / unsubscribe_trading_statuses => Channel::Statuses, "trading statuses";
        subscribe_lulds / unsubscribe_lulds => Channel::Lulds, "limit up / limit down bands";
    }

    /// Receives corrections to previously reported trades.
    ///
    /// Not a subscription: corrections arrive with the trades subscription and
    /// are rejected if named in a subscribe payload.
    pub fn register_trade_corrections<I, S>(&mut self, symbols: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.inner.subscribe(Channel::Corrections, symbols);
        self
    }

    /// Receives cancellations of previously reported trades.
    ///
    /// Like corrections, these ride along with the trades subscription.
    pub fn register_trade_cancels<I, S>(&mut self, symbols: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.inner.subscribe(Channel::CancelErrors, symbols);
        self
    }

    common!();
}

/// Live market data for crypto.
pub struct CryptoDataStream {
    inner: DataStream,
}

impl CryptoDataStream {
    /// A stream against `feed`.
    ///
    /// # Errors
    /// Returns [`Error::InvalidRequest`] for a feed this crate does not know.
    /// `CryptoFeed` carries an `Unknown(String)` variant so an unrecognised
    /// value decodes rather than fails, and that string would otherwise be
    /// interpolated straight into the endpoint URL.
    pub fn new(credentials: Credentials, feed: CryptoFeed) -> Result<Self> {
        let endpoint = format!(
            "{}/v1beta3/crypto/{}",
            BaseUrl::MarketDataStream,
            known_feed(&feed)?
        );
        Ok(Self {
            inner: DataStream::new(credentials, StreamConfig::new(endpoint)),
        })
    }

    /// A stream against a custom endpoint, for proxies and tests.
    #[must_use]
    pub fn with_endpoint(credentials: Credentials, endpoint: impl Into<String>) -> Self {
        Self {
            inner: DataStream::new(credentials, StreamConfig::new(endpoint)),
        }
    }

    subscriptions! {
        subscribe_trades / unsubscribe_trades => Channel::Trades, "trades";
        subscribe_quotes / unsubscribe_quotes => Channel::Quotes, "quotes";
        subscribe_bars / unsubscribe_bars => Channel::Bars, "minute bars";
        subscribe_updated_bars / unsubscribe_updated_bars => Channel::UpdatedBars, "updated bars";
        subscribe_daily_bars / unsubscribe_daily_bars => Channel::DailyBars, "daily bars";
        subscribe_orderbooks / unsubscribe_orderbooks => Channel::Orderbooks, "orderbook updates";
    }

    common!();
}

/// Live market data for options.
pub struct OptionDataStream {
    inner: DataStream,
}

impl OptionDataStream {
    /// A stream against `feed`.
    ///
    /// # Errors
    /// Returns [`Error::InvalidRequest`] for a feed this crate does not know,
    /// for the same reason as [`CryptoDataStream::new`].
    pub fn new(credentials: Credentials, feed: OptionsFeed) -> Result<Self> {
        let endpoint = format!(
            "{}/v1beta1/{}",
            BaseUrl::MarketDataStream,
            known_feed(&feed)?
        );
        Ok(Self {
            inner: DataStream::new(credentials, StreamConfig::new(endpoint)),
        })
    }

    /// A stream against a custom endpoint, for proxies and tests.
    #[must_use]
    pub fn with_endpoint(credentials: Credentials, endpoint: impl Into<String>) -> Self {
        Self {
            inner: DataStream::new(credentials, StreamConfig::new(endpoint)),
        }
    }

    subscriptions! {
        subscribe_trades / unsubscribe_trades => Channel::Trades, "trades";
        subscribe_quotes / unsubscribe_quotes => Channel::Quotes, "quotes";
    }

    common!();
}

/// Live news articles.
pub struct NewsDataStream {
    inner: DataStream,
}

impl NewsDataStream {
    /// A stream against the news endpoint.
    #[must_use]
    pub fn new(credentials: Credentials) -> Self {
        let endpoint = format!("{}/v1beta1/news", BaseUrl::MarketDataStream);
        Self {
            inner: DataStream::new(credentials, StreamConfig::new(endpoint)),
        }
    }

    /// A stream against a custom endpoint, for proxies and tests.
    #[must_use]
    pub fn with_endpoint(credentials: Credentials, endpoint: impl Into<String>) -> Self {
        Self {
            inner: DataStream::new(credentials, StreamConfig::new(endpoint)),
        }
    }

    subscriptions! {
        subscribe_news / unsubscribe_news => Channel::News, "news articles";
    }

    common!();
}
