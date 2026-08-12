//! Frames the market data stream sends, and the channels they belong to.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::data::models::{
    Bar, News, Orderbook, Quote, Trade, TradeCancel, TradeCorrection, TradingStatus,
};

/// A subscription channel.
///
/// The wire names are what goes in a subscribe payload; the message type is the
/// one-letter `T` on an incoming frame. alpaca-py keeps these in two parallel
/// dicts, `_MsgType` and `_CHANNEL_TYPES`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Channel {
    /// Executed trades.
    Trades,
    /// Best bid and ask updates.
    Quotes,
    /// Orderbook snapshots and updates, for crypto.
    Orderbooks,
    /// Minute bars.
    Bars,
    /// Corrections to previously sent bars.
    UpdatedBars,
    /// Daily bars.
    DailyBars,
    /// Trading status changes, such as halts.
    Statuses,
    /// Limit up / limit down bands.
    Lulds,
    /// News articles.
    News,
    /// Corrections to previously reported trades.
    Corrections,
    /// Cancellations of previously reported trades.
    CancelErrors,
}

impl Channel {
    /// The name used in a subscribe or unsubscribe payload.
    #[must_use]
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::Trades => "trades",
            Self::Quotes => "quotes",
            Self::Orderbooks => "orderbooks",
            Self::Bars => "bars",
            Self::UpdatedBars => "updatedBars",
            Self::DailyBars => "dailyBars",
            Self::Statuses => "statuses",
            Self::Lulds => "lulds",
            Self::News => "news",
            Self::Corrections => "corrections",
            Self::CancelErrors => "cancelErrors",
        }
    }

    /// The channel a `T` value belongs to, if it names market data.
    #[must_use]
    pub fn from_message_type(message_type: &str) -> Option<Self> {
        Some(match message_type {
            "t" => Self::Trades,
            "q" => Self::Quotes,
            "o" => Self::Orderbooks,
            "b" => Self::Bars,
            "u" => Self::UpdatedBars,
            "d" => Self::DailyBars,
            "s" => Self::Statuses,
            "l" => Self::Lulds,
            "n" => Self::News,
            "c" => Self::Corrections,
            "x" => Self::CancelErrors,
            _ => return None,
        })
    }

    /// Whether this channel may appear in a subscribe payload.
    ///
    /// Corrections and cancel errors arrive with the trades subscription and are
    /// rejected if named explicitly, so alpaca-py filters them out of the
    /// subscribe message and out of the "is anything subscribed" check.
    #[must_use]
    pub const fn is_subscribable(self) -> bool {
        !matches!(self, Self::Corrections | Self::CancelErrors)
    }

    /// Every channel, for iteration.
    pub const ALL: [Self; 11] = [
        Self::Trades,
        Self::Quotes,
        Self::Orderbooks,
        Self::Bars,
        Self::UpdatedBars,
        Self::DailyBars,
        Self::Statuses,
        Self::Lulds,
        Self::News,
        Self::Corrections,
        Self::CancelErrors,
    ];
}

/// What the server currently believes this connection is subscribed to.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subscriptions {
    /// Symbols subscribed for trades.
    #[serde(default)]
    pub trades: Vec<String>,
    /// Symbols subscribed for quotes.
    #[serde(default)]
    pub quotes: Vec<String>,
    /// Symbols subscribed for orderbooks.
    #[serde(default)]
    pub orderbooks: Vec<String>,
    /// Symbols subscribed for bars.
    #[serde(default)]
    pub bars: Vec<String>,
    /// Symbols subscribed for updated bars.
    #[serde(default, rename = "updatedBars")]
    pub updated_bars: Vec<String>,
    /// Symbols subscribed for daily bars.
    #[serde(default, rename = "dailyBars")]
    pub daily_bars: Vec<String>,
    /// Symbols subscribed for trading statuses.
    #[serde(default)]
    pub statuses: Vec<String>,
    /// Symbols subscribed for limit up / limit down bands.
    #[serde(default)]
    pub lulds: Vec<String>,
    /// Symbols subscribed for news.
    #[serde(default)]
    pub news: Vec<String>,
}

/// An error frame from the server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamError {
    /// Alpaca's numeric error code.
    #[serde(default)]
    pub code: Option<i64>,
    /// The human-readable message.
    #[serde(default, rename = "msg")]
    pub message: String,
}

/// One frame from the market data stream.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum StreamMessage {
    /// An executed trade.
    Trade(Trade),
    /// A quote update.
    Quote(Quote),
    /// An orderbook update.
    Orderbook(Orderbook),
    /// A minute bar.
    Bar(Bar),
    /// A correction to a previously sent bar.
    UpdatedBar(Bar),
    /// A daily bar.
    DailyBar(Bar),
    /// A trading status change.
    TradingStatus(TradingStatus),
    /// A news article.
    News(News),
    /// A correction to a previously reported trade.
    Correction(TradeCorrection),
    /// A cancellation of a previously reported trade.
    CancelError(TradeCancel),
    /// A frame this crate does not model.
    ///
    /// Limit up / limit down bands land here: alpaca-py has no model for them
    /// either and hands back the raw payload, so inventing a shape would be a
    /// guess rather than a port.
    Other {
        /// The `T` value identifying the frame.
        message_type: String,
        /// The frame as sent.
        raw: Value,
    },
    /// The server confirming what this connection is subscribed to.
    Subscription(Subscriptions),
    /// An error frame. The stream stays open unless it was fatal.
    Error(StreamError),
}

impl StreamMessage {
    /// The symbol this frame concerns, when it has one.
    #[must_use]
    pub fn symbol(&self) -> Option<&str> {
        match self {
            Self::Trade(t) => Some(&t.symbol),
            Self::Quote(q) => Some(&q.symbol),
            Self::Orderbook(o) => Some(&o.symbol),
            Self::Bar(b) | Self::UpdatedBar(b) | Self::DailyBar(b) => Some(&b.symbol),
            Self::TradingStatus(s) => Some(&s.symbol),
            Self::Correction(c) => Some(&c.symbol),
            Self::CancelError(c) => Some(&c.symbol),
            Self::News(_) | Self::Other { .. } | Self::Subscription(_) | Self::Error(_) => None,
        }
    }

    /// The channel this frame arrived on, when it is market data.
    #[must_use]
    pub fn channel(&self) -> Option<Channel> {
        Some(match self {
            Self::Trade(_) => Channel::Trades,
            Self::Quote(_) => Channel::Quotes,
            Self::Orderbook(_) => Channel::Orderbooks,
            Self::Bar(_) => Channel::Bars,
            Self::UpdatedBar(_) => Channel::UpdatedBars,
            Self::DailyBar(_) => Channel::DailyBars,
            Self::TradingStatus(_) => Channel::Statuses,
            Self::News(_) => Channel::News,
            Self::Correction(_) => Channel::Corrections,
            Self::CancelError(_) => Channel::CancelErrors,
            Self::Other { message_type, .. } => Channel::from_message_type(message_type)?,
            Self::Subscription(_) | Self::Error(_) => return None,
        })
    }

    /// Whether this frame is market data rather than a control frame.
    ///
    /// Only market data resets the staleness clock. A subscription
    /// acknowledgement or an error must not, or a stream that is subscribed but
    /// silent would look healthy forever and the escalating backoff would never
    /// engage.
    #[must_use]
    pub fn is_market_data(&self) -> bool {
        !matches!(self, Self::Subscription(_) | Self::Error(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_types_map_to_channels() {
        assert_eq!(Channel::from_message_type("t"), Some(Channel::Trades));
        assert_eq!(Channel::from_message_type("u"), Some(Channel::UpdatedBars));
        assert_eq!(Channel::from_message_type("x"), Some(Channel::CancelErrors));
        assert_eq!(Channel::from_message_type("subscription"), None);
        assert_eq!(Channel::from_message_type("error"), None);
    }

    #[test]
    fn wire_names_are_camel_case_where_alpaca_uses_it() {
        assert_eq!(Channel::UpdatedBars.wire_name(), "updatedBars");
        assert_eq!(Channel::DailyBars.wire_name(), "dailyBars");
        assert_eq!(Channel::CancelErrors.wire_name(), "cancelErrors");
        assert_eq!(Channel::Trades.wire_name(), "trades");
    }

    #[test]
    fn corrections_and_cancel_errors_are_not_subscribable() {
        // They ride along with the trades subscription; naming them explicitly
        // is an error, so they never appear in a subscribe payload.
        assert!(!Channel::Corrections.is_subscribable());
        assert!(!Channel::CancelErrors.is_subscribable());

        for channel in Channel::ALL {
            if !matches!(channel, Channel::Corrections | Channel::CancelErrors) {
                assert!(channel.is_subscribable(), "{channel:?}");
            }
        }
    }

    #[test]
    fn control_frames_are_not_market_data() {
        assert!(!StreamMessage::Subscription(Subscriptions::default()).is_market_data());
        assert!(
            !StreamMessage::Error(StreamError {
                code: Some(400),
                message: "bad".to_owned(),
            })
            .is_market_data()
        );
    }

    #[test]
    fn subscription_frames_deserialize_with_camel_case_keys() {
        let subs: Subscriptions = serde_json::from_str(
            r#"{"trades":["AAPL"],"updatedBars":["SPY"],"dailyBars":[],"quotes":["MSFT"]}"#,
        )
        .unwrap();

        assert_eq!(subs.trades, ["AAPL"]);
        assert_eq!(subs.updated_bars, ["SPY"]);
        assert_eq!(subs.quotes, ["MSFT"]);
        assert!(subs.daily_bars.is_empty());
    }
}
