//! Account activities and the CIP records.
//!
//! Activities page a third way — by cursor, where the cursor is the last
//! activity's own id — so the walk is tested against a mock that hands back
//! pages and then an empty one.
//!
//! The CIP models have never met a real payload: alpaca-py's two CIP methods are
//! empty stubs, so there is no fixture to check them against. What is here is
//! built from `alpaca/broker/models/cip.py` and asserts the shape the port
//! commits to, not the shape Alpaca is known to send.

#![cfg(feature = "broker")]

use alpaca_sdk::broker::{BrokerClient, CIPInfo, GetAccountActivitiesRequest};
use alpaca_sdk::trading::{Activity, ActivityType};
use alpaca_sdk::types::Sort;
use alpaca_sdk::{Credentials, RestConfig, RetryConfig};
use serde_json::json;
use uuid::Uuid;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const ACCOUNT_ID: &str = "3dcb795c-3ccc-402a-abb9-07e26a1b1326";

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

const ACTIVITIES: &str = "broker/test_account_activities_routes__test_get_activities_for_account_max_items_and_single_request_date__01.json";

// ------------------------------------------------------------ activities

#[test]
fn broker_activities_are_the_trading_models_unchanged() {
    // alpaca-py imports TradeActivity and NonTradeActivity from alpaca.trading
    // rather than subclassing them, so unlike Order and TradeAccount there is
    // nothing extra to model. account_id is already on the trading record,
    // which matters here: this route spans every account.
    let activities: Vec<Activity> = parse(ACTIVITIES);

    assert_eq!(activities.len(), 5);
    let Activity::Trade(trade) = &activities[0] else {
        panic!("expected a trade activity, got {:?}", activities[0]);
    };
    assert_eq!(trade.account_id.to_string(), ACCOUNT_ID);
    assert_eq!(trade.symbol, "AMZN");
    assert_eq!(trade.activity_type, ActivityType::Fill);
    assert!(trade.id.contains("::"));
}

#[tokio::test]
async fn the_activity_type_filter_is_one_comma_separated_parameter() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/accounts/activities"))
        .and(query_param("activity_types", "FILL,DIV"))
        .and(query_param("account_id", ACCOUNT_ID))
        .and(query_param("direction", "asc"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(ACTIVITIES)))
        .expect(1)
        .mount(&server)
        .await;

    let filter = GetAccountActivitiesRequest {
        account_id: Some(Uuid::parse_str(ACCOUNT_ID).unwrap()),
        activity_types: Some(vec![ActivityType::Fill, ActivityType::Div]),
        direction: Some(Sort::Asc),
        ..Default::default()
    };

    let activities = client(&server)
        .get_account_activities(Some(&filter))
        .await
        .unwrap();
    assert_eq!(activities.len(), 5);
}

#[tokio::test]
async fn date_cannot_be_combined_with_after_or_until() {
    let server = MockServer::start().await;
    let filter = GetAccountActivitiesRequest {
        date: Some("2022-03-04T00:00:00Z".parse().unwrap()),
        after: Some("2022-03-01T00:00:00Z".parse().unwrap()),
        ..Default::default()
    };

    let error = client(&server)
        .get_account_activities(Some(&filter))
        .await
        .unwrap_err();

    assert!(matches!(error, alpaca_sdk::Error::InvalidRequest(_)));
    assert!(server.received_requests().await.unwrap().is_empty());
}

fn activity(id: &str) -> serde_json::Value {
    json!({
        "id": id,
        "account_id": ACCOUNT_ID,
        "activity_type": "FILL",
        "transaction_time": "2022-03-04T18:54:20.903569Z",
        "type": "fill",
        "price": "2907.15",
        "qty": "1",
        "side": "buy",
        "symbol": "AMZN",
        "leaves_qty": "0",
        "order_id": "cddf433b-1a41-497d-ae31-50b1fee56fff",
        "cum_qty": "1",
        "order_status": "filled"
    })
}

#[tokio::test]
async fn the_walk_pages_from_the_last_activitys_own_id() {
    // Not an offset, and not a token the server hands back: the cursor is the
    // id of the last activity on the page just read.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/accounts/activities"))
        .and(query_param("page_token", "second::b"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/accounts/activities"))
        .and(query_param("page_token", "first::b"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!([activity("second::a"), activity("second::b")])),
        )
        .expect(1)
        .mount(&server)
        .await;
    // The first request carries no cursor.
    Mock::given(method("GET"))
        .and(path("/v1/accounts/activities"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!([activity("first::a"), activity("first::b")])),
        )
        .expect(1)
        .mount(&server)
        .await;

    let activities = client(&server)
        .get_all_account_activities(None, None)
        .await
        .unwrap();

    assert_eq!(activities.len(), 4);
    assert_eq!(server.received_requests().await.unwrap().len(), 3);
}

#[tokio::test]
async fn max_items_narrows_the_page_size_and_truncates() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/accounts/activities"))
        .and(query_param("page_size", "3"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            activity("a::1"),
            activity("a::2"),
            activity("a::3"),
            activity("a::4")
        ])))
        .expect(1)
        .mount(&server)
        .await;

    // The endpoint may ignore page_size entirely when `date` is set and answer
    // with everything, so the cap has to hold on the client side too.
    let activities = client(&server)
        .get_all_account_activities(None, Some(3))
        .await
        .unwrap();

    assert_eq!(activities.len(), 3);
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

// ------------------------------------------------------------------- CIP

#[tokio::test]
async fn cip_records_round_trip_through_both_routes() {
    // No fixture exists for these; alpaca-py never implemented them. This
    // asserts the shape the port commits to, and that both routes are wired to
    // the same path.
    let payload = json!({
        "id": "c3b8e2a1-4f6d-4f0a-9c1e-2b7d6a5e4f30",
        "account_id": ACCOUNT_ID,
        "provider_name": ["onfido"],
        "created_at": "2022-03-04T18:54:20.903569Z",
        "updated_at": "2022-03-04T18:54:20.903569Z",
        "kyc": {
            "id": "kyc-1",
            "risk_score": 10,
            "applicant_name": "Jane Doe",
            "approval_status": "approved"
        },
        "photo": {
            "id": "photo-1",
            "result": "clear",
            "face_comparision": "clear"
        }
    });

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/accounts/{ACCOUNT_ID}/cip")))
        .respond_with(ResponseTemplate::new(200).set_body_json(payload.clone()))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("/v1/accounts/{ACCOUNT_ID}/cip")))
        .respond_with(ResponseTemplate::new(200).set_body_json(payload))
        .expect(1)
        .mount(&server)
        .await;

    let account_id = Uuid::parse_str(ACCOUNT_ID).unwrap();
    let client = client(&server);

    let cip: CIPInfo = client
        .get_cip_data_for_account_by_id(account_id)
        .await
        .unwrap();

    let kyc = cip.kyc.as_ref().expect("kyc");
    assert_eq!(kyc.risk_score, Some(10));
    assert_eq!(kyc.applicant_name.as_deref(), Some("Jane Doe"));
    // Absent optional fields stay absent rather than failing the parse.
    assert_eq!(kyc.nationality, None);
    assert_eq!(cip.document, None);

    // face_comparision, spelled the way Alpaca spells it, reaches the field
    // spelled the way English does.
    let photo = cip.photo.as_ref().expect("photo");
    assert!(photo.face_comparison.is_some());

    client
        .upload_cip_data_for_account_by_id(account_id, &cip)
        .await
        .unwrap();
}
