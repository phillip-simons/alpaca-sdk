//! Request types for the market data API.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::data::enums::{
    Adjustment, CorporateActionsType, DataFeed, MarketType, MostActivesBy, OptionsFeed,
};
use crate::data::timeframe::TimeFrame;

use crate::types::serde_util::comma_separated;
use crate::types::{ContractType, Sort, SupportedCurrencies};

/// One symbol or several.
///
/// Sent as one comma-separated `symbols` parameter either way, which is what
/// the market data routes expect.
///
/// ```
/// # use alpaca_sdk::data::Symbols;
/// let one: Symbols = "AAPL".into();
/// let many: Symbols = vec!["AAPL", "SPY"].into();
///
/// assert_eq!(one.to_string(), "AAPL");
/// assert_eq!(many.to_string(), "AAPL,SPY");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Symbols(Vec<String>);

impl Symbols {
    /// The symbols as a slice.
    #[must_use]
    pub fn as_slice(&self) -> &[String] {
        &self.0
    }

    /// Whether no symbols were given.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl std::fmt::Display for Symbols {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0.join(","))
    }
}

impl From<&str> for Symbols {
    fn from(symbol: &str) -> Self {
        Self(vec![symbol.to_owned()])
    }
}

impl From<String> for Symbols {
    fn from(symbol: String) -> Self {
        Self(vec![symbol])
    }
}

impl From<Vec<String>> for Symbols {
    fn from(symbols: Vec<String>) -> Self {
        Self(symbols)
    }
}

impl From<Vec<&str>> for Symbols {
    fn from(symbols: Vec<&str>) -> Self {
        Self(symbols.into_iter().map(ToOwned::to_owned).collect())
    }
}

impl<const N: usize> From<[&str; N]> for Symbols {
    fn from(symbols: [&str; N]) -> Self {
        Self(symbols.iter().map(|s| (*s).to_owned()).collect())
    }
}

/// Fields shared by every historical time series request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct TimeseriesRequest {
    /// The symbols to query, sent as one comma-separated `symbols` parameter.
    #[serde(rename = "symbols")]
    pub symbol_or_symbols: Symbols,
    /// The earliest timestamp to return.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<DateTime<Utc>>,
    /// The latest timestamp to return.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<DateTime<Utc>>,
    /// Maximum number of items across all pages.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// The currency to denominate prices in, for local currency trading.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub currency: Option<SupportedCurrencies>,
    /// Chronological ordering of the response.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort: Option<Sort>,
}

impl TimeseriesRequest {
    /// A request for `symbols` with no other filters.
    #[must_use]
    pub fn new(symbols: impl Into<Symbols>) -> Self {
        Self {
            symbol_or_symbols: symbols.into(),
            start: None,
            end: None,
            limit: None,
            currency: None,
            sort: None,
        }
    }

    /// Restricts the window to `start` onwards.
    #[must_use]
    pub fn start(mut self, start: DateTime<Utc>) -> Self {
        self.start = Some(start);
        self
    }

    /// Restricts the window to before `end`.
    #[must_use]
    pub fn end(mut self, end: DateTime<Utc>) -> Self {
        self.end = Some(end);
        self
    }

    /// Caps the total number of items returned across all pages.
    #[must_use]
    pub fn limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Sets the chronological ordering.
    #[must_use]
    pub fn sort(mut self, sort: Sort) -> Self {
        self.sort = Some(sort);
        self
    }

    /// Sets the denominating currency.
    #[must_use]
    pub fn currency(mut self, currency: SupportedCurrencies) -> Self {
        self.currency = Some(currency);
        self
    }
}

/// Historical bars for stocks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct StockBarsRequest {
    /// The shared time series filters.
    #[serde(flatten)]
    pub base: TimeseriesRequest,
    /// The bar interval.
    pub timeframe: TimeFrame,
    /// How corporate actions are reflected in prices.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adjustment: Option<Adjustment>,
    /// Which data feed to read from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feed: Option<DataFeed>,
    /// The as-of date for symbol mapping.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asof: Option<String>,
}

impl StockBarsRequest {
    /// Bars for `symbols` at `timeframe`.
    #[must_use]
    pub fn new(symbols: impl Into<Symbols>, timeframe: TimeFrame) -> Self {
        Self {
            base: TimeseriesRequest::new(symbols),
            timeframe,
            adjustment: None,
            feed: None,
            asof: None,
        }
    }

    /// Sets the corporate action adjustment.
    #[must_use]
    pub fn adjustment(mut self, adjustment: Adjustment) -> Self {
        self.adjustment = Some(adjustment);
        self
    }

    /// Sets the data feed.
    #[must_use]
    pub fn feed(mut self, feed: DataFeed) -> Self {
        self.feed = Some(feed);
        self
    }
}

/// Historical bars for crypto.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct CryptoBarsRequest {
    /// The shared time series filters.
    #[serde(flatten)]
    pub base: TimeseriesRequest,
    /// The bar interval.
    pub timeframe: TimeFrame,
}

impl CryptoBarsRequest {
    /// Bars for `symbols` at `timeframe`.
    pub fn new(symbols: impl Into<Symbols>, timeframe: TimeFrame) -> Self {
        Self {
            base: TimeseriesRequest::new(symbols),
            timeframe,
        }
    }
}

/// Historical bars for options.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct OptionBarsRequest {
    /// The shared time series filters.
    #[serde(flatten)]
    pub base: TimeseriesRequest,
    /// The bar interval.
    pub timeframe: TimeFrame,
}

impl OptionBarsRequest {
    /// Bars for `symbols` at `timeframe`.
    pub fn new(symbols: impl Into<Symbols>, timeframe: TimeFrame) -> Self {
        Self {
            base: TimeseriesRequest::new(symbols),
            timeframe,
        }
    }
}

/// Historical quotes or trades for stocks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct StockTimeseriesRequest {
    /// The shared time series filters.
    #[serde(flatten)]
    pub base: TimeseriesRequest,
    /// Which data feed to read from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feed: Option<DataFeed>,
    /// The as-of date for symbol mapping.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asof: Option<String>,
}

impl StockTimeseriesRequest {
    /// A request for `symbols`.
    pub fn new(symbols: impl Into<Symbols>) -> Self {
        Self {
            base: TimeseriesRequest::new(symbols),
            feed: None,
            asof: None,
        }
    }

    /// Sets the data feed.
    #[must_use]
    pub fn feed(mut self, feed: DataFeed) -> Self {
        self.feed = Some(feed);
        self
    }
}

/// Generates delegating setters for requests that wrap [`TimeseriesRequest`],
/// so callers write `.limit(50)` rather than reaching into `.base`.
macro_rules! timeseries_delegates {
    ($ty:ty) => {
        impl $ty {
            /// Restricts the window to `start` onwards.
            #[must_use]
            pub fn start(mut self, start: DateTime<Utc>) -> Self {
                self.base = self.base.start(start);
                self
            }

            /// Restricts the window to before `end`.
            #[must_use]
            pub fn end(mut self, end: DateTime<Utc>) -> Self {
                self.base = self.base.end(end);
                self
            }

            /// Caps the total number of items returned across all pages.
            #[must_use]
            pub fn limit(mut self, limit: u32) -> Self {
                self.base = self.base.limit(limit);
                self
            }

            /// Sets the chronological ordering.
            #[must_use]
            pub fn sort(mut self, sort: Sort) -> Self {
                self.base = self.base.sort(sort);
                self
            }

            /// Sets the denominating currency.
            #[must_use]
            pub fn currency(mut self, currency: SupportedCurrencies) -> Self {
                self.base = self.base.currency(currency);
                self
            }
        }
    };
}

timeseries_delegates!(StockBarsRequest);
timeseries_delegates!(CryptoBarsRequest);
timeseries_delegates!(OptionBarsRequest);
timeseries_delegates!(StockTimeseriesRequest);
timeseries_delegates!(StockAuctionsRequest);

/// A request for the most recent stock data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct StockLatestRequest {
    /// The symbols to query.
    #[serde(rename = "symbols")]
    pub symbol_or_symbols: Symbols,
    /// Which data feed to read from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feed: Option<DataFeed>,
    /// The currency to denominate prices in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub currency: Option<SupportedCurrencies>,
}

impl StockLatestRequest {
    /// The latest data for `symbols`.
    #[must_use]
    pub fn new(symbols: impl Into<Symbols>) -> Self {
        Self {
            symbol_or_symbols: symbols.into(),
            feed: None,
            currency: None,
        }
    }

    /// Sets the data feed.
    #[must_use]
    pub fn feed(mut self, feed: DataFeed) -> Self {
        self.feed = Some(feed);
        self
    }
}

/// A request for the most recent crypto data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct CryptoLatestRequest {
    /// The symbols to query.
    #[serde(rename = "symbols")]
    pub symbol_or_symbols: Symbols,
}

impl CryptoLatestRequest {
    /// The latest data for `symbols`.
    pub fn new(symbols: impl Into<Symbols>) -> Self {
        Self {
            symbol_or_symbols: symbols.into(),
        }
    }
}

/// A request for the most recent option data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct OptionLatestRequest {
    /// The symbols to query.
    #[serde(rename = "symbols")]
    pub symbol_or_symbols: Symbols,
    /// Which data feed to read from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feed: Option<OptionsFeed>,
}

impl OptionLatestRequest {
    /// The latest data for `symbols`.
    pub fn new(symbols: impl Into<Symbols>) -> Self {
        Self {
            symbol_or_symbols: symbols.into(),
            feed: None,
        }
    }

    /// Sets the data feed.
    #[must_use]
    pub fn feed(mut self, feed: OptionsFeed) -> Self {
        self.feed = Some(feed);
        self
    }
}

/// A request for stock snapshots.
pub type StockSnapshotRequest = StockLatestRequest;

/// A request for crypto snapshots.
pub type CryptoSnapshotRequest = CryptoLatestRequest;

/// A request for option snapshots.
pub type OptionSnapshotRequest = OptionLatestRequest;

/// A request for every contract in an underlying's option chain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct OptionChainRequest {
    /// The underlying symbol.
    ///
    /// Goes in the path rather than the query string, so it is skipped when
    /// serializing.
    #[serde(skip)]
    pub underlying_symbol: String,
    /// Which data feed to read from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feed: Option<OptionsFeed>,
    /// Only calls or only puts.
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub contract_type: Option<ContractType>,
    /// Only contracts struck at or above this price.
    ///
    /// `Decimal`, matching `trading::OptionContract::strike_price` and the identical
    /// filter on the trading API's own option-contracts request. It was the one
    /// `f64` money field in this crate's request surface, so paging from a
    /// contract into this filter meant a `to_f64` round trip through the exact
    /// type the crate exists to avoid.
    ///
    #[serde(
        default,
        with = "crate::types::option_decimal",
        skip_serializing_if = "Option::is_none"
    )]
    pub strike_price_gte: Option<Decimal>,
    /// Only contracts struck at or below this price.
    #[serde(
        default,
        with = "crate::types::option_decimal",
        skip_serializing_if = "Option::is_none"
    )]
    pub strike_price_lte: Option<Decimal>,
    /// Only contracts expiring on this date.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expiration_date: Option<NaiveDate>,
    /// Only contracts expiring on or after this date.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expiration_date_gte: Option<NaiveDate>,
    /// Only contracts expiring on or before this date.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expiration_date_lte: Option<NaiveDate>,
    /// Only contracts with this root symbol.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_symbol: Option<String>,
    /// Only contracts updated since this time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_since: Option<DateTime<Utc>>,
}

impl OptionChainRequest {
    /// The chain for `underlying_symbol`.
    pub fn new(underlying_symbol: impl Into<String>) -> Self {
        Self {
            underlying_symbol: underlying_symbol.into(),
            feed: None,
            contract_type: None,
            strike_price_gte: None,
            strike_price_lte: None,
            expiration_date: None,
            expiration_date_gte: None,
            expiration_date_lte: None,
            root_symbol: None,
            updated_since: None,
        }
    }

    /// Restricts to calls or puts.
    #[must_use]
    pub fn contract_type(mut self, contract_type: ContractType) -> Self {
        self.contract_type = Some(contract_type);
        self
    }

    /// Sets the data feed.
    #[must_use]
    pub fn feed(mut self, feed: OptionsFeed) -> Self {
        self.feed = Some(feed);
        self
    }
}

/// Historical auctions for stocks.
///
/// Only the `sip` feed serves auctions; the reference says so in as many words,
/// and the field is left open rather than fixed so a future feed does not need a
/// crate release.
///
/// See <https://docs.alpaca.markets/us/reference/stockauctions-1>.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct StockAuctionsRequest {
    /// The shared time series filters.
    #[serde(flatten)]
    pub base: TimeseriesRequest,
    /// Which data feed to read from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feed: Option<DataFeed>,
    /// The as-of date for symbol mapping.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asof: Option<String>,
}

impl StockAuctionsRequest {
    /// Auctions for `symbols`.
    pub fn new(symbols: impl Into<Symbols>) -> Self {
        Self {
            base: TimeseriesRequest::new(symbols),
            feed: None,
            asof: None,
        }
    }

    /// Sets the data feed.
    #[must_use]
    pub fn feed(mut self, feed: DataFeed) -> Self {
        self.feed = Some(feed);
        self
    }
}

/// A request against one of the single-symbol market data routes.
///
/// The symbol goes in the path, so the `symbols` parameter its multi-symbol
/// sibling sends is absent here rather than empty.
///
/// See <https://docs.alpaca.markets/us/reference/stockbarsingle-1>.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct SingleSymbolRequest {
    /// The earliest timestamp to return.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<DateTime<Utc>>,
    /// The latest timestamp to return.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<DateTime<Utc>>,
    /// Maximum number of items across all pages.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// The bar interval. Only the bars routes take one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeframe: Option<TimeFrame>,
    /// How corporate actions are reflected in prices. Bars only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adjustment: Option<Adjustment>,
    /// Which data feed to read from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feed: Option<DataFeed>,
    /// The as-of date for symbol mapping.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asof: Option<String>,
    /// The currency to denominate prices in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub currency: Option<SupportedCurrencies>,
    /// Chronological ordering of the response.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort: Option<Sort>,
}

impl SingleSymbolRequest {
    /// A request with no filters.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Restricts the window to `start` onwards.
    #[must_use]
    pub fn start(mut self, start: DateTime<Utc>) -> Self {
        self.start = Some(start);
        self
    }

    /// Restricts the window to before `end`.
    #[must_use]
    pub fn end(mut self, end: DateTime<Utc>) -> Self {
        self.end = Some(end);
        self
    }

    /// Caps the total number of items returned across all pages.
    #[must_use]
    pub fn limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Sets the bar interval.
    #[must_use]
    pub fn timeframe(mut self, timeframe: TimeFrame) -> Self {
        self.timeframe = Some(timeframe);
        self
    }

    /// Sets the corporate action adjustment.
    #[must_use]
    pub fn adjustment(mut self, adjustment: Adjustment) -> Self {
        self.adjustment = Some(adjustment);
        self
    }

    /// Sets the data feed.
    #[must_use]
    pub fn feed(mut self, feed: DataFeed) -> Self {
        self.feed = Some(feed);
        self
    }

    /// Sets the chronological ordering.
    #[must_use]
    pub fn sort(mut self, sort: Sort) -> Self {
        self.sort = Some(sort);
        self
    }

    /// Sets the denominating currency.
    #[must_use]
    pub fn currency(mut self, currency: SupportedCurrencies) -> Self {
        self.currency = Some(currency);
        self
    }
}

/// Historical forex rates for currency pairs.
///
/// See <https://docs.alpaca.markets/us/reference/rates-1>.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ForexRatesRequest {
    /// The pairs to query, sent as one comma-separated parameter.
    ///
    /// Pairs are six-letter concatenations such as `USDJPY`, not the slashed
    /// form the crypto routes use.
    #[serde(rename = "currency_pairs")]
    pub currency_pairs: Symbols,
    /// The snapshot frequency.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeframe: Option<TimeFrame>,
    /// The earliest timestamp to return.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<DateTime<Utc>>,
    /// The latest timestamp to return.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<DateTime<Utc>>,
    /// Maximum number of rates across all pages.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// Chronological ordering of the response.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort: Option<Sort>,
}

impl ForexRatesRequest {
    /// Rates for `currency_pairs`.
    pub fn new(currency_pairs: impl Into<Symbols>) -> Self {
        Self {
            currency_pairs: currency_pairs.into(),
            timeframe: None,
            start: None,
            end: None,
            limit: None,
            sort: None,
        }
    }

    /// Sets the snapshot frequency.
    #[must_use]
    pub fn timeframe(mut self, timeframe: TimeFrame) -> Self {
        self.timeframe = Some(timeframe);
        self
    }

    /// Restricts the window.
    #[must_use]
    pub fn between(mut self, start: DateTime<Utc>, end: DateTime<Utc>) -> Self {
        self.start = Some(start);
        self.end = Some(end);
        self
    }

    /// Caps the total number of rates returned.
    #[must_use]
    pub fn limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }
}

/// The latest forex rates for currency pairs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ForexLatestRatesRequest {
    /// The pairs to query.
    #[serde(rename = "currency_pairs")]
    pub currency_pairs: Symbols,
}

impl ForexLatestRatesRequest {
    /// The latest rates for `currency_pairs`.
    pub fn new(currency_pairs: impl Into<Symbols>) -> Self {
        Self {
            currency_pairs: currency_pairs.into(),
        }
    }
}

/// A request for the most actively traded stocks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct MostActivesRequest {
    /// How many to return.
    pub top: u32,
    /// What to rank by.
    pub by: MostActivesBy,
}

impl MostActivesRequest {
    /// The top `top` symbols ranked by `by`.
    #[must_use]
    pub fn new(top: u32, by: MostActivesBy) -> Self {
        Self { top, by }
    }
}

impl Default for MostActivesRequest {
    fn default() -> Self {
        // The route's own default is the top 10 by volume.
        Self {
            top: 10,
            by: MostActivesBy::Volume,
        }
    }
}

fn default_market_type() -> MarketType {
    MarketType::Stocks
}

/// A request for the day's biggest movers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct MarketMoversRequest {
    /// How many of each direction to return.
    pub top: u32,
    /// Which market to rank.
    ///
    /// Goes in the path rather than the query string.
    #[serde(skip_serializing, default = "default_market_type")]
    pub market_type: MarketType,
}

impl MarketMoversRequest {
    /// The top `top` movers in `market_type`.
    #[must_use]
    pub fn new(top: u32, market_type: MarketType) -> Self {
        Self { top, market_type }
    }
}

impl Default for MarketMoversRequest {
    fn default() -> Self {
        Self {
            top: 10,
            market_type: MarketType::Stocks,
        }
    }
}

/// A request for news articles.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct NewsRequest {
    /// The earliest article to return.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<DateTime<Utc>>,
    /// The latest article to return.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<DateTime<Utc>>,
    /// Chronological ordering of the response.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort: Option<Sort>,
    /// Only articles mentioning these symbols.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "comma_separated"
    )]
    pub symbols: Option<Vec<String>>,
    /// Maximum number of articles across all pages.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// Whether to include the article body.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_content: Option<bool>,
    /// Whether to drop articles with no body.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exclude_contentless: Option<bool>,
    /// Token for resuming a manual pagination.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_token: Option<String>,
}

impl NewsRequest {
    /// A request with no filters.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Only articles mentioning these symbols.
    #[must_use]
    pub fn symbols(mut self, symbols: Vec<String>) -> Self {
        self.symbols = Some(symbols);
        self
    }

    /// Caps the total number of articles returned.
    #[must_use]
    pub fn limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }
}

/// A request for corporate actions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct CorporateActionsRequest {
    /// Only actions for these symbols.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "comma_separated"
    )]
    pub symbols: Option<Vec<String>>,
    /// Only actions for these CUSIPs.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "comma_separated"
    )]
    pub cusips: Option<Vec<String>>,
    /// Only actions of these kinds.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "comma_separated"
    )]
    pub types: Option<Vec<CorporateActionsType>>,
    /// The earliest date to return.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<NaiveDate>,
    /// The latest date to return.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<NaiveDate>,
    /// Only actions with these ids.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "comma_separated"
    )]
    pub ids: Option<Vec<String>>,
    /// Maximum number of actions across all pages.
    ///
    /// `None` walks every page. This is a cap on the **total** across all
    /// pages, not a page size, so setting it to the endpoint's own page size
    /// stops the walk after page one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// Chronological ordering of the response.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort: Option<Sort>,
}

impl CorporateActionsRequest {
    /// A request that walks every page, ascending.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Only actions for these symbols.
    #[must_use]
    pub fn symbols(mut self, symbols: Vec<String>) -> Self {
        self.symbols = Some(symbols);
        self
    }

    /// Only actions of these kinds.
    #[must_use]
    pub fn types(mut self, types: Vec<CorporateActionsType>) -> Self {
        self.types = Some(types);
        self
    }

    /// Restricts the date window.
    #[must_use]
    pub fn between(mut self, start: NaiveDate, end: NaiveDate) -> Self {
        self.start = Some(start);
        self.end = Some(end);
        self
    }
}

impl Default for CorporateActionsRequest {
    fn default() -> Self {
        Self {
            symbols: None,
            cusips: None,
            types: None,
            start: None,
            end: None,
            ids: None,
            // `None`, like every other request type in this module. It was
            // 1,000 — the route's own page size — and because `limit` caps the
            // total across all pages rather than the page size, page one filled
            // the cap exactly and the walk stopped there. A year of market-wide
            // dividends came back silently truncated to the first page, with
            // the `next_page_token` discarded and unrecoverable.
            limit: None,
            sort: Some(Sort::Asc),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::timeframe::TimeFrameUnit;

    #[test]
    fn symbols_render_as_one_comma_separated_parameter() {
        let request = TimeseriesRequest::new(["AAPL", "SPY"]);
        let json = serde_json::to_value(&request).unwrap();

        // symbol_or_symbols is renamed to `symbols` and comma-joined, and the
        // unset fields are absent from the query rather than sent empty.
        assert_eq!(json["symbols"], serde_json::json!(["AAPL", "SPY"]));
        assert_eq!(request.symbol_or_symbols.to_string(), "AAPL,SPY");
    }

    #[test]
    fn a_single_symbol_is_accepted_without_a_vec() {
        let request = TimeseriesRequest::new("AAPL");
        assert_eq!(request.symbol_or_symbols.as_slice(), ["AAPL"]);
    }

    #[test]
    fn unset_filters_are_omitted() {
        let json = serde_json::to_value(TimeseriesRequest::new("AAPL")).unwrap();
        let object = json.as_object().unwrap();

        assert_eq!(object.len(), 1, "{object:?}");
        assert!(object.contains_key("symbols"));
    }

    #[test]
    fn bars_flatten_the_shared_filters_alongside_the_timeframe() {
        let request =
            StockBarsRequest::new("AAPL", TimeFrame::new(5, TimeFrameUnit::Minute).unwrap())
                .feed(DataFeed::Iex);
        let json = serde_json::to_value(&request).unwrap();

        assert_eq!(json["symbols"], serde_json::json!(["AAPL"]));
        assert_eq!(json["timeframe"], "5Min");
        assert_eq!(json["feed"], "iex");
    }

    #[test]
    fn option_chain_keeps_the_underlying_out_of_the_query() {
        // It goes in the path instead.
        let request = OptionChainRequest::new("AAPL").contract_type(ContractType::Call);
        let json = serde_json::to_value(&request).unwrap();

        assert!(json.get("underlying_symbol").is_none());
        assert_eq!(json["type"], "call");
    }

    #[test]
    fn market_movers_keeps_the_market_type_out_of_the_query() {
        let request = MarketMoversRequest::new(5, MarketType::Crypto);
        let json = serde_json::to_value(&request).unwrap();

        assert!(json.get("market_type").is_none());
        assert_eq!(json["top"], 5);
    }

    /// `limit` caps the *total* across every page, and the corporate-actions
    /// page size is itself 1,000 — so a default of `Some(1000)` filled the cap
    /// with page one and ended the walk there, discarding a `next_page_token`
    /// the request type has no field to send back. Every other request in this
    /// module defaults to `None`, and so does this one now.
    #[test]
    fn corporate_actions_default_to_walking_every_page_ascending() {
        let request = CorporateActionsRequest::new();
        assert_eq!(request.limit, None);
        assert_eq!(request.sort, Some(Sort::Asc));
    }

    #[test]
    fn most_actives_defaults_to_ten_by_volume() {
        let request = MostActivesRequest::default();
        assert_eq!(request.top, 10);
        assert_eq!(request.by, MostActivesBy::Volume);
    }
}
