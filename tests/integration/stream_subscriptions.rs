//! What each live stream will let a caller subscribe to, and where it connects.
//!
//! Every one of these is checkable without a socket, which is the point: the
//! stream tests that need a server are slow and few, so the subscription
//! surface — four asset classes with different channel sets — is pinned here
//! instead.
//!
//! The channel sets are not interchangeable. Options carry trades and quotes and
//! nothing else; crypto carries orderbooks and no trading statuses; news carries
//! only news. Offering a channel an asset class does not have would be a
//! subscription the server rejects, and rejection costs the single connection
//! Alpaca allows per account.

#![cfg(feature = "data")]

use std::time::Duration;

use crate::common::credentials;
use alpaca_sdk::Credentials;
use alpaca_sdk::data::{
    Channel, CryptoDataStream, CryptoFeed, DataFeed, NewsDataStream, OptionDataStream, OptionsFeed,
    StockDataStream,
};

// ------------------------------------------------------------------ stocks

#[test]
fn a_stock_stream_offers_every_equity_channel() {
    let mut stream = StockDataStream::new(credentials(), DataFeed::Iex).unwrap();

    stream.subscribe_trades(["AAPL"]);
    stream.subscribe_quotes(["AAPL"]);
    stream.subscribe_bars(["AAPL"]);
    stream.subscribe_updated_bars(["AAPL"]);
    stream.subscribe_daily_bars(["AAPL"]);
    stream.subscribe_trading_statuses(["AAPL"]);
    stream.subscribe_lulds(["AAPL"]);

    let set = stream.subscriptions();
    for channel in [
        Channel::Trades,
        Channel::Quotes,
        Channel::Bars,
        Channel::UpdatedBars,
        Channel::DailyBars,
        Channel::Statuses,
        Channel::Lulds,
    ] {
        assert_eq!(set.symbols(channel), ["AAPL"], "{channel:?}");
    }
}

#[test]
fn unsubscribing_removes_only_the_symbols_named() {
    let mut stream = StockDataStream::new(credentials(), DataFeed::Sip).unwrap();

    stream.subscribe_trades(["AAPL", "MSFT", "SPY"]);
    stream.unsubscribe_trades(["MSFT"]);

    assert_eq!(
        stream.subscriptions().symbols(Channel::Trades),
        ["AAPL", "SPY"]
    );

    // Unsubscribing from a channel that was never subscribed is not an error.
    stream.unsubscribe_quotes(["AAPL"]);
    assert!(stream.subscriptions().symbols(Channel::Quotes).is_empty());
}

/// Corrections and cancel errors arrive with the trades subscription and are
/// rejected if named in a subscribe payload, so registering for them must not
/// make the connection look subscribed on its own.
#[test]
fn registering_for_corrections_does_not_count_as_a_subscription() {
    let mut stream = StockDataStream::new(credentials(), DataFeed::Iex).unwrap();

    stream.register_trade_corrections(["AAPL"]);
    stream.register_trade_cancels(["AAPL"]);

    assert_eq!(
        stream.subscriptions().symbols(Channel::Corrections),
        ["AAPL"]
    );
    assert_eq!(
        stream.subscriptions().symbols(Channel::CancelErrors),
        ["AAPL"]
    );
    assert!(
        stream.subscriptions().is_empty(),
        "neither channel is subscribable, so nothing is subscribed yet"
    );

    stream.subscribe_trades(["AAPL"]);
    assert!(!stream.subscriptions().is_empty());
}

#[test]
fn only_the_two_feeds_that_carry_a_live_stock_stream_are_accepted() {
    assert!(StockDataStream::new(credentials(), DataFeed::Iex).is_ok());
    assert!(StockDataStream::new(credentials(), DataFeed::Sip).is_ok());
    assert!(StockDataStream::new(credentials(), DataFeed::Otc).is_err());
    assert!(StockDataStream::new(credentials(), DataFeed::DelayedSip).is_err());
}

// ------------------------------------------------------------------ crypto

#[test]
fn a_crypto_stream_carries_orderbooks_and_no_trading_statuses() {
    let mut stream = CryptoDataStream::new(credentials(), CryptoFeed::Us).unwrap();

    stream.subscribe_trades(["BTC/USD"]);
    stream.subscribe_quotes(["BTC/USD"]);
    stream.subscribe_bars(["BTC/USD"]);
    stream.subscribe_updated_bars(["BTC/USD"]);
    stream.subscribe_daily_bars(["BTC/USD"]);
    stream.subscribe_orderbooks(["BTC/USD"]);

    let set = stream.subscriptions();
    assert_eq!(set.symbols(Channel::Orderbooks), ["BTC/USD"]);
    assert_eq!(set.symbols(Channel::Trades), ["BTC/USD"]);
    // No `subscribe_trading_statuses` exists here, so nothing can set it.
    assert!(set.symbols(Channel::Statuses).is_empty());

    stream.unsubscribe_orderbooks(["BTC/USD"]);
    assert!(
        stream
            .subscriptions()
            .symbols(Channel::Orderbooks)
            .is_empty()
    );
}

// ----------------------------------------------------------------- options

#[test]
fn an_option_stream_carries_trades_and_quotes_only() {
    let mut stream = OptionDataStream::new(credentials(), OptionsFeed::Opra).unwrap();

    stream.subscribe_trades(["AAPL240119C00150000"]);
    stream.subscribe_quotes(["AAPL240119C00150000"]);

    let set = stream.subscriptions();
    assert_eq!(set.symbols(Channel::Trades), ["AAPL240119C00150000"]);
    assert_eq!(set.symbols(Channel::Quotes), ["AAPL240119C00150000"]);
    assert!(set.symbols(Channel::Bars).is_empty());

    stream.unsubscribe_trades(["AAPL240119C00150000"]);
    assert!(stream.subscriptions().symbols(Channel::Trades).is_empty());
}

// -------------------------------------------------------------------- news

/// News subscribes by symbol like the rest, and `*` is how a caller asks for
/// everything — a wildcard the other streams do not take.
#[test]
fn a_news_stream_carries_one_channel() {
    let mut stream = NewsDataStream::new(credentials());

    stream.subscribe_news(["*"]);
    assert_eq!(stream.subscriptions().symbols(Channel::News), ["*"]);

    stream.unsubscribe_news(["*"]);
    assert!(stream.subscriptions().is_empty());
}

// ------------------------------------------------------------------ config

#[test]
fn a_staleness_timeout_must_be_positive() {
    let mut stream = StockDataStream::new(credentials(), DataFeed::Iex).unwrap();

    assert!(stream.set_data_timeout(Duration::from_secs(30)).is_ok());
    // Zero would reconnect continuously rather than never.
    assert!(stream.set_data_timeout(Duration::ZERO).is_err());
}

/// Every stream takes an explicit endpoint, which is what the mock-server tests
/// are built on — and what a caller pointing at a proxy needs.
#[test]
fn every_stream_can_be_pointed_somewhere_else() {
    let endpoint = "ws://127.0.0.1:1";

    let mut stock = StockDataStream::with_endpoint(credentials(), endpoint);
    stock.subscribe_trades(["AAPL"]);
    assert!(!stock.subscriptions().is_empty());

    let mut crypto = CryptoDataStream::with_endpoint(credentials(), endpoint);
    crypto.subscribe_trades(["BTC/USD"]);
    assert!(!crypto.subscriptions().is_empty());

    let mut options = OptionDataStream::with_endpoint(credentials(), endpoint);
    options.subscribe_quotes(["AAPL240119C00150000"]);
    assert!(!options.subscriptions().is_empty());

    let mut news = NewsDataStream::with_endpoint(credentials(), endpoint);
    news.subscribe_news(["*"]);
    assert!(!news.subscriptions().is_empty());
}

// ------------------------------------------------------------ feed guards

/// A `wire_enum!`'s `Unknown(String)` variant is publicly constructible, so an
/// unrecognised feed name would otherwise be interpolated straight into the
/// websocket endpoint URL — the same hazard the REST path encoder exists for.
/// There is no live stream behind a feed this crate does not know, so refusing
/// loses nothing.
#[test]
fn an_unknown_feed_is_refused_rather_than_put_in_the_endpoint() {
    let creds = || Credentials::new("key", "secret").unwrap();

    let crypto = CryptoDataStream::new(creds(), CryptoFeed::from("../../v2/account"));
    assert!(
        matches!(crypto, Err(alpaca_sdk::Error::InvalidRequest(_))),
        "an unknown crypto feed must not reach the endpoint URL"
    );

    let options = OptionDataStream::new(creds(), OptionsFeed::from("../../v2/account"));
    assert!(
        matches!(options, Err(alpaca_sdk::Error::InvalidRequest(_))),
        "an unknown options feed must not reach the endpoint URL"
    );

    // And a stock feed that is known but has no live stream is refused too.
    assert!(StockDataStream::new(creds(), DataFeed::Otc).is_err());
}

/// The known feeds still build.
///
/// The test above refuses an unknown feed; on its own, a `known_feed` that
/// refused *everything* would satisfy it. This is the other half, and the pair
/// is what pins the boundary rather than one side of it.
#[test]
fn the_known_feeds_still_construct() {
    let creds = || Credentials::new("key", "secret").unwrap();

    assert!(CryptoDataStream::new(creds(), CryptoFeed::Us).is_ok());
    assert!(OptionDataStream::new(creds(), OptionsFeed::Opra).is_ok());
    assert!(OptionDataStream::new(creds(), OptionsFeed::Indicative).is_ok());
    assert!(StockDataStream::new(creds(), DataFeed::Iex).is_ok());
    assert!(StockDataStream::new(creds(), DataFeed::Sip).is_ok());
}
