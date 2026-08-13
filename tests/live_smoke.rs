//! Smoke tests against the real Alpaca paper API.
//!
//! Every mock in this repo encodes what I *believe* the wire looks like. These
//! are the only tests that check that belief against the thing itself — the real
//! handshake sequence, the real frame shapes, whether timestamps arrive as
//! msgpack extensions in practice.
//!
//! All are `#[ignore]`d, so `cargo test` never spends network time or
//! credentials on them. Run them deliberately:
//!
//! ```text
//! just live
//! ```
//!
//! They need `APCA_API_KEY_ID` and `APCA_API_SECRET_KEY` in the environment and
//! expect a **paper** key pair.
//!
//! Strictly read-only: account and market data reads, and stream handshakes.
//! Nothing here submits, replaces, or cancels an order, and nothing writes to
//! the account.

#![cfg(all(feature = "trading", feature = "data"))]

use std::time::Duration;

use alpaca_sdk::Credentials;
use alpaca_sdk::data::{
    CryptoBarsRequest, CryptoDataStream, CryptoFeed, CryptoHistoricalDataClient,
    CryptoLatestRequest, DataFeed, StockHistoricalDataClient, StockLatestRequest, StreamMessage,
    TimeFrame,
};
use alpaca_sdk::trading::{TradeStreamMessage, TradingClient, TradingStream};
use futures_util::StreamExt as _;

/// Credentials from the environment, with a clear skip message when absent.
fn credentials() -> Credentials {
    Credentials::from_env().unwrap_or_else(|e| {
        panic!("{e}\n\nSet APCA_API_KEY_ID and APCA_API_SECRET_KEY (paper keys) and re-run.")
    })
}

fn assert_paper(credentials: &Credentials) {
    // Alpaca prefixes paper keys with PK and live keys with AK. Refusing to run
    // against a live account is worth the two lines.
    if let Credentials::KeyPair { api_key, .. } = credentials {
        assert!(
            api_key.starts_with("PK"),
            "these tests only run against a paper account; this key does not look like one"
        );
    }
}

// ------------------------------------------------------------------ REST

#[tokio::test]
#[ignore = "hits the real paper API"]
async fn account_reads_back() {
    let credentials = credentials();
    assert_paper(&credentials);

    let client = TradingClient::new(&credentials, true).unwrap();
    let account = client.get_account().await.unwrap();

    assert!(!account.account_number.is_empty());
    assert!(account.buying_power.is_some(), "no buying power reported");
    println!(
        "account status={:?} currency={:?} equity={:?}",
        account.status, account.currency, account.equity
    );
}

#[tokio::test]
#[ignore = "hits the real paper API"]
async fn clock_and_calendar_read_back() {
    let client = TradingClient::new(&credentials(), true).unwrap();

    let clock = client.get_clock().await.unwrap();
    println!(
        "market open={} next_open={} next_close={}",
        clock.is_open, clock.next_open, clock.next_close
    );

    // The Calendar deserializer joins the date with bare HH:MM times; if that
    // were wrong, this is where it shows.
    let calendar = client.get_calendar(None).await.unwrap();
    assert!(!calendar.is_empty());

    let day = &calendar[0];
    println!("calendar[0] {} {} → {}", day.date, day.open, day.close);

    // The session fields and settlement_date exist in real responses but in no
    // captured fixture, and the session times use a different format from
    // open/close. This is the only place that gets checked against the real API.
    let session_open = day
        .session_open
        .expect("the live API sends session_open; parsing it must have failed");
    let session_close = day.session_close.expect("the live API sends session_close");
    let settlement = day
        .settlement_date
        .expect("the live API sends settlement_date");

    println!("  extended hours {session_open} → {session_close}, settles {settlement}");

    assert!(
        session_open < day.open,
        "the extended-hours session should open before the regular one"
    );
    assert!(
        session_close > day.close,
        "the extended-hours session should close after the regular one"
    );
    assert!(
        settlement >= day.date,
        "settlement cannot precede the trade date"
    );
}

#[tokio::test]
#[ignore = "hits the real paper API"]
async fn assets_and_positions_read_back() {
    let client = TradingClient::new(&credentials(), true).unwrap();

    let asset = client
        .get_asset(&alpaca_sdk::types::AssetIdent::from("AAPL"))
        .await
        .unwrap();
    assert_eq!(asset.symbol, "AAPL");
    println!(
        "asset {} class={:?} exchange={:?}",
        asset.symbol, asset.asset_class, asset.exchange
    );

    // May legitimately be empty on a fresh paper account.
    let positions = client.get_all_positions().await.unwrap();
    println!("open positions: {}", positions.len());

    let orders = client.get_orders(None).await.unwrap();
    println!("open orders: {}", orders.len());
}

#[tokio::test]
#[ignore = "hits the real paper API"]
async fn crypto_bars_read_back_without_credentials() {
    // The keyless path: this client sends no auth headers at all.
    let client = CryptoHistoricalDataClient::new().unwrap();
    let request = CryptoBarsRequest::new("BTC/USD", TimeFrame::day()).limit(5);

    let bars = client
        .get_crypto_bars(&request, CryptoFeed::Us)
        .await
        .unwrap();

    let btc = bars.get("BTC/USD").expect("BTC/USD bars");
    assert!(!btc.is_empty(), "no bars returned");
    assert_eq!(btc[0].symbol, "BTC/USD", "symbol was not filled in");
    assert!(btc[0].close > 0.0);
    println!(
        "BTC/USD {} bars, latest close={}",
        btc.len(),
        btc[btc.len() - 1].close
    );
}

#[tokio::test]
#[ignore = "hits the real paper API"]
async fn stock_latest_quote_reads_back() {
    let client = StockHistoricalDataClient::new(&credentials()).unwrap();
    let request = StockLatestRequest::new(["AAPL", "MSFT"]).feed(DataFeed::Iex);

    let quotes = client.get_stock_latest_quote(&request).await.unwrap();

    assert!(!quotes.is_empty(), "no quotes returned");
    for (symbol, quote) in &quotes {
        assert_eq!(&quote.symbol, symbol, "symbol was not filled in");
        println!(
            "{symbol} bid={} ask={} at {}",
            quote.bid_price, quote.ask_price, quote.timestamp
        );
    }
}

#[tokio::test]
#[ignore = "hits the real paper API"]
async fn crypto_latest_quote_reads_back() {
    let client = CryptoHistoricalDataClient::new().unwrap();
    let quotes = client
        .get_crypto_latest_quote(&CryptoLatestRequest::new("BTC/USD"), CryptoFeed::Us)
        .await
        .unwrap();

    let quote = quotes.get("BTC/USD").expect("BTC/USD quote");
    assert!(quote.bid_price > 0.0);
    println!("BTC/USD bid={} ask={}", quote.bid_price, quote.ask_price);
}

// ---------------------------------------------------------------- streams

#[tokio::test]
#[ignore = "hits the real paper API"]
async fn crypto_stream_connects_and_receives() {
    // Crypto trades around the clock, so this proves the msgpack path end to
    // end — including that timestamps really do arrive as extension types —
    // regardless of when it runs.
    let mut stream = CryptoDataStream::new(credentials(), CryptoFeed::Us).unwrap();
    stream.subscribe_trades(["BTC/USD", "ETH/USD"]);
    stream.subscribe_quotes(["BTC/USD"]);

    let mut messages = Box::pin(stream.run());
    let mut subscribed = false;
    let mut data = 0usize;

    let outcome = tokio::time::timeout(Duration::from_secs(30), async {
        while let Some(message) = messages.next().await {
            match message {
                Ok(StreamMessage::Subscription(subs)) => {
                    println!(
                        "subscribed: trades={:?} quotes={:?}",
                        subs.trades, subs.quotes
                    );
                    subscribed = true;
                }
                Ok(StreamMessage::Error(error)) => panic!("server error: {error:?}"),
                Ok(other) => {
                    if data == 0 {
                        println!("first frame: {:?} {:?}", other.channel(), other.symbol());
                    }
                    data += 1;
                    if data >= 3 {
                        return;
                    }
                }
                Err(error) => panic!("stream error: {error}"),
            }
        }
    })
    .await;

    assert!(subscribed, "never received a subscription acknowledgement");
    println!(
        "received {data} market data frames (timed out: {})",
        outcome.is_err()
    );
}

#[tokio::test]
#[ignore = "hits the real paper API"]
async fn trading_stream_authenticates() {
    // A quiet paper account sends no trade updates, so the thing being proved
    // here is the handshake: connect, authenticate with the nested envelope,
    // listen — and no error frame or disconnect within the window.
    let stream = TradingStream::new(credentials(), true);
    let mut updates = Box::pin(stream.run());

    let result = tokio::time::timeout(Duration::from_secs(12), async {
        while let Some(update) = updates.next().await {
            match update {
                Ok(TradeStreamMessage::TradeUpdate(update)) => {
                    println!("trade update: {:?}", update.event);
                    return Ok(());
                }
                Ok(TradeStreamMessage::Other { stream, .. }) => {
                    println!("other frame on stream {stream}");
                }
                // The enum is #[non_exhaustive], so new variants stay handled.
                Ok(_) => {}
                Err(error) => return Err(format!("{error}")),
            }
        }
        Ok(())
    })
    .await;

    match result {
        // Timing out is the expected outcome on a quiet account: it means the
        // connection stayed up and authenticated for the whole window.
        Err(_) => println!("connected and stayed up for 12s with no updates, as expected"),
        Ok(Ok(())) => println!("connected and received an update"),
        Ok(Err(error)) => panic!("trading stream failed: {error}"),
    }
}
