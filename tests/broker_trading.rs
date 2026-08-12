//! The broker routes that act on behalf of an account, against a mock server.
//!
//! These paths all nest under `/trading/accounts/{account_id}`, and the models
//! they return are the trading API's plus a correspondent-only field or two.
//! Both are easy to get subtly wrong and still compile, so they are asserted on
//! the wire rather than trusted.

#![cfg(feature = "broker")]

use alpaca_sdk::broker::{BrokerClient, CreateOptionExerciseRequest, Order, OrderRequest};
use alpaca_sdk::trading::{
    GetOrderByIdRequest, GetOrdersRequest, OrderAmount, OrderSide, QueryOrderStatus, TimeInForce,
};
use alpaca_sdk::types::{AssetIdent, SupportedCurrencies};
use alpaca_sdk::{Credentials, RestConfig, RetryConfig};
use rust_decimal::Decimal;
use serde_json::json;
use uuid::Uuid;
use wiremock::matchers::{body_json, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const ACCOUNT_ID: &str = "2a87c088-ffb6-472b-a4a3-cd9305c8605c";
const ORDER_ID: &str = "61e69015-8549-4bfd-b9c3-01e75843f47d";

fn fixture(name: &str) -> serde_json::Value {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name);
    let body = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    serde_json::from_str(&body).unwrap()
}

fn parse<T: serde::de::DeserializeOwned>(name: &str) -> T {
    let value = fixture(name);
    serde_json::from_value(value.clone()).unwrap_or_else(|e| panic!("{name}: {e}\n{value:#}"))
}

fn client(server: &MockServer) -> BrokerClient {
    let credentials = Credentials::new("broker-key", "broker-secret").unwrap();
    BrokerClient::with_config(
        &credentials,
        RestConfig::new(server.uri())
            .api_version("v1")
            .retry(RetryConfig::none()),
    )
    .unwrap()
}

fn account_id() -> Uuid {
    Uuid::parse_str(ACCOUNT_ID).unwrap()
}

// ------------------------------------------------------------------ orders

#[test]
fn a_broker_order_carries_the_commission_the_trading_model_has_no_field_for() {
    // The one difference between broker and trading orders, and the reason the
    // broker routes cannot simply return `trading::Order`: alpaca-py subclasses
    // it to add this. It arrives as a JSON number here.
    let order: Order =
        parse("broker/test_trading_routes__test_close_position_for_account_with_qty__01.json");

    assert_eq!(order.commission, Some(Decimal::new(125, 2)));
    // ...and the flattened trading fields still parse.
    assert_eq!(order.order.symbol.as_deref(), Some("AAPL`"));
    assert_eq!(order.order.id.to_string(), ORDER_ID);
}

#[tokio::test]
async fn submit_order_for_account_posts_under_the_account() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(format!("/v1/trading/accounts/{ACCOUNT_ID}/orders")))
        .and(body_json(json!({
            "symbol": "AAPL",
            "qty": "1",
            "side": "buy",
            "type": "market",
            "time_in_force": "day",
            "commission": "1.25"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "broker/test_trading_routes__test_close_position_for_account_with_qty__01.json",
        )))
        .expect(1)
        .mount(&server)
        .await;

    let order = OrderRequest::new(alpaca_sdk::trading::OrderRequest::market(
        "AAPL",
        OrderSide::Buy,
        OrderAmount::Qty(Decimal::ONE),
        TimeInForce::Day,
    ))
    .commission(Decimal::new(125, 2));

    client(&server)
        .submit_order_for_account(account_id(), &order)
        .await
        .unwrap();
}

#[test]
fn a_local_currency_order_must_be_a_market_order() {
    // alpaca-py enforces this in a model validator; local currency trading only
    // supports market orders.
    let limit = OrderRequest::new(alpaca_sdk::trading::OrderRequest::limit(
        "AAPL",
        OrderSide::Buy,
        OrderAmount::Qty(Decimal::ONE),
        TimeInForce::Day,
        Decimal::from(100),
    ))
    .currency(SupportedCurrencies::Gbp);
    assert!(limit.validate().is_err());

    let market = OrderRequest::new(alpaca_sdk::trading::OrderRequest::market(
        "AAPL",
        OrderSide::Buy,
        OrderAmount::Qty(Decimal::ONE),
        TimeInForce::Day,
    ))
    .currency(SupportedCurrencies::Gbp);
    assert!(market.validate().is_ok());

    // A limit order in USD is fine; the rule is about the currency, not the type.
    let usd_limit = OrderRequest::new(alpaca_sdk::trading::OrderRequest::limit(
        "AAPL",
        OrderSide::Buy,
        OrderAmount::Qty(Decimal::ONE),
        TimeInForce::Day,
        Decimal::from(100),
    ))
    .currency(SupportedCurrencies::Usd);
    assert!(usd_limit.validate().is_ok());
}

#[tokio::test]
async fn get_orders_for_account_sends_its_filter() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/trading/accounts/{ACCOUNT_ID}/orders")))
        .and(query_param("status", "open"))
        // A list, sent as one comma-separated parameter rather than repeated.
        .and(query_param("symbols", "AAPL,TSLA"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .expect(1)
        .mount(&server)
        .await;

    let filter = GetOrdersRequest {
        status: Some(QueryOrderStatus::Open),
        symbols: Some(vec!["AAPL".to_owned(), "TSLA".to_owned()]),
        ..Default::default()
    };

    client(&server)
        .get_orders_for_account(account_id(), Some(&filter))
        .await
        .unwrap();
}

#[tokio::test]
async fn get_order_for_account_by_id_passes_nested() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/v1/trading/accounts/{ACCOUNT_ID}/orders/{ORDER_ID}"
        )))
        .and(query_param("nested", "true"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "broker/test_trading_routes__test_close_position_for_account_with_qty__01.json",
        )))
        .expect(1)
        .mount(&server)
        .await;

    client(&server)
        .get_order_for_account_by_id(
            account_id(),
            Uuid::parse_str(ORDER_ID).unwrap(),
            Some(&GetOrderByIdRequest { nested: true }),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn get_order_for_account_by_client_id_uses_the_colon_route() {
    // `orders:by_client_order_id` is a literal path segment with a colon in it,
    // not a path parameter — percent-encoding it would 404.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/v1/trading/accounts/{ACCOUNT_ID}/orders:by_client_order_id"
        )))
        .and(query_param("client_order_id", "my-order-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "broker/test_trading_routes__test_close_position_for_account_with_qty__01.json",
        )))
        .expect(1)
        .mount(&server)
        .await;

    client(&server)
        .get_order_for_account_by_client_id(account_id(), "my-order-1")
        .await
        .unwrap();
}

#[tokio::test]
async fn cancel_order_for_account_by_id_tolerates_an_empty_body() {
    // Alpaca answers 204 here, which is not JSON.
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path(format!(
            "/v1/trading/accounts/{ACCOUNT_ID}/orders/{ORDER_ID}"
        )))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    client(&server)
        .cancel_order_for_account_by_id(account_id(), Uuid::parse_str(ORDER_ID).unwrap())
        .await
        .unwrap();
}

// --------------------------------------------------------------- positions

#[tokio::test]
async fn get_open_position_for_account_takes_a_symbol_or_an_id() {
    let position = json!({
        "asset_id": "904837e3-3b76-47ec-b432-046db621571b",
        "symbol": "AAPL",
        "exchange": "NASDAQ",
        "asset_class": "us_equity",
        "avg_entry_price": "100.0",
        "qty": "5",
        "side": "long",
        "market_value": "600.0",
        "cost_basis": "500.0",
        "unrealized_pl": "100.0",
        "unrealized_plpc": "0.20",
        "unrealized_intraday_pl": "10.0",
        "unrealized_intraday_plpc": "0.0084",
        "current_price": "120.0",
        "lastday_price": "119.0",
        "change_today": "0.0084"
    });

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/v1/trading/accounts/{ACCOUNT_ID}/positions/AAPL"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(position))
        .expect(1)
        .mount(&server)
        .await;

    let held = client(&server)
        .get_open_position_for_account(account_id(), &AssetIdent::from("AAPL"))
        .await
        .unwrap();

    assert_eq!(held.symbol, "AAPL");
}

#[tokio::test]
async fn exercising_an_option_sends_the_commission_only_when_one_is_set() {
    // alpaca-py drops unset request fields, so a commission-free exercise posts
    // an empty object rather than `{"commission": null}`.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(format!(
            "/v1/trading/accounts/{ACCOUNT_ID}/positions/AAPL240119C00150000/exercise"
        )))
        .and(body_json(json!({})))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .expect(1)
        .mount(&server)
        .await;

    client(&server)
        .exercise_options_position_for_account_by_id(
            account_id(),
            &AssetIdent::from("AAPL240119C00150000"),
            &CreateOptionExerciseRequest::new(),
        )
        .await
        .unwrap();

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(format!(
            "/v1/trading/accounts/{ACCOUNT_ID}/positions/AAPL240119C00150000/exercise"
        )))
        .and(body_json(json!({"commission": "0.5"})))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .expect(1)
        .mount(&server)
        .await;

    client(&server)
        .exercise_options_position_for_account_by_id(
            account_id(),
            &AssetIdent::from("AAPL240119C00150000"),
            &CreateOptionExerciseRequest::new().commission(Decimal::new(5, 1)),
        )
        .await
        .unwrap();
}

// -------------------------------------------------------------- watchlists

#[tokio::test]
async fn delete_watchlist_from_account_tolerates_an_empty_body() {
    let watchlist_id = Uuid::parse_str("fb306d55-2d64-4b8b-8c2a-3d0d9e0b7d47").unwrap();

    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path(format!(
            "/v1/trading/accounts/{ACCOUNT_ID}/watchlists/{watchlist_id}"
        )))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    client(&server)
        .delete_watchlist_from_account_by_id(account_id(), watchlist_id)
        .await
        .unwrap();
}

// ------------------------------------------------------------------ assets

#[tokio::test]
async fn assets_are_not_account_scoped() {
    // The asset master is the same for every account the correspondent serves,
    // so this route sits at the top level rather than under /trading/accounts.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/assets"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "trading/test_asset_routes__test_get_all_assets__01.json",
        )))
        .expect(1)
        .mount(&server)
        .await;

    let assets = client(&server).get_all_assets(None).await.unwrap();
    assert!(!assets.is_empty());

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/assets/AAPL"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "trading/test_asset_routes__test_get_asset__01.json",
        )))
        .expect(1)
        .mount(&server)
        .await;

    client(&server)
        .get_asset(&AssetIdent::from("AAPL"))
        .await
        .unwrap();
}
