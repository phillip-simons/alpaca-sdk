//! The market data routes no SDK ported: auctions, the single-symbol variants,
//! the `meta` decoder rings, forex and logos.
//!
//! Every payload here is real. The auction, exchange and condition fixtures come
//! from `just capture` against the live API (`fixtures/live/`) or from the Go
//! SDK's tests (`fixtures/go/`). Forex and logos have neither — both answer 403
//! on a plan that reaches SIP — so those two tests assert the request this crate
//! sends and the shape the published reference documents, and say so.

#![cfg(feature = "data")]

use alpaca_sdk::data::{
    Codes, DataFeed, ForexDataClient, ForexLatestRatesRequest, ForexRatesRequest, LogoClient,
    LogoRequest, OptionHistoricalDataClient, SingleSymbolRequest, StockAuctionsRequest,
    StockHistoricalDataClient, Tape, TickType,
};
use alpaca_sdk::{Credentials, RestConfig, RetryConfig};
use serde_json::json;
use wiremock::matchers::{method, path, query_param, query_param_is_missing};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn fixture(name: &str) -> serde_json::Value {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name);
    let body = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    serde_json::from_str(&body).unwrap()
}

fn config(server: &MockServer, version: &str) -> RestConfig {
    RestConfig::new(server.uri())
        .api_version(version)
        .retry(RetryConfig::none())
}

fn credentials() -> Credentials {
    Credentials::new("key", "secret").unwrap()
}

fn stock_client(server: &MockServer) -> StockHistoricalDataClient {
    StockHistoricalDataClient::with_config(&credentials(), config(server, "v2")).unwrap()
}

// --------------------------------------------------------------- auctions

#[tokio::test]
async fn auctions_deserialize_from_the_go_sdk_payload() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/stocks/auctions"))
        .and(query_param("symbols", "AAPL,IBM,TSLA"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(fixture("go/marketdata__test_get_multi_auctions__01.json")),
        )
        // The captured page carries a `next_page_token`, so the walk asks for a
        // second page and gets the same token back. Two requests, then the
        // repeated-token guard stops it — a walk that trusted the token would
        // run until it ran out of memory.
        .expect(2)
        .mount(&server)
        .await;

    let request = StockAuctionsRequest::new(["AAPL", "IBM", "TSLA"]).feed(DataFeed::Sip);
    let auctions = stock_client(&server)
        .get_stock_auctions(&request)
        .await
        .unwrap();

    let apple = &auctions["AAPL"];
    assert_eq!(apple[0].symbol, "AAPL", "filled in from the response key");
    assert_eq!(apple[0].date, "2022-10-17".parse().unwrap());

    // Opening and closing prints are separate lists, and a print carries one
    // condition code rather than the list a trade carries.
    let close = &apple[0].closing[0];
    assert_eq!(close.condition, "M");
    assert_eq!(close.exchange, "P");
    assert_eq!(close.price, 142.4);
    assert_eq!(close.size, Some(100.0));
    assert!(!apple[0].opening.is_empty());
}

#[tokio::test]
async fn auctions_deserialize_from_the_live_capture() {
    // A second, independent payload: the Go fixture is four years old, and this
    // one came off the live API in 2026. Both parse through the same model.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/stocks/auctions"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(fixture("live/stocks_auctions.json")),
        )
        .expect(1)
        .mount(&server)
        .await;

    let auctions = stock_client(&server)
        .get_stock_auctions(&StockAuctionsRequest::new("AAPL"))
        .await
        .unwrap();

    assert!(!auctions["AAPL"][0].closing.is_empty());
}

// ------------------------------------------------------- single symbol

#[tokio::test]
async fn the_single_symbol_route_returns_a_bare_list_with_the_symbol_beside_it() {
    // Not an alias of the multi-symbol route: the payload has no map keyed by
    // symbol, so the symbol has to be filled in from the path instead.
    //
    // Captured live by `just capture` — no SDK's tests cover these routes, so
    // there was no other way to get a real one.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/stocks/AAPL/bars"))
        .and(query_param_is_missing("symbols"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(fixture("live/stocks_bars_single.json")),
        )
        .expect(1)
        .mount(&server)
        .await;

    let bars = stock_client(&server)
        .get_stock_bars_for_symbol("AAPL", &SingleSymbolRequest::new())
        .await
        .unwrap();

    assert_eq!(bars.len(), 2);
    assert_eq!(bars[0].symbol, "AAPL");
    assert_eq!(bars[0].close, 308.26);
}

#[tokio::test]
async fn the_single_symbol_latest_route_returns_one_record_under_a_singular_key() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/stocks/AAPL/trades/latest"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(fixture("live/stocks_latest_trade_single.json")),
        )
        .expect(1)
        .mount(&server)
        .await;

    let trade = stock_client(&server)
        .get_stock_latest_trade_for_symbol("AAPL", &SingleSymbolRequest::new())
        .await
        .unwrap();

    // The record is under a *singular* key with the symbol beside it, so the
    // symbol comes from the path rather than from a map key.
    assert_eq!(trade.symbol, "AAPL");
    assert_eq!(trade.price, 301.3);
    assert_eq!(trade.conditions.as_deref().map(<[_]>::len), Some(3));
}

#[tokio::test]
async fn the_single_symbol_snapshot_has_no_wrapping_key_at_all() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/stocks/AAPL/snapshot"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(fixture("live/stocks_snapshot_single.json")),
        )
        .expect(1)
        .mount(&server)
        .await;

    let snapshot = stock_client(&server)
        .get_stock_snapshot_for_symbol("AAPL", &SingleSymbolRequest::new())
        .await
        .unwrap();

    assert_eq!(snapshot.symbol, "AAPL");
    // The symbol propagates into the nested records too, as it does for the
    // multi-symbol snapshot.
    assert_eq!(snapshot.latest_trade.unwrap().symbol, "AAPL");
    assert_eq!(snapshot.daily_bar.unwrap().symbol, "AAPL");
}

// ------------------------------------------------------------------ meta

#[tokio::test]
async fn exchange_codes_come_back_as_a_lookup_table() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/stocks/meta/exchanges"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(fixture("live/stocks_meta_exchanges.json")),
        )
        .expect(1)
        .mount(&server)
        .await;

    let codes = stock_client(&server)
        .get_stock_exchange_codes()
        .await
        .unwrap();

    assert_eq!(codes.name("V"), Some("IEX"));
    assert_eq!(codes.name("N"), Some("New York Stock Exchange"));
    assert_eq!(codes.name("!"), None);
}

#[tokio::test]
async fn stock_condition_codes_require_a_tape() {
    // The route answers 400 without one. Nothing in the OpenAPI spec's gap list
    // hints at the asymmetry with the option route, and a port written from the
    // spec alone would have shipped something that always fails.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/stocks/meta/conditions/trade"))
        .and(query_param("tape", "A"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(fixture("live/stocks_meta_conditions_trade.json")),
        )
        .expect(1)
        .mount(&server)
        .await;

    let codes = stock_client(&server)
        .get_stock_condition_codes(&TickType::Trade, Tape::A)
        .await
        .unwrap();

    // The whole reason `Codes` is not a bare map.
    assert_eq!(codes.name(" "), Some("Regular Sale"));
    assert_eq!(codes.name("I"), Some("Odd Lot Trade"));
}

#[tokio::test]
async fn option_condition_codes_take_no_tape() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1beta1/options/meta/conditions/trade"))
        .and(query_param_is_missing("tape"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(fixture("live/options_meta_conditions_trade.json")),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client =
        OptionHistoricalDataClient::with_config(&credentials(), config(&server, "v1beta1"))
            .unwrap();
    let codes = client
        .get_option_condition_codes(&TickType::Trade)
        .await
        .unwrap();

    assert!(!codes.is_empty());
}

#[test]
fn the_condition_lookup_does_not_trim() {
    let codes: Codes = serde_json::from_value(fixture("live/stocks_meta_conditions_trade.json"))
        .expect("the captured table deserializes");

    assert_eq!(codes.name(" "), Some("Regular Sale"));
    // Trimming, splitting on whitespace, or treating "" as absent all lose the
    // single most common condition on the tape.
    assert_eq!(codes.name(""), None);
}

// ----------------------------------------------------------------- forex

#[tokio::test]
async fn forex_rates_follow_the_reference_shape() {
    // Unverifiable against a real response: the route answers
    // 403 `forbidden: insufficient grants` on a paid plan that reaches SIP, so
    // forex is a separate entitlement. This payload is the reference page's own
    // example, and is marked as such rather than passed off as captured.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1beta1/forex/rates"))
        .and(query_param("currency_pairs", "USDJPY"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "next_page_token": null,
            "rates": {
                "USDJPY": [
                    {"ap": 115.18, "bp": 114.192, "mp": 115.144, "t": "2022-01-03T00:01:00Z"},
                    {"ap": 115.185, "bp": 114.189, "mp": 115.138, "t": "2022-01-03T00:02:00Z"},
                ],
            },
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = ForexDataClient::with_config(&credentials(), config(&server, "v1beta1")).unwrap();
    let rates = client
        .get_forex_rates(&ForexRatesRequest::new("USDJPY"))
        .await
        .unwrap();

    let usdjpy = &rates["USDJPY"];
    assert_eq!(usdjpy.len(), 2);
    assert_eq!(usdjpy[0].currency_pair, "USDJPY");
    assert_eq!(usdjpy[0].bid_price, 114.192);
    assert_eq!(usdjpy[0].mid_price, 115.144);
    assert_eq!(usdjpy[0].ask_price, 115.18);
}

#[tokio::test]
async fn latest_forex_rates_send_no_limit() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1beta1/forex/latest/rates"))
        // The latest endpoints reject a `limit`, so the pagination loop must not
        // add one.
        .and(query_param_is_missing("limit"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "rates": {"USDJPY": {"ap": 115.18, "bp": 114.192, "mp": 115.144,
                                 "t": "2022-01-03T00:01:00Z"}},
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = ForexDataClient::with_config(&credentials(), config(&server, "v1beta1")).unwrap();
    let rates = client
        .get_forex_latest_rates(&ForexLatestRatesRequest::new("USDJPY"))
        .await
        .unwrap();

    assert_eq!(rates["USDJPY"].currency_pair, "USDJPY");
}

// ----------------------------------------------------------------- logos

#[tokio::test]
async fn a_logo_comes_back_as_bytes_rather_than_json() {
    // The one route in the crate that does not answer with JSON. Putting it
    // through the usual path would try to parse a PNG.
    const PNG_HEADER: &[u8] = b"\x89PNG\r\n\x1a\n";

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1beta1/logos/AAPL"))
        .and(query_param("placeholder", "false"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(PNG_HEADER.to_vec(), "image/png"))
        .expect(1)
        .mount(&server)
        .await;

    let client = LogoClient::with_config(&credentials(), config(&server, "v1beta1")).unwrap();
    let logo = client
        .get_logo("AAPL", &LogoRequest::new().placeholder(false))
        .await
        .unwrap();

    assert_eq!(logo, PNG_HEADER);
}
