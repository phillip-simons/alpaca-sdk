//! Funding: ACH relationships, recipient banks, and transfers.
//!
//! Money routes, so the wire contract is asserted rather than assumed: the
//! amounts are decimals that arrive as strings, the status filter is one
//! comma-separated parameter, and the transfer list paginates by offset with no
//! token to say when it is done.

#![cfg(feature = "broker")]

use alpaca_sdk::broker::{
    ACHRelationship, ACHRelationshipStatus, Bank, BankAccountType, BankAddress, BrokerClient,
    CreateACHRelationshipRequest, CreateACHTransferRequest, CreateBankRequest,
    CreateBankTransferRequest, CreateTransferRequest, FeePaymentMethod, GetTransfersRequest,
    IdentifierType, ManualACHRelationship, PlaidACHRelationship, Transfer, TransferDirection,
    TransferStatus, TransferTiming, TransferType,
};
use alpaca_sdk::{Credentials, RestConfig, RetryConfig};
use rust_decimal::Decimal;
use serde_json::json;
use uuid::Uuid;
use wiremock::matchers::{body_json, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const ACCOUNT_ID: &str = "2a87c088-ffb6-472b-a4a3-cd9305c8605c";
const RELATIONSHIP_ID: &str = "0f08c6bc-8e9f-463d-a73f-fd047fdb5e94";
const BANK_ID: &str = "9a7fb9b5-1f4d-420f-b6d4-0fd32008cec8";
const TRANSFER_ID: &str = "be3c368a-4c7c-4384-808e-f02c9f5a8afe";

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

/// `BankAddress` is `#[non_exhaustive]` and all five fields are `String`, so it
/// is filled in by name rather than positionally — which is the point.
fn bank_address() -> BankAddress {
    let mut address = BankAddress::default();
    address.country = "USA".to_owned();
    address.state_province = "CA".to_owned();
    address.postal_code = "94401".to_owned();
    address.city = "San Mateo".to_owned();
    address.street_address = "20 N San Mateo Dr".to_owned();
    address
}

fn account_id() -> Uuid {
    Uuid::parse_str(ACCOUNT_ID).unwrap()
}

// ------------------------------------------------------ ACH relationships

#[test]
fn an_ach_relationship_parses_from_the_captured_payload() {
    let relationship: ACHRelationship =
        parse("broker/test_funding_routes__test_create_ach_relationship_for_account__01.json");

    assert_eq!(relationship.status, ACHRelationshipStatus::Queued);
    assert_eq!(relationship.bank_account_type, BankAccountType::Savings);
    assert_eq!(relationship.account_owner_name, "John Doe");
    // Absent from the payload rather than null.
    assert_eq!(relationship.nickname, None);
    assert_eq!(relationship.processor_token, None);
}

#[tokio::test]
async fn creating_an_ach_relationship_takes_bank_details_or_a_plaid_token() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(format!("/v1/accounts/{ACCOUNT_ID}/ach_relationships")))
        .and(body_json(json!({
            "account_owner_name": "John Doe",
            "bank_account_type": "SAVINGS",
            "bank_account_number": "123456789abc",
            "bank_routing_number": "123456789"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "broker/test_funding_routes__test_create_ach_relationship_for_account__01.json",
        )))
        .expect(1)
        .mount(&server)
        .await;

    client(&server)
        .create_ach_relationship_for_account(
            account_id(),
            &CreateACHRelationshipRequest::Manual(ManualACHRelationship::new(
                "John Doe",
                BankAccountType::Savings,
                "123456789abc",
                "123456789",
            )),
        )
        .await
        .unwrap();

    // The Plaid variant posts only the token, to the same route.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(format!("/v1/accounts/{ACCOUNT_ID}/ach_relationships")))
        .and(body_json(
            json!({ "processor_token": "processor-sandbox-abc" }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "broker/test_funding_routes__test_create_ach_relationship_for_account__01.json",
        )))
        .expect(1)
        .mount(&server)
        .await;

    client(&server)
        .create_ach_relationship_for_account(
            account_id(),
            &CreateACHRelationshipRequest::Plaid(PlaidACHRelationship::new(
                "processor-sandbox-abc",
            )),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn the_status_filter_is_one_comma_separated_parameter() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/accounts/{ACCOUNT_ID}/ach_relationships")))
        .and(query_param("statuses", "QUEUED,APPROVED"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "broker/test_funding_routes__test_get_ach_relationships_for_account_with_statuses__01.json",
        )))
        .expect(1)
        .mount(&server)
        .await;

    let relationships = client(&server)
        .get_ach_relationships_for_account(
            account_id(),
            &[
                ACHRelationshipStatus::Queued,
                ACHRelationshipStatus::Approved,
            ],
        )
        .await
        .unwrap();

    assert_eq!(relationships.len(), 1);
}

#[tokio::test]
async fn an_empty_status_filter_sends_no_parameter_at_all() {
    // Sending `statuses=` would filter for the empty status rather than for
    // everything, so an empty list must still be sent as an empty list.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/accounts/{ACCOUNT_ID}/ach_relationships")))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "broker/test_funding_routes__test_get_ach_relationships_for_account__01.json",
        )))
        .expect(1)
        .mount(&server)
        .await;

    client(&server)
        .get_ach_relationships_for_account(account_id(), &[])
        .await
        .unwrap();

    let received = &server.received_requests().await.unwrap()[0];
    assert_eq!(received.url.query(), None);
}

#[tokio::test]
async fn deleting_an_ach_relationship_tolerates_an_empty_body() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path(format!(
            "/v1/accounts/{ACCOUNT_ID}/ach_relationships/{RELATIONSHIP_ID}"
        )))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    client(&server)
        .delete_ach_relationship_for_account(
            account_id(),
            Uuid::parse_str(RELATIONSHIP_ID).unwrap(),
        )
        .await
        .unwrap();
}

// ------------------------------------------------------------------ banks

#[test]
fn a_bank_parses_from_the_captured_payload() {
    let bank: Bank = parse("broker/test_funding_routes__test_create_bank_for_account__01.json");

    assert_eq!(bank.name, "my bank detail");
    assert_eq!(bank.bank_code_type, IdentifierType::Aba);
    // A domestic bank has these as empty strings rather than null or absent.
    assert_eq!(bank.country, "");
    assert_eq!(bank.city, "");
}

#[test]
fn a_domestic_bank_may_not_carry_an_address_and_an_international_one_must() {
    // Both directions are rejected before sending, and the API
    // rejects the request either way, so it is worth catching before the call.
    let mut domestic = CreateBankRequest::domestic("My Bank", "123456789", "123456789abc");
    assert!(domestic.validate().is_ok());

    domestic.city = Some("San Mateo".to_owned());
    assert!(domestic.validate().is_err());

    let international =
        CreateBankRequest::international("My Bank", "BOFAUS3N", "123456789abc", bank_address());
    assert!(international.validate().is_ok());

    // The reference marks every one of the five address fields optional, so an
    // incomplete international bank is Alpaca's to reject — not ours.
    let mut incomplete = international;
    incomplete.postal_code = None;
    assert!(incomplete.validate().is_ok());
}

#[tokio::test]
async fn creating_a_bank_posts_to_recipient_banks() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(format!("/v1/accounts/{ACCOUNT_ID}/recipient_banks")))
        .and(body_json(json!({
            "name": "my bank detail",
            "bank_code_type": "ABA",
            "bank_code": "123456789",
            "account_number": "123456789abc"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "broker/test_funding_routes__test_create_bank_for_account__01.json",
        )))
        .expect(1)
        .mount(&server)
        .await;

    client(&server)
        .create_bank_for_account(
            account_id(),
            &CreateBankRequest::domestic("my bank detail", "123456789", "123456789abc"),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn a_bank_with_a_bad_address_never_reaches_the_network() {
    let server = MockServer::start().await;
    // No mock is mounted: any request at all fails the test.
    let mut bank = CreateBankRequest::domestic("My Bank", "123456789", "123456789abc");
    bank.country = Some("USA".to_owned());

    let error = client(&server)
        .create_bank_for_account(account_id(), &bank)
        .await
        .unwrap_err();

    assert!(matches!(error, alpaca_sdk::Error::InvalidRequest(_)));
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn deleting_a_bank_tolerates_an_empty_body() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path(format!(
            "/v1/accounts/{ACCOUNT_ID}/recipient_banks/{BANK_ID}"
        )))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    client(&server)
        .delete_bank_for_account(account_id(), Uuid::parse_str(BANK_ID).unwrap())
        .await
        .unwrap();
}

// -------------------------------------------------------------- transfers

#[test]
fn a_transfer_keeps_its_amounts_exact() {
    let transfer: Transfer =
        parse("broker/test_funding_routes__test_create_transfer_for_account__01.json");

    // amount is what lands after fees; requested_amount is what was asked for.
    assert_eq!(transfer.amount, Decimal::from(498));
    assert_eq!(transfer.requested_amount, Some(Decimal::from(500)));
    assert_eq!(transfer.fee, Some(Decimal::from(2)));
    assert_eq!(transfer.transfer_type, TransferType::Ach);
    assert_eq!(transfer.status, TransferStatus::Complete);
    assert_eq!(transfer.direction, TransferDirection::Incoming);
    assert_eq!(transfer.fee_payment_method, Some(FeePaymentMethod::User));
    // Null in the payload, not absent.
    assert_eq!(transfer.reason, None);
    // A wire-only field, absent here.
    assert_eq!(transfer.bank_id, None);
}

#[tokio::test]
async fn a_transfer_pins_its_own_type_in_the_body() {
    // One type per transfer kind would need a runtime validator that
    // rejects the other value. The enum makes that unrepresentable, but the
    // field still has to reach the wire.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(format!("/v1/accounts/{ACCOUNT_ID}/transfers")))
        .and(body_json(json!({
            "amount": "500",
            "direction": "INCOMING",
            "timing": "immediate",
            "relationship_id": RELATIONSHIP_ID,
            "transfer_type": "ach"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "broker/test_funding_routes__test_create_transfer_for_account__01.json",
        )))
        .expect(1)
        .mount(&server)
        .await;

    client(&server)
        .create_transfer_for_account(
            account_id(),
            &CreateTransferRequest::Ach(CreateACHTransferRequest::new(
                Decimal::from(500),
                TransferDirection::Incoming,
                TransferTiming::Immediate,
                Uuid::parse_str(RELATIONSHIP_ID).unwrap(),
            )),
        )
        .await
        .unwrap();

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(format!("/v1/accounts/{ACCOUNT_ID}/transfers")))
        .and(body_json(json!({
            "amount": "500",
            "direction": "OUTGOING",
            "timing": "immediate",
            "bank_id": BANK_ID,
            "transfer_type": "wire"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "broker/test_funding_routes__test_create_transfer_for_account__01.json",
        )))
        .expect(1)
        .mount(&server)
        .await;

    client(&server)
        .create_transfer_for_account(
            account_id(),
            &CreateTransferRequest::Wire(CreateBankTransferRequest::new(
                Decimal::from(500),
                TransferDirection::Outgoing,
                TransferTiming::Immediate,
                Uuid::parse_str(BANK_ID).unwrap(),
            )),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn a_non_positive_transfer_never_reaches_the_network() {
    let server = MockServer::start().await;
    let error = client(&server)
        .create_transfer_for_account(
            account_id(),
            &CreateTransferRequest::Ach(CreateACHTransferRequest::new(
                Decimal::ZERO,
                TransferDirection::Incoming,
                TransferTiming::Immediate,
                Uuid::parse_str(RELATIONSHIP_ID).unwrap(),
            )),
        )
        .await
        .unwrap_err();

    assert!(matches!(error, alpaca_sdk::Error::InvalidRequest(_)));
    assert!(server.received_requests().await.unwrap().is_empty());
}

/// A page of `count` distinct transfers, numbered from `first`.
///
/// The ids have to differ. The walk drops a transfer it has already collected,
/// because a server that ignores `offset` and repeats a page is otherwise
/// indistinguishable from one with more to give — so a page of clones would
/// arrive as a single transfer and test nothing about paging.
fn transfer_page(first: usize, count: usize) -> serde_json::Value {
    let one = fixture("broker/test_funding_routes__test_create_transfer_for_account__01.json");
    serde_json::Value::Array(
        (first..first + count)
            .map(|n| {
                let mut transfer = one.clone();
                transfer["id"] = json!(format!("00000000-0000-4000-8000-{n:012}"));
                transfer
            })
            .collect(),
    )
}

#[tokio::test]
async fn walking_the_transfer_pages_stops_on_the_first_empty_one() {
    // This endpoint pages by offset and returns [] when it is done — there is no
    // token or total to check, so an empty page is the only stop condition.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/accounts/{ACCOUNT_ID}/transfers")))
        .and(query_param("offset", "0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(transfer_page(0, 2)))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/accounts/{ACCOUNT_ID}/transfers")))
        .and(query_param("offset", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(transfer_page(2, 1)))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/accounts/{ACCOUNT_ID}/transfers")))
        .and(query_param("offset", "3"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .expect(1)
        .mount(&server)
        .await;

    let transfers = client(&server)
        .get_all_transfers_for_account(account_id(), None, None)
        .await
        .unwrap();

    assert_eq!(transfers.len(), 3);
}

#[tokio::test]
async fn max_items_truncates_mid_page_and_stops_requesting() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/accounts/{ACCOUNT_ID}/transfers")))
        .and(query_param("offset", "0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(transfer_page(0, 5)))
        .expect(1)
        .mount(&server)
        .await;

    let transfers = client(&server)
        .get_all_transfers_for_account(account_id(), None, Some(3))
        .await
        .unwrap();

    // Truncated to the cap, and the second page was never asked for.
    assert_eq!(transfers.len(), 3);
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn one_page_of_transfers_honours_the_filter_as_given() {
    // The single-request form does not touch the offset, so a caller paging by
    // hand keeps control of it.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/accounts/{ACCOUNT_ID}/transfers")))
        .and(query_param("direction", "INCOMING"))
        .and(query_param("limit", "10"))
        .and(query_param("offset", "20"))
        .respond_with(ResponseTemplate::new(200).set_body_json(transfer_page(0, 1)))
        .expect(1)
        .mount(&server)
        .await;

    let mut filter = GetTransfersRequest::default();
    filter.direction = Some(TransferDirection::Incoming);
    filter.limit = Some(10);
    filter.offset = Some(20);

    let transfers = client(&server)
        .get_transfers_for_account(account_id(), Some(&filter))
        .await
        .unwrap();

    assert_eq!(transfers.len(), 1);
}

#[tokio::test]
async fn cancelling_a_transfer_tolerates_an_empty_body() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path(format!(
            "/v1/accounts/{ACCOUNT_ID}/transfers/{TRANSFER_ID}"
        )))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    client(&server)
        .cancel_transfer_for_account(account_id(), Uuid::parse_str(TRANSFER_ID).unwrap())
        .await
        .unwrap();
}

#[tokio::test]
async fn the_transfers_walk_stops_when_the_server_ignores_the_offset() {
    // This route has no token and no total, so an empty page is the only ending
    // it can express. A server that ignores `offset` never sends one — the walk
    // has to notice for itself that it has stopped learning anything.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/accounts/{ACCOUNT_ID}/transfers")))
        .respond_with(ResponseTemplate::new(200).set_body_json(transfer_page(0, 2)))
        .mount(&server)
        .await;

    let transfers = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        client(&server).get_all_transfers_for_account(account_id(), None, None),
    )
    .await
    .expect("the transfers walk never terminated against a server that ignores the offset")
    .unwrap();

    assert_eq!(transfers.len(), 2);
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}
