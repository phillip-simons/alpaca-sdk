//! Rebalancing: portfolios, subscriptions, and runs.
//!
//! Three things here differ from the rest of the broker API. Percentages are
//! declared `float` and arrive as strings; a condition's `sub_type` belongs to
//! one of two enums depending on its `type`; and subscriptions and runs page by
//! token while portfolios do not page at all.

#![cfg(feature = "broker")]

use alpaca_sdk::broker::{
    BrokerClient, CalendarSubType, CreatePortfolioRequest, CreateRunRequest,
    CreateSubscriptionRequest, DriftBandSubType, GetRunsRequest, GetSubscriptionsRequest,
    Portfolio, PortfolioStatus, RebalancingConditionsType, RebalancingRun, RebalancingSubType,
    RunStatus, RunType, Subscription, SubscriptionsPage, UpdatePortfolioRequest, Weight,
    WeightType,
};
use alpaca_sdk::{Credentials, RestConfig, RetryConfig};
use rust_decimal::Decimal;
use serde_json::json;
use uuid::Uuid;
use wiremock::matchers::{body_json, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const PORTFOLIO_ID: &str = "57d4ec79-9658-4916-9eb1-7c672be97e3e";
const SUBSCRIPTION_ID: &str = "9341be15-8786-4d23-ba1a-fc10ef4f90f4";
// The walk drops a subscription it has already collected, so a page of clones
// would arrive as one subscription and assert nothing about paging.
const OTHER_SUBSCRIPTION_ID: &str = "9341be15-8786-4d23-ba1a-fc10ef4f90f5";
const THIRD_SUBSCRIPTION_ID: &str = "9341be15-8786-4d23-ba1a-fc10ef4f90f6";
const RUN_ID: &str = "2ad28f83-796c-4c4d-895e-d360aeb95297";
const ACCOUNT_ID: &str = "bf2b0f93-f296-4276-a9cf-288586cf4fb7";

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

// ----------------------------------------------------------------- models

#[test]
fn a_portfolio_reads_its_percentages_as_decimals() {
    let portfolios: Vec<Portfolio> =
        parse("broker/test_rebalancing_routes__test_get_all_portfolios__01.json");

    let portfolio = &portfolios[0];
    assert_eq!(portfolio.name, "My Portfolio");
    assert_eq!(portfolio.status, PortfolioStatus::Active);
    assert_eq!(portfolio.cooldown_days, Some(2));

    // "35" on the wire: a string, not a number.
    assert_eq!(portfolio.weights.len(), 3);
    assert_eq!(portfolio.weights[0].percent, Decimal::from(35));
    assert_eq!(portfolio.weights[0].weight_type, WeightType::Asset);
    assert_eq!(portfolio.weights[0].symbol.as_deref(), Some("AAPL"));
}

#[test]
fn a_conditions_sub_type_is_resolved_by_its_value_not_its_position() {
    // Trying each of the two enums in turn cannot work here: every wire enum
    // takes any string into Unknown, so the first would always win. The two
    // value sets are disjoint, so the value itself decides.
    let portfolios: Vec<Portfolio> =
        parse("broker/test_rebalancing_routes__test_get_all_portfolios__01.json");

    let conditions = portfolios[0]
        .rebalance_conditions
        .as_ref()
        .expect("rebalance conditions");
    assert_eq!(
        conditions[0].condition_type,
        RebalancingConditionsType::DriftBand
    );
    assert_eq!(
        conditions[0].sub_type,
        RebalancingSubType::DriftBand(DriftBandSubType::Absolute)
    );
    assert_eq!(conditions[0].percent, Some(Decimal::from(5)));
    assert_eq!(conditions[0].day, None);
}

#[test]
fn a_calendar_sub_type_lands_in_the_calendar_half() {
    let calendar: RebalancingSubType = serde_json::from_value(json!("quarterly")).unwrap();
    assert_eq!(
        calendar,
        RebalancingSubType::Calendar(CalendarSubType::Quarterly)
    );

    let drift: RebalancingSubType = serde_json::from_value(json!("relative")).unwrap();
    assert_eq!(
        drift,
        RebalancingSubType::DriftBand(DriftBandSubType::Relative)
    );

    // A value in neither set degrades rather than failing, and keeps the string.
    let unknown: RebalancingSubType = serde_json::from_value(json!("fortnightly")).unwrap();
    assert_eq!(
        unknown,
        RebalancingSubType::Unknown("fortnightly".to_owned())
    );
    assert_eq!(unknown.as_str(), "fortnightly");

    // And it round-trips as the bare string it arrived as.
    assert_eq!(
        serde_json::to_value(&unknown).unwrap(),
        json!("fortnightly")
    );
}

#[test]
fn a_run_carries_its_orders_skipped_and_placed() {
    let run: RebalancingRun = parse("broker/test_rebalancing_routes__test_get_run_by_id__01.json");

    assert_eq!(run.run_type, RunType::FullRebalance);
    assert_eq!(run.status, RunStatus::Canceled);
    assert_eq!(run.amount, None);
    assert!(run.orders.as_ref().is_some_and(Vec::is_empty));

    let skipped = run.skipped_orders.expect("skipped orders");
    assert_eq!(skipped[0].symbol, "SPY");
    assert_eq!(skipped[0].side, Some(alpaca_sdk::trading::OrderSide::Buy));
    assert_eq!(skipped[0].notional, Some(Decimal::ZERO));
    assert_eq!(skipped[0].reason, "ORDER_LESS_THAN_MIN_NOTIONAL");

    // A cash weight has no symbol, which is absence rather than "".
    assert_eq!(run.weights[0].weight_type, WeightType::Cash);
    assert_eq!(run.weights[0].symbol, None);
}

// ------------------------------------------------------------- validation

#[test]
fn the_constructors_round_a_percentage_to_two_places() {
    // The constructors round; a field assigned directly is the caller's.
    assert_eq!(
        Weight::asset("AAPL", Decimal::new(33333, 3)).percent,
        Decimal::new(3333, 2)
    );
    assert_eq!(
        Weight::cash(Decimal::new(66667, 3)).percent,
        Decimal::new(6667, 2)
    );

    // A percentage Alpaca sent is kept exactly as sent — rounding on the way in
    // would be the port editing the server's numbers.
    let weight: Weight =
        serde_json::from_value(json!({ "type": "cash", "symbol": null, "percent": "5.005" }))
            .unwrap();
    assert_eq!(weight.percent, Decimal::new(5005, 3));
}

#[test]
fn a_weight_must_be_positive_and_an_asset_weight_must_name_a_symbol() {
    assert!(Weight::asset("AAPL", Decimal::from(35)).validate().is_ok());
    assert!(Weight::cash(Decimal::from(5)).validate().is_ok());

    assert!(Weight::asset("AAPL", Decimal::ZERO).validate().is_err());
    assert!(Weight::cash(Decimal::from(-1)).validate().is_err());

    let mut anonymous = Weight::asset("AAPL", Decimal::from(35));
    anonymous.symbol = None;
    assert!(anonymous.validate().is_err());
}

#[tokio::test]
async fn a_portfolio_with_a_bad_weight_never_reaches_the_network() {
    let server = MockServer::start().await;
    let portfolio = CreatePortfolioRequest::new(
        "My Portfolio",
        "Some description",
        vec![Weight::asset("AAPL", Decimal::ZERO)],
        2,
    );

    let error = client(&server)
        .create_portfolio(&portfolio)
        .await
        .unwrap_err();

    assert!(matches!(error, alpaca_sdk::Error::InvalidRequest(_)));
    assert!(server.received_requests().await.unwrap().is_empty());
}

// ----------------------------------------------------------------- routes

#[tokio::test]
async fn creating_a_portfolio_sends_its_weights_as_strings() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/rebalancing/portfolios"))
        .and(body_json(json!({
            "name": "My Portfolio",
            "description": "Some description",
            "cooldown_days": 2,
            // A cash weight has no symbol, so the key is *omitted* rather than
            // sent as `null` — the same rule `AccountConfiguration` needed,
            // where a `null` reached a field the schema does not document.
            "weights": [
                { "type": "asset", "symbol": "AAPL", "percent": "35" },
                { "type": "cash", "percent": "65" }
            ]
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "broker/test_rebalancing_routes__test_create_portfolio__01.json",
        )))
        .expect(1)
        .mount(&server)
        .await;

    client(&server)
        .create_portfolio(&CreatePortfolioRequest::new(
            "My Portfolio",
            "Some description",
            vec![
                Weight::asset("AAPL", Decimal::from(35)),
                Weight::cash(Decimal::from(65)),
            ],
            2,
        ))
        .await
        .unwrap();
}

#[tokio::test]
async fn portfolios_are_a_bare_array_with_no_paging() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/rebalancing/portfolios"))
        .and(query_param("status", "active"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "broker/test_rebalancing_routes__test_get_all_portfolios__01.json",
        )))
        .expect(1)
        .mount(&server)
        .await;

    let mut filter = alpaca_sdk::broker::GetPortfoliosRequest::default();
    filter.status = Some(PortfolioStatus::Active);

    let portfolios = client(&server)
        .get_all_portfolios(Some(&filter))
        .await
        .unwrap();
    assert_eq!(portfolios.len(), 3);
}

#[tokio::test]
async fn updating_and_inactivating_a_portfolio() {
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path(format!("/v1/rebalancing/portfolios/{PORTFOLIO_ID}")))
        .and(body_json(json!({ "cooldown_days": 7 })))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "broker/test_rebalancing_routes__test_update_portfolio_by_id__01.json",
        )))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path(format!("/v1/rebalancing/portfolios/{PORTFOLIO_ID}")))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    let portfolio_id = Uuid::parse_str(PORTFOLIO_ID).unwrap();
    let client = client(&server);

    let mut update = UpdatePortfolioRequest::default();
    update.cooldown_days = Some(7);

    client
        .update_portfolio_by_id(portfolio_id, &update)
        .await
        .unwrap();
    client
        .inactivate_portfolio_by_id(portfolio_id)
        .await
        .unwrap();
}

#[tokio::test]
async fn subscribing_and_unsubscribing_an_account() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/rebalancing/subscriptions"))
        .and(body_json(json!({
            "account_id": ACCOUNT_ID,
            "portfolio_id": PORTFOLIO_ID
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "broker/test_rebalancing_routes__test_create_subscription__01.json",
        )))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path(format!(
            "/v1/rebalancing/subscriptions/{SUBSCRIPTION_ID}"
        )))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    let client = client(&server);
    client
        .create_subscription(&CreateSubscriptionRequest::new(
            Uuid::parse_str(ACCOUNT_ID).unwrap(),
            Uuid::parse_str(PORTFOLIO_ID).unwrap(),
        ))
        .await
        .unwrap();
    client
        .unsubscribe_account(Uuid::parse_str(SUBSCRIPTION_ID).unwrap())
        .await
        .unwrap();
}

#[test]
fn a_subscription_page_carries_its_next_token() {
    let page: SubscriptionsPage =
        parse("broker/test_rebalancing_routes__test_get_all_subscriptions__01.json");

    assert_eq!(page.subscriptions.len(), 1);
    // null, meaning this was the last page.
    assert_eq!(page.next_page_token, None);
    assert_eq!(page.subscriptions[0].last_rebalanced_at, None);
}

fn subscription(id: &str) -> serde_json::Value {
    json!({
        "id": id,
        "account_id": ACCOUNT_ID,
        "portfolio_id": PORTFOLIO_ID,
        "last_rebalanced_at": null,
        "created_at": "2022-08-07T23:52:05.942964Z"
    })
}

#[tokio::test]
async fn walking_subscriptions_follows_the_token_until_it_is_absent() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/rebalancing/subscriptions"))
        .and(query_param("page_token", "page-two"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "subscriptions": [subscription(THIRD_SUBSCRIPTION_ID)],
            "next_page_token": null
        })))
        .expect(1)
        .mount(&server)
        .await;
    // The first request carries no token at all.
    Mock::given(method("GET"))
        .and(path("/v1/rebalancing/subscriptions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "subscriptions": [subscription(SUBSCRIPTION_ID), subscription(OTHER_SUBSCRIPTION_ID)],
            "next_page_token": "page-two"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let subscriptions = client(&server)
        .get_all_subscriptions(None, None)
        .await
        .unwrap();

    assert_eq!(subscriptions.len(), 3);
}

#[tokio::test]
async fn an_empty_page_stops_the_walk_even_with_a_token() {
    // A token pointing at an empty page would otherwise loop forever.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/rebalancing/subscriptions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "subscriptions": [],
            "next_page_token": "there-is-always-another-page"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let subscriptions = client(&server)
        .get_all_subscriptions(None, None)
        .await
        .unwrap();

    assert!(subscriptions.is_empty());
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn max_items_narrows_the_page_size_rather_than_over_fetching() {
    // The last request asks for exactly what is still wanted.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/rebalancing/subscriptions"))
        .and(query_param("limit", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "subscriptions": [subscription(SUBSCRIPTION_ID), subscription(OTHER_SUBSCRIPTION_ID)],
            "next_page_token": "page-two"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let subscriptions = client(&server)
        .get_all_subscriptions(None, Some(2))
        .await
        .unwrap();

    assert_eq!(subscriptions.len(), 2);
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn runs_filter_by_type_under_the_type_key() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/rebalancing/runs"))
        .and(query_param("type", "full_rebalance"))
        .and(query_param("account_id", ACCOUNT_ID))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "runs": [fixture("broker/test_rebalancing_routes__test_get_run_by_id__01.json")],
            "next_page_token": null
        })))
        .expect(1)
        .mount(&server)
        .await;

    let mut filter = GetRunsRequest::default();
    filter.account_id = Some(Uuid::parse_str(ACCOUNT_ID).unwrap());
    filter.run_type = Some(RunType::FullRebalance);

    let page = client(&server).get_runs(Some(&filter)).await.unwrap();
    assert_eq!(page.runs.len(), 1);
}

#[tokio::test]
async fn creating_fetching_and_cancelling_a_run() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/rebalancing/runs"))
        .and(body_json(json!({
            "account_id": ACCOUNT_ID,
            "type": "full_rebalance",
            "weights": [{ "type": "asset", "symbol": "SPY", "percent": "100" }]
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "broker/test_rebalancing_routes__test_create_manual_run__01.json",
        )))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/rebalancing/runs/{RUN_ID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "broker/test_rebalancing_routes__test_get_run_by_id__01.json",
        )))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path(format!("/v1/rebalancing/runs/{RUN_ID}")))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    let client = client(&server);
    client
        .create_manual_run(&CreateRunRequest::new(
            Uuid::parse_str(ACCOUNT_ID).unwrap(),
            RunType::FullRebalance,
            vec![Weight::asset("SPY", Decimal::from(100))],
        ))
        .await
        .unwrap();

    let run_id = Uuid::parse_str(RUN_ID).unwrap();
    client.get_run_by_id(run_id).await.unwrap();
    client.cancel_run_by_id(run_id).await.unwrap();
}

#[tokio::test]
async fn subscriptions_and_runs_are_separate_paths() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/v1/rebalancing/subscriptions/{SUBSCRIPTION_ID}"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "broker/test_rebalancing_routes__test_get_subscription_by_id__01.json",
        )))
        .expect(1)
        .mount(&server)
        .await;

    let subscription: Subscription = client(&server)
        .get_subscription_by_id(Uuid::parse_str(SUBSCRIPTION_ID).unwrap())
        .await
        .unwrap();

    assert_eq!(subscription.id.to_string(), SUBSCRIPTION_ID);
}

#[tokio::test]
async fn a_subscriptions_filter_reaches_the_query_string() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/rebalancing/subscriptions"))
        .and(query_param("account_id", ACCOUNT_ID))
        .and(query_param("limit", "50"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "broker/test_rebalancing_routes__test_get_all_subscriptions__01.json",
        )))
        .expect(1)
        .mount(&server)
        .await;

    let mut filter = GetSubscriptionsRequest::default();
    filter.account_id = Some(Uuid::parse_str(ACCOUNT_ID).unwrap());
    filter.limit = Some(50);

    client(&server)
        .get_subscriptions(Some(&filter))
        .await
        .unwrap();
}

#[tokio::test]
async fn the_subscriptions_walk_stops_when_the_token_never_changes() {
    // An echoed token pages in a circle. The token cannot be trusted to advance,
    // so the walk measures progress in subscriptions it had not already seen.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/rebalancing/subscriptions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "subscriptions": [subscription(SUBSCRIPTION_ID), subscription(OTHER_SUBSCRIPTION_ID)],
            "next_page_token": "the-same-token-forever"
        })))
        .mount(&server)
        .await;

    let subscriptions = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        client(&server).get_all_subscriptions(None, None),
    )
    .await
    .expect("the subscriptions walk never terminated against a repeated page token")
    .unwrap();

    assert_eq!(subscriptions.len(), 2);
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

#[tokio::test]
async fn the_runs_walk_stops_when_the_token_never_changes() {
    let server = MockServer::start().await;
    let one = fixture("broker/test_rebalancing_routes__test_get_run_by_id__01.json");
    let mut other = one.clone();
    other["id"] = json!("00000000-0000-4000-8000-000000000001");
    Mock::given(method("GET"))
        .and(path("/v1/rebalancing/runs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "runs": [one, other],
            "next_page_token": "the-same-token-forever"
        })))
        .mount(&server)
        .await;

    let runs = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        client(&server).get_all_runs(None, None),
    )
    .await
    .expect("the rebalancing runs walk never terminated against a repeated page token")
    .unwrap();

    assert_eq!(runs.len(), 2);
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}
