//! Every market data route no other test calls, checked for where it goes and
//! what it decodes.
//!
//! Unlike the broker smoke test, these serve a real body: the market data
//! responses are small, their shapes are captured in `fixtures/`, and the
//! wrapping key is part of the contract — `{"bars": {…}}` against
//! `{"quotes": {…}}` is a difference no path assertion would catch, and the
//! pagination loop reads that key by name.

#![cfg(feature = "data")]

use crate::common::credentials;
use alpaca_sdk::data::{
    CorporateActionsClient, CryptoFeed, CryptoHistoricalDataClient, CryptoLatestRequest,
    CryptoSnapshotRequest, ForexDataClient, LogoClient, NewsClient, OptionHistoricalDataClient,
    OptionLatestRequest, ScreenerClient, SingleSymbolRequest, StockHistoricalDataClient,
    StockLatestRequest, StockTimeseriesRequest, TimeseriesRequest,
};
use alpaca_sdk::{RestConfig, RetryConfig};
use futures_util::StreamExt as _;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn config(server: &MockServer, version: &str) -> RestConfig {
    RestConfig::new(server.uri())
        .api_version(version)
        .retry(RetryConfig::none())
}

fn stocks(server: &MockServer) -> StockHistoricalDataClient {
    StockHistoricalDataClient::with_config(&credentials(), config(server, "v2")).unwrap()
}

fn options(server: &MockServer) -> OptionHistoricalDataClient {
    OptionHistoricalDataClient::with_config(&credentials(), config(server, "v1beta1")).unwrap()
}

fn crypto(server: &MockServer) -> CryptoHistoricalDataClient {
    CryptoHistoricalDataClient::with_config(None, config(server, "v1beta3")).unwrap()
}

/// Serves `body` on exactly one path, and fails on drop if it was not called.
async fn serving(http_path: &str, body: serde_json::Value) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(http_path.to_owned()))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .expect(1)
        .mount(&server)
        .await;
    server
}

fn quote() -> serde_json::Value {
    json!({"t": "2022-03-09T09:00:00Z", "ap": 158.65, "as": 1, "bp": 159.52, "bs": 4})
}

fn trade() -> serde_json::Value {
    json!({"t": "2022-03-09T09:00:00Z", "p": 158.65, "s": 10})
}

fn bar() -> serde_json::Value {
    json!({"t": "2022-03-09T00:00:00Z", "o": 1.0, "h": 2.0, "l": 0.5, "c": 1.5, "v": 100.0})
}

// ---------------------------------------------------------------- versions

/// The version segment each constructor picks for itself.
///
/// Every other test in this file hands the version in through `config`, so the
/// segment the shipped constructors choose is asserted by nothing: change `v2`
/// to `v3` inside `StockHistoricalDataClient::new` and every stock route 404s
/// in production while the suite stays green. These read the choice back off
/// the client rather than off a mock because `new` bakes in Alpaca's own base
/// URL alongside the version — there is no seam to point a `MockServer` at.
/// That the configured version then reaches the wire is what the rest of this
/// file already proves.
#[test]
fn each_data_client_constructor_picks_its_own_version_segment() {
    let credentials = credentials();

    // Stocks are the only data surface that is not beta.
    let stocks = StockHistoricalDataClient::new(&credentials).unwrap();
    assert_eq!(stocks.rest().config().api_version, "v2");

    // Crypto is three majors ahead of the other beta surfaces, and both of its
    // constructors have to agree.
    let crypto = CryptoHistoricalDataClient::new().unwrap();
    assert_eq!(crypto.rest().config().api_version, "v1beta3");
    let crypto = CryptoHistoricalDataClient::with_credentials(&credentials).unwrap();
    assert_eq!(crypto.rest().config().api_version, "v1beta3");

    let options = OptionHistoricalDataClient::new(&credentials).unwrap();
    assert_eq!(options.rest().config().api_version, "v1beta1");
    let forex = ForexDataClient::new(&credentials).unwrap();
    assert_eq!(forex.rest().config().api_version, "v1beta1");
    let logos = LogoClient::new(&credentials).unwrap();
    assert_eq!(logos.rest().config().api_version, "v1beta1");
    let news = NewsClient::new(&credentials).unwrap();
    assert_eq!(news.rest().config().api_version, "v1beta1");
    let screener = ScreenerClient::new(&credentials).unwrap();
    assert_eq!(screener.rest().config().api_version, "v1beta1");

    // Corporate actions is `v1` for the polled route. Its event stream is
    // `v1beta1` and does not read this field at all — see
    // `the_corporate_action_event_stream_is_v1beta1_on_a_v1_client`.
    let corporate_actions = CorporateActionsClient::new(&credentials).unwrap();
    assert_eq!(corporate_actions.rest().config().api_version, "v1");
}

// ------------------------------------------------------------------ stocks

/// The multi-symbol time series routes, and the wrapping key each one reads.
/// The key is not decoration: `get_marketdata` merges pages by it, so reading
/// the wrong one returns nothing at all rather than failing.
#[tokio::test]
async fn stock_quotes_and_trades_read_their_own_wrapping_keys() {
    let server = serving("/v2/stocks/quotes", json!({"quotes": {"AAPL": [quote()]}})).await;
    let quotes = stocks(&server)
        .get_stock_quotes(&StockTimeseriesRequest::new("AAPL"))
        .await
        .unwrap();
    assert_eq!(quotes["AAPL"].len(), 1);
    // The symbol is filled in from the response key, not from the record.
    assert_eq!(quotes["AAPL"][0].symbol, "AAPL");

    let server = serving("/v2/stocks/trades", json!({"trades": {"AAPL": [trade()]}})).await;
    let trades = stocks(&server)
        .get_stock_trades(&StockTimeseriesRequest::new("AAPL"))
        .await
        .unwrap();
    assert_eq!(trades["AAPL"][0].price, 158.65);
}

/// The multi-symbol latest quote. A different route from the single-symbol
/// `/stocks/AAPL/quotes/latest` below, and the only one of the two that answers
/// with a map — reaching the single-symbol path with a list of symbols returns
/// one arbitrary symbol's quote rather than an error.
#[tokio::test]
async fn the_latest_stock_quote_is_keyed_by_symbol() {
    let server = serving(
        "/v2/stocks/quotes/latest",
        json!({"quotes": {"AAPL": quote()}}),
    )
    .await;

    let quotes = stocks(&server)
        .get_stock_latest_quote(&StockLatestRequest::new("AAPL"))
        .await
        .unwrap();

    assert_eq!(quotes["AAPL"].ask_price, 158.65);
    assert_eq!(quotes["AAPL"].symbol, "AAPL");
}

#[tokio::test]
async fn the_latest_stock_bar_is_keyed_by_symbol() {
    let server = serving("/v2/stocks/bars/latest", json!({"bars": {"AAPL": bar()}})).await;

    let bars = stocks(&server)
        .get_stock_latest_bar(&StockLatestRequest::new("AAPL"))
        .await
        .unwrap();

    assert_eq!(bars["AAPL"].close, 1.5);
    assert_eq!(bars["AAPL"].symbol, "AAPL");
}

/// The single-symbol routes put the symbol in the path and answer with a bare
/// list or object — no per-symbol map to unwrap.
#[tokio::test]
async fn the_single_symbol_routes_put_the_symbol_in_the_path() {
    let server = serving(
        "/v2/stocks/AAPL/quotes",
        json!({"quotes": [quote()], "symbol": "AAPL"}),
    )
    .await;
    let quotes = stocks(&server)
        .get_stock_quotes_for_symbol("AAPL", &SingleSymbolRequest::new())
        .await
        .unwrap();
    assert_eq!(quotes.len(), 1);

    let server = serving(
        "/v2/stocks/AAPL/trades",
        json!({"trades": [trade()], "symbol": "AAPL"}),
    )
    .await;
    assert_eq!(
        stocks(&server)
            .get_stock_trades_for_symbol("AAPL", &SingleSymbolRequest::new())
            .await
            .unwrap()
            .len(),
        1
    );

    let server = serving(
        "/v2/stocks/AAPL/auctions",
        json!({"auctions": [{"d": "2024-04-26", "o": [], "c": []}], "symbol": "AAPL"}),
    )
    .await;
    assert_eq!(
        stocks(&server)
            .get_stock_auctions_for_symbol("AAPL", &SingleSymbolRequest::new())
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn the_single_symbol_latest_routes_answer_with_one_record() {
    let server = serving(
        "/v2/stocks/AAPL/bars/latest",
        json!({"bar": bar(), "symbol": "AAPL"}),
    )
    .await;
    assert_eq!(
        stocks(&server)
            .get_stock_latest_bar_for_symbol("AAPL", &SingleSymbolRequest::new())
            .await
            .unwrap()
            .close,
        1.5
    );

    let server = serving(
        "/v2/stocks/AAPL/quotes/latest",
        json!({"quote": quote(), "symbol": "AAPL"}),
    )
    .await;
    assert_eq!(
        stocks(&server)
            .get_stock_latest_quote_for_symbol("AAPL", &SingleSymbolRequest::new())
            .await
            .unwrap()
            .ask_price,
        158.65
    );
}

// ------------------------------------------------------------------ crypto

/// Crypto puts the feed in the path — `/crypto/us/quotes` — so a client built
/// for one feed and called with another silently reads a different market.
#[tokio::test]
async fn the_crypto_routes_put_the_feed_in_the_path() {
    let server = serving(
        "/v1beta3/crypto/us/quotes",
        json!({"quotes": {"BTC/USD": [quote()]}}),
    )
    .await;
    let quotes = crypto(&server)
        .get_crypto_quotes(&TimeseriesRequest::new("BTC/USD"), CryptoFeed::Us)
        .await
        .unwrap();
    assert_eq!(quotes["BTC/USD"].len(), 1);

    let server = serving(
        "/v1beta3/crypto/us/trades",
        json!({"trades": {"BTC/USD": [trade()]}}),
    )
    .await;
    assert_eq!(
        crypto(&server)
            .get_crypto_trades(&TimeseriesRequest::new("BTC/USD"), CryptoFeed::Us)
            .await
            .unwrap()["BTC/USD"]
            .len(),
        1
    );
}

/// The crypto latest routes read `/latest/bars`, not `/bars/latest` — the stock
/// API's ordering, reversed.
#[tokio::test]
async fn the_crypto_latest_routes_reverse_the_stock_path_order() {
    let server = serving(
        "/v1beta3/crypto/us/latest/bars",
        json!({"bars": {"BTC/USD": bar()}}),
    )
    .await;
    assert_eq!(
        crypto(&server)
            .get_crypto_latest_bar(&CryptoLatestRequest::new("BTC/USD"), CryptoFeed::Us)
            .await
            .unwrap()["BTC/USD"]
            .close,
        1.5
    );

    let server = serving(
        "/v1beta3/crypto/us/latest/orderbooks",
        json!({"orderbooks": {"BTC/USD": {
            "t": "2022-03-09T09:00:00Z",
            "b": [{"p": 1.0, "s": 2.0}],
            "a": [{"p": 1.5, "s": 3.0}]
        }}}),
    )
    .await;
    let books = crypto(&server)
        .get_crypto_latest_orderbook(&CryptoLatestRequest::new("BTC/USD"), CryptoFeed::Us)
        .await
        .unwrap();
    assert_eq!(books["BTC/USD"].bids.len(), 1);

    let server = serving(
        "/v1beta3/crypto/us/latest/quotes",
        json!({"quotes": {"BTC/USD": quote()}}),
    )
    .await;
    assert_eq!(
        crypto(&server)
            .get_crypto_latest_quote(&CryptoLatestRequest::new("BTC/USD"), CryptoFeed::Us)
            .await
            .unwrap()["BTC/USD"]
            .ask_price,
        158.65
    );
}

#[tokio::test]
async fn a_crypto_snapshot_decodes_its_nested_records() {
    let server = serving(
        "/v1beta3/crypto/us/snapshots",
        json!({"snapshots": {"BTC/USD": {
            "latestTrade": trade(),
            "latestQuote": quote(),
            "dailyBar": bar()
        }}}),
    )
    .await;

    let snapshots = crypto(&server)
        .get_crypto_snapshot(&CryptoSnapshotRequest::new("BTC/USD"), CryptoFeed::Us)
        .await
        .unwrap();

    assert!(snapshots["BTC/USD"].daily_bar.is_some());
    assert!(snapshots["BTC/USD"].latest_quote.is_some());
}

// ------------------------------------------------- corporate action events

/// The push twin of `/v1/corporate-actions`, on a version segment of its own.
///
/// `get_corporate_action_events` writes `v1beta1` into the URL directly instead
/// of taking it from the client, so this asserts the segment against a client
/// configured for `v1` — which is what a caller building one with `new` gets.
/// Reading the configured version here instead would send the stream to
/// `/v1/events/corporate-actions`, a 404 with nothing else to show for it.
#[tokio::test]
async fn the_corporate_action_event_stream_is_v1beta1_on_a_v1_client() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1beta1/events/corporate-actions"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string("event: cash_dividend\ndata: {\"symbol\":\"AAPL\"}\n\n"),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client =
        CorporateActionsClient::with_config(&credentials(), config(&server, "v1")).unwrap();
    let events: Vec<_> = client
        .get_corporate_action_events(None)
        .await
        .unwrap()
        .collect()
        .await;

    assert_eq!(events.len(), 1);
    // The payload is JSON the caller types itself: the envelope's event name
    // selects which of fifteen shapes it takes and none of them are modelled.
    assert_eq!(events[0].as_ref().unwrap().name, "cash_dividend");
}

// ----------------------------------------------------------------- options

#[tokio::test]
async fn the_option_routes_live_on_v1beta1() {
    let server = serving(
        "/v1beta1/options/trades",
        json!({"trades": {"AAPL240119C00150000": [trade()]}}),
    )
    .await;
    assert_eq!(
        options(&server)
            .get_option_trades(&TimeseriesRequest::new("AAPL240119C00150000"))
            .await
            .unwrap()["AAPL240119C00150000"]
            .len(),
        1
    );

    let server = serving(
        "/v1beta1/options/quotes/latest",
        json!({"quotes": {"AAPL240119C00150000": quote()}}),
    )
    .await;
    assert_eq!(
        options(&server)
            .get_option_latest_quote(&OptionLatestRequest::new("AAPL240119C00150000"))
            .await
            .unwrap()
            .len(),
        1
    );

    let server = serving(
        "/v1beta1/options/trades/latest",
        json!({"trades": {"AAPL240119C00150000": trade()}}),
    )
    .await;
    assert_eq!(
        options(&server)
            .get_option_latest_trade(&OptionLatestRequest::new("AAPL240119C00150000"))
            .await
            .unwrap()
            .len(),
        1
    );
}
