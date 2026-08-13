//! The broker routes found in the published reference rather than in any
//! captured payload: fixed income, instant funding, JIT, FPSL, IPOs, reporting,
//! OAuth, funding wallets, tokenization, crypto wallets, and the account odds
//! and ends.
//!
//! Two kinds of evidence here, and the difference matters:
//!
//! - **Fixed income** parses payloads harvested from the Go SDK's tests, where
//!   they are raw JSON pasted into backtick literals. Those are real.
//! - **Everything else** has no captured payload in any SDK — this account has
//!   no broker sandbox key — so the bodies are the published reference's own
//!   examples. They pin down the request the crate sends, especially the
//!   **version segment**, which `just coverage` cannot check. They do not prove
//!   the response models right. Treat a decode failure against a first real
//!   payload as expected work, as with the `CIP*` models.

#![cfg(feature = "broker")]

use alpaca_sdk::broker::{
    BrokerClient, CreateInstantFundingRequest, CreateJitSettlementRequest,
    CreateRecipientBankRequest, CreateWithdrawalRequest, GetEntryRequirementsRequest,
    GetJitReportRequest, GetUsCorporatesRequest, GetUsTreasuriesRequest, InstantFundingStatus,
    IpoAvailability, JitReport, JitReportType, OAuthRequest, OptionsLevel,
    RequestOptionsApprovalRequest, RiskRating, SettlementAssetClass, TreasurySubtype,
};
use alpaca_sdk::types::SupportedCurrencies;
use alpaca_sdk::{Credentials, RestConfig, RetryConfig};
use rust_decimal::Decimal;
use serde_json::json;
use uuid::Uuid;
use wiremock::matchers::{body_json, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn fixture(name: &str) -> serde_json::Value {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name);
    let body = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    serde_json::from_str(&body).unwrap()
}

fn client(server: &MockServer) -> BrokerClient {
    BrokerClient::with_config(
        &Credentials::new("key", "secret").unwrap(),
        RestConfig::new(server.uri())
            .api_version("v1")
            .retry(RetryConfig::none()),
    )
    .unwrap()
}

const ACCOUNT: Uuid = Uuid::nil();

// ---------------------------------------------------------- fixed income

#[tokio::test]
async fn us_corporates_parse_the_go_sdk_payload() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/assets/fixed_income/us_corporates"))
        .and(query_param("cusips", "06051GJH9"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(fixture("go/alpaca__test_get_us_corporates__01.json")),
        )
        .expect(1)
        .mount(&server)
        .await;

    let filter = GetUsCorporatesRequest::new().cusips(vec!["06051GJH9".to_owned()]);
    let corporates = client(&server)
        .get_us_corporates(Some(&filter))
        .await
        .unwrap();

    let bond = &corporates.us_corporates[0];
    assert_eq!(bond.cusip, "06051GJH9");
    assert_eq!(bond.issuer, "Bank of America Corporation");
    // Prices and yields are JSON numbers on this family, so they stay f64 —
    // unlike the entry requirements, which arrive as strings.
    assert_eq!(bond.coupon, 3.5);
    assert!(bond.callable);
    // The spec marks `fractionable` required and this payload omits it. The
    // payload wins: a required-field model would reject the only real corporate
    // bond anyone has captured.
    assert!(!bond.fractionable);
}

#[tokio::test]
async fn us_treasuries_parse_the_go_sdk_payload() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/assets/fixed_income/us_treasuries"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(fixture("go/alpaca__test_get_us_treasuries__01.json")),
        )
        .expect(1)
        .mount(&server)
        .await;

    let treasuries = client(&server)
        .get_us_treasuries(Some(&GetUsTreasuriesRequest::new()))
        .await
        .unwrap();

    // The captured payload holds a zero-coupon bill with no coupon dates at
    // all, which is why they are all optional.
    let bill = &treasuries.us_treasuries[0];
    assert_eq!(bill.subtype, TreasurySubtype::Bill);
    assert_eq!(bill.coupon, 0.0);
    assert_eq!(bill.first_coupon_date, None);
}

#[tokio::test]
async fn entry_requirements_come_back_as_decimals() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/assets/entry-requirements"))
        .and(query_param("symbols", "AAPL,TSLA"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"symbol": "AAPL", "regt_long": "0.5", "regt_short": "1.5"},
            {"symbol": "TSLA", "regt_long": "0.75"},
        ])))
        .expect(1)
        .mount(&server)
        .await;

    let request = GetEntryRequirementsRequest::new(vec!["AAPL".to_owned(), "TSLA".to_owned()]);
    let requirements = client(&server)
        .get_entry_requirements(&request)
        .await
        .unwrap();

    assert_eq!(requirements.len(), 2);
    assert_eq!(requirements[0].regt_long, Some(Decimal::new(5, 1)));
    assert_eq!(requirements[1].regt_short, None);
}

// -------------------------------------------------------- instant funding

#[tokio::test]
async fn an_instant_funding_advance_round_trips() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/instant_funding"))
        .and(body_json(json!({
            "account_no": "123",
            "source_account_no": "456",
            "amount": "1000",
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "account_no": "123",
            "source_account_no": "456",
            "amount": "1000",
            "remaining_payable": "1000",
            "total_interest": "0",
            "status": "EXECUTED",
            "system_date": "2026-01-02",
            "deadline": "2026-01-05",
            "created_at": "2026-01-02T15:04:05Z",
            "fees": [{"id": "550e8400-e29b-41d4-a716-446655440001",
                      "type": "alpaca", "amount": "1.00"}],
            "interests": [],
        })))
        .expect(1)
        .mount(&server)
        .await;

    let request = CreateInstantFundingRequest::new("123", "456", Decimal::new(1000, 0));
    let funding = client(&server)
        .create_instant_funding(&request)
        .await
        .unwrap();

    assert_eq!(funding.status, InstantFundingStatus::Executed);
    assert_eq!(funding.fees.len(), 1);
    assert!(funding.interests.is_empty());
}

#[tokio::test]
async fn a_zero_advance_never_reaches_the_server() {
    let server = MockServer::start().await;
    let error = client(&server)
        .create_instant_funding(&CreateInstantFundingRequest::new("1", "2", Decimal::ZERO))
        .await
        .unwrap_err();

    assert!(matches!(error, alpaca_sdk::Error::InvalidRequest(_)));
}

// -------------------------------------------------------------------- JIT

#[tokio::test]
async fn a_jit_report_decodes_as_a_download_link() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/transfers/jit/reports"))
        .and(query_param("report_type", "net_summary"))
        .and(query_param("system_date", "2026-01-02"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "url": "https://example.invalid/report.csv",
            "filename": "report.csv",
            "expires_at": "2026-01-02T16:00:00Z",
        })))
        .expect(1)
        .mount(&server)
        .await;

    let request =
        GetJitReportRequest::new(JitReportType::NetSummary, "2026-01-02".parse().unwrap());
    let report = client(&server).get_jit_report(&request).await.unwrap();

    match report {
        JitReport::Download(download) => assert_eq!(download.filename, "report.csv"),
        JitReport::Inline(_) => panic!("expected the download shape"),
    }
}

#[tokio::test]
async fn a_jit_settlement_with_no_accounts_is_refused() {
    let server = MockServer::start().await;
    let request = CreateJitSettlementRequest::new(
        Vec::new(),
        SettlementAssetClass::UsEquity,
        SupportedCurrencies::Usd,
    );
    let error = client(&server)
        .create_jit_settlement(&request)
        .await
        .unwrap_err();

    assert!(matches!(error, alpaca_sdk::Error::InvalidRequest(_)));
}

// ------------------------------------------------------------------ IPOs

#[tokio::test]
async fn ipo_offerings_unwrap_their_data_envelope() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/ipos"))
        .and(query_param("availability", "available"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [{
                "ipo_reference": "IPO123",
                "name": "Example Corp",
                "offering_type": "ipo",
                "availability": "available",
                "no_new_orders": true,
                "min_price": "17.00",
                "max_price": "19.00",
                "ticker_symbol": "EXMP",
            }],
            "next_page_token": null,
        })))
        .expect(1)
        .mount(&server)
        .await;

    let filter =
        alpaca_sdk::broker::GetIpoOfferingsRequest::new().availability(IpoAvailability::Available);
    let page = client(&server)
        .get_ipo_offerings(Some(&filter))
        .await
        .unwrap();

    assert_eq!(page.offerings.len(), 1);
    // Available and refusing new orders at the same time is a real state, and
    // both are reported rather than collapsed into one.
    assert_eq!(page.offerings[0].availability, IpoAvailability::Available);
    assert!(page.offerings[0].no_new_orders);
}

// ------------------------------------------------------------- reporting

#[tokio::test]
async fn eod_positions_reuse_the_trading_position_model() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/reporting/eod/positions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "asof": "2026-01-02",
            "next_page_token": null,
            "positions": {
                "00000000-0000-0000-0000-000000000000": [{
                    "asset_id": "550e8400-e29b-41d4-a716-446655440001",
                    "symbol": "AAPL",
                    "exchange": "NASDAQ",
                    "asset_class": "us_equity",
                    "avg_entry_price": "180.00",
                    "qty": "10",
                    "qty_available": "10",
                    "side": "long",
                    "market_value": "1850.00",
                    "cost_basis": "1800.00",
                    "unrealized_pl": "50.00",
                    "unrealized_plpc": "0.0277",
                    "unrealized_intraday_pl": "5.00",
                    "unrealized_intraday_plpc": "0.0027",
                    "current_price": "185.00",
                    "lastday_price": "184.50",
                    "change_today": "0.0027",
                }],
            },
        })))
        .expect(1)
        .mount(&server)
        .await;

    let report = client(&server).get_eod_positions(None).await.unwrap();
    assert_eq!(report.positions.len(), 1);
}

// ----------------------------------------------------------------- OAuth

#[tokio::test]
async fn the_oauth_token_route_sends_json_not_a_form() {
    // The convention for OAuth token endpoints is form encoding. This one takes
    // JSON, and the test exists so nobody "corrects" it.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/oauth/token"))
        .and(body_json(json!({
            "account_id": "00000000-0000-0000-0000-000000000000",
            "client_id": "client",
            "client_secret": "secret",
            "redirect_uri": "https://example.invalid/cb",
            "scope": "trading",
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "token",
            "scope": "trading",
            "token_type": "Bearer",
        })))
        .expect(1)
        .mount(&server)
        .await;

    let request = OAuthRequest::new(
        ACCOUNT,
        "client",
        "secret",
        "https://example.invalid/cb",
        "trading",
    );
    let token = client(&server).issue_oauth_token(&request).await.unwrap();

    assert_eq!(token.token_type, "Bearer");
}

// -------------------------------------------------------- funding wallets

#[tokio::test]
async fn funding_wallets_are_a_v1beta_route_on_a_v1_client() {
    // The broker client is v1. These are not, and a v1 path would 404 while
    // `just coverage` still showed a tick.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/v1beta/accounts/00000000-0000-0000-0000-000000000000/funding_wallet",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "account_id": "00000000-0000-0000-0000-000000000000",
            "status": "active",
            "created_at": "2026-01-02T15:04:05Z",
        })))
        .expect(1)
        .mount(&server)
        .await;

    let wallet = client(&server).get_funding_wallet(ACCOUNT).await.unwrap();
    assert_eq!(wallet.account_id, ACCOUNT);
}

#[tokio::test]
async fn a_recipient_bank_sends_only_what_was_set() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(
            "/v1beta/accounts/00000000-0000-0000-0000-000000000000/funding_wallet/recipient_bank",
        ))
        // No iban, no routing_code: the reference marks all of them optional,
        // and enforcing a combination would refuse requests Alpaca accepts.
        .and(body_json(json!({
            "account_number": "12345678",
            "bank_name": "Example Bank",
            "bank_country": "GB",
            "currency": "GBP",
            "street_address": "1 Example Street",
            "city": "London",
            "bic_swift": "EXMPGB2L",
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "account_number": "12345678",
            "bic_swift": "EXMPGB2L",
        })))
        .expect(1)
        .mount(&server)
        .await;

    let request = CreateRecipientBankRequest::new(
        "12345678",
        "Example Bank",
        "GB",
        SupportedCurrencies::Gbp,
        "1 Example Street",
        "London",
    )
    .bic_swift("EXMPGB2L");

    let bank = client(&server)
        .create_recipient_bank(ACCOUNT, &request)
        .await
        .unwrap();

    assert_eq!(bank.bic_swift.as_deref(), Some("EXMPGB2L"));
    assert!(bank.payment_types.is_empty());
}

#[tokio::test]
async fn a_zero_withdrawal_never_reaches_the_server() {
    let server = MockServer::start().await;
    let request = CreateWithdrawalRequest::new(Decimal::ZERO, "GBP");
    let error = client(&server)
        .create_funding_wallet_withdrawal(ACCOUNT, &request)
        .await
        .unwrap_err();

    assert!(matches!(error, alpaca_sdk::Error::InvalidRequest(_)));
}

// ------------------------------------------------------ onboarding extras

#[tokio::test]
async fn options_level_zero_cannot_be_requested() {
    // It is an outcome the approved side reports, not a level a request may
    // name — so it fails at construction rather than at the server.
    assert!(RequestOptionsApprovalRequest::new(OptionsLevel::Zero).is_err());
}

#[tokio::test]
async fn country_info_comes_back_keyed_by_country_code() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/country-info"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "USA": {
                "full_name": "United States",
                "securities_risk_rating": "low",
                "crypto_risk_rating": "low",
                "crypto_supported_states": ["CA", "NY"],
            },
            "GBR": {
                "full_name": "United Kingdom",
                "securities_risk_rating": "low",
                "crypto_risk_rating": "prohibited",
            },
        })))
        .expect(1)
        .mount(&server)
        .await;

    let countries = client(&server).get_country_info().await.unwrap();

    assert_eq!(countries["USA"].crypto_supported_states.len(), 2);
    assert_eq!(countries["GBR"].crypto_risk_rating, RiskRating::Prohibited);
    // A country with no state carve-outs sends no list at all.
    assert!(countries["GBR"].crypto_supported_states.is_empty());
}

// ---------------------------------------------------------- market extras

#[tokio::test]
async fn the_brokers_per_market_calendar_is_v2_where_tradings_is_v3() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/calendar/XLON"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "market": {"acronym": "LSE", "name": "London Stock Exchange",
                       "timezone": "Europe/London"},
            "calendar": [],
        })))
        .expect(1)
        .mount(&server)
        .await;

    let calendar = client(&server)
        .get_market_calendar(&alpaca_sdk::trading::Market::Xlon, None)
        .await
        .unwrap();

    assert_eq!(calendar.market.acronym, "LSE");
}

// ------------------------------------------------------ activity filters

#[tokio::test]
async fn category_and_activity_types_cannot_both_be_set() {
    // The one exclusivity the reference documents on this route: "Cannot be
    // used with `activity_types` parameter". A documented rule, so it is
    // enforced — unlike the plausible date/after/until conflict, which nothing
    // documents and this crate does not reject.
    use alpaca_sdk::broker::{ActivityCategory, GetAccountActivitiesRequest};
    use alpaca_sdk::trading::ActivityType;

    let mut both = GetAccountActivitiesRequest::default();
    both.category = Some(ActivityCategory::TradeActivity);
    both.activity_types = Some(vec![ActivityType::Fill]);
    assert!(both.validate().is_err());

    let one = GetAccountActivitiesRequest::default().category(ActivityCategory::NonTradeActivity);
    assert!(one.validate().is_ok());

    // And the rule this crate declines to enforce still reaches the server, in
    // the same shape as the `expect(0)` tests on the retired event streams.
    let mut dated = GetAccountActivitiesRequest::default();
    dated.date = Some("2026-01-02T00:00:00Z".parse().unwrap());
    dated.after = Some("2026-01-01T00:00:00Z".parse().unwrap());
    assert!(
        dated.validate().is_ok(),
        "an undocumented rule has not crept back in"
    );
}

// ------------------------------------------ routes nothing else exercised

/// The withdrawal — money leaving the account — had only a negative test, which
/// returned from `validate()` before any HTTP. Neither the `v1beta` version
/// segment, nor the path, nor the body was asserted anywhere, so a dropped
/// `.at_version("v1beta")` would have 404'd with the whole suite still green.
#[tokio::test]
async fn a_funding_wallet_withdrawal_posts_to_the_v1beta_route_with_the_amount_as_a_string() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(format!(
            "/v1beta/accounts/{ACCOUNT}/funding_wallet/withdrawal"
        )))
        // Money goes out as a string, like every other amount this crate sends.
        .and(body_json(json!({
            "usd_amount": "250.75",
            "desired_currency": "GBP"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "b0b6dd9d-8b9b-48a9-ba46-b9d54906e415",
            "status": "PENDING"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let request = CreateWithdrawalRequest::new(Decimal::new(25_075, 2), "GBP");
    client(&server)
        .create_funding_wallet_withdrawal(ACCOUNT, &request)
        .await
        .unwrap();
}

/// The only route method on either client with no test at all — not even in the
/// route smoke test that exists for exactly this. It is also a PATCH of account
/// configuration, so it inherits the omit-rather-than-null fix.
#[tokio::test]
async fn updating_an_accounts_trade_configuration_patches_the_right_route() {
    let configuration = json!({
        "no_shorting": false,
        "suspend_trade": false,
        "fractional_trading": true,
        "max_margin_multiplier": "4",
        "trade_confirm_email": "all",
        "ptp_no_exception_entry": false,
        "max_options_trading_level": 1
    });

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/v1/trading/accounts/{ACCOUNT}/account/configurations"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(configuration.clone()))
        .expect(1)
        .mount(&server)
        .await;

    let fetched = client(&server)
        .get_trade_configuration_for_account(ACCOUNT)
        .await
        .unwrap();

    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path(format!(
            "/v1/trading/accounts/{ACCOUNT}/account/configurations"
        )))
        // Round-tripped unchanged, with no `null`s invented for the three
        // fields the current response shape omits.
        .and(body_json(configuration.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_json(configuration))
        .expect(1)
        .mount(&server)
        .await;

    client(&server)
        .update_trade_configuration_for_account(ACCOUNT, &fetched)
        .await
        .unwrap();
}
