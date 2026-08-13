//! The trading routes with no captured payload behind them: locates,
//! tokenization, crypto funding, watchlists by name, the per-market calendar,
//! activities of one type, and do-not-exercise.
//!
//! None of these has a captured payload in any SDK, so the bodies here are the
//! published reference's own examples. That is weaker evidence than
//! `fixtures/`, and the tests say so rather than implying otherwise. What they
//! *do* pin down with certainty is the request this crate sends — the method,
//! the path, and above all the **version segment**, which is what the port got
//! wrong on three event streams and is the thing `just coverage` cannot check.

#![cfg(feature = "trading")]

use alpaca_sdk::trading::{
    ActivityCategory, ActivityType, CreateLocateRequest, CreateWhitelistedAddressRequest,
    CryptoChain, GetAccountActivitiesRequest, GetLocateQuotesRequest, GetLocatesRequest,
    LocateStatus, Market, MintTokenRequest, TokenizationIssuer, TokenizationNetwork,
    TokenizationStatus, TradingClient, UpdateWatchlistRequest, WhitelistStatus,
};
use alpaca_sdk::types::{AssetIdent, Sort};
use alpaca_sdk::{Credentials, RestConfig, RetryConfig};

fn fixture(name: &str) -> serde_json::Value {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name);
    let body = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    serde_json::from_str(&body).unwrap()
}
use rust_decimal::Decimal;
use serde_json::json;
use wiremock::matchers::{body_json, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn client(server: &MockServer) -> TradingClient {
    TradingClient::with_config(
        &Credentials::new("key", "secret").unwrap(),
        RestConfig::new(server.uri()).retry(RetryConfig::none()),
    )
    .unwrap()
}

// --------------------------------------------------------------- locates

#[tokio::test]
async fn locates_are_a_v1_route_on_a_v2_client() {
    // The whole point of `at_version`. The trading client is v2; these are not,
    // and a v2 path here would 404 while `just coverage` still showed a tick.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/locates"))
        .and(query_param("status", "active"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "locates": [{
                "all_or_none": false,
                "created_at": "2026-01-02T15:04:05Z",
                "expires_at": "2026-01-03T01:00:00Z",
                "id": "550e8400-e29b-41d4-a716-446655440000",
                "limit_price": "0.05",
                "located_price": "0.05",
                "located_qty": 100,
                "requested_qty": 100,
                "status": "active",
                "symbol": "TSLA",
                "total_fee": "5.00",
            }],
            "next_page_token": null,
        })))
        .expect(1)
        .mount(&server)
        .await;

    let filter = GetLocatesRequest::new().status(LocateStatus::Active);
    let page = client(&server).get_locates(Some(&filter)).await.unwrap();

    assert_eq!(page.locates.len(), 1);
    assert_eq!(page.locates[0].symbol, "TSLA");
    // Fees cross the wire as strings, so they are Decimal.
    assert_eq!(page.locates[0].total_fee, Some(Decimal::new(500, 2)));
    assert_eq!(page.next_page_token, None);
}

#[tokio::test]
async fn a_locate_quote_reports_per_symbol_failures_beside_the_quotes() {
    // Asking about an easy-to-borrow symbol is not an error; it comes back
    // under `errors` with the rest of the request still served.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/locates/quotes"))
        .and(query_param("symbols", "TSLA,AAPL"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "quotes": [{
                "available_qty": 5000,
                "price": "0.05",
                "quoted_at": "2026-01-02T15:04:05Z",
                "symbol": "TSLA",
            }],
            "errors": [{
                "code": "easy_to_borrow",
                "message": "AAPL is easy to borrow and does not require a locate",
                "symbol": "AAPL",
            }],
        })))
        .expect(1)
        .mount(&server)
        .await;

    let request = GetLocateQuotesRequest::new(vec!["TSLA".to_owned(), "AAPL".to_owned()]);
    let quotes = client(&server).get_locate_quotes(&request).await.unwrap();

    assert_eq!(quotes.quotes.len(), 1);
    assert_eq!(quotes.errors.len(), 1);
    assert_eq!(quotes.errors[0].symbol, "AAPL");
}

#[tokio::test]
async fn a_degenerate_locate_never_reaches_the_server() {
    let server = MockServer::start().await;
    // No mock mounted: any request at all would fail the test.
    let error = client(&server)
        .create_locate(&CreateLocateRequest::new("TSLA", 0))
        .await
        .unwrap_err();

    assert!(matches!(error, alpaca_sdk::Error::InvalidRequest(_)));
}

// ---------------------------------------------------------------- calendar

#[tokio::test]
async fn the_per_market_calendar_is_a_v3_route() {
    // Three versions of the same idea are live at once: /v2/calendar here,
    // /v3/calendar/{market} for this one, and /v2/calendar/{market} on the
    // broker API. The payload was captured from the real v3 route by
    // `just capture`, which is the only way to confirm the segment — a mock
    // answers whatever it is pointed at.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v3/calendar/XNYS"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(fixture("live/trading_calendar_market_v3.json")),
        )
        .expect(1)
        .mount(&server)
        .await;

    let calendar = client(&server)
        .get_market_calendar(&Market::Xnys, None)
        .await
        .unwrap();

    assert_eq!(calendar.calendar.len(), 5);
    // The sessions arrive as offset timestamps (`-04:00`), not `Z` — absolute
    // instants either way, unlike the v2 calendar's naive eastern-time open and
    // close. Reading one as the other is an off-by-four-hours bug.
    assert_eq!(
        calendar.calendar[0].core_start.to_rfc3339(),
        "2026-08-10T13:30:00+00:00"
    );
    // NYSE takes no lunch break; the field is absent rather than null.
    assert_eq!(calendar.calendar[0].lunch_start, None);
    assert!(calendar.calendar[0].pre_start.is_some());
}

// ------------------------------------------------------ watchlists by name

#[tokio::test]
async fn a_watchlist_by_name_puts_the_name_in_the_query_not_the_path() {
    // The route is literally `/v2/watchlists:by_name`, colon and all, and the
    // name is a parameter. Folding it into the path would skip its encoding.
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/v2/watchlists:by_name"))
        .and(query_param("name", "my list"))
        .and(body_json(json!({"name": "renamed"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "account_id": "550e8400-e29b-41d4-a716-446655440000",
            "id": "550e8400-e29b-41d4-a716-446655440001",
            "name": "renamed",
            "created_at": "2026-01-02T15:04:05Z",
            "updated_at": "2026-01-02T15:04:05Z",
            "assets": [],
        })))
        .expect(1)
        .mount(&server)
        .await;

    let update = UpdateWatchlistRequest::new().name("renamed");
    let watchlist = client(&server)
        .update_watchlist_by_name("my list", &update)
        .await
        .unwrap();

    assert_eq!(watchlist.name, "renamed");
}

#[tokio::test]
async fn deleting_a_watchlist_by_name_sends_the_name_and_expects_no_body() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/v2/watchlists:by_name"))
        .and(query_param("name", "gone"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    client(&server)
        .delete_watchlist_by_name("gone")
        .await
        .unwrap();
}

// -------------------------------------------------------------- activities

#[tokio::test]
async fn activities_of_one_type_move_the_type_into_the_path() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/account/activities/FILL"))
        // The narrowed route returns the same element type as the unfiltered
        // one, so this is the shape `fixtures/` already verifies rather than
        // the looser one the spec draws.
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
            "activity_type": "FILL",
            "id": "20260102000000000::1",
            "account_id": "550e8400-e29b-41d4-a716-446655440000",
            "symbol": "AAPL",
            "qty": "10",
            "price": "185.00",
            "side": "buy",
            "type": "fill",
            "leaves_qty": "0",
            "cum_qty": "10",
            "order_id": "550e8400-e29b-41d4-a716-446655440002",
            "order_status": "filled",
            "transaction_time": "2026-01-02T15:04:05Z",
        }])))
        .expect(1)
        .mount(&server)
        .await;

    let activities = client(&server)
        .get_account_activities_by_type(&alpaca_sdk::trading::ActivityType::Fill, None)
        .await
        .unwrap();

    assert_eq!(activities.len(), 1);
}

/// These filters were reachable before — the method took a raw `&[(&str,
/// String)]` — but not nameable, which is why `just parameters` reported them
/// missing. The broker's equivalent route has had a typed request all along.
#[tokio::test]
async fn account_activities_send_their_typed_filters() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/account/activities"))
        .and(query_param("activity_types", "FILL,DIV"))
        .and(query_param("page_size", "50"))
        .and(query_param("direction", "asc"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .expect(1)
        .mount(&server)
        .await;

    let filter = GetAccountActivitiesRequest {
        activity_types: Some(vec![ActivityType::Fill, ActivityType::Div]),
        page_size: Some(50),
        direction: Some(Sort::Asc),
        ..GetAccountActivitiesRequest::default()
    };

    client(&server)
        .get_account_activities(Some(&filter))
        .await
        .unwrap();
}

/// "Cannot be used with `activity_types` parameter" is the reference's own
/// wording, so it is a rule and not a guess. Rejected before the request is
/// sent, like the broker's copy of the same filter.
#[tokio::test]
async fn activity_types_and_category_cannot_be_combined() {
    let server = MockServer::start().await;

    let filter = GetAccountActivitiesRequest {
        activity_types: Some(vec![ActivityType::Fill]),
        category: Some(ActivityCategory::TradeActivity),
        ..GetAccountActivitiesRequest::default()
    };

    let error = client(&server)
        .get_account_activities(Some(&filter))
        .await
        .unwrap_err();

    assert!(
        matches!(error, alpaca_sdk::Error::InvalidRequest(_)),
        "{error:?}"
    );
    // Nothing was sent: the mock server saw no request at all.
    assert!(server.received_requests().await.unwrap().is_empty());
}

// -------------------------------------------------------------------- dne

#[tokio::test]
async fn do_not_exercise_answers_with_no_content() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v2/positions/AAPL260116C00150000/do-not-exercise"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    let contract: AssetIdent = "AAPL260116C00150000".into();
    client(&server)
        .exercise_do_not_exercise(&contract)
        .await
        .unwrap();
}

// ----------------------------------------------------------- tokenization

#[tokio::test]
async fn minting_a_token_sends_the_reference_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v2/tokenization/mint"))
        .and(body_json(json!({
            "underlying_symbol": "AAPL",
            "qty": "1.5",
            "issuer": "xstocks",
            "network": "solana",
            "wallet_address": "9xQeWvG816bUx9EPa2",
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "created_at": "2026-01-02T15:04:05Z",
            "issuer": "xstocks",
            "network": "solana",
            "qty": "1.5",
            "status": "pending",
            "token_symbol": "AAPLx",
            "tokenization_request_id": "abc",
            "underlying_symbol": "AAPL",
        })))
        .expect(1)
        .mount(&server)
        .await;

    let request = MintTokenRequest::new(
        "AAPL",
        Decimal::new(15, 1),
        TokenizationIssuer::Xstocks,
        TokenizationNetwork::Solana,
        "9xQeWvG816bUx9EPa2",
    );
    let minted = client(&server).mint_token(&request).await.unwrap();

    assert_eq!(minted.status, TokenizationStatus::Pending);
    assert_eq!(minted.token_symbol, "AAPLx");
}

// --------------------------------------------------------- crypto funding

#[tokio::test]
async fn a_new_whitelisted_address_starts_pending() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v2/wallets/whitelists"))
        .and(body_json(json!({
            "address": "0xabc",
            "asset": "ETH",
            "chain": "ETH",
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "address": "0xabc",
            "asset": "ETH",
            "chain": "ETH",
            "created_at": "2026-01-02T15:04:05Z",
            "id": "1",
            "status": "PENDING",
        })))
        .expect(1)
        .mount(&server)
        .await;

    let request = CreateWhitelistedAddressRequest::new("0xabc", "ETH", CryptoChain::Eth);
    let entry = client(&server)
        .create_whitelisted_address(&request)
        .await
        .unwrap();

    // Upper case on the wire, unlike the fiat transfer statuses.
    assert_eq!(entry.status, Some(WhitelistStatus::Pending));
}
