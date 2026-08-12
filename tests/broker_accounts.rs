//! Broker models and routes, against the payloads alpaca-py captured.

#![cfg(feature = "broker")]

use alpaca_sdk::broker::{Account, BrokerClient};
use alpaca_sdk::trading::AccountStatus;
use alpaca_sdk::{Credentials, RestConfig, RetryConfig};
use serde_json::json;
use uuid::Uuid;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const ACCOUNT_ID: &str = "2a87c088-ffb6-472b-a4a3-cd9305c8605c";

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

// ------------------------------------------------------------------ auth

#[tokio::test]
async fn the_broker_client_uses_basic_auth_not_apca_headers() {
    // This is the one thing that differs from every other client in the crate:
    // alpaca-py sets use_basic_auth=True on BrokerClient alone.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/accounts/{ACCOUNT_ID}")))
        // base64("broker-key:broker-secret")
        .and(header(
            "authorization",
            "Basic YnJva2VyLWtleTpicm9rZXItc2VjcmV0",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "broker/test_accounts_routes__test_get_account__01.json",
        )))
        .expect(1)
        .mount(&server)
        .await;

    client(&server)
        .get_account_by_id(Uuid::parse_str(ACCOUNT_ID).unwrap())
        .await
        .unwrap();

    let received = &server.received_requests().await.unwrap()[0];
    assert!(
        received.headers.get("APCA-API-KEY-ID").is_none(),
        "the key-pair headers must not be sent to the broker API"
    );
}

#[tokio::test]
async fn the_broker_client_targets_v1() {
    // Every other surface is v2 or v1beta*; the broker API is v1.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/clock"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "timestamp": "2026-08-12T14:07:04.451420928-04:00",
            "is_open": true,
            "next_open": "2026-08-13T09:30:00-04:00",
            "next_close": "2026-08-12T16:00:00-04:00"
        })))
        .expect(1)
        .mount(&server)
        .await;

    assert!(client(&server).get_clock().await.unwrap().is_open);
}

// -------------------------------------------------------------- accounts

#[test]
fn account_deserializes_from_the_captured_payload() {
    let account: Account = parse("broker/test_accounts_routes__test_get_account__01.json");

    assert_eq!(account.account_number, "601865070");
    assert_eq!(account.status, AccountStatus::Active);
    assert_eq!(account.crypto_status, Some(AccountStatus::Inactive));
    assert_eq!(account.currency.as_deref(), Some("USD"));

    // A string on the wire, kept exact rather than rounded through f64.
    assert_eq!(
        account.last_equity.unwrap().to_string(),
        "47604.17306484226"
    );
}

#[test]
fn account_carries_its_nested_records() {
    let account: Account = parse("broker/test_accounts_routes__test_get_account__01.json");

    let contact = account.contact.expect("contact");
    assert_eq!(contact.city.as_deref(), Some("San Mateo"));
    assert_eq!(contact.street_address.len(), 1);

    let identity = account.identity.expect("identity");
    assert_eq!(identity.given_name, "Agitated");
    assert_eq!(
        identity.tax_id_type,
        Some(alpaca_sdk::broker::TaxIdType::UsaSsn)
    );
    assert_eq!(identity.funding_source.len(), 1);
    // Present in the payload but null; must not become a parse error.
    assert_eq!(identity.visa_type, None);
    assert_eq!(identity.date_of_departure_from_usa, None);

    let agreements = account.agreements.expect("agreements");
    assert_eq!(agreements.len(), 3);
    assert!(agreements.iter().any(|a| a.revision.is_none()));

    let documents = account.documents.expect("documents");
    assert_eq!(documents.len(), 1);
    assert!(documents[0].id.is_some());

    let trusted = account.trusted_contact.expect("trusted contact");
    assert_eq!(trusted.given_name.as_deref(), Some("Jane"));
}

#[test]
fn account_keeps_kyc_results_as_raw_json() {
    // The per-check payloads vary by verification provider. alpaca-py does not
    // model them either; guessing a shape would be worse than passing them
    // through intact.
    let account: Account = parse("broker/test_accounts_routes__test_get_account__01.json");

    let kyc = account.kyc_results.expect("kyc results");
    assert_eq!(kyc.summary.as_deref(), Some("pass"));
    assert!(kyc.accept.is_some());
    assert!(kyc.reject.is_some());
}

#[tokio::test]
async fn get_account_by_id_uses_the_account_path() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/accounts/{ACCOUNT_ID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "broker/test_accounts_routes__test_get_account__01.json",
        )))
        .expect(1)
        .mount(&server)
        .await;

    let account = client(&server)
        .get_account_by_id(Uuid::parse_str(ACCOUNT_ID).unwrap())
        .await
        .unwrap();

    assert_eq!(account.id.to_string(), ACCOUNT_ID);
}

#[tokio::test]
async fn all_accounts_positions_is_keyed_by_account() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/accounts/positions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "broker/test_trading_routes__test_get_all_accounts_positions__01.json",
        )))
        .expect(1)
        .mount(&server)
        .await;

    let positions = client(&server).get_all_accounts_positions().await.unwrap();
    assert!(!positions.positions.is_empty());
}

#[tokio::test]
async fn closing_an_account_posts_to_the_close_action() {
    // Not a DELETE: the account's records survive, and alpaca-py's
    // `delete_account` is a deprecated alias that posts here too.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(format!("/v1/accounts/{ACCOUNT_ID}/actions/close")))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    client(&server)
        .close_account(Uuid::parse_str(ACCOUNT_ID).unwrap())
        .await
        .unwrap();
}

#[test]
fn the_trade_account_carries_the_fields_only_the_broker_api_returns() {
    // alpaca-py subclasses the trading TradeAccount to add these; here the
    // trading record is flattened in, so both halves must survive one parse.
    let account: alpaca_sdk::broker::TradeAccount =
        parse("broker/test_accounts_routes__test_get_trade_account_by_id__01.json");

    assert_eq!(account.account.account_number, "684486106");
    assert_eq!(account.cash_withdrawable.unwrap().to_string(), "0");
    assert_eq!(account.cash_transferable.unwrap().to_string(), "0");
    assert_eq!(
        account.clearing_broker,
        Some(alpaca_sdk::broker::ClearingBroker::Velox)
    );
    assert!(account.previous_close.is_some());
    assert_eq!(account.last_daytrade_count, Some(0));
}

#[test]
fn the_trade_account_tolerates_the_pdt_fields_alpaca_removed() {
    // Alpaca stopped sending these on 2026-07-06 in the FINRA intraday-margin
    // migration. They are absent from this payload, on both halves of the
    // flattened record.
    let account: alpaca_sdk::broker::TradeAccount = parse(
        "broker/test_accounts_routes__test_get_trade_account_by_id_without_deprecated_pdt_fields__01.json",
    );

    assert_eq!(account.last_daytrading_buying_power, None);
    assert_eq!(account.last_daytrade_count, None);
    assert_eq!(account.account.daytrading_buying_power, None);
    assert_eq!(account.account.pattern_day_trader, None);
}

#[tokio::test]
async fn the_trade_account_and_its_configuration_are_separate_routes() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/trading/accounts/{ACCOUNT_ID}/account")))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "broker/test_accounts_routes__test_get_trade_account_by_id__01.json",
        )))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/v1/trading/accounts/{ACCOUNT_ID}/account/configurations"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "broker/test_accounts_routes__test_get_trade_configuration_for_account__01.json",
        )))
        .expect(1)
        .mount(&server)
        .await;

    let account_id = Uuid::parse_str(ACCOUNT_ID).unwrap();
    let client = client(&server);

    client.get_trade_account_by_id(account_id).await.unwrap();
    let configuration = client
        .get_trade_configuration_for_account(account_id)
        .await
        .unwrap();

    assert!(configuration.fractional_trading);
}

// ------------------------------------------- trading on behalf of accounts

#[tokio::test]
async fn positions_for_an_account_use_the_trading_path() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/trading/accounts/{ACCOUNT_ID}/positions")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .expect(1)
        .mount(&server)
        .await;

    client(&server)
        .get_all_positions_for_account(Uuid::parse_str(ACCOUNT_ID).unwrap())
        .await
        .unwrap();
}

#[tokio::test]
async fn watchlist_routes_nest_under_the_account() {
    let watchlist = json!({
        "id": "fb306d55-2d64-4b8b-8c2a-3d0d9e0b7d47",
        "account_id": ACCOUNT_ID,
        "name": "Primary",
        "created_at": "2022-04-28T14:07:04.451420Z",
        "updated_at": "2022-04-28T14:07:04.451420Z"
    });

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/v1/trading/accounts/{ACCOUNT_ID}/watchlists"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([watchlist])))
        .expect(1)
        .mount(&server)
        .await;

    let watchlists = client(&server)
        .get_watchlists_for_account(Uuid::parse_str(ACCOUNT_ID).unwrap())
        .await
        .unwrap();

    assert_eq!(watchlists.len(), 1);
    assert_eq!(watchlists[0].name, "Primary");
}

#[tokio::test]
async fn portfolio_history_for_an_account() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/v1/trading/accounts/{ACCOUNT_ID}/account/portfolio/history"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "broker/test_trading_routes__test_get_portfolio_history__01.json",
        )))
        .expect(1)
        .mount(&server)
        .await;

    let history = client(&server)
        .get_portfolio_history_for_account(Uuid::parse_str(ACCOUNT_ID).unwrap(), None)
        .await
        .unwrap();

    assert!(!history.timestamp.is_empty());
}
