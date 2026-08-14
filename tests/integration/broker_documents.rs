//! Trade documents: listing, downloading, and uploading.
//!
//! The download is the only route in the crate that follows a redirect, and the
//! only one that must *not* carry the broker credentials the whole way. Both
//! halves of that are asserted here against a pair of mock servers.

#![cfg(feature = "broker")]

use alpaca_sdk::broker::{
    BrokerClient, DocumentType, GetTradeDocumentsRequest, TradeDocument, TradeDocumentType,
    UploadDocument, UploadDocumentMimeType, UploadDocumentRequest, UploadDocumentSubType,
    UploadW8BenDocumentRequest, W8BenDocument,
};
use alpaca_sdk::{Credentials, RestConfig, RetryConfig};
use serde_json::json;
use uuid::Uuid;
use wiremock::matchers::{body_json, header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const ACCOUNT_ID: &str = "2a87c088-ffb6-472b-a4a3-cd9305c8605c";
const DOCUMENT_ID: &str = "1b560b0f-9efd-44b4-8004-dfd520c7cdc0";

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

fn document_id() -> Uuid {
    Uuid::parse_str(DOCUMENT_ID).unwrap()
}

// ----------------------------------------------------------------- models

#[test]
fn an_empty_sub_type_reads_as_no_sub_type() {
    // Alpaca sends "" rather than omitting the field. Parsing that as a sub type
    // would be an error, so an empty string reads as absent.
    let documents: Vec<TradeDocument> =
        parse("broker/test_documents_routes__test_get_trade_documents_for_account__01.json");

    assert_eq!(documents.len(), 2);
    assert_eq!(
        documents[0].document_type,
        TradeDocumentType::AccountStatement
    );
    assert_eq!(documents[0].sub_type, None);
    assert_eq!(documents[0].date.to_string(), "2022-02-27");
}

// ----------------------------------------------------------------- routes

#[tokio::test]
async fn listing_documents_sends_the_date_window_and_type() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/accounts/{ACCOUNT_ID}/documents")))
        .and(query_param("start", "2022-02-01"))
        .and(query_param("end", "2022-02-28"))
        .and(query_param("type", "account_statement"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "broker/test_documents_routes__test_get_trade_documents_for_account__01.json",
        )))
        .expect(1)
        .mount(&server)
        .await;

    let mut filter = GetTradeDocumentsRequest::default();
    filter.start = Some("2022-02-01".parse().unwrap());
    filter.end = Some("2022-02-28".parse().unwrap());
    filter.document_type = Some(TradeDocumentType::AccountStatement);

    let documents = client(&server)
        .get_trade_documents_for_account(account_id(), Some(&filter))
        .await
        .unwrap();

    assert_eq!(documents.len(), 2);
}

#[tokio::test]
async fn a_backwards_date_window_never_reaches_the_network() {
    let server = MockServer::start().await;
    let mut filter = GetTradeDocumentsRequest::default();
    filter.start = Some("2022-02-28".parse().unwrap());
    filter.end = Some("2022-02-01".parse().unwrap());

    let error = client(&server)
        .get_trade_documents_for_account(account_id(), Some(&filter))
        .await
        .unwrap_err();

    assert!(matches!(error, alpaca_sdk::Error::InvalidRequest(_)));
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn fetching_one_document_returns_its_record_not_its_bytes() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/v1/accounts/{ACCOUNT_ID}/documents/{DOCUMENT_ID}"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture(
            "broker/test_documents_routes__test_get_trade_document_for_account_by_id__01.json",
        )))
        .expect(1)
        .mount(&server)
        .await;

    let document = client(&server)
        .get_trade_document_for_account_by_id(account_id(), document_id())
        .await
        .unwrap();

    assert_eq!(document.id, document_id());
}

// --------------------------------------------------------------- download

#[tokio::test]
async fn downloading_a_document_follows_the_redirect_to_storage() {
    // The API answers 301 with a presigned URL rather than the file itself.
    // Every other route in this crate refuses redirects on purpose, so this one
    // needs its own client — and that is worth pinning down.
    let storage = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/presigned/statement.pdf"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"%PDF-1.4 fake".to_vec()))
        .expect(1)
        .mount(&storage)
        .await;

    let api = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/v1/accounts/{ACCOUNT_ID}/documents/{DOCUMENT_ID}/download"
        )))
        .respond_with(ResponseTemplate::new(301).insert_header(
            "location",
            format!("{}/presigned/statement.pdf", storage.uri()).as_str(),
        ))
        .expect(1)
        .mount(&api)
        .await;

    let bytes = client(&api)
        .download_trade_document_for_account_by_id(account_id(), document_id())
        .await
        .unwrap();

    assert_eq!(bytes, b"%PDF-1.4 fake");
}

#[tokio::test]
async fn the_broker_credentials_do_not_follow_the_redirect() {
    // A presigned URL carries its own authorisation. Forwarding an API key to
    // whatever host the redirect names would hand the correspondent's
    // credentials to a third party.
    let storage = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/presigned/statement.pdf"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"bytes".to_vec()))
        .expect(1)
        .mount(&storage)
        .await;

    let api = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/v1/accounts/{ACCOUNT_ID}/documents/{DOCUMENT_ID}/download"
        )))
        // The first hop is authenticated; it is the broker API.
        .and(header(
            "authorization",
            "Basic YnJva2VyLWtleTpicm9rZXItc2VjcmV0",
        ))
        .respond_with(ResponseTemplate::new(301).insert_header(
            "location",
            format!("{}/presigned/statement.pdf", storage.uri()).as_str(),
        ))
        .expect(1)
        .mount(&api)
        .await;

    client(&api)
        .download_trade_document_for_account_by_id(account_id(), document_id())
        .await
        .unwrap();

    let forwarded = &storage.received_requests().await.unwrap()[0];
    assert!(
        forwarded.headers.get("authorization").is_none(),
        "credentials must not cross to the storage host"
    );
}

#[tokio::test]
async fn a_failed_download_surfaces_the_api_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/v1/accounts/{ACCOUNT_ID}/documents/{DOCUMENT_ID}/download"
        )))
        .respond_with(
            ResponseTemplate::new(404)
                .set_body_json(json!({ "code": 40410000, "message": "document not found" })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let error = client(&server)
        .download_trade_document_for_account_by_id(account_id(), document_id())
        .await
        .unwrap_err();

    assert_eq!(error.status(), Some(404));
}

// ----------------------------------------------------------------- upload

#[tokio::test]
async fn uploading_posts_an_array_and_expects_no_body_back() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(format!("/v1/accounts/{ACCOUNT_ID}/documents/upload")))
        .and(body_json(json!([{
            "document_type": "identity_verification",
            "content": "QSBkb2N1bWVudA==",
            "mime_type": "application/pdf"
        }])))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    client(&server)
        .upload_documents_to_account(
            account_id(),
            &[UploadDocument::Document(UploadDocumentRequest::new(
                DocumentType::IdentityVerification,
                "QSBkb2N1bWVudA==",
                UploadDocumentMimeType::Pdf,
            ))],
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn more_than_ten_documents_is_alpacas_to_refuse_not_ours() {
    // Alpaca documents a 10MB ceiling on each document's *contents* and no
    // limit on the count, so a count cap would be a guess — plausibly right,
    // but not ours to enforce. Eleven documents go
    // to the server, and the server decides.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(format!("/v1/accounts/{ACCOUNT_ID}/documents/upload")))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    let document = UploadDocument::Document(UploadDocumentRequest::new(
        DocumentType::IdentityVerification,
        "QQ==",
        UploadDocumentMimeType::Pdf,
    ));
    let eleven = vec![document; 11];

    client(&server)
        .upload_documents_to_account(account_id(), &eleven)
        .await
        .unwrap();

    // The conventional number is still exposed, for a caller who wants it.
    assert_eq!(alpaca_sdk::broker::DOCUMENT_UPLOAD_LIMIT, 10);
}

#[test]
fn a_w8ben_may_not_be_uploaded_as_a_general_document() {
    // Both directions: a W-8BEN through the general request, and a general
    // document through the W-8BEN one.
    let by_type =
        UploadDocumentRequest::new(DocumentType::W8ben, "QQ==", UploadDocumentMimeType::Pdf);
    assert!(matches!(
        by_type.validate().unwrap_err(),
        alpaca_sdk::Error::InvalidRequest(_)
    ));

    let mut by_sub_type = UploadDocumentRequest::new(
        DocumentType::IdentityVerification,
        "QQ==",
        UploadDocumentMimeType::Pdf,
    );
    by_sub_type.document_sub_type = Some(UploadDocumentSubType::FormW8Ben);
    assert!(matches!(
        by_sub_type.validate().unwrap_err(),
        alpaca_sdk::Error::InvalidRequest(_)
    ));
}

fn w8ben() -> W8BenDocument {
    W8BenDocument {
        country_citizen: "CAN".to_owned(),
        date: "2022-02-01".parse().unwrap(),
        date_of_birth: "1980-02-01".parse().unwrap(),
        full_name: "Jane Doe".to_owned(),
        ip_address: "127.0.0.1".parse().unwrap(),
        permanent_address_city_state: "Toronto, ON".to_owned(),
        permanent_address_country: "CAN".to_owned(),
        permanent_address_street: "1 Front St".to_owned(),
        revision: "October 2021".to_owned(),
        signer_full_name: "Jane Doe".to_owned(),
        timestamp: "2022-02-01T12:00:00Z".parse().unwrap(),
        additional_conditions: None,
        foreign_tax_id: Some("123456789".to_owned()),
        ftin_not_required: None,
        income_type: None,
        mailing_address_city_state: None,
        mailing_address_country: None,
        mailing_address_street: None,
        paragraph_number: None,
        percent_rate_withholding: None,
        reference_number: None,
        residency: None,
        tax_id_ssn: None,
    }
}

#[test]
fn a_w8ben_upload_takes_content_or_fields_but_not_both() {
    let as_file = UploadW8BenDocumentRequest::from_content("QQ==", UploadDocumentMimeType::Pdf);
    assert!(as_file.validate().is_ok());

    let as_fields = UploadW8BenDocumentRequest::from_fields(w8ben());
    assert!(as_fields.validate().is_ok());
    // Fields are always sent as JSON.
    assert_eq!(as_fields.mime_type, UploadDocumentMimeType::Json);

    let mut both = as_fields.clone();
    both.content = Some("QQ==".to_owned());
    assert!(matches!(
        both.validate().unwrap_err(),
        alpaca_sdk::Error::InvalidRequest(_)
    ));

    let mut neither = as_fields.clone();
    neither.content_data = None;
    assert!(matches!(
        neither.validate().unwrap_err(),
        alpaca_sdk::Error::InvalidRequest(_)
    ));

    let mut wrong_mime = as_fields;
    wrong_mime.mime_type = UploadDocumentMimeType::Pdf;
    assert!(matches!(
        wrong_mime.validate().unwrap_err(),
        alpaca_sdk::Error::InvalidRequest(_)
    ));
}

#[test]
fn a_w8ben_must_identify_the_applicant_for_tax() {
    // If neither tax id is given, the form has to say one is not required.
    let mut anonymous = w8ben();
    anonymous.foreign_tax_id = None;
    assert!(matches!(
        anonymous.validate().unwrap_err(),
        alpaca_sdk::Error::InvalidRequest(_)
    ));

    anonymous.ftin_not_required = Some(true);
    assert!(anonymous.validate().is_ok());

    let mut with_ssn = w8ben();
    with_ssn.foreign_tax_id = None;
    with_ssn.tax_id_ssn = Some("123-45-6789".to_owned());
    assert!(with_ssn.validate().is_ok());
}

/// The download shares a retry policy with every other route.
///
/// It runs its own loop — the response body is bytes, not JSON — and that loop
/// used to sleep a flat `retry.wait` on every attempt, ignoring both the backoff
/// curve and the server's own `Retry-After`. Every other route honours both.
///
/// The two are separated by making them disagree: `wait` is set to 3s and the
/// server asks for 1s. A loop that ignores `Retry-After` sleeps its own 3s; one
/// that honours it sleeps 1s. Leaving `wait` at its 1s default against a
/// `Retry-After: 1` would make the test pass either way — the values have to
/// disagree for the assertion to mean anything.
#[tokio::test]
async fn a_rate_limited_download_honours_retry_after_over_its_own_wait() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path(format!(
            "/v1/accounts/{ACCOUNT_ID}/documents/{DOCUMENT_ID}/download"
        )))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "1"))
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!(
            "/v1/accounts/{ACCOUNT_ID}/documents/{DOCUMENT_ID}/download"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"%PDF-1.4".to_vec()))
        .expect(1)
        .mount(&server)
        .await;

    let retrying = BrokerClient::with_config(
        &Credentials::new("broker-key", "broker-secret").unwrap(),
        RestConfig::new(server.uri())
            .api_version("v1")
            .retry(RetryConfig::default().wait(std::time::Duration::from_secs(3))),
    )
    .unwrap();

    let started = std::time::Instant::now();
    let bytes = retrying
        .download_trade_document_for_account_by_id(
            Uuid::parse_str(ACCOUNT_ID).unwrap(),
            Uuid::parse_str(DOCUMENT_ID).unwrap(),
        )
        .await
        .unwrap();
    let elapsed = started.elapsed();

    assert_eq!(bytes, b"%PDF-1.4");
    assert!(
        elapsed >= std::time::Duration::from_millis(800),
        "the retry did not wait at all: {elapsed:?}"
    );
    assert!(
        elapsed < std::time::Duration::from_millis(2500),
        "`Retry-After: 1` was ignored in favour of the client's own 3s wait: {elapsed:?}"
    );
}
