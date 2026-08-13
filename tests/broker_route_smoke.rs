//! Every broker route no other test calls, checked for where it goes.
//!
//! This asserts routing and nothing else: the HTTP method, the version segment,
//! and the path — including which parameters land in it. The server answers 404
//! to every one, so the call fails and the decoding is not exercised. That is
//! deliberate; these routes have no captured payloads, and a body invented here
//! would assert that the model matches the guess rather than the API.
//!
//! Routing is worth its own test because it is the failure this crate has
//! actually shipped: three event streams pointed at routes Alpaca had retired,
//! and every model behind them was correct. A wrong version segment or a
//! transposed id fails exactly this way — a 404 in production, nothing at
//! compile time, and `just coverage` matching the path it *meant* to call.
//!
//! The assertion is the mock's `expect(1)`, verified when the server drops: if
//! the client called any other path, the mounted route was never hit and the
//! test fails there.

#![cfg(feature = "broker")]

use alpaca_sdk::broker::{
    BatchCreateFundingWalletsRequest, BrokerClient, CreateInstantFundingSettlementRequest,
    DemoFundingRequest, EstimateOrderRequest, GetAccountLimitsRequest,
    GetAggregatePositionsRequest, GetRunsRequest, OAuthRequest, OptionsLevel,
    RequestOptionsApprovalRequest, SettlementTransfer, TransmitterInfo, UpdateOnfidoOutcomeRequest,
};
use alpaca_sdk::trading::{
    ByClientRequestId, CreateWatchlistRequest, CreateWhitelistedAddressRequest, CryptoChain,
    MintTokenRequest, ReplaceOrderRequest, TokenizationIssuer, TokenizationNetwork,
    TransferFeeEstimateRequest, UpdateWatchlistRequest,
};
use alpaca_sdk::types::{AssetIdent, SupportedCurrencies};
use alpaca_sdk::{Credentials, RestConfig, RetryConfig};
use rust_decimal::Decimal;
use serde_json::json;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const ACCOUNT: &str = "2a87c088-ffb6-472b-a4a3-cd9305c8605c";
const OTHER: &str = "61e69015-8549-4bfd-b9c3-01e75843f47d";

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

fn account() -> Uuid {
    Uuid::parse_str(ACCOUNT).unwrap()
}

fn other() -> Uuid {
    Uuid::parse_str(OTHER).unwrap()
}

/// Mounts the one route the call under test must reach.
async fn expect_route(http_method: &str, http_path: &str) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method(http_method))
        .and(path(http_path.to_owned()))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&server)
        .await;
    server
}

// --------------------------------------------------------------- accounts

#[tokio::test]
async fn account_scoped_reads_nest_under_the_account_id() {
    let server = expect_route("GET", &format!("/v1/accounts/{ACCOUNT}/recipient_banks")).await;
    assert!(
        client(&server)
            .get_banks_for_account(account())
            .await
            .is_err()
    );

    let server = expect_route("GET", &format!("/v1/trading/accounts/{ACCOUNT}/limits")).await;
    assert!(client(&server).get_trading_limits(account()).await.is_err());

    // `v2beta1`, matching the activities *stream* rather than the client's v1:
    // one event and the stream of them live on the same version.
    let server = expect_route(
        "GET",
        &format!("/v2beta1/accounts/{ACCOUNT}/events/activities/evt-1"),
    )
    .await;
    assert!(
        client(&server)
            .get_account_activity_event(account(), "evt-1")
            .await
            .is_err()
    );

    let server = expect_route("GET", &format!("/v1/accounts/{ACCOUNT}/onfido/sdk/tokens")).await;
    assert!(
        client(&server)
            .get_onfido_token(account(), None)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn the_account_wide_reads_take_no_id() {
    let server = expect_route("GET", "/v1/accounts/ira_excess_contributions").await;
    assert!(
        client(&server)
            .get_ira_excess_contributions()
            .await
            .is_err()
    );

    let server = expect_route("GET", "/v1/accounts/options/approvals").await;
    assert!(client(&server).get_options_approvals(None).await.is_err());
}

/// Two ids in one path, which is the shape a transposition hides in.
#[tokio::test]
async fn two_id_paths_put_them_in_the_documented_order() {
    let server = expect_route(
        "GET",
        &format!("/v1/trading/accounts/{ACCOUNT}/watchlists/{OTHER}"),
    )
    .await;
    assert!(
        client(&server)
            .get_watchlist_for_account_by_id(account(), other())
            .await
            .is_err()
    );

    let server = expect_route(
        "GET",
        &format!("/v1/accounts/{ACCOUNT}/tokenization/requests/{OTHER}"),
    )
    .await;
    assert!(
        client(&server)
            .get_tokenization_request_for_account(account(), other())
            .await
            .is_err()
    );

    let server = expect_route(
        "GET",
        &format!("/v1/accounts/{ACCOUNT}/documents/w8ben/{OTHER}/download"),
    )
    .await;
    assert!(
        client(&server)
            .download_w8ben_document(account(), other())
            .await
            .is_err()
    );
}

// -------------------------------------------------------- instant funding

#[tokio::test]
async fn the_instant_funding_routes_hang_off_one_prefix() {
    let server = expect_route("GET", "/v1/instant_funding").await;
    assert!(client(&server).get_instant_funding(None).await.is_err());

    let server = expect_route("GET", "/v1/instant_funding/fund-1").await;
    assert!(
        client(&server)
            .get_instant_funding_by_id("fund-1")
            .await
            .is_err()
    );

    let server = expect_route("GET", "/v1/instant_funding/limits").await;
    assert!(client(&server).get_instant_funding_limits().await.is_err());

    let server = expect_route("GET", "/v1/instant_funding/reports").await;
    assert!(
        client(&server)
            .get_instant_funding_reports(None)
            .await
            .is_err()
    );

    let server = expect_route("GET", "/v1/instant_funding/settlements").await;
    assert!(
        client(&server)
            .get_instant_funding_settlements(None)
            .await
            .is_err()
    );

    let server = expect_route("GET", &format!("/v1/instant_funding/settlements/{OTHER}")).await;
    assert!(
        client(&server)
            .get_instant_funding_settlement(other())
            .await
            .is_err()
    );
}

// -------------------------------------------------------------------- JIT

/// The JIT ledgers live under `/transfers/jit` and its settlements under
/// `/jit` — a split that reads like a mistake and is not.
#[tokio::test]
async fn jit_ledgers_and_jit_settlements_are_different_prefixes() {
    let server = expect_route("GET", "/v1/transfers/jit/ledgers").await;
    assert!(client(&server).get_jit_ledgers().await.is_err());

    let server = expect_route("GET", "/v1/transfers/jit/limits").await;
    assert!(client(&server).get_jit_trading_limits().await.is_err());

    let server = expect_route("GET", "/v1/transfers/jit/ledger-1/balances").await;
    assert!(
        client(&server)
            .get_jit_ledger_balances("ledger-1", None)
            .await
            .is_err()
    );

    let server = expect_route("GET", "/v1/jit/settlements").await;
    assert!(client(&server).get_jit_settlements(None).await.is_err());

    let server = expect_route("GET", &format!("/v1/jit/settlements/{OTHER}")).await;
    assert!(client(&server).get_jit_settlement(other()).await.is_err());
}

// ------------------------------------------------------------------- FPSL

#[tokio::test]
async fn the_fpsl_routes_reach_their_own_prefix() {
    let server = expect_route("GET", "/v1/fpsl/loans").await;
    assert!(client(&server).get_fpsl_loans(None).await.is_err());

    let server = expect_route("GET", "/v1/fpsl/tiers").await;
    assert!(client(&server).get_fpsl_tiers().await.is_err());

    // The account id is in the middle of this one, not at the end.
    let server = expect_route("GET", &format!("/v1/fpsl/analytics/{ACCOUNT}/loans")).await;
    assert!(
        client(&server)
            .get_fpsl_analytics(account(), None)
            .await
            .is_err()
    );
}

// -------------------------------------------------------------- reporting

#[tokio::test]
async fn the_reporting_routes_reach_their_own_prefix() {
    let server = expect_route("GET", "/v1/reporting/eod/cash_interest").await;
    assert!(client(&server).get_eod_cash_interest(None).await.is_err());

    let server = expect_route("GET", "/v1/cash_interest/apr_tiers").await;
    assert!(client(&server).get_apr_tiers().await.is_err());
}

// ---------------------------------------------------------------- funding

/// The funding wallet routes are `v1beta`, not the client's `v1`. This is the
/// exact class of mistake that shipped the retired event streams.
#[tokio::test]
async fn the_funding_wallet_routes_are_v1beta_not_v1() {
    let server = expect_route(
        "GET",
        &format!("/v1beta/accounts/{ACCOUNT}/funding_wallet/recipient_bank"),
    )
    .await;
    assert!(client(&server).get_recipient_bank(account()).await.is_err());

    let server = expect_route(
        "GET",
        &format!("/v1beta/accounts/{ACCOUNT}/funding_wallet/transfers"),
    )
    .await;
    assert!(
        client(&server)
            .get_funding_wallet_transfers(account())
            .await
            .is_err()
    );

    let server = expect_route(
        "GET",
        &format!("/v1beta/accounts/{ACCOUNT}/funding_wallet/transfers/{OTHER}"),
    )
    .await;
    assert!(
        client(&server)
            .get_funding_wallet_transfer(account(), other())
            .await
            .is_err()
    );

    let server = expect_route(
        "GET",
        &format!("/v1beta/accounts/{ACCOUNT}/funding_wallet/funding_details"),
    )
    .await;
    assert!(
        client(&server)
            .get_funding_details(account(), None)
            .await
            .is_err()
    );
}

// ----------------------------------------------------------- crypto wallets

#[tokio::test]
async fn the_crypto_wallet_routes_nest_under_wallets() {
    let server = expect_route("GET", &format!("/v1/accounts/{ACCOUNT}/wallets")).await;
    assert!(
        client(&server)
            .get_crypto_wallets_for_account(account(), None)
            .await
            .is_err()
    );

    let server = expect_route("GET", &format!("/v1/accounts/{ACCOUNT}/wallets/transfers")).await;
    assert!(
        client(&server)
            .get_crypto_transfers_for_account(account())
            .await
            .is_err()
    );

    let server = expect_route(
        "GET",
        &format!("/v1/accounts/{ACCOUNT}/wallets/transfers/transfer-1"),
    )
    .await;
    assert!(
        client(&server)
            .get_crypto_transfer_for_account(account(), "transfer-1")
            .await
            .is_err()
    );

    let server = expect_route("GET", &format!("/v1/accounts/{ACCOUNT}/wallets/whitelists")).await;
    assert!(
        client(&server)
            .get_whitelisted_addresses_for_account(account())
            .await
            .is_err()
    );
}

// ------------------------------------------------------------------ other

#[tokio::test]
async fn the_remaining_reads_reach_their_documented_paths() {
    let server = expect_route("GET", &format!("/v1/rebalancing/portfolios/{OTHER}")).await;
    assert!(client(&server).get_portfolio_by_id(other()).await.is_err());

    let server = expect_route("GET", "/v1/ipos/offer-1").await;
    assert!(client(&server).get_ipo_offering("offer-1").await.is_err());

    let server = expect_route("GET", &format!("/v1/oauth/clients/{OTHER}")).await;
    assert!(
        client(&server)
            .get_oauth_client(other(), None)
            .await
            .is_err()
    );

    // Deprecated by Alpaca and still answering, so it still has to route.
    let server = expect_route(
        "GET",
        &format!("/v1/corporate_actions/announcements/{OTHER}"),
    )
    .await;
    #[allow(deprecated)]
    let called = client(&server)
        .get_corporate_announcement_by_id(other())
        .await;
    assert!(called.is_err());
}

// ------------------------------------------------------------ event streams

/// The four streams the reference sweep found, and the versions they live on:
/// activities is `v2beta1` and the other three are `v2`, on a `v1` client.
#[tokio::test]
async fn the_newer_event_streams_carry_their_own_version_segments() {
    let server = expect_route("GET", "/v2beta1/events/activities").await;
    assert!(client(&server).get_activity_events(None).await.is_err());

    let server = expect_route("GET", "/v2/events/admin-actions").await;
    assert!(client(&server).get_admin_action_events(None).await.is_err());

    let server = expect_route("GET", "/v2/events/ipos").await;
    assert!(client(&server).get_ipo_events(None).await.is_err());

    let server = expect_route("GET", "/v2/events/system").await;
    assert!(client(&server).get_system_events(None).await.is_err());
}

// ------------------------------------------------------- writes on behalf

/// The account-scoped writes. A `POST` that reaches the right path with the
/// wrong verb is a 405 in production and compiles perfectly.
#[tokio::test]
async fn the_trading_writes_use_the_verb_alpaca_documents() {
    let server = expect_route(
        "DELETE",
        &format!("/v1/trading/accounts/{ACCOUNT}/positions"),
    )
    .await;
    assert!(
        client(&server)
            .close_all_positions_for_account(account(), Some(true))
            .await
            .is_err()
    );

    let server = expect_route(
        "DELETE",
        &format!("/v1/trading/accounts/{ACCOUNT}/positions/AAPL"),
    )
    .await;
    assert!(
        client(&server)
            .close_position_for_account(account(), &AssetIdent::from("AAPL"), None)
            .await
            .is_err()
    );

    let server = expect_route("DELETE", &format!("/v1/trading/accounts/{ACCOUNT}/orders")).await;
    assert!(
        client(&server)
            .cancel_orders_for_account(account())
            .await
            .is_err()
    );

    let server = expect_route(
        "PATCH",
        &format!("/v1/trading/accounts/{ACCOUNT}/orders/{OTHER}"),
    )
    .await;
    assert!(
        client(&server)
            .replace_order_for_account_by_id(account(), other(), Some(&ReplaceOrderRequest::new()))
            .await
            .is_err()
    );

    let server = expect_route(
        "POST",
        &format!("/v1/trading/accounts/{ACCOUNT}/orders/estimation"),
    )
    .await;
    assert!(
        client(&server)
            .estimate_order(account(), &EstimateOrderRequest::default())
            .await
            .is_err()
    );

    let server = expect_route(
        "POST",
        &format!("/v1/trading/accounts/{ACCOUNT}/positions/AAPL240119C00150000/do-not-exercise"),
    )
    .await;
    assert!(
        client(&server)
            .do_not_exercise(account(), &AssetIdent::from("AAPL240119C00150000"))
            .await
            .is_err()
    );
}

/// The four watchlist verbs on one path pair. `POST` adds an asset and `PUT`
/// replaces the list — swapping them silently rewrites a caller's watchlist.
#[tokio::test]
async fn the_watchlist_verbs_are_not_interchangeable() {
    let server = expect_route(
        "POST",
        &format!("/v1/trading/accounts/{ACCOUNT}/watchlists"),
    )
    .await;
    assert!(
        client(&server)
            .create_watchlist_for_account(account(), &CreateWatchlistRequest::new("mine", vec![]))
            .await
            .is_err()
    );

    let server = expect_route(
        "PUT",
        &format!("/v1/trading/accounts/{ACCOUNT}/watchlists/{OTHER}"),
    )
    .await;
    assert!(
        client(&server)
            .update_watchlist_for_account_by_id(
                account(),
                other(),
                &UpdateWatchlistRequest::new().name("renamed")
            )
            .await
            .is_err()
    );

    let server = expect_route(
        "POST",
        &format!("/v1/trading/accounts/{ACCOUNT}/watchlists/{OTHER}"),
    )
    .await;
    assert!(
        client(&server)
            .add_asset_to_watchlist_for_account_by_id(account(), other(), "AAPL")
            .await
            .is_err()
    );

    let server = expect_route(
        "DELETE",
        &format!("/v1/trading/accounts/{ACCOUNT}/watchlists/{OTHER}/AAPL"),
    )
    .await;
    assert!(
        client(&server)
            .remove_asset_from_watchlist_for_account_by_id(account(), other(), "AAPL")
            .await
            .is_err()
    );
}

// -------------------------------------------------- instant funding writes

#[tokio::test]
async fn the_instant_funding_writes_reach_their_paths() {
    let server = expect_route("DELETE", "/v1/instant_funding/fund-1").await;
    assert!(
        client(&server)
            .cancel_instant_funding("fund-1")
            .await
            .is_err()
    );

    // This one could not be called at all until the list was comma-joined: a
    // bare `Vec` in a query struct fails reqwest's builder with "unsupported
    // value", so the request never left the process and the mock below saw
    // nothing. The 404 it now gets is the improvement.
    let server = expect_route("GET", "/v1/instant_funding/limits/accounts").await;
    assert!(
        client(&server)
            .get_instant_funding_account_limits(&GetAccountLimitsRequest::new(vec![
                "9001".to_owned(),
                "9002".to_owned(),
            ]))
            .await
            .is_err()
    );
    let sent = &server.received_requests().await.unwrap()[0];
    assert_eq!(sent.url.query(), Some("account_numbers=9001%2C9002"));

    // The settlement must name at least one transfer — an empty one is rejected
    // before it is sent, and would leave this mock never called.
    let server = expect_route("POST", "/v1/instant_funding/settlements").await;
    let settlement = CreateInstantFundingSettlementRequest::new(vec![SettlementTransfer {
        instant_transfer_id: other(),
        transmitter_info: TransmitterInfo::default(),
    }]);
    assert!(
        client(&server)
            .create_instant_funding_settlement(&settlement)
            .await
            .is_err()
    );
}

// ----------------------------------------------------------- funding wallet

/// These are `v1beta` too, and two of them sit on paths that differ only by
/// whether an account id is present.
#[tokio::test]
async fn the_funding_wallet_writes_are_v1beta() {
    let server = expect_route("POST", "/v1beta/accounts/funding_wallet").await;
    assert!(
        client(&server)
            .batch_create_funding_wallets(&BatchCreateFundingWalletsRequest::new(vec![account()]))
            .await
            .is_err()
    );

    let server = expect_route(
        "POST",
        &format!("/v1beta/accounts/{ACCOUNT}/funding_wallet"),
    )
    .await;
    assert!(
        client(&server)
            .create_funding_wallet(account())
            .await
            .is_err()
    );

    let server = expect_route(
        "DELETE",
        &format!("/v1beta/accounts/{ACCOUNT}/funding_wallet/recipient_bank"),
    )
    .await;
    assert!(
        client(&server)
            .delete_recipient_bank(account())
            .await
            .is_err()
    );

    let server = expect_route("POST", "/v1beta/demo/banking/funding").await;
    assert!(
        client(&server)
            .create_demo_funding(&DemoFundingRequest::new(
                Decimal::ONE,
                SupportedCurrencies::Usd,
                "9001"
            ))
            .await
            .is_err()
    );
}

// -------------------------------------------------------------- onboarding

#[tokio::test]
async fn the_onboarding_writes_reach_their_paths() {
    let server = expect_route("POST", &format!("/v1/accounts/{ACCOUNT}/options/approval")).await;
    assert!(
        client(&server)
            .request_options_approval(
                account(),
                &RequestOptionsApprovalRequest::new(OptionsLevel::Two).unwrap()
            )
            .await
            .is_err()
    );

    let server = expect_route("PATCH", &format!("/v1/accounts/{ACCOUNT}/onfido/sdk")).await;
    assert!(
        client(&server)
            .update_onfido_outcome(
                account(),
                &UpdateOnfidoOutcomeRequest::new("tok", "USER_EXITED")
            )
            .await
            .is_err()
    );
}

// ------------------------------------------------------------ tokenization

/// Two of these are `:by_…` routes — a colon in the path, not a sub-resource.
/// URL-encoding the colon would 404.
#[tokio::test]
async fn the_tokenization_routes_keep_their_colons() {
    let server = expect_route("POST", &format!("/v1/accounts/{ACCOUNT}/tokenization/mint")).await;
    assert!(
        client(&server)
            .mint_token_for_account(
                account(),
                &MintTokenRequest::new(
                    "AAPL",
                    Decimal::ONE,
                    TokenizationIssuer::Xstocks,
                    TokenizationNetwork::Ethereum,
                    "0xabc",
                )
            )
            .await
            .is_err()
    );

    let server = expect_route(
        "GET",
        &format!("/v1/accounts/{ACCOUNT}/tokenization/requests"),
    )
    .await;
    assert!(
        client(&server)
            .get_tokenization_requests_for_account(account(), None)
            .await
            .is_err()
    );

    let server = expect_route(
        "GET",
        &format!("/v1/accounts/{ACCOUNT}/tokenization/requests:by_client_request_id"),
    )
    .await;
    assert!(
        client(&server)
            .get_tokenization_request_by_client_id_for_account(
                account(),
                &ByClientRequestId::new("req-1")
            )
            .await
            .is_err()
    );

    let server = expect_route(
        "GET",
        &format!("/v1/accounts/{ACCOUNT}/tokenization/requests:by_issuer_request_id"),
    )
    .await;
    assert!(
        client(&server)
            .get_tokenization_request_by_issuer_id_for_account(account(), "iss-1")
            .await
            .is_err()
    );

    for callback in ["mint", "redeem"] {
        let server = expect_route(
            "POST",
            &format!("/v1/accounts/{ACCOUNT}/tokenization/callback/{callback}"),
        )
        .await;
        let body = json!({});
        let called = if callback == "mint" {
            client(&server)
                .tokenization_mint_callback(account(), &body)
                .await
        } else {
            client(&server)
                .tokenization_redeem_callback(account(), &body)
                .await
        };
        assert!(called.is_err());
    }
}

// ---------------------------------------------------------- crypto writes

#[tokio::test]
async fn the_crypto_wallet_writes_reach_their_paths() {
    let server = expect_route("POST", &format!("/v1/accounts/{ACCOUNT}/wallets/transfers")).await;
    assert!(
        client(&server)
            .create_crypto_transfer_for_account(account(), &json!({}))
            .await
            .is_err()
    );

    let server = expect_route(
        "POST",
        &format!("/v1/accounts/{ACCOUNT}/wallets/whitelists"),
    )
    .await;
    assert!(
        client(&server)
            .create_whitelisted_address_for_account(
                account(),
                &CreateWhitelistedAddressRequest::new("0xabc", "ETH", CryptoChain::Eth)
            )
            .await
            .is_err()
    );

    let server = expect_route(
        "DELETE",
        &format!("/v1/accounts/{ACCOUNT}/wallets/whitelists/addr-1"),
    )
    .await;
    assert!(
        client(&server)
            .delete_whitelisted_address_for_account(account(), "addr-1")
            .await
            .is_err()
    );

    let server = expect_route("GET", "/v1/wallets/fees/estimate").await;
    assert!(
        client(&server)
            .estimate_crypto_transfer_fee(&TransferFeeEstimateRequest::new(
                "ETH",
                "0xfrom",
                "0xto",
                Decimal::ONE
            ))
            .await
            .is_err()
    );
}

// --------------------------------------------------------------- remaining

#[tokio::test]
async fn the_last_few_reads_reach_their_paths() {
    let server = expect_route("GET", "/v1/rebalancing/runs").await;
    assert!(
        client(&server)
            .get_all_runs(Some(&GetRunsRequest::default()), Some(1))
            .await
            .is_err()
    );

    let server = expect_route("GET", "/v1/reporting/eod/aggregate_positions").await;
    assert!(
        client(&server)
            .get_aggregate_positions(&GetAggregatePositionsRequest::new(
                "2024-04-26".parse().unwrap()
            ))
            .await
            .is_err()
    );

    let server = expect_route("POST", "/v1/oauth/authorize").await;
    assert!(
        client(&server)
            .authorize_oauth(&OAuthRequest::new(
                account(),
                "client",
                "secret",
                "https://example.test/cb",
                "account:write"
            ))
            .await
            .is_err()
    );
}
