//! Every `TradingClient` route, checked against a mock server.
//!
//! These assert the wire contract — method, path, query string, and request body
//! — which is where a port drifts silently. A renamed parameter still compiles
//! and still deserializes; it just quietly stops filtering.

#![cfg(feature = "trading")]

use alpaca_sdk::trading::{
    AssetStatus, ClosePositionRequest, CreateWatchlistRequest, GetAssetsRequest,
    GetOptionContractsRequest, GetOrderByIdRequest, GetOrdersRequest, OrderAmount, OrderRequest,
    OrderSide, QueryOrderStatus, ReplaceOrderRequest, TimeInForce, TradingClient,
    UpdateWatchlistRequest,
};
use alpaca_sdk::types::AssetIdent;
use alpaca_sdk::{Credentials, RestConfig, RetryConfig};
use rust_decimal::Decimal;
use serde_json::json;
use uuid::Uuid;
use wiremock::matchers::{body_json, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const ORDER_ID: &str = "61e69015-8549-4bfd-b9c3-01e75843f47d";
const WATCHLIST_ID: &str = "fb306d55-2d64-4b8b-8c2a-3d0d9e0b7d47";

fn client(server: &MockServer) -> TradingClient {
    let credentials = Credentials::new("key", "secret").unwrap();
    TradingClient::with_config(
        &credentials,
        RestConfig::new(server.uri()).retry(RetryConfig::none()),
    )
    .unwrap()
}

fn fixture(name: &str) -> serde_json::Value {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name);
    let body = std::fs::read_to_string(&path).unwrap();
    serde_json::from_str(&body).unwrap()
}

/// Mounts a single expected request and returns the server.
async fn expect(http_method: &str, http_path: &str, response: serde_json::Value) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method(http_method))
        .and(path(http_path))
        .respond_with(ResponseTemplate::new(200).set_body_json(response))
        .expect(1)
        .mount(&server)
        .await;
    server
}

// ------------------------------------------------------------------ orders

#[tokio::test]
async fn submit_order_posts_the_serialized_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v2/orders"))
        .and(body_json(json!({
            "symbol": "AAPL",
            "qty": "1",
            "side": "buy",
            "type": "market",
            "time_in_force": "day",
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "trading/test_order_routes__test_market_order__01.json",
        )))
        .expect(1)
        .mount(&server)
        .await;

    let order = OrderRequest::market(
        "AAPL",
        OrderSide::Buy,
        OrderAmount::Qty(Decimal::from(1)),
        TimeInForce::Day,
    );
    let response = client(&server).submit_order(&order).await.unwrap();

    assert_eq!(response.symbol.as_deref(), Some("AAPL`"));
}

#[tokio::test]
async fn submit_order_validates_before_sending() {
    // No request should reach the server: the mock asserts zero calls on drop.
    let server = MockServer::start().await;

    let mut order = OrderRequest::market(
        "AAPL",
        OrderSide::Buy,
        OrderAmount::Qty(Decimal::from(1)),
        TimeInForce::Day,
    );
    order.order_class = Some(alpaca_sdk::trading::OrderClass::Bracket);

    let err = client(&server).submit_order(&order).await.unwrap_err();

    assert!(
        matches!(err, alpaca_sdk::Error::InvalidRequest(_)),
        "{err:?}"
    );
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn get_orders_sends_its_filters() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/orders"))
        .and(query_param("status", "open"))
        .and(query_param("limit", "50"))
        .and(query_param("nested", "true"))
        // A list becomes one comma-separated parameter, not repeated ones.
        .and(query_param("symbols", "AAPL,SPY"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .expect(1)
        .mount(&server)
        .await;

    let mut filter = GetOrdersRequest::default();
    filter.status = Some(QueryOrderStatus::Open);
    filter.limit = Some(50);
    filter.nested = Some(true);
    filter.symbols = Some(vec!["AAPL".to_owned(), "SPY".to_owned()]);

    client(&server).get_orders(Some(&filter)).await.unwrap();
}

#[tokio::test]
async fn get_orders_without_a_filter_sends_no_query() {
    let server = expect("GET", "/v2/orders", json!([])).await;

    client(&server).get_orders(None).await.unwrap();

    let request = &server.received_requests().await.unwrap()[0];
    assert_eq!(request.url.query(), None);
}

#[tokio::test]
async fn get_order_by_id_uses_the_id_in_the_path() {
    let server = expect(
        "GET",
        &format!("/v2/orders/{ORDER_ID}"),
        fixture("trading/test_order_routes__test_get_order_by_id__01.json"),
    )
    .await;

    let mut filter = GetOrderByIdRequest::default();
    filter.nested = true;
    client(&server)
        .get_order_by_id(Uuid::parse_str(ORDER_ID).unwrap(), Some(&filter))
        .await
        .unwrap();

    let request = &server.received_requests().await.unwrap()[0];
    assert_eq!(request.url.query(), Some("nested=true"));
}

#[tokio::test]
async fn get_order_by_client_id_uses_the_colon_route() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        // A colon in the path, not a sub-resource: "/orders:by_client_order_id".
        .and(path("/v2/orders:by_client_order_id"))
        .and(query_param("client_order_id", "my-order-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "trading/test_order_routes__test_get_order_by_client_id__01.json",
        )))
        .expect(1)
        .mount(&server)
        .await;

    client(&server)
        .get_order_by_client_id("my-order-1")
        .await
        .unwrap();
}

#[tokio::test]
async fn replace_order_patches() {
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path(format!("/v2/orders/{ORDER_ID}")))
        .and(body_json(json!({"qty": "2"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "trading/test_order_routes__test_replace_order__01.json",
        )))
        .expect(1)
        .mount(&server)
        .await;

    let replacement = ReplaceOrderRequest::new().qty(Decimal::from(2));
    client(&server)
        .replace_order_by_id(Uuid::parse_str(ORDER_ID).unwrap(), Some(&replacement))
        .await
        .unwrap();
}

#[tokio::test]
async fn replace_order_validates_before_sending() {
    let server = MockServer::start().await;

    let replacement = ReplaceOrderRequest::new().qty(Decimal::ZERO);
    let err = client(&server)
        .replace_order_by_id(Uuid::parse_str(ORDER_ID).unwrap(), Some(&replacement))
        .await
        .unwrap_err();

    assert!(matches!(err, alpaca_sdk::Error::InvalidRequest(_)));
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn cancel_orders_deletes_the_collection() {
    let server = expect(
        "DELETE",
        "/v2/orders",
        fixture("trading/test_order_routes__test_cancel_orders__01.json"),
    )
    .await;

    let responses = client(&server).cancel_orders().await.unwrap();

    assert!(!responses.is_empty());
    assert_eq!(responses[0].status, 200);
}

#[tokio::test]
async fn cancel_order_by_id_accepts_an_empty_response() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path(format!("/v2/orders/{ORDER_ID}")))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    client(&server)
        .cancel_order_by_id(Uuid::parse_str(ORDER_ID).unwrap())
        .await
        .unwrap();
}

// --------------------------------------------------------------- positions

#[tokio::test]
async fn get_all_positions() {
    let server = expect(
        "GET",
        "/v2/positions",
        fixture("trading/test_position_routes__test_get_all_positions__01.json"),
    )
    .await;

    let positions = client(&server).get_all_positions().await.unwrap();
    assert_eq!(positions[0].symbol, "AAPL");
}

#[tokio::test]
async fn get_open_position_accepts_a_symbol_or_an_id() {
    let positions = fixture("trading/test_position_routes__test_get_all_positions__01.json");
    let position = positions[0].clone();

    let server = expect("GET", "/v2/positions/AAPL", position.clone()).await;
    client(&server)
        .get_open_position(&AssetIdent::from("AAPL"))
        .await
        .unwrap();

    let asset_id = "904837e3-3b76-47ec-b432-046db621571b";
    let server = expect("GET", &format!("/v2/positions/{asset_id}"), position).await;
    client(&server)
        .get_open_position(&AssetIdent::from(asset_id))
        .await
        .unwrap();
}

#[tokio::test]
async fn close_all_positions_passes_cancel_orders() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/v2/positions"))
        .and(query_param("cancel_orders", "true"))
        .respond_with(ResponseTemplate::new(207).set_body_json(fixture(
            "trading/test_position_routes__test_close_all_positions__01.json",
        )))
        .expect(1)
        .mount(&server)
        .await;

    let responses = client(&server)
        .close_all_positions(Some(true))
        .await
        .unwrap();

    assert_eq!(responses.len(), 3);
}

#[tokio::test]
async fn close_position_sends_qty_or_percentage() {
    let order = fixture("trading/test_position_routes__test_close_position_with_qty__01.json");

    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/v2/positions/AAPL"))
        .and(query_param("qty", "1.5"))
        .respond_with(ResponseTemplate::new(200).set_body_json(order.clone()))
        .expect(1)
        .mount(&server)
        .await;

    client(&server)
        .close_position(
            &AssetIdent::from("AAPL"),
            Some(ClosePositionRequest::Qty(Decimal::new(15, 1))),
        )
        .await
        .unwrap();

    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/v2/positions/AAPL"))
        .and(query_param("percentage", "50"))
        .respond_with(ResponseTemplate::new(200).set_body_json(order))
        .expect(1)
        .mount(&server)
        .await;

    client(&server)
        .close_position(
            &AssetIdent::from("AAPL"),
            Some(ClosePositionRequest::Percentage(Decimal::from(50))),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn exercise_options_position_tolerates_a_non_json_body() {
    // This route answers with a bare string rather than JSON.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v2/positions/AAPL240119C00150000/exercise"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .expect(1)
        .mount(&server)
        .await;

    client(&server)
        .exercise_options_position(&AssetIdent::from("AAPL240119C00150000"))
        .await
        .unwrap();
}

// ----------------------------------------------------------------- account

#[tokio::test]
async fn get_account() {
    let server = expect(
        "GET",
        "/v2/account",
        fixture("trading/test_account_routes__test_get_account__01.json"),
    )
    .await;

    let account = client(&server).get_account().await.unwrap();
    assert_eq!(account.account_number, "010203ABCD");
}

#[tokio::test]
async fn get_and_set_account_configurations() {
    let config = fixture("trading/test_account_routes__test_get_account_configurations__01.json");

    let server = expect("GET", "/v2/account/configurations", config.clone()).await;
    let fetched = client(&server).get_account_configurations().await.unwrap();

    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path("/v2/account/configurations"))
        .respond_with(ResponseTemplate::new(200).set_body_json(config))
        .expect(1)
        .mount(&server)
        .await;

    client(&server)
        .set_account_configurations(&fetched)
        .await
        .unwrap();
}

#[tokio::test]
async fn get_portfolio_history() {
    let server = expect(
        "GET",
        "/v2/account/portfolio/history",
        json!({
            "timestamp": [1_580_826_600, 1_580_827_500],
            "equity": [27_423.73, 27_408.19],
            "profit_loss": [11.8, -3.74],
            "profit_loss_pct": [0.000_430_469_507_254_688, -0.000_136_396_875_857_82],
            "base_value": 27_411.93,
            "timeframe": "15Min"
        }),
    )
    .await;

    let history = client(&server).get_portfolio_history(None).await.unwrap();
    assert_eq!(history.timeframe, "15Min");
    assert_eq!(history.timestamp.len(), 2);
}

// ------------------------------------------------------------------ assets

#[tokio::test]
async fn get_all_assets_sends_its_filters() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/assets"))
        .and(query_param("status", "active"))
        .and(query_param("asset_class", "us_equity"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "trading/test_asset_routes__test_get_all_assets__01.json",
        )))
        .expect(1)
        .mount(&server)
        .await;

    let mut filter = GetAssetsRequest::default();
    filter.status = Some(AssetStatus::Active);
    filter.asset_class = Some(alpaca_sdk::trading::AssetClass::UsEquity);

    client(&server).get_all_assets(Some(&filter)).await.unwrap();
}

#[tokio::test]
async fn get_asset_by_symbol() {
    let server = expect(
        "GET",
        "/v2/assets/AAPL",
        fixture("trading/test_asset_routes__test_get_asset__01.json"),
    )
    .await;

    let asset = client(&server)
        .get_asset(&AssetIdent::from("AAPL"))
        .await
        .unwrap();

    assert_eq!(asset.symbol, "AAPL");
}

// ----------------------------------------------------------- market status

#[tokio::test]
async fn get_clock() {
    let server = expect(
        "GET",
        "/v2/clock",
        json!({
            "timestamp": "2022-04-28T14:07:04.451420928-04:00",
            "is_open": true,
            "next_open": "2022-04-29T09:30:00-04:00",
            "next_close": "2022-04-28T16:00:00-04:00"
        }),
    )
    .await;

    assert!(client(&server).get_clock().await.unwrap().is_open);
}

#[tokio::test]
async fn get_calendar() {
    let server = expect(
        "GET",
        "/v2/calendar",
        json!([{"date": "2022-04-13", "open": "09:30", "close": "16:00"}]),
    )
    .await;

    let calendar = client(&server).get_calendar(None).await.unwrap();
    assert_eq!(calendar[0].open.to_string(), "2022-04-13 09:30:00");
}

// -------------------------------------------------------------- watchlists

fn watchlist_json() -> serde_json::Value {
    json!({
        "id": WATCHLIST_ID,
        "account_id": "3f2504e0-4f89-11d3-9a0c-0305e82c3301",
        "name": "Primary",
        "created_at": "2022-04-28T14:07:04.451420Z",
        "updated_at": "2022-04-28T14:07:04.451420Z"
    })
}

#[tokio::test]
async fn watchlist_crud_hits_the_right_routes() {
    let id = Uuid::parse_str(WATCHLIST_ID).unwrap();

    let server = expect("GET", "/v2/watchlists", json!([watchlist_json()])).await;
    assert_eq!(client(&server).get_watchlists().await.unwrap().len(), 1);

    let server = expect(
        "GET",
        &format!("/v2/watchlists/{WATCHLIST_ID}"),
        watchlist_json(),
    )
    .await;
    client(&server).get_watchlist_by_id(id).await.unwrap();

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v2/watchlists"))
        .and(body_json(json!({"name": "Primary", "symbols": ["AAPL"]})))
        .respond_with(ResponseTemplate::new(200).set_body_json(watchlist_json()))
        .expect(1)
        .mount(&server)
        .await;
    client(&server)
        .create_watchlist(&CreateWatchlistRequest::new(
            "Primary",
            vec!["AAPL".to_owned()],
        ))
        .await
        .unwrap();

    // Updates use PUT, while adding a single asset uses POST to the same path.
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path(format!("/v2/watchlists/{WATCHLIST_ID}")))
        .and(body_json(json!({"name": "Renamed"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(watchlist_json()))
        .expect(1)
        .mount(&server)
        .await;
    client(&server)
        .update_watchlist_by_id(id, &UpdateWatchlistRequest::new().name("Renamed"))
        .await
        .unwrap();

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(format!("/v2/watchlists/{WATCHLIST_ID}")))
        .and(body_json(json!({"symbol": "SPY"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(watchlist_json()))
        .expect(1)
        .mount(&server)
        .await;
    client(&server)
        .add_asset_to_watchlist_by_id(id, "SPY")
        .await
        .unwrap();

    let server = expect(
        "DELETE",
        &format!("/v2/watchlists/{WATCHLIST_ID}/SPY"),
        watchlist_json(),
    )
    .await;
    client(&server)
        .remove_asset_from_watchlist_by_id(id, "SPY")
        .await
        .unwrap();

    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path(format!("/v2/watchlists/{WATCHLIST_ID}")))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    client(&server).delete_watchlist_by_id(id).await.unwrap();
}

#[tokio::test]
async fn update_watchlist_validates_before_sending() {
    let server = MockServer::start().await;

    let err = client(&server)
        .update_watchlist_by_id(
            Uuid::parse_str(WATCHLIST_ID).unwrap(),
            &UpdateWatchlistRequest::new(),
        )
        .await
        .unwrap_err();

    assert!(matches!(err, alpaca_sdk::Error::InvalidRequest(_)));
    assert!(server.received_requests().await.unwrap().is_empty());
}

// ----------------------------------------------------------------- options

#[tokio::test]
async fn get_option_contracts_joins_underlying_symbols() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/options/contracts"))
        .and(query_param("underlying_symbols", "AAPL,SPY"))
        .and(query_param("status", "active"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "trading/test_option_routes__test_get_option_contracts__01.json",
        )))
        .expect(1)
        .mount(&server)
        .await;

    let filter = GetOptionContractsRequest::new(vec!["AAPL".to_owned(), "SPY".to_owned()]);
    client(&server).get_option_contracts(&filter).await.unwrap();
}

/// The Penny Program filter. `false` is as meaningful as `true` here — it selects the contracts
/// outside the programme — so it must reach the wire rather than be skipped as
/// a default.
#[tokio::test]
async fn get_option_contracts_sends_the_penny_program_filter() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/options/contracts"))
        .and(query_param("ppind", "false"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "trading/test_option_routes__test_get_option_contracts__01.json",
        )))
        .expect(1)
        .mount(&server)
        .await;

    let mut filter = GetOptionContractsRequest::new(vec!["AAPL".to_owned()]);
    filter.ppind = Some(false);
    client(&server).get_option_contracts(&filter).await.unwrap();
}

#[tokio::test]
async fn get_option_contract_by_symbol() {
    let server = expect(
        "GET",
        "/v2/options/contracts/AAPL240119C00150000",
        fixture("trading/test_option_routes__test_get_option_contract__01.json"),
    )
    .await;

    client(&server)
        .get_option_contract(&AssetIdent::from("AAPL240119C00150000"))
        .await
        .unwrap();
}

// ------------------------------------------------------- corporate actions

#[tokio::test]
async fn get_corporate_announcements() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/corporate_actions/announcements"))
        .and(query_param("ca_types", "dividend"))
        .and(query_param("since", "2021-01-01"))
        .and(query_param("until", "2021-02-01"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "trading/test_corporate_announcements__test_get_announcements__01.json",
        )))
        .expect(1)
        .mount(&server)
        .await;

    let filter = alpaca_sdk::trading::GetCorporateAnnouncementsRequest::new(
        vec![alpaca_sdk::trading::CorporateActionType::Dividend],
        chrono::NaiveDate::from_ymd_opt(2021, 1, 1).unwrap(),
        chrono::NaiveDate::from_ymd_opt(2021, 2, 1).unwrap(),
    );

    #[allow(deprecated)]
    let announcements = client(&server)
        .get_corporate_announcements(&filter)
        .await
        .unwrap();

    assert!(!announcements.is_empty());
}

// ------------------------------------------------------------------ errors

#[tokio::test]
async fn an_api_error_surfaces_the_code_and_message() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v2/orders"))
        .respond_with(ResponseTemplate::new(403).set_body_json(json!({
            "code": 40_310_000,
            "message": "insufficient buying power"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let order = OrderRequest::market(
        "AAPL",
        OrderSide::Buy,
        OrderAmount::Qty(Decimal::from(1)),
        TimeInForce::Day,
    );
    let err = client(&server).submit_order(&order).await.unwrap_err();

    match err {
        alpaca_sdk::Error::Api(api) => {
            assert_eq!(api.status, 403);
            assert_eq!(api.code, Some(40_310_000));
            assert_eq!(api.message, "insufficient buying power");
        }
        other => panic!("expected Api, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Path segments
//
// Every route above builds its path with `format!`, and what gets interpolated
// is caller-supplied. Until these were encoded, a crypto pair split into two
// path segments — so no crypto position could be read or closed through this
// crate — and a `..` in a symbol addressed a route the caller never named.
// Every route test above uses `AAPL` or a UUID, which is why none of them saw
// it.
// ---------------------------------------------------------------------------

/// A crypto pair is one segment, percent-encoded, which is the form Alpaca's
/// own reference asks for: `/v2/assets/BTC%2FUSDT`.
#[tokio::test]
async fn a_crypto_pair_addresses_one_path_segment() {
    let positions = fixture("trading/test_position_routes__test_get_all_positions__01.json");
    // wiremock matches the path as it arrives on the wire, so mounting the
    // encoded form is itself the assertion: before the fix the request went to
    // `/v2/positions/BTC/USD`, two segments, and this mock would never match.
    let server = expect("GET", "/v2/positions/BTC%2FUSD", positions[0].clone()).await;

    client(&server)
        .get_open_position(&AssetIdent::from("BTC/USD"))
        .await
        .unwrap();

    let received = &server.received_requests().await.unwrap()[0];
    assert_eq!(received.url.path(), "/v2/positions/BTC%2FUSD");
}

/// The traversal case. `..` cannot be expressed as a literal path segment by any
/// encoding — a URL parser removes it — so it is refused rather than sent.
/// Unrefused, `close_position` reached `DELETE /v2/positions`, the close-all
/// route, liquidating every position — and then failed to decode the array it
/// answers with into the single `Order` the caller asked for, so the error told
/// the caller nothing about what had just happened to their account.
#[tokio::test]
async fn a_dot_segment_symbol_is_refused_before_any_request_is_made() {
    let server = MockServer::start().await;
    // No mock is mounted: reaching the server at all is the failure.

    for symbol in ["..", ".", ""] {
        let err = client(&server)
            .close_position(&AssetIdent::Symbol(symbol.to_owned()), None)
            .await
            .unwrap_err();

        assert!(
            matches!(err, alpaca_sdk::Error::InvalidRequest(_)),
            "expected {symbol:?} to be refused, got {err:?}"
        );
    }

    assert!(
        server.received_requests().await.unwrap().is_empty(),
        "a refused symbol must not reach the network"
    );
}

/// A symbol carrying a query delimiter cannot inject parameters ahead of the
/// ones the crate serializes.
#[tokio::test]
async fn a_symbol_cannot_inject_query_parameters() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({})))
        .mount(&server)
        .await;

    let _ = client(&server)
        .get_asset(&AssetIdent::Symbol("AAPL?foo=bar".to_owned()))
        .await;

    let received = &server.received_requests().await.unwrap()[0];
    assert_eq!(
        received.url.query(),
        None,
        "the `?` must not start a query string"
    );
    assert_eq!(received.url.path(), "/v2/assets/AAPL%3Ffoo%3Dbar");
}

/// The read-modify-write round trip, against the *current* response shape.
///
/// Every field on `AccountConfiguration` except three is non-`Option`, so
/// read-modify-write is the only way to change one setting. The three optional
/// ones had no `skip_serializing_if`, so a round trip of a current-shape
/// response `PATCH`ed `"dtbp_check": null`, `"pdt_check": null` and
/// `"max_options_trading_level": null` — two fields the PATCH schema does not
/// document at all, and a `null` into an integer enum of `[0,1,2,3]`.
///
/// The existing round-trip test above mounts a PATCH with no `body_json`
/// matcher, so it passed whatever was sent; this one asserts the body.
#[tokio::test]
async fn setting_account_configuration_omits_absent_fields_rather_than_nulling_them() {
    let config = fixture(
        "trading/test_account_routes__test_get_account_configurations_without_deprecated_pdt_fields__01.json",
    );

    let server = expect("GET", "/v2/account/configurations", config.clone()).await;
    let mut fetched = client(&server).get_account_configurations().await.unwrap();
    fetched.suspend_trade = true;

    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path("/v2/account/configurations"))
        // Exactly the fields the response carried, with the one edit applied —
        // and no `dtbp_check` or `pdt_check` keys at all.
        .and(body_json(json!({
            "no_shorting": false,
            "suspend_trade": true,
            "fractional_trading": true,
            "max_margin_multiplier": "4",
            "trade_confirm_email": "all",
            "ptp_no_exception_entry": false,
            "max_options_trading_level": 1
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(config))
        .expect(1)
        .mount(&server)
        .await;

    client(&server)
        .set_account_configurations(&fetched)
        .await
        .unwrap();
}

/// The crypto funding list routes answer with an array. They were typed as a
/// single object, so every call failed to decode — and nothing noticed, because
/// the route smoke test mounts a 404 and the live capture is recorded as
/// `refused`. Both shapes are accepted now, since no payload has ever been seen.
#[tokio::test]
async fn crypto_funding_lists_decode_from_an_array() {
    let addresses = json!([
        {"id": "1", "address": "0xabc", "asset": "USDT", "status": "ACTIVE"},
        {"id": "2", "address": "0xdef", "asset": "USDC", "status": "PENDING"}
    ]);

    let server = expect("GET", "/v2/wallets/whitelists", addresses).await;
    let listed = client(&server).get_whitelisted_addresses().await.unwrap();
    assert_eq!(listed.len(), 2);
}

/// And from the single-object form the wallets route is documented to use when
/// an asset filter is supplied.
#[tokio::test]
async fn crypto_funding_lists_also_decode_from_a_single_object() {
    let wallet = json!({"id": "1", "address": "0xabc", "asset": "USDT"});

    let server = expect("GET", "/v2/wallets", wallet).await;
    let wallets = client(&server).get_crypto_wallets(None).await.unwrap();
    assert_eq!(
        wallets.len(),
        1,
        "a single object should become one element"
    );
}

/// `/v2/orders` caps a page at 500 and has no `next_page_token` — it is walked
/// with `before_order_id`. There was no walker at all, so an account with more
/// than 500 orders silently reconciled against a truncated history.
#[tokio::test]
async fn get_all_orders_follows_the_id_cursor_across_pages() {
    fn order_page(ids: &[&str]) -> serde_json::Value {
        serde_json::Value::Array(
            ids.iter()
                .map(|id| {
                    let mut order =
                        fixture("trading/test_order_routes__test_get_order_by_id__01.json");
                    order["id"] = json!(id);
                    order
                })
                .collect(),
        )
    }

    // A full page of 500 is what signals "there may be more", so the first page
    // has to actually be that long.
    let first: Vec<String> = (0..500).map(|i| Uuid::from_u128(i).to_string()).collect();
    let first_refs: Vec<&str> = first.iter().map(String::as_str).collect();
    let last_id = first_refs[499];

    let server = MockServer::start().await;

    const SECOND: &str = "aaaaaaaa-0000-0000-0000-000000000001";

    // wiremock matches in mount order, first match wins, so the cursor-bearing
    // mocks go ahead of the general one.
    //
    // Page three: empty, which is what ends the walk. A *short* page does not,
    // because a short page is also what a server silently capping `limit`
    // returns — and stopping there is the truncation this walk exists to fix.
    Mock::given(method("GET"))
        .and(path("/v2/orders"))
        .and(query_param("before_order_id", SECOND))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .expect(1)
        .mount(&server)
        .await;

    // Page two, reached only by sending the oldest id from page one.
    Mock::given(method("GET"))
        .and(path("/v2/orders"))
        .and(query_param("before_order_id", last_id))
        .respond_with(ResponseTemplate::new(200).set_body_json(order_page(&[SECOND])))
        .expect(1)
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/v2/orders"))
        .and(query_param("limit", "500"))
        .and(query_param("direction", "desc"))
        .respond_with(ResponseTemplate::new(200).set_body_json(order_page(&first_refs)))
        .expect(1)
        .mount(&server)
        .await;

    let orders = client(&server).get_all_orders(None, None).await.unwrap();
    assert_eq!(orders.len(), 501, "the second page was not fetched");
}

/// And `max_items` stops the walk rather than being a per-page hint.
#[tokio::test]
async fn get_all_orders_respects_max_items() {
    let page = serde_json::Value::Array(
        (0..500)
            .map(|i| {
                let mut order = fixture("trading/test_order_routes__test_get_order_by_id__01.json");
                order["id"] = json!(Uuid::from_u128(i).to_string());
                order
            })
            .collect(),
    );

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/orders"))
        // The cap is also sent as `limit`, so a small `max_items` does not pull
        // a full 500-order page off the wire to hand back ten. Without this
        // matcher the mock answers whatever it is asked for, and deleting the
        // narrowing would leave the test green.
        .and(query_param("limit", "10"))
        .respond_with(ResponseTemplate::new(200).set_body_json(page))
        // One request: the cap is reached inside the first page.
        .expect(1)
        .mount(&server)
        .await;

    let orders = client(&server)
        .get_all_orders(None, Some(10))
        .await
        .unwrap();
    assert_eq!(orders.len(), 10);
}

/// A server that cycles between two pages does not hang the walk.
///
/// The first guard here compared consecutive cursors, which only catches a
/// server repeating the *same* id. A server alternating between two pages moves
/// the cursor every time and still teaches the walk nothing — and that spun
/// forever. Progress is now measured in new orders, which `seen` already knows.
#[tokio::test]
async fn a_cycling_server_does_not_spin_the_order_walk() {
    let page = |id: &str| {
        let mut order = fixture("trading/test_order_routes__test_get_order_by_id__01.json");
        order["id"] = json!(id);
        serde_json::Value::Array(vec![order])
    };
    const A: &str = "aaaaaaaa-0000-0000-0000-00000000000a";
    const B: &str = "bbbbbbbb-0000-0000-0000-00000000000b";

    let server = MockServer::start().await;
    // Asking for anything after B returns A; anything else returns B. Neither
    // page is empty, and the cursor changes on every request.
    Mock::given(method("GET"))
        .and(path("/v2/orders"))
        .and(query_param("before_order_id", B))
        .respond_with(ResponseTemplate::new(200).set_body_json(page(A)))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v2/orders"))
        .respond_with(ResponseTemplate::new(200).set_body_json(page(B)))
        .mount(&server)
        .await;

    let orders = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        client(&server).get_all_orders(None, None),
    )
    .await
    .expect("the walk must terminate")
    .unwrap();

    // Both distinct orders, each once.
    assert_eq!(orders.len(), 2, "expected A and B exactly once: {orders:?}");
}

/// `max_items` narrows the page size on *every* request, not just the first.
///
/// Capping only the first page still pulls a full 500 orders to satisfy a
/// `max_items` that spans two pages. The existing `max_items` test uses a cap
/// satisfied inside page one, so it cannot see the difference — this one asks
/// for 600 and asserts the second request only asks for the 100 outstanding.
#[tokio::test]
async fn get_all_orders_narrows_the_page_size_on_every_request() {
    fn order_page(n: usize, offset: u128) -> serde_json::Value {
        serde_json::Value::Array(
            (0..n)
                .map(|i| {
                    let mut order =
                        fixture("trading/test_order_routes__test_get_order_by_id__01.json");
                    order["id"] = json!(Uuid::from_u128(offset + i as u128).to_string());
                    order
                })
                .collect(),
        )
    }

    let server = MockServer::start().await;

    // The second request must ask for the 100 still outstanding, not another 500.
    Mock::given(method("GET"))
        .and(path("/v2/orders"))
        .and(query_param("limit", "100"))
        .respond_with(ResponseTemplate::new(200).set_body_json(order_page(100, 500)))
        .expect(1)
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/v2/orders"))
        .and(query_param("limit", "500"))
        .respond_with(ResponseTemplate::new(200).set_body_json(order_page(500, 0)))
        .expect(1)
        .mount(&server)
        .await;

    let orders = client(&server)
        .get_all_orders(None, Some(600))
        .await
        .unwrap();

    assert_eq!(orders.len(), 600);
}
