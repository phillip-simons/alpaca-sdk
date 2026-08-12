//! Market data models, ported from `alpaca/data/models/`.
//!
//! The wire format uses single-letter keys — `t`, `o`, `h`, `l`, `c`, `v` — which
//! alpaca-py translates through the lookup tables in `alpaca/data/mappings.py`.
//! Here they are `#[serde(rename)]` attributes, so the whole mapping layer
//! disappears.
//!
//! Prices and sizes stay `f64` rather than becoming [`rust_decimal::Decimal`]:
//! market data arrives as JSON numbers and is already approximate on the wire,
//! unlike the order and account money fields, which arrive as strings.
//!
//! # Symbols
//!
//! Responses key data by symbol at the level above the record, so the records
//! themselves carry no symbol. alpaca-py passes it into each model's
//! constructor; here the collection types fill it in after deserializing. A
//! record deserialized on its own therefore has an empty `symbol`.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::data::enums::NewsImageSize;
use crate::types::serde_util::string_or_list;

/// One bar of aggregated trade data over an interval.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bar {
    /// The symbol this bar is for, filled in from the response key.
    #[serde(default, skip_deserializing)]
    pub symbol: String,
    /// The opening timestamp of the interval.
    #[serde(rename = "t", with = "crate::types::timestamp")]
    pub timestamp: DateTime<Utc>,
    /// The opening price.
    #[serde(rename = "o")]
    pub open: f64,
    /// The highest price during the interval.
    #[serde(rename = "h")]
    pub high: f64,
    /// The lowest price during the interval.
    #[serde(rename = "l")]
    pub low: f64,
    /// The closing price.
    #[serde(rename = "c")]
    pub close: f64,
    /// Volume traded over the interval.
    #[serde(rename = "v")]
    pub volume: f64,
    /// Number of trades that occurred.
    #[serde(rename = "n", default)]
    pub trade_count: Option<f64>,
    /// Volume weighted average price.
    #[serde(rename = "vw", default)]
    pub vwap: Option<f64>,
}

/// One quote: the best bid and ask at a point in time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Quote {
    /// The symbol this quote is for, filled in from the response key.
    #[serde(default, skip_deserializing)]
    pub symbol: String,
    /// When the quote was generated.
    #[serde(rename = "t", with = "crate::types::timestamp")]
    pub timestamp: DateTime<Utc>,
    /// The highest buy offer.
    #[serde(rename = "bp")]
    pub bid_price: f64,
    /// Size of the bid, in round lots.
    #[serde(rename = "bs")]
    pub bid_size: f64,
    /// Exchange the bid is on.
    #[serde(rename = "bx", default)]
    pub bid_exchange: Option<String>,
    /// The lowest sell offer.
    #[serde(rename = "ap")]
    pub ask_price: f64,
    /// Size of the ask, in round lots.
    #[serde(rename = "as")]
    pub ask_size: f64,
    /// Exchange the ask is on.
    #[serde(rename = "ax", default)]
    pub ask_exchange: Option<String>,
    /// Condition codes.
    ///
    /// Stocks send a list and crypto sends a bare string; both normalize here.
    #[serde(rename = "c", default, deserialize_with = "string_or_list")]
    pub conditions: Option<Vec<String>>,
    /// The tape the quote came from.
    #[serde(rename = "z", default)]
    pub tape: Option<String>,
}

/// One executed trade.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Trade {
    /// The symbol this trade is for, filled in from the response key.
    #[serde(default, skip_deserializing)]
    pub symbol: String,
    /// When the trade executed.
    #[serde(rename = "t", with = "crate::types::timestamp")]
    pub timestamp: DateTime<Utc>,
    /// Exchange the trade executed on.
    #[serde(rename = "x", default)]
    pub exchange: Option<String>,
    /// Price per share.
    #[serde(rename = "p")]
    pub price: f64,
    /// Number of shares.
    #[serde(rename = "s")]
    pub size: f64,
    /// The trade's identifier, unique per exchange and day.
    #[serde(rename = "i", default)]
    pub id: Option<i64>,
    /// Condition codes.
    #[serde(rename = "c", default, deserialize_with = "string_or_list")]
    pub conditions: Option<Vec<String>>,
    /// The tape the trade came from.
    #[serde(rename = "z", default)]
    pub tape: Option<String>,
    /// Which side took the liquidity, for crypto.
    #[serde(rename = "tks", default)]
    pub taker_side: Option<String>,
}

/// A trading status update, such as a halt or resumption.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TradingStatus {
    /// The symbol this status is for, filled in from the response key.
    #[serde(default, skip_deserializing)]
    pub symbol: String,
    /// When the status changed.
    #[serde(rename = "t", with = "crate::types::timestamp")]
    pub timestamp: DateTime<Utc>,
    /// The status code.
    #[serde(rename = "sc")]
    pub status_code: String,
    /// A description of the status.
    #[serde(rename = "sm")]
    pub status_message: String,
    /// The reason code.
    #[serde(rename = "rc")]
    pub reason_code: String,
    /// A description of the reason.
    #[serde(rename = "rm")]
    pub reason_message: String,
    /// The tape the status came from.
    #[serde(rename = "z")]
    pub tape: String,
}

/// A cancellation of a previously reported trade.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TradeCancel {
    /// The symbol this cancellation is for, filled in from the response key.
    #[serde(default, skip_deserializing)]
    pub symbol: String,
    /// When the cancellation was issued.
    #[serde(rename = "t", with = "crate::types::timestamp")]
    pub timestamp: DateTime<Utc>,
    /// Exchange the cancelled trade was on.
    #[serde(rename = "x")]
    pub exchange: String,
    /// Price of the cancelled trade.
    #[serde(rename = "p")]
    pub price: f64,
    /// Size of the cancelled trade.
    #[serde(rename = "s")]
    pub size: f64,
    /// Identifier of the cancelled trade.
    #[serde(rename = "i", default)]
    pub id: Option<i64>,
    /// The cancellation action taken.
    #[serde(rename = "a", default)]
    pub action: Option<String>,
    /// The tape the cancellation came from.
    #[serde(rename = "z")]
    pub tape: String,
}

/// A correction to a previously reported trade.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TradeCorrection {
    /// The symbol this correction is for, filled in from the response key.
    #[serde(default, skip_deserializing)]
    pub symbol: String,
    /// When the correction was issued.
    #[serde(rename = "t", with = "crate::types::timestamp")]
    pub timestamp: DateTime<Utc>,
    /// Exchange the corrected trade was on.
    #[serde(rename = "x")]
    pub exchange: String,
    /// Identifier of the original trade.
    #[serde(rename = "oi", default)]
    pub original_id: Option<i64>,
    /// Price as originally reported.
    #[serde(rename = "op")]
    pub original_price: f64,
    /// Size as originally reported.
    #[serde(rename = "os")]
    pub original_size: f64,
    /// Condition codes as originally reported.
    #[serde(rename = "oc", default)]
    pub original_conditions: Vec<String>,
    /// Identifier of the corrected trade.
    #[serde(rename = "ci", default)]
    pub corrected_id: Option<i64>,
    /// The corrected price.
    #[serde(rename = "cp")]
    pub corrected_price: f64,
    /// The corrected size.
    #[serde(rename = "cs")]
    pub corrected_size: f64,
    /// The corrected condition codes.
    #[serde(rename = "cc", default)]
    pub corrected_conditions: Vec<String>,
    /// The tape the correction came from.
    #[serde(rename = "z")]
    pub tape: String,
}

/// One price level in an orderbook.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderbookQuote {
    /// Price at this level.
    #[serde(rename = "p")]
    pub price: f64,
    /// Size available at this level.
    #[serde(rename = "s")]
    pub size: f64,
}

/// A snapshot of the bids and asks for a symbol.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Orderbook {
    /// The symbol this book is for, filled in from the response key.
    #[serde(default, skip_deserializing)]
    pub symbol: String,
    /// When the book was captured.
    #[serde(rename = "t", with = "crate::types::timestamp")]
    pub timestamp: DateTime<Utc>,
    /// Bid levels, best first.
    #[serde(rename = "b", default)]
    pub bids: Vec<OrderbookQuote>,
    /// Ask levels, best first.
    #[serde(rename = "a", default)]
    pub asks: Vec<OrderbookQuote>,
    /// Whether this message resets the book rather than updating it.
    #[serde(rename = "r", default)]
    pub reset: bool,
}

/// The most recent trade, quote, and bars for a symbol.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    /// The symbol this snapshot is for, filled in from the response key.
    #[serde(default, skip_deserializing)]
    pub symbol: String,
    /// The latest trade.
    #[serde(rename = "latestTrade", default)]
    pub latest_trade: Option<Trade>,
    /// The latest quote.
    #[serde(rename = "latestQuote", default)]
    pub latest_quote: Option<Quote>,
    /// The most recent minute bar.
    #[serde(rename = "minuteBar", default)]
    pub minute_bar: Option<Bar>,
    /// The most recent daily bar.
    #[serde(rename = "dailyBar", default)]
    pub daily_bar: Option<Bar>,
    /// The previous daily bar.
    #[serde(rename = "prevDailyBar", default)]
    pub previous_daily_bar: Option<Bar>,
}

/// The option greeks for a contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OptionsGreeks {
    /// Sensitivity to the underlying price.
    pub delta: f64,
    /// Rate of change of delta.
    pub gamma: f64,
    /// Sensitivity to the interest rate.
    pub rho: f64,
    /// Sensitivity to time decay.
    pub theta: f64,
    /// Sensitivity to volatility.
    pub vega: f64,
}

/// The latest trade, quote, and analytics for an option contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OptionsSnapshot {
    /// The contract symbol, filled in from the response key.
    #[serde(default, skip_deserializing)]
    pub symbol: String,
    /// The latest trade.
    #[serde(rename = "latestTrade", default)]
    pub latest_trade: Option<Trade>,
    /// The latest quote.
    #[serde(rename = "latestQuote", default)]
    pub latest_quote: Option<Quote>,
    /// The implied volatility.
    #[serde(rename = "impliedVolatility", default)]
    pub implied_volatility: Option<f64>,
    /// The greeks.
    #[serde(default)]
    pub greeks: Option<OptionsGreeks>,
}

/// One image attached to a news article.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewsImage {
    /// The rendition size.
    pub size: NewsImageSize,
    /// Where the image is hosted.
    pub url: String,
}

/// A news article.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct News {
    /// Alpaca's identifier for the article.
    pub id: i64,
    /// The headline.
    pub headline: String,
    /// Who published it.
    pub source: String,
    /// Where to read it.
    #[serde(default)]
    pub url: Option<String>,
    /// A summary of the article.
    #[serde(default)]
    pub summary: String,
    /// When the article was created.
    #[serde(with = "crate::types::timestamp")]
    pub created_at: DateTime<Utc>,
    /// When the article was last updated.
    #[serde(with = "crate::types::timestamp")]
    pub updated_at: DateTime<Utc>,
    /// Symbols the article concerns.
    ///
    /// The API always sends this, possibly empty, but the live stream can omit
    /// it — so it defaults rather than being required.
    #[serde(default)]
    pub symbols: Vec<String>,
    /// Who wrote it.
    #[serde(default)]
    pub author: String,
    /// The article body.
    #[serde(default)]
    pub content: String,
    /// Images attached to the article.
    #[serde(default)]
    pub images: Option<Vec<NewsImage>>,
}

/// A page of news articles.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewsSet {
    /// The articles.
    #[serde(default)]
    pub news: Vec<News>,
    /// Token for the next page, when the caller paginates manually.
    #[serde(default)]
    pub next_page_token: Option<String>,
}

/// One of the most actively traded stocks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActiveStock {
    /// The ticker symbol.
    pub symbol: String,
    /// Volume traded.
    pub volume: f64,
    /// Number of trades.
    pub trade_count: f64,
}

/// The most actively traded stocks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MostActives {
    /// The ranked list.
    pub most_actives: Vec<ActiveStock>,
    /// When the ranking was computed.
    pub last_updated: DateTime<Utc>,
}

/// One symbol in the movers ranking.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mover {
    /// The ticker symbol.
    pub symbol: String,
    /// Change as a percentage.
    pub percent_change: f64,
    /// Absolute change.
    pub change: f64,
    /// The current price.
    pub price: f64,
}

/// The day's biggest gainers and losers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Movers {
    /// Symbols that rose the most.
    pub gainers: Vec<Mover>,
    /// Symbols that fell the most.
    pub losers: Vec<Mover>,
    /// Which market the ranking covers.
    pub market_type: crate::data::enums::MarketType,
    /// When the ranking was computed.
    pub last_updated: DateTime<Utc>,
}

/// Multi-symbol data keyed by symbol.
///
/// alpaca-py wraps these in `BarSet`, `QuoteSet`, and `TradeSet`, which support
/// `set[symbol]` lookup and a `.df` property. In Rust the map is the natural
/// shape; the `polars` feature adds the frame conversion.
pub type BarSet = HashMap<String, Vec<Bar>>;

/// Multi-symbol quotes keyed by symbol.
pub type QuoteSet = HashMap<String, Vec<Quote>>;

/// Multi-symbol trades keyed by symbol.
pub type TradeSet = HashMap<String, Vec<Trade>>;

/// Fills in the symbol field that the response carries one level up.
pub(crate) trait WithSymbol {
    fn set_symbol(&mut self, symbol: &str);
}

macro_rules! impl_with_symbol {
    ($($ty:ty),+ $(,)?) => {
        $(
            impl WithSymbol for $ty {
                fn set_symbol(&mut self, symbol: &str) {
                    self.symbol = symbol.to_owned();
                }
            }
        )+
    };
}

impl_with_symbol!(
    Bar,
    Quote,
    Trade,
    TradingStatus,
    TradeCancel,
    TradeCorrection,
    Orderbook,
);

impl WithSymbol for Snapshot {
    fn set_symbol(&mut self, symbol: &str) {
        self.symbol = symbol.to_owned();
        // The nested records are keyed by the same symbol.
        if let Some(trade) = &mut self.latest_trade {
            trade.set_symbol(symbol);
        }
        if let Some(quote) = &mut self.latest_quote {
            quote.set_symbol(symbol);
        }
        for bar in [
            self.minute_bar.as_mut(),
            self.daily_bar.as_mut(),
            self.previous_daily_bar.as_mut(),
        ]
        .into_iter()
        .flatten()
        {
            bar.set_symbol(symbol);
        }
    }
}

impl WithSymbol for OptionsSnapshot {
    fn set_symbol(&mut self, symbol: &str) {
        self.symbol = symbol.to_owned();
        if let Some(trade) = &mut self.latest_trade {
            trade.set_symbol(symbol);
        }
        if let Some(quote) = &mut self.latest_quote {
            quote.set_symbol(symbol);
        }
    }
}
