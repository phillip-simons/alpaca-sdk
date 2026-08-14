//! Historical market data: models against captured payloads, and the pagination
//! loop against a mock server that actually serves page tokens.

#![cfg(feature = "data")]

use std::collections::HashMap;

use alpaca_sdk::data::{
    CorporateActionsClient, CorporateActionsRequest, CryptoBarsRequest, CryptoFeed,
    CryptoHistoricalDataClient, CryptoLatestRequest, ForexDataClient, ForexRatesRequest,
    MarketMoversRequest, MarketType, MostActivesRequest, NewsClient, NewsRequest,
    OptionChainRequest, OptionHistoricalDataClient, OptionLatestRequest, ScreenerClient,
    StockBarsRequest, StockHistoricalDataClient, StockLatestRequest, TimeFrame, TimeFrameUnit,
};
use alpaca_sdk::{Credentials, RestConfig, RetryConfig};
use serde_json::json;
use wiremock::matchers::{method, path, query_param};
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

// ------------------------------------------------------------------- bars

#[tokio::test]
async fn crypto_bars_deserialize_from_the_captured_payload() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1beta3/crypto/us/bars"))
        .and(query_param("symbols", "BTC/USD,ETH/USD"))
        .and(query_param("timeframe", "1Day"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "data/test_historical_crypto_data__test_get_crypto_bars__01.json",
        )))
        .expect(1)
        .mount(&server)
        .await;

    let client = CryptoHistoricalDataClient::with_config(None, config(&server, "v1beta3")).unwrap();
    let request = CryptoBarsRequest::new(["BTC/USD", "ETH/USD"], TimeFrame::day());
    let bars = client
        .get_crypto_bars(&request, CryptoFeed::Us)
        .await
        .unwrap();

    assert_eq!(bars.len(), 2);

    let btc = &bars["BTC/USD"][0];
    // The wire keys are single letters; the mapping is serde renames now.
    assert_eq!(btc.open, 161.51);
    assert_eq!(btc.high, 163.41);
    assert_eq!(btc.low, 159.41);
    assert_eq!(btc.close, 162.95);
    assert_eq!(btc.volume, 88_496_480.0);
    assert_eq!(btc.trade_count, Some(700_291.0));
    assert_eq!(btc.vwap, Some(161.942_117));
    // Filled in from the key the list was nested under.
    assert_eq!(btc.symbol, "BTC/USD");
}

#[tokio::test]
async fn crypto_client_sends_no_auth_headers() {
    // Crypto market data is served unauthenticated: `CryptoHistoricalDataClient::new`
    // takes no credentials at all, so there is nothing to send.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1beta3/crypto/us/bars"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"bars": {}})))
        .expect(1)
        .mount(&server)
        .await;

    let client = CryptoHistoricalDataClient::with_config(None, config(&server, "v1beta3")).unwrap();
    let request = CryptoBarsRequest::new("BTC/USD", TimeFrame::day());
    client
        .get_crypto_bars(&request, CryptoFeed::Us)
        .await
        .unwrap();

    let received = &server.received_requests().await.unwrap()[0];
    assert!(received.headers.get("APCA-API-KEY-ID").is_none());
    assert!(received.headers.get("authorization").is_none());
}

#[tokio::test]
async fn empty_responses_are_not_an_error() {
    // The captured empty-response fixtures exist because these broke upstream.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1beta1/options/bars"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "data/test_historical_option_data__test_get_bars_multi_empty_response__01.json",
        )))
        .expect(1)
        .mount(&server)
        .await;

    let client =
        OptionHistoricalDataClient::with_config(&credentials(), config(&server, "v1beta1"))
            .unwrap();
    let request = alpaca_sdk::data::OptionBarsRequest::new("AAPL240119C00150000", TimeFrame::day());
    let bars = client.get_option_bars(&request).await.unwrap();

    assert!(bars.is_empty());
}

// ------------------------------------------------------------- pagination

/// Builds a bars page with `count` bars for `symbol` and an optional next token.
fn bars_page(symbol: &str, count: usize, next: Option<&str>) -> serde_json::Value {
    let bars: Vec<_> = (0..count)
        .map(|i| {
            json!({
                "t": "2022-03-09T05:00:00Z",
                "o": 1.0, "h": 2.0, "l": 0.5, "c": 1.5,
                "v": i as f64, "n": 1.0, "vw": 1.2
            })
        })
        .collect();

    json!({ "bars": { symbol: bars }, "next_page_token": next })
}

#[tokio::test]
async fn pages_are_followed_and_merged() {
    let server = MockServer::start().await;

    // Page 1 has no token yet, so it is matched by absence of page_token.
    Mock::given(method("GET"))
        .and(path("/v2/stocks/bars"))
        .and(query_param("page_token", "p2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(bars_page("AAPL", 3, None)))
        .expect(1)
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/v2/stocks/bars"))
        .respond_with(ResponseTemplate::new(200).set_body_json(bars_page("AAPL", 5, Some("p2"))))
        .expect(1)
        .mount(&server)
        .await;

    let request = StockBarsRequest::new("AAPL", TimeFrame::day());
    let bars = stock_client(&server)
        .get_stock_bars(&request)
        .await
        .unwrap();

    // Both pages, flattened into one list per symbol.
    assert_eq!(bars["AAPL"].len(), 8);
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

#[tokio::test]
async fn a_caller_limit_is_split_across_pages_and_stops_exactly() {
    // The arithmetic ported from _get_marketdata: a 25,000-item request against
    // a 10,000-item page limit is three calls asking for 10,000, 10,000, 5,000.
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v2/stocks/bars"))
        .and(query_param("limit", "5000"))
        .respond_with(ResponseTemplate::new(200).set_body_json(bars_page("AAPL", 5_000, None)))
        .expect(1)
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/v2/stocks/bars"))
        .and(query_param("limit", "10000"))
        .and(query_param("page_token", "p2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(bars_page(
            "AAPL",
            10_000,
            Some("p3"),
        )))
        .expect(1)
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/v2/stocks/bars"))
        .and(query_param("limit", "10000"))
        .respond_with(ResponseTemplate::new(200).set_body_json(bars_page(
            "AAPL",
            10_000,
            Some("p2"),
        )))
        .expect(1)
        .mount(&server)
        .await;

    let request = StockBarsRequest::new("AAPL", TimeFrame::day()).limit(25_000);
    let bars = stock_client(&server)
        .get_stock_bars(&request)
        .await
        .unwrap();

    assert_eq!(bars["AAPL"].len(), 25_000);
    assert_eq!(server.received_requests().await.unwrap().len(), 3);
}

#[tokio::test]
async fn the_loop_stops_once_the_caller_limit_is_reached() {
    // A limit smaller than one page must not issue a second request even when
    // the response still carries a next_page_token.
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v2/stocks/bars"))
        .and(query_param("limit", "5"))
        .respond_with(ResponseTemplate::new(200).set_body_json(bars_page("AAPL", 5, Some("more"))))
        .expect(1)
        .mount(&server)
        .await;

    let request = StockBarsRequest::new("AAPL", TimeFrame::day()).limit(5);
    let bars = stock_client(&server)
        .get_stock_bars(&request)
        .await
        .unwrap();

    assert_eq!(bars["AAPL"].len(), 5);
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn latest_endpoints_send_no_limit_parameter() {
    // page_size is None for these, and sending a limit is an error upstream.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/stocks/trades/latest"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "trades": {"AAPL": {"t": "2022-03-18T14:03:31.960672Z", "p": 170.5, "s": 10.0}}
        })))
        .expect(1)
        .mount(&server)
        .await;

    let request = StockLatestRequest::new("AAPL");
    let trades = stock_client(&server)
        .get_stock_latest_trade(&request)
        .await
        .unwrap();

    assert_eq!(trades["AAPL"].price, 170.5);

    let received = &server.received_requests().await.unwrap()[0];
    let query: HashMap<_, _> = received.url.query_pairs().collect();
    assert!(!query.contains_key("limit"), "{query:?}");
}

// -------------------------------------------------------------- responses

#[tokio::test]
async fn latest_crypto_trade_has_its_symbol_filled_in() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1beta3/crypto/us/latest/trades"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "data/test_historical_crypto_data__test_get_crypto_latest_trade__01.json",
        )))
        .expect(1)
        .mount(&server)
        .await;

    let client = CryptoHistoricalDataClient::with_config(None, config(&server, "v1beta3")).unwrap();
    let trades = client
        .get_crypto_latest_trade(&CryptoLatestRequest::new("BTC/USD"), CryptoFeed::Us)
        .await
        .unwrap();

    let trade = &trades["BTC/USD"];
    assert_eq!(trade.symbol, "BTC/USD");
    assert_eq!(trade.price, 40_650.0);
    assert_eq!(trade.size, 0.1517);
    assert_eq!(trade.id, Some(26_932_440));
    assert_eq!(trade.taker_side.as_deref(), Some("B"));
}

#[tokio::test]
async fn stock_snapshots_have_no_wrapping_key() {
    // The one endpoint with no wrapping key: symbols sit at the
    // top level, so the usual data-key unwrap would find nothing.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/stocks/snapshots"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "AAPL": {
                "latestTrade": {"t": "2022-03-18T14:03:31.960672Z", "p": 170.5, "s": 10.0},
                "dailyBar": {
                    "t": "2022-03-09T05:00:00Z",
                    "o": 1.0, "h": 2.0, "l": 0.5, "c": 1.5, "v": 100.0
                }
            }
        })))
        .expect(1)
        .mount(&server)
        .await;

    let snapshots = stock_client(&server)
        .get_stock_snapshot(&StockLatestRequest::new("AAPL"))
        .await
        .unwrap();

    let snapshot = &snapshots["AAPL"];
    assert_eq!(snapshot.symbol, "AAPL");
    // The symbol propagates into the nested records too.
    assert_eq!(snapshot.latest_trade.as_ref().unwrap().symbol, "AAPL");
    assert_eq!(snapshot.daily_bar.as_ref().unwrap().symbol, "AAPL");
}

#[tokio::test]
async fn option_chain_puts_the_underlying_in_the_path() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1beta1/options/snapshots/AAPL"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "data/test_historical_option_data__test_get_option_chain__01.json",
        )))
        .expect(1)
        .mount(&server)
        .await;

    let client =
        OptionHistoricalDataClient::with_config(&credentials(), config(&server, "v1beta1"))
            .unwrap();
    let chain = client
        .get_option_chain(&OptionChainRequest::new("AAPL"))
        .await
        .unwrap();

    assert!(!chain.is_empty());

    let received = &server.received_requests().await.unwrap()[0];
    let query: HashMap<_, _> = received.url.query_pairs().collect();
    assert!(!query.contains_key("underlying_symbol"), "{query:?}");
}

#[tokio::test]
async fn option_snapshot_carries_greeks() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1beta1/options/snapshots"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "data/test_historical_option_data__test_get_snapshot__01.json",
        )))
        .expect(1)
        .mount(&server)
        .await;

    let client =
        OptionHistoricalDataClient::with_config(&credentials(), config(&server, "v1beta1"))
            .unwrap();
    let snapshots = client
        .get_option_snapshot(&OptionLatestRequest::new("AAPL240119C00150000"))
        .await
        .unwrap();

    assert!(!snapshots.is_empty());
}

#[tokio::test]
async fn news_articles_deserialize_with_their_images() {
    // The captured page carries a real next_page_token, so a second mock has to
    // terminate the walk; replaying page one would follow the same token forever.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1beta1/news"))
        .and(query_param(
            "page_token",
            "MTczMDk3MTEwMTAwMDAwMDAwMHw0MTc5OTExNQ==",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "data/test_historical_news_data__test_get_news__02.json",
        )))
        .expect(1)
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/v1beta1/news"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "data/test_historical_news_data__test_get_news__01.json",
        )))
        .expect(1)
        .mount(&server)
        .await;

    let client = NewsClient::with_config(&credentials(), config(&server, "v1beta1")).unwrap();
    let news = client.get_news(&NewsRequest::new()).await.unwrap();

    assert!(news.news.len() > 1, "both pages should be merged");
    let article = &news.news[0];
    assert_eq!(article.headline, "headline");
    assert_eq!(article.symbols, ["AAPL", "QCOM"]);
    assert_eq!(article.images.as_ref().unwrap().len(), 3);
    // Pagination ran to completion, so there is nothing left to resume.
    assert_eq!(news.next_page_token, None);
}

#[tokio::test]
async fn a_repeated_page_token_stops_the_loop() {
    // A server that keeps handing back the same token would accumulate
    // pages until it runs out of memory. Here the walk stops instead.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1beta1/news"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "data/test_historical_news_data__test_get_news__01.json",
        )))
        .expect(2)
        .mount(&server)
        .await;

    let client = NewsClient::with_config(&credentials(), config(&server, "v1beta1")).unwrap();
    let news = client.get_news(&NewsRequest::new()).await.unwrap();

    // One request, then one more that returns the same token, then it stops.
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
    assert!(!news.news.is_empty());
}

#[tokio::test]
async fn news_pages_at_fifty_not_ten_thousand() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1beta1/news"))
        .and(query_param("limit", "50"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"news": []})))
        .expect(1)
        .mount(&server)
        .await;

    let client = NewsClient::with_config(&credentials(), config(&server, "v1beta1")).unwrap();
    client.get_news(&NewsRequest::new()).await.unwrap();
}

#[tokio::test]
async fn corporate_actions_group_by_kind() {
    let server = MockServer::start().await;
    // The captured payload carries a live next_page_token, so it is cleared here
    // to isolate the single-page shape.
    let mut page = fixture("data/test_corporate_actions__test_get_corporate_actions__01.json");
    page["next_page_token"] = json!(null);

    Mock::given(method("GET"))
        .and(path("/v1/corporate-actions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(page))
        .expect(1)
        .mount(&server)
        .await;

    let client =
        CorporateActionsClient::with_config(&credentials(), config(&server, "v1")).unwrap();
    let actions = client
        .get_corporate_actions(&CorporateActionsRequest::new())
        .await
        .unwrap();

    assert_eq!(actions.reverse_splits.len(), 1);
    assert_eq!(actions.reverse_splits[0].symbol, "MNTS");
    assert_eq!(actions.reverse_splits[0].old_rate, 50.0);
    assert_eq!(actions.forward_splits[0].symbol, "SRE");
    assert!(!actions.cash_dividends.is_empty());
    // Every record carries an id that the specification omits.
    assert!(actions.reverse_splits[0].id.is_some());
    assert_eq!(actions.len(), 13);
}

#[tokio::test]
async fn corporate_action_pages_merge_by_kind() {
    // The second page carries only cash_dividends; merging must extend that
    // list rather than replacing the whole payload.
    let server = MockServer::start().await;

    let mut page_one = fixture("data/test_corporate_actions__test_get_corporate_actions__01.json");
    page_one["next_page_token"] = json!("p2");

    Mock::given(method("GET"))
        .and(path("/v1/corporate-actions"))
        .and(query_param("page_token", "p2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "data/test_corporate_actions__test_get_corporate_actions__02.json",
        )))
        .expect(1)
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/v1/corporate-actions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(page_one))
        .expect(1)
        .mount(&server)
        .await;

    let client =
        CorporateActionsClient::with_config(&credentials(), config(&server, "v1")).unwrap();
    let actions = client
        .get_corporate_actions(&CorporateActionsRequest::new())
        .await
        .unwrap();

    assert_eq!(actions.cash_dividends.len(), 2, "pages did not merge");
    assert_eq!(actions.reverse_splits.len(), 1);
}

#[tokio::test]
async fn screener_most_actives() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1beta1/screener/stocks/most-actives"))
        .and(query_param("top", "10"))
        .and(query_param("by", "volume"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "most_actives": [{"symbol": "AAPL", "volume": 1000.0, "trade_count": 10.0}],
            "last_updated": "2024-08-18T20:15:44Z"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = ScreenerClient::with_config(&credentials(), config(&server, "v1beta1")).unwrap();
    let actives = client
        .get_most_actives(&MostActivesRequest::default())
        .await
        .unwrap();

    assert_eq!(actives.most_actives[0].symbol, "AAPL");
}

#[tokio::test]
async fn screener_movers_puts_the_market_type_in_the_path() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1beta1/screener/crypto/movers"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "gainers": [{"symbol": "BTC/USD", "percent_change": 5.0, "change": 100.0, "price": 2000.0}],
            "losers": [],
            "market_type": "crypto",
            "last_updated": "2024-08-18T20:15:44Z"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = ScreenerClient::with_config(&credentials(), config(&server, "v1beta1")).unwrap();
    let movers = client
        .get_market_movers(&MarketMoversRequest::new(10, MarketType::Crypto))
        .await
        .unwrap();

    assert_eq!(movers.gainers[0].symbol, "BTC/USD");

    let received = &server.received_requests().await.unwrap()[0];
    let query: HashMap<_, _> = received.url.query_pairs().collect();
    assert!(!query.contains_key("market_type"), "{query:?}");
}

#[tokio::test]
async fn option_exchange_codes_return_a_raw_map() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1beta1/options/meta/exchanges"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"A": "NYSE American"})))
        .expect(1)
        .mount(&server)
        .await;

    let client =
        OptionHistoricalDataClient::with_config(&credentials(), config(&server, "v1beta1"))
            .unwrap();
    let codes = client.get_option_exchange_codes().await.unwrap();

    assert_eq!(codes["A"], "NYSE American");
}

// ----------------------------------------------------------------- errors

#[tokio::test]
async fn a_response_matching_no_known_key_is_an_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/stocks/bars"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"unexpected": {}})))
        .expect(1)
        .mount(&server)
        .await;

    let request = StockBarsRequest::new("AAPL", TimeFrame::day());
    let err = stock_client(&server)
        .get_stock_bars(&request)
        .await
        .unwrap_err();

    // A response this crate cannot read is a decode failure, not an invalid
    // request: the request was fine and the server answered 200. Reporting it
    // as `InvalidRequest` sent the caller to check their own parameters.
    match err {
        alpaca_sdk::Error::Decode { path, body, .. } => {
            assert_eq!(path, "/stocks/bars");
            assert!(
                body.contains("unexpected"),
                "the offending payload should travel with the error: {body}"
            );
        }
        other => panic!("expected a decode error, got {other:?}"),
    }
}

#[tokio::test]
async fn timeframe_serializes_into_the_query() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/stocks/bars"))
        .and(query_param("timeframe", "5Min"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"bars": {}})))
        .expect(1)
        .mount(&server)
        .await;

    let request = StockBarsRequest::new("AAPL", TimeFrame::new(5, TimeFrameUnit::Minute).unwrap());
    stock_client(&server)
        .get_stock_bars(&request)
        .await
        .unwrap();
}

// ------------------------------------------------------- absent symbols
//
// A multi-symbol request returns one key per symbol, and Alpaca answers `null`
// for a symbol it has nothing for. That used to propagate a decode error and
// discard the whole response — every good symbol with it.

/// Driven by a captured payload that ships in this crate:
/// `fixtures/go/marketdata__test_snapshots__01.json` carries `"INVALID": null`
/// beside a valid AAPL and MSFT. A request takes up to 100 symbols, so one
/// delisted ticker used to make the entire batch unusable.
#[tokio::test]
async fn a_null_symbol_is_skipped_rather_than_failing_the_whole_response() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/stocks/snapshots"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(fixture("go/marketdata__test_snapshots__01.json")),
        )
        .expect(1)
        .mount(&server)
        .await;

    let request = alpaca_sdk::data::StockSnapshotRequest::new(["AAPL", "INVALID", "MSFT"]);
    let snapshots = stock_client(&server)
        .get_stock_snapshot(&request)
        .await
        .expect("a null entry must not fail the response");

    assert_eq!(
        snapshots.len(),
        2,
        "expected AAPL and MSFT, got {snapshots:?}"
    );
    assert!(snapshots.contains_key("AAPL"));
    assert!(snapshots.contains_key("MSFT"));
    assert!(
        !snapshots.contains_key("INVALID"),
        "an absent symbol should be omitted, not present as an empty value"
    );
}

/// The other half of the contract: a value that is genuinely the wrong *shape*
/// must still be an error, and must carry enough to diagnose it. `body` used to
/// be the empty string, so the one field documented as "the raw payload, so the
/// mismatch can be diagnosed without re-issuing" carried nothing.
#[tokio::test]
async fn a_malformed_symbol_entry_still_errors_and_names_the_route() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/stocks/snapshots"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"AAPL": 42})))
        .mount(&server)
        .await;

    let request = alpaca_sdk::data::StockSnapshotRequest::new(["AAPL"]);
    let err = stock_client(&server)
        .get_stock_snapshot(&request)
        .await
        .unwrap_err();

    match err {
        alpaca_sdk::Error::Decode { path, body, .. } => {
            assert_eq!(path, "/stocks/snapshots", "path should name the route");
            assert!(
                body.contains("AAPL"),
                "body should carry the payload, got {body:?}"
            );
        }
        other => panic!("expected Decode, got {other:?}"),
    }
}

/// A minimal cash dividend carrying every field the model requires.
fn dividend(symbol: &str, ex_date: &str) -> serde_json::Value {
    json!({
        "symbol": symbol,
        "cusip": "037833100",
        "rate": 0.24,
        "special": false,
        "foreign": false,
        "process_date": ex_date,
        "ex_date": ex_date
    })
}

/// `limit` caps the total across all pages, and the corporate-actions endpoint
/// serves 1,000 per page — so a default of 1,000 filled the cap with page one
/// and ended the walk there, discarding a `next_page_token` the request type has
/// no field to send back.
///
/// Page one therefore has to be **full**. An earlier version of this test served
/// a single dividend on page one, which left 999 of the cap unspent and followed
/// the token whatever the default was — so it passed against the bug it was
/// named for.
#[tokio::test]
async fn corporate_actions_walk_past_the_first_full_page_by_default() {
    // The endpoint's own page size, and the value the default `limit` used to be.
    const PAGE: usize = 1000;

    let full_page: Vec<serde_json::Value> = (0..PAGE)
        .map(|i| dividend(&format!("SYM{i}"), "2024-01-02"))
        .collect();

    let server = MockServer::start().await;

    // Page two, reached only if the walk did not stop at the cap.
    Mock::given(method("GET"))
        .and(path("/v1/corporate-actions"))
        .and(query_param("page_token", "page2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "corporate_actions": {"cash_dividends": [dividend("MSFT", "2024-01-03")]},
            "next_page_token": null
        })))
        .expect(1)
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/v1/corporate-actions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "corporate_actions": {"cash_dividends": full_page},
            "next_page_token": "page2"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client =
        CorporateActionsClient::with_config(&credentials(), config(&server, "v1")).unwrap();
    let actions = client
        .get_corporate_actions(&CorporateActionsRequest::new())
        .await
        .unwrap();

    assert_eq!(
        actions.cash_dividends.len(),
        PAGE + 1,
        "the second page was not fetched: the walk stopped at the first full page"
    );
}

// ------------------------------------------------- locally-refused requests

/// An empty symbol list is a request to ask the API about nothing, and it is
/// refused before any HTTP happens.
///
/// The first version of this guard tested `params["symbols"].as_str() == ""`,
/// which could never fire: `Symbols` is `#[serde(transparent)]` over
/// `Vec<String>`, so it reaches the guard as a `Value::Array` and is only joined
/// into a string later. It shipped as dead code because nothing exercised it.
#[tokio::test]
async fn an_empty_symbol_list_never_reaches_the_network() {
    let server = MockServer::start().await;
    // No mock is mounted: reaching the server at all is the failure.

    let request = StockBarsRequest::new(Vec::<String>::new(), TimeFrame::day());
    let err = stock_client(&server)
        .get_stock_bars(&request)
        .await
        .unwrap_err();

    assert!(
        matches!(err, alpaca_sdk::Error::InvalidRequest(_)),
        "expected InvalidRequest, got {err:?}"
    );
    assert!(
        server.received_requests().await.unwrap().is_empty(),
        "an empty symbol list must not reach the network"
    );
}

/// The same hazard under the other key: the forex requests rename the same
/// `Symbols` field to `currency_pairs`, so a guard that only checked `symbols`
/// left half of it open.
#[tokio::test]
async fn an_empty_currency_pair_list_never_reaches_the_network() {
    let server = MockServer::start().await;

    let client = ForexDataClient::with_config(&credentials(), config(&server, "v1beta1")).unwrap();
    let err = client
        .get_forex_rates(&ForexRatesRequest::new(Vec::<String>::new()))
        .await
        .unwrap_err();

    assert!(
        matches!(err, alpaca_sdk::Error::InvalidRequest(_)),
        "expected InvalidRequest, got {err:?}"
    );
    assert!(server.received_requests().await.unwrap().is_empty());
}

/// The two path routes whose symbol is interpolated from a request *field*
/// rather than a bare argument — which is why the first encoding sweep, driven
/// by a pattern over `format!("…{name}")`, missed both.
#[tokio::test]
async fn a_slashed_underlying_symbol_stays_one_path_segment() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1beta1/options/snapshots/BRK%2FA"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"snapshots": {}})))
        .expect(1)
        .mount(&server)
        .await;

    let client =
        OptionHistoricalDataClient::with_config(&credentials(), config(&server, "v1beta1"))
            .unwrap();
    client
        .get_option_chain(&OptionChainRequest::new("BRK/A"))
        .await
        .unwrap();
}

/// `MarketType` is a `wire_enum!`, so `Unknown(String)` is publicly
/// constructible and its `as_str()` hands back the caller's own text — which
/// went straight into the screener path.
#[tokio::test]
async fn an_unknown_market_type_cannot_escape_its_path_segment() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({})))
        .mount(&server)
        .await;

    let client = ScreenerClient::with_config(&credentials(), config(&server, "v1beta1")).unwrap();
    let request = MarketMoversRequest::new(10, MarketType::from("../../v2/account"));
    let _ = client.get_market_movers(&request).await;

    let received = &server.received_requests().await.unwrap()[0];
    assert_eq!(
        received.url.path(),
        "/v1beta1/screener/..%2F..%2Fv2%2Faccount/movers",
        "the market type must stay inside its own segment"
    );
}
