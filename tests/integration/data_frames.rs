//! The `polars` feature: market data collections as `DataFrame`s.
//!
//! The dtype assertions are the point of this file. A frame that holds the right
//! numbers under the wrong types is worse than no frame — the arithmetic still
//! works and the joins silently do not.

#![cfg(feature = "polars")]

use std::collections::HashMap;

use alpaca_sdk::data::{
    Bar, DailyAuctions, ForexRate, Quote, StockBarsRequest, StockHistoricalDataClient, TimeFrame,
    ToFrame, Trade,
};
use alpaca_sdk::polars::prelude::*;
use alpaca_sdk::{Credentials, RestConfig, RetryConfig};
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn fixture(name: &str) -> serde_json::Value {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name);
    let body = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    serde_json::from_str(&body).unwrap()
}

fn timestamp(text: &str) -> chrono::DateTime<chrono::Utc> {
    text.parse().unwrap()
}

/// Builds a market data model from the wire shape Alpaca sends.
///
/// The models are `#[non_exhaustive]` — Alpaca adds fields to them without a
/// version bump, and the crate should be able to follow that without a major
/// release — so they are not constructible as struct literals from outside the
/// crate. Going through the deserializer is the honest route anyway: it is the
/// path a real response takes.
fn from_wire<T: serde::de::DeserializeOwned>(value: serde_json::Value) -> T {
    serde_json::from_value(value).unwrap()
}

/// Serves one captured payload and returns what the client made of it.
async fn bars_from(fixture_name: &str) -> HashMap<String, Vec<Bar>> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/stocks/bars"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(fixture_name)))
        .mount(&server)
        .await;

    let credentials = Credentials::new("key", "secret").unwrap();
    let client = StockHistoricalDataClient::with_config(
        &credentials,
        RestConfig::new(server.uri())
            .api_version("v2")
            .retry(RetryConfig::none()),
    )
    .unwrap();

    client
        .get_stock_bars(&StockBarsRequest::new("AAPL", TimeFrame::day()).limit(2))
        .await
        .unwrap()
}

/// The whole path, not just the conversion: a captured payload, through the
/// client that fills in the symbols, into a frame.
#[tokio::test]
async fn a_captured_bar_payload_becomes_a_typed_frame() {
    let bars = bars_from("data/test_historical_stock_data__test_get_bars__01.json").await;
    let frame = bars.df().unwrap();

    assert_eq!(frame.shape(), (2, 9));
    assert_eq!(
        frame
            .get_column_names()
            .iter()
            .map(|name| name.as_str())
            .collect::<Vec<_>>(),
        [
            "symbol",
            "timestamp",
            "open",
            "high",
            "low",
            "close",
            "volume",
            "trade_count",
            "vwap",
        ]
    );

    // Nanoseconds and tagged UTC. A naive column would compare wrong against
    // anything the caller brings in with a zone attached.
    assert_eq!(
        frame.column("timestamp").unwrap().dtype(),
        &DataType::Datetime(TimeUnit::Nanoseconds, Some(TimeZone::UTC))
    );
    assert_eq!(frame.column("symbol").unwrap().dtype(), &DataType::String);
    assert_eq!(frame.column("open").unwrap().dtype(), &DataType::Float64);

    assert_eq!(
        frame.column("symbol").unwrap().str().unwrap().get(0),
        Some("AAPL")
    );
    assert_eq!(
        frame.column("open").unwrap().f64().unwrap().get(0),
        Some(174.0)
    );
    assert_eq!(
        frame.column("close").unwrap().f64().unwrap().get(1),
        Some(175.84)
    );
    assert_eq!(
        frame
            .column("timestamp")
            .unwrap()
            .datetime()
            .unwrap()
            .physical()
            .get(0)
            .unwrap(),
        timestamp("2022-02-01T05:00:00Z")
            .timestamp_nanos_opt()
            .unwrap()
    );
}

/// A `HashMap` iterates in an arbitrary order, so without the sort the row order
/// would change between runs of the same program.
#[test]
fn rows_are_grouped_and_sorted_by_key() {
    let payload = fixture("data/test_historical_stock_data__test_multisymbol_quotes__01.json");
    let quotes: HashMap<String, Vec<Quote>> =
        serde_json::from_value(payload["quotes"].clone()).unwrap();

    let frame = quotes.df().unwrap();
    assert_eq!(
        frame
            .column("symbol")
            .unwrap()
            .str()
            .unwrap()
            .iter()
            .flatten()
            .collect::<Vec<_>>(),
        ["AAPL", "TSLA"]
    );
}

/// Deserializing the inner map directly leaves every `symbol` field empty — the
/// collection types fill it in one level up. The frame takes the key instead, so
/// it is right either way.
#[test]
fn the_map_key_wins_over_the_records_own_field() {
    let payload = fixture("data/test_historical_stock_data__test_multisymbol_quotes__01.json");
    let quotes: HashMap<String, Vec<Quote>> =
        serde_json::from_value(payload["quotes"].clone()).unwrap();

    assert!(
        quotes["AAPL"][0].symbol.is_empty(),
        "the fixture stopped exercising this case"
    );

    let frame = quotes.df().unwrap();
    assert_eq!(
        frame.column("symbol").unwrap().str().unwrap().get(0),
        Some("AAPL")
    );
}

/// Building the list column from a `Vec<Option<Series>>` would infer its element
/// type from the first non-null entry and hand back `List(Null)` when there is
/// none. Crypto quotes routinely carry no conditions.
#[test]
fn a_conditions_column_is_a_string_list_even_when_every_row_is_null() {
    // Built from the wire form rather than as a struct literal: the models are
    // `#[non_exhaustive]`, because Alpaca adds fields to them without warning.
    let bare: Quote = from_wire(json!({
        "t": "2022-03-09T09:00:00Z",
        "bp": 1.0, "bs": 1.0,
        "ap": 2.0, "as": 1.0
    }));

    let frame = [bare].as_slice().df().unwrap();
    assert_eq!(
        frame.column("conditions").unwrap().dtype(),
        &DataType::List(Box::new(DataType::String))
    );
    assert_eq!(frame.column("conditions").unwrap().null_count(), 1);
    // An absent exchange is a null, not an empty string.
    assert_eq!(frame.column("bid_exchange").unwrap().null_count(), 1);
}

/// The nested type. One row per print rather than one per day, because a frame
/// of list columns makes every question start with an explode.
#[test]
fn a_day_of_auctions_flattens_to_one_row_per_print() {
    let print = |exchange: &str, price: f64| json!({"t": "2024-04-26T13:30:00Z", "x": exchange, "p": price, "c": "Q"});

    let day: DailyAuctions = from_wire(json!({
        "d": "2024-04-26",
        "o": [print("P", 100.0)],
        "c": [print("Q", 101.0), print("V", 102.0)]
    }));

    let auctions: HashMap<String, Vec<DailyAuctions>> =
        HashMap::from([("AAPL".to_owned(), vec![day])]);
    let frame = auctions.df().unwrap();

    assert_eq!(frame.shape(), (3, 8));
    assert_eq!(
        frame
            .column("session")
            .unwrap()
            .str()
            .unwrap()
            .iter()
            .flatten()
            .collect::<Vec<_>>(),
        ["opening", "closing", "closing"]
    );
    assert_eq!(frame.column("date").unwrap().dtype(), &DataType::Date);
    // The symbol repeats per print, not per day.
    assert_eq!(frame.column("symbol").unwrap().null_count(), 0);
    assert_eq!(frame.column("size").unwrap().null_count(), 3);
}

/// Forex is keyed by pair, and the column says so: a frame of rates joined to a
/// frame of bars on "symbol" would be a mistake the column name prevents.
#[test]
fn forex_rates_are_keyed_by_currency_pair() {
    let rate: ForexRate = from_wire(json!({
        "t": "2024-04-26T13:30:00Z",
        "bp": 1.07, "mp": 1.075, "ap": 1.08
    }));

    let rates: HashMap<String, Vec<ForexRate>> = HashMap::from([("EURUSD".to_owned(), vec![rate])]);
    let frame = rates.df().unwrap();

    assert!(frame.column("symbol").is_err());
    assert_eq!(
        frame.column("currency_pair").unwrap().str().unwrap().get(0),
        Some("EURUSD")
    );
}

/// An empty response is common — a symbol with no trades in the window — and
/// must produce a frame with the right columns rather than an error or a frame
/// with none.
#[test]
fn an_empty_collection_still_has_its_columns() {
    let trades: HashMap<String, Vec<Trade>> = HashMap::new();
    let frame = trades.df().unwrap();

    assert_eq!(frame.shape(), (0, 9));
    assert_eq!(
        frame.column("timestamp").unwrap().dtype(),
        &DataType::Datetime(TimeUnit::Nanoseconds, Some(TimeZone::UTC))
    );
    assert_eq!(
        frame.column("conditions").unwrap().dtype(),
        &DataType::List(Box::new(DataType::String))
    );
}

/// A caller can build a market data record without going through the wire form.
///
/// These types are `#[non_exhaustive]`, which is right — Alpaca adds fields to
/// them — but it also removes the struct literal, and building a synthetic bar
/// is a real need: a backtest harness, a fixture, a frame over rows that did not
/// come from the API. `Default` plus public fields is the replacement, and this
/// pins that it exists. Without it the only route is `serde_json`, which is a
/// workaround every downstream user would have to reinvent.
#[test]
fn market_data_records_can_be_built_without_json() {
    let mut bar = Bar::default();
    bar.symbol = "AAPL".to_owned();
    bar.timestamp = timestamp("2024-04-26T13:30:00Z");
    bar.open = 100.0;
    bar.high = 105.0;
    bar.low = 99.0;
    bar.close = 104.0;
    bar.volume = 1_000.0;

    let frame = [bar].as_slice().df().unwrap();
    assert_eq!(frame.height(), 1);
    assert_eq!(
        frame.column("close").unwrap().f64().unwrap().get(0),
        Some(104.0)
    );

    // And the other four the frame conversion accepts.
    let mut quote = Quote::default();
    quote.ask_price = 1.5;
    let mut rate = ForexRate::default();
    rate.currency_pair = "EURUSD".to_owned();
    let mut day = DailyAuctions::default();
    day.symbol = "AAPL".to_owned();

    assert_eq!(quote.ask_price, 1.5);
    assert_eq!(rate.currency_pair, "EURUSD");
    assert!(day.opening.is_empty());
}
