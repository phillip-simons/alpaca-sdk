//! Journals: moving cash and securities between accounts.
//!
//! Two things here are easy to get wrong from the Python alone. The amounts are
//! declared `float` and arrive as strings, and a batch journal reports failures
//! per entry rather than failing the request.

#![cfg(feature = "broker")]

use alpaca_sdk::broker::{
    BatchJournalRequestEntry, BatchJournalResponse, BrokerClient, CreateBatchJournalRequest,
    CreateJournalRequest, CreateReverseBatchJournalRequest, GetJournalsRequest, Journal,
    JournalEntryType, JournalStatus, ReverseBatchJournalRequestEntry,
};
use alpaca_sdk::{Credentials, RestConfig, RetryConfig};
use rust_decimal::Decimal;
use serde_json::json;
use uuid::Uuid;
use wiremock::matchers::{body_json, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const JOURNAL_ID: &str = "a7a50677-2983-4c68-96dc-aff62fe3b8cf";
const FROM_ACCOUNT: &str = "ff7b9e35-90e7-453d-a410-b508e1971a36";
const TO_ACCOUNT: &str = "a4c80770-edca-45bc-b35c-cfdf2ed46649";

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

fn from_account() -> Uuid {
    Uuid::parse_str(FROM_ACCOUNT).unwrap()
}

fn to_account() -> Uuid {
    Uuid::parse_str(TO_ACCOUNT).unwrap()
}

// ----------------------------------------------------------------- models

#[test]
fn a_journal_reads_its_amount_as_a_decimal_not_a_float() {
    // net_amount, qty and price arrive as strings — "115.5" — not numbers.
    // Reading them as floats is the precision loss the money types exist to
    // avoid.
    let journal: Journal = parse("broker/test_journal_routes__test_create_journal__01.json");

    assert_eq!(journal.net_amount, Some(Decimal::new(1155, 1)));
    assert_eq!(journal.entry_type, JournalEntryType::Cash);
    assert_eq!(journal.status, JournalStatus::Executed);
    assert_eq!(journal.settle_date.unwrap().to_string(), "2020-12-24");
    // Cash journal, so the security fields are absent.
    assert_eq!(journal.symbol, None);
    assert_eq!(journal.qty, None);
}

#[test]
fn a_batch_response_carries_each_entrys_outcome() {
    // The batch route answers 200 with per-entry results; a failed entry says so
    // in error_message rather than failing the request. Here both succeeded, so
    // the field is the empty string — which must read as "no error", not as one.
    let responses: Vec<BatchJournalResponse> =
        parse("broker/test_journal_routes__test_batch_journal__01.json");

    assert_eq!(responses.len(), 2);
    assert!(responses.iter().all(|r| r.error_message.is_none()));
    assert_eq!(responses[0].journal.net_amount, Some(Decimal::from(10)));
    assert_eq!(responses[1].journal.net_amount, Some(Decimal::from(100)));
    assert_eq!(responses[0].journal.status, JournalStatus::Pending);
    // "symbol": "" on a cash journal is absence, not a symbol.
    assert_eq!(responses[0].journal.symbol, None);
}

#[test]
fn a_failed_batch_entry_keeps_its_error_message() {
    let mut payload = fixture("broker/test_journal_routes__test_batch_journal__01.json");
    payload[0]["error_message"] = json!("insufficient buying power");

    let responses: Vec<BatchJournalResponse> = serde_json::from_value(payload).unwrap();
    assert_eq!(
        responses[0].error_message.as_deref(),
        Some("insufficient buying power")
    );
    assert!(responses[1].error_message.is_none());
}

// ------------------------------------------------------------- validation

#[test]
fn cash_and_security_journals_may_not_borrow_each_others_fields() {
    // amount belongs to cash journals, symbol and qty to security journals,
    // and neither may be empty.
    let cash = CreateJournalRequest::cash(from_account(), to_account(), Decimal::from(50));
    assert!(cash.validate().is_ok());

    let mut cash_with_symbol = cash.clone();
    cash_with_symbol.symbol = Some("AAPL".to_owned());
    assert!(cash_with_symbol.validate().is_err());

    let mut cash_without_amount = cash;
    cash_without_amount.amount = None;
    assert!(cash_without_amount.validate().is_err());

    let security =
        CreateJournalRequest::security(from_account(), to_account(), "AAPL", Decimal::from(2));
    assert!(security.validate().is_ok());

    let mut security_with_amount = security.clone();
    security_with_amount.amount = Some(Decimal::from(50));
    assert!(security_with_amount.validate().is_err());

    let mut security_without_qty = security;
    security_without_qty.qty = None;
    assert!(security_without_qty.validate().is_err());
}

#[tokio::test]
async fn an_invalid_journal_never_reaches_the_network() {
    let server = MockServer::start().await;
    let mut journal = CreateJournalRequest::cash(from_account(), to_account(), Decimal::from(50));
    journal.qty = Some(Decimal::from(1));

    let error = client(&server).create_journal(&journal).await.unwrap_err();

    assert!(matches!(error, alpaca_sdk::Error::InvalidRequest(_)));
    assert!(server.received_requests().await.unwrap().is_empty());
}

// ----------------------------------------------------------------- routes

#[tokio::test]
async fn creating_a_journal_posts_both_accounts_in_the_body() {
    // Not account-scoped: the path carries no id, both accounts are in the body.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/journals"))
        .and(body_json(json!({
            "from_account": FROM_ACCOUNT,
            "to_account": TO_ACCOUNT,
            "entry_type": "JNLC",
            "amount": "115.5"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "broker/test_journal_routes__test_create_journal__01.json",
        )))
        .expect(1)
        .mount(&server)
        .await;

    client(&server)
        .create_journal(&CreateJournalRequest::cash(
            from_account(),
            to_account(),
            Decimal::new(1155, 1),
        ))
        .await
        .unwrap();
}

#[tokio::test]
async fn a_security_journal_sends_symbol_and_qty_instead_of_an_amount() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/journals"))
        .and(body_json(json!({
            "from_account": FROM_ACCOUNT,
            "to_account": TO_ACCOUNT,
            "entry_type": "JNLS",
            "symbol": "AAPL",
            "qty": "2"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "broker/test_journal_routes__test_create_journal__01.json",
        )))
        .expect(1)
        .mount(&server)
        .await;

    client(&server)
        .create_journal(&CreateJournalRequest::security(
            from_account(),
            to_account(),
            "AAPL",
            Decimal::from(2),
        ))
        .await
        .unwrap();
}

#[tokio::test]
async fn the_two_batch_routes_are_distinct_paths() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/journals/batch"))
        .and(body_json(json!({
            "entry_type": "JNLC",
            "from_account": FROM_ACCOUNT,
            "entries": [{ "to_account": TO_ACCOUNT, "amount": "10" }]
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "broker/test_journal_routes__test_batch_journal__01.json",
        )))
        .expect(1)
        .mount(&server)
        .await;

    let responses = client(&server)
        .create_batch_journal(&CreateBatchJournalRequest::new(
            from_account(),
            vec![BatchJournalRequestEntry::new(
                to_account(),
                Decimal::from(10),
            )],
        ))
        .await
        .unwrap();
    assert_eq!(responses.len(), 2);

    // The reverse batch draws from many accounts into one, so its entries name
    // a from_account and the request names the to_account.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/journals/reverse_batch"))
        .and(body_json(json!({
            "entry_type": "JNLC",
            "to_account": TO_ACCOUNT,
            "entries": [{ "from_account": FROM_ACCOUNT, "amount": "10" }]
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "broker/test_journal_routes__test_reverse_batch_journal__01.json",
        )))
        .expect(1)
        .mount(&server)
        .await;

    client(&server)
        .create_reverse_batch_journal(&CreateReverseBatchJournalRequest::new(
            to_account(),
            vec![ReverseBatchJournalRequestEntry::new(
                from_account(),
                Decimal::from(10),
            )],
        ))
        .await
        .unwrap();
}

#[tokio::test]
async fn listing_journals_sends_every_filter_it_is_given() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/journals"))
        .and(query_param("after", "2020-12-01"))
        .and(query_param("before", "2020-12-31"))
        .and(query_param("status", "executed"))
        .and(query_param("entry_type", "JNLC"))
        .and(query_param("to_account", TO_ACCOUNT))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "broker/test_journal_routes__test_get_journals__01.json",
        )))
        .expect(1)
        .mount(&server)
        .await;

    let filter = GetJournalsRequest {
        after: Some("2020-12-01".parse().unwrap()),
        before: Some("2020-12-31".parse().unwrap()),
        status: Some(JournalStatus::Executed),
        entry_type: Some(JournalEntryType::Cash),
        to_account: Some(to_account()),
        from_account: None,
    };

    let journals = client(&server).get_journals(Some(&filter)).await.unwrap();
    assert!(!journals.is_empty());
}

#[tokio::test]
async fn fetching_and_cancelling_one_journal() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/journals/{JOURNAL_ID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "broker/test_journal_routes__test_get_journal_by_id__01.json",
        )))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path(format!("/v1/journals/{JOURNAL_ID}")))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    let journal_id = Uuid::parse_str(JOURNAL_ID).unwrap();
    let client = client(&server);

    client.get_journal_by_id(journal_id).await.unwrap();
    client.cancel_journal_by_id(journal_id).await.unwrap();
}
