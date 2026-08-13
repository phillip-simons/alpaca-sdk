//! Broker models and routes, against captured payloads.

#![cfg(feature = "broker")]

use alpaca_sdk::broker::{
    Account, AccountEntities, Agreement, AgreementType, BrokerClient, Contact,
    CreateAccountRequest, Disclosures, FundingSource, Identity, ListAccountsRequest, TaxIdType,
    UpdatableContact, UpdateAccountRequest,
};
use alpaca_sdk::trading::AccountStatus;
use alpaca_sdk::types::Sort;
use alpaca_sdk::{Credentials, RestConfig, RetryConfig};
use serde_json::json;
use uuid::Uuid;
use wiremock::matchers::{body_json, header, method, path, query_param};
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
    // The broker API authenticates with basic auth; the others take headers.
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
    // The per-check payloads vary by verification provider, so this crate does
    // not model them; guessing a shape would be worse than passing them through
    // intact.
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

    #[allow(deprecated)]
    let positions = client(&server)
        .get_all_accounts_positions(None)
        .await
        .unwrap();
    assert!(!positions.positions.is_empty());
}

/// The route's only parameter, and not a cosmetic one: without it a partner
/// with more accounts than fit on a page cannot reach the rest of them.
#[tokio::test]
async fn all_accounts_positions_can_ask_for_a_later_page() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/accounts/positions"))
        .and(query_param("page", "3"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "broker/test_trading_routes__test_get_all_accounts_positions__01.json",
        )))
        .expect(1)
        .mount(&server)
        .await;

    #[allow(deprecated)]
    let positions = client(&server)
        .get_all_accounts_positions(Some(3))
        .await
        .unwrap();
    assert!(!positions.positions.is_empty());
}

#[tokio::test]
async fn closing_an_account_posts_to_the_close_action() {
    // Not a DELETE: the account's records survive, and the older
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
    // The broker record is the trading TradeAccount plus these; here the
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

#[test]
fn a_null_list_reads_as_an_empty_one() {
    // `"funding_source": null` sits in the same list response as a populated
    // one. #[serde(default)] covers an absent field, not a present-and-null
    // one, so before this the whole response failed to decode — and the fixture
    // that proves it had been in the tree the whole time, never parsed as a
    // list.
    let accounts: Vec<Account> = parse(
        "broker/test_accounts_routes__test_list_accounts_parses_entities_if_present__01.json",
    );

    assert_eq!(accounts.len(), 2);
    let first = accounts[0].identity.as_ref().expect("identity");
    assert!(first.funding_source.is_empty());
    let second = accounts[1].identity.as_ref().expect("identity");
    assert_eq!(second.funding_source.len(), 1);
}

// ------------------------------------------------------- account requests

fn valid_application() -> CreateAccountRequest {
    let mut contact = Contact {
        email_address: "jane@example.com".to_owned(),
        street_address: vec!["20 N San Mateo Dr".to_owned()],
        ..Contact::default()
    };
    contact.city = Some("San Mateo".to_owned());

    let identity = Identity {
        given_name: "Jane".to_owned(),
        family_name: "Doe".to_owned(),
        date_of_birth: Some("1990-01-01".parse().unwrap()),
        tax_id_type: Some(TaxIdType::UsaSsn),
        country_of_tax_residence: Some("USA".to_owned()),
        funding_source: vec![FundingSource::EmploymentIncome],
        ..Identity::default()
    };

    let disclosures = Disclosures {
        is_control_person: Some(false),
        is_affiliated_exchange_or_finra: Some(false),
        is_politically_exposed: Some(false),
        immediate_family_exposed: Some(false),
        ..Disclosures::default()
    };

    CreateAccountRequest::new(
        contact,
        identity,
        disclosures,
        vec![Agreement {
            agreement: AgreementType::Customer,
            signed_at: "2022-04-28T14:07:04.451420Z".parse().unwrap(),
            ip_address: Some("127.0.0.1".to_owned()),
            revision: None,
        }],
    )
}

#[test]
fn a_complete_application_validates() {
    valid_application().validate().unwrap();
}

#[test]
fn every_field_the_reference_marks_required_is_checked() {
    // The required set comes from the API reference. It is worth pinning: the
    // obvious alternative source, another SDK's validator, requires a field the
    // reference does not and misses six that it does.
    /// A field name paired with a way to remove it from a valid application.
    type Case = (&'static str, Box<dyn Fn(&mut CreateAccountRequest)>);

    let cases: Vec<Case> = vec![
        (
            "contact.email_address",
            Box::new(|r: &mut CreateAccountRequest| r.contact.email_address.clear()),
        ),
        (
            "contact.street_address",
            Box::new(|r: &mut CreateAccountRequest| r.contact.street_address.clear()),
        ),
        (
            "contact.city",
            Box::new(|r: &mut CreateAccountRequest| r.contact.city = None),
        ),
        (
            "identity.given_name",
            Box::new(|r: &mut CreateAccountRequest| r.identity.given_name.clear()),
        ),
        (
            "identity.family_name",
            Box::new(|r: &mut CreateAccountRequest| r.identity.family_name.clear()),
        ),
        (
            "identity.date_of_birth",
            Box::new(|r: &mut CreateAccountRequest| r.identity.date_of_birth = None),
        ),
        (
            "identity.tax_id_type",
            Box::new(|r: &mut CreateAccountRequest| r.identity.tax_id_type = None),
        ),
        (
            "identity.country_of_tax_residence",
            Box::new(|r: &mut CreateAccountRequest| r.identity.country_of_tax_residence = None),
        ),
        (
            "identity.funding_source",
            Box::new(|r: &mut CreateAccountRequest| r.identity.funding_source.clear()),
        ),
        (
            "disclosures.is_control_person",
            Box::new(|r: &mut CreateAccountRequest| r.disclosures.is_control_person = None),
        ),
        (
            "disclosures.is_affiliated_exchange_or_finra",
            Box::new(|r: &mut CreateAccountRequest| {
                r.disclosures.is_affiliated_exchange_or_finra = None
            }),
        ),
        (
            "disclosures.is_politically_exposed",
            Box::new(|r: &mut CreateAccountRequest| r.disclosures.is_politically_exposed = None),
        ),
        (
            "disclosures.immediate_family_exposed",
            Box::new(|r: &mut CreateAccountRequest| r.disclosures.immediate_family_exposed = None),
        ),
        (
            "agreements",
            Box::new(|r: &mut CreateAccountRequest| r.agreements.clear()),
        ),
        (
            "agreements[].ip_address",
            Box::new(|r: &mut CreateAccountRequest| r.agreements[0].ip_address = None),
        ),
    ];

    for (field, break_it) in cases {
        let mut request = valid_application();
        break_it(&mut request);
        let Err(error) = request.validate() else {
            panic!("{field} must be required");
        };
        assert!(
            format!("{error}").contains(field),
            "error for {field} should name it, said: {error}"
        );
    }
}

#[test]
fn a_phone_number_is_not_required_by_the_reference() {
    // A stricter reading rejects an application with no phone_number. The
    // reference does
    // not list it as required, and refusing a request Alpaca would accept is
    // the worse failure of the two.
    let mut request = valid_application();
    request.contact.phone_number = None;
    request.validate().unwrap();
}

#[tokio::test]
async fn an_incomplete_application_never_reaches_the_network() {
    let server = MockServer::start().await;
    let mut request = valid_application();
    request.identity.tax_id_type = None;

    let error = client(&server).create_account(&request).await.unwrap_err();

    assert!(matches!(error, alpaca_sdk::Error::InvalidRequest(_)));
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn listing_accounts_sends_entities_as_one_comma_separated_parameter() {
    // The list route trims each account to keep the response small; `entities`
    // fills the detail back in. The reference is explicit that it is
    // "comma-delimited", so a repeated parameter would silently filter nothing.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/accounts"))
        .and(query_param("entities", "identity,contact"))
        .and(query_param("sort", "asc"))
        .and(query_param("query", "jane"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "broker/test_accounts_routes__test_list_accounts_parses_entities_if_present__01.json",
        )))
        .expect(1)
        .mount(&server)
        .await;

    let filter = ListAccountsRequest {
        query: Some("jane".to_owned()),
        sort: Some(Sort::Asc),
        entities: Some(vec![AccountEntities::Identity, AccountEntities::Contact]),
        ..Default::default()
    };

    let accounts = client(&server).list_accounts(Some(&filter)).await.unwrap();
    assert!(!accounts.is_empty());
}

#[tokio::test]
async fn an_update_sends_only_the_fields_it_names() {
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path(format!("/v1/accounts/{ACCOUNT_ID}")))
        .and(body_json(json!({
            "contact": { "email_address": "new@example.com" }
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "broker/test_accounts_routes__test_update_account__01.json",
        )))
        .expect(1)
        .mount(&server)
        .await;

    let update = UpdateAccountRequest {
        contact: Some(UpdatableContact {
            email_address: Some("new@example.com".to_owned()),
            ..Default::default()
        }),
        ..Default::default()
    };

    client(&server)
        .update_account(Uuid::parse_str(ACCOUNT_ID).unwrap(), &update)
        .await
        .unwrap();
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
