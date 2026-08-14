//! Account activities and the CIP records.
//!
//! Activities page a third way — by cursor, where the cursor is the last
//! activity's own id — so the walk is tested against a mock that hands back
//! pages and then an empty one.
//!
//! The CIP models have never met a real payload — the sandbox is reported to
//! answer 404 for those routes — so there is no fixture to check them against.
//! What is here is built from the broker spec and asserts the shape the crate
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
    // The broker returns the trading activity records unchanged: unlike Order
    // and TradeAccount there is no correspondent-only field to add.
    // account_id is already on the trading record,
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

    let mut filter = GetAccountActivitiesRequest::default();
    filter.account_id = Some(Uuid::parse_str(ACCOUNT_ID).unwrap());
    filter.activity_types = Some(vec![ActivityType::Fill, ActivityType::Div]);
    filter.direction = Some(Sort::Asc);

    let activities = client(&server)
        .get_account_activities(Some(&filter))
        .await
        .unwrap();
    assert_eq!(activities.len(), 5);
}

#[tokio::test]
async fn date_with_after_is_alpacas_to_reject_not_ours() {
    // `date` alongside `after` or `until` is a plausible conflict. The
    // reference documents no rule against it — the one exclusivity it does document is between
    // `category` and `activity_types`, which this filter does not carry. So the
    // combination is sent and Alpaca answers.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/accounts/activities"))
        .and(query_param("date", "2022-03-04T00:00:00Z"))
        .and(query_param("after", "2022-03-01T00:00:00Z"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(ACTIVITIES)))
        .expect(1)
        .mount(&server)
        .await;

    let mut filter = GetAccountActivitiesRequest::default();
    filter.date = Some("2022-03-04T00:00:00Z".parse().unwrap());
    filter.after = Some("2022-03-01T00:00:00Z".parse().unwrap());

    client(&server)
        .get_account_activities(Some(&filter))
        .await
        .unwrap();
}

/// The by-type route took a raw `&[(&str, String)]` until the parameter check
/// prompted a look at its sibling. It was never *reported* missing anything —
/// the check widens each route to its whole module, and `page_size` was already
/// named there by the other request type. A demonstrated false negative, and the
/// reason that limitation is written down rather than assumed away.
#[tokio::test]
async fn activities_of_one_type_take_the_same_typed_filter() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/accounts/activities/FILL"))
        .and(query_param("page_size", "25"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(ACTIVITIES)))
        .expect(1)
        .mount(&server)
        .await;

    let mut filter = GetAccountActivitiesRequest::default();
    filter.page_size = Some(25);

    let activities = client(&server)
        .get_account_activities_by_type(&ActivityType::Fill, Some(&filter))
        .await
        .unwrap();
    assert_eq!(activities.len(), 5);
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
    // No fixture exists for these. This asserts the shape the crate commits to, and that both routes are wired to
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

#[tokio::test]
async fn the_activities_walk_stops_when_the_server_ignores_the_cursor() {
    // Setting `date` makes this endpoint answer with everything and ignore
    // paging, which the walk sees as a full page whose cursor has no effect.
    // Ending only on an empty page would spin here forever, collecting the same
    // activities on every pass until the process ran out of memory.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/accounts/activities"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!([activity("a::1"), activity("a::2")])),
        )
        .mount(&server)
        .await;

    let activities = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        client(&server).get_all_account_activities(None, None),
    )
    .await
    .expect("the activities walk never terminated against a server that ignores the cursor")
    .unwrap();

    // The repeated page is recognised as ground already covered, so the second
    // request ends the walk rather than extending it.
    assert_eq!(activities.len(), 2);
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}
