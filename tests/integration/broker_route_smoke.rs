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

use crate::common::broker_client as client;
use alpaca_sdk::Credentials;
use alpaca_sdk::broker::{
    BatchCreateFundingWalletsRequest, BrokerClient, CreateInstantFundingSettlementRequest,
    CreateJitSettlementRequest, DemoFundingRequest, EstimateOrderRequest, GetAccountLimitsRequest,
    GetAggregatePositionsRequest, GetRunsRequest, JitSettlementAccount, OAuthRequest, OptionsLevel,
    RequestOptionsApprovalRequest, SettlementAssetClass, SettlementTransfer, TransmitterInfo,
    UpdateOnfidoOutcomeRequest,
};
use alpaca_sdk::trading::{
    ByClientRequestId, CreateCryptoTransferRequest, CreateWatchlistRequest,
    CreateWhitelistedAddressRequest, CryptoChain, MintTokenRequest, ReplaceOrderRequest,
    TokenizationIssuer, TokenizationMintCallback, TokenizationNetwork, TokenizationRedeemRequest,
    TransferFeeEstimateRequest, UpdateWatchlistRequest,
};
use alpaca_sdk::types::{AssetIdent, SupportedCurrencies};
use rust_decimal::Decimal;
use std::future::Future;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const ACCOUNT: &str = "2a87c088-ffb6-472b-a4a3-cd9305c8605c";
const OTHER: &str = "61e69015-8549-4bfd-b9c3-01e75843f47d";

fn account() -> Uuid {
    Uuid::parse_str(ACCOUNT).unwrap()
}

fn other() -> Uuid {
    Uuid::parse_str(OTHER).unwrap()
}

/// Mounts the one route the call under test must reach, runs the call against a
/// client pointed at it, and returns the requests the mock recorded.
///
/// The call is a closure rather than the server being handed back so that each
/// server drops — which is where its `expect(1)` is verified — before the next
/// one starts. A `let server = ...` per route keeps every server in the test
/// alive until it returns, and seventy-odd listeners and runtimes across
/// nineteen concurrent tests exhausts the file descriptor limit of a stock
/// macOS shell. That failed as an unroutable request rather than as `EMFILE`:
/// the mock reported no request arrived, on a different set of routes each run.
async fn expect_route<F, Fut, T, E>(
    http_method: &str,
    http_path: &str,
    call: F,
) -> Vec<wiremock::Request>
where
    F: FnOnce(BrokerClient) -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    let server = MockServer::start().await;
    Mock::given(method(http_method))
        .and(path(http_path.to_owned()))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&server)
        .await;
    assert!(
        call(client(&server)).await.is_err(),
        "{http_method} {http_path} did not return the 404 the mock answers with"
    );
    server.received_requests().await.unwrap()
}

// --------------------------------------------------------------- versions

/// The version segment `BrokerClient::new` picks for itself.
///
/// Every other test in this file supplies `v1` through `with_config`, so the
/// constructor's own choice is asserted by nothing — change it and the suite
/// stays green while every broker route 404s. It is read back off the client
/// rather than off a mock because `new` bakes in Alpaca's base URL alongside
/// the version; that the configured version then reaches the wire is what the
/// rest of this file already proves.
#[test]
fn the_broker_client_constructor_picks_v1() {
    let credentials = Credentials::new("broker-key", "broker-secret").unwrap();

    // Both environments, because the sandbox flag picks the base URL and could
    // just as easily have been wired to pick a version.
    for sandbox in [true, false] {
        let broker = BrokerClient::new(&credentials, sandbox).unwrap();
        assert_eq!(broker.rest().config().api_version, "v1");
    }
}

// --------------------------------------------------------------- accounts

#[tokio::test]
async fn account_scoped_reads_nest_under_the_account_id() {
    expect_route(
        "GET",
        &format!("/v1/accounts/{ACCOUNT}/recipient_banks"),
        |broker| async move { broker.get_banks_for_account(account()).await },
    )
    .await;

    expect_route(
        "GET",
        &format!("/v1/trading/accounts/{ACCOUNT}/limits"),
        |broker| async move { broker.get_trading_limits(account()).await },
    )
    .await;

    // `v2beta1`, matching the activities *stream* rather than the client's v1:
    // one event and the stream of them live on the same version.
    expect_route(
        "GET",
        &format!("/v2beta1/accounts/{ACCOUNT}/events/activities/evt-1"),
        |broker| async move { broker.get_account_activity_event(account(), "evt-1").await },
    )
    .await;

    expect_route(
        "GET",
        &format!("/v1/accounts/{ACCOUNT}/onfido/sdk/tokens"),
        |broker| async move { broker.get_onfido_token(account(), None).await },
    )
    .await;
}

#[tokio::test]
async fn the_account_wide_reads_take_no_id() {
    expect_route(
        "GET",
        "/v1/accounts/ira_excess_contributions",
        |broker| async move { broker.get_ira_excess_contributions().await },
    )
    .await;

    expect_route(
        "GET",
        "/v1/accounts/options/approvals",
        |broker| async move { broker.get_options_approvals(None).await },
    )
    .await;
}

/// Two ids in one path, which is the shape a transposition hides in.
#[tokio::test]
async fn two_id_paths_put_them_in_the_documented_order() {
    expect_route(
        "GET",
        &format!("/v1/trading/accounts/{ACCOUNT}/watchlists/{OTHER}"),
        |broker| async move {
            broker
                .get_watchlist_for_account_by_id(account(), other())
                .await
        },
    )
    .await;

    expect_route(
        "GET",
        &format!("/v1/accounts/{ACCOUNT}/tokenization/requests/{OTHER}"),
        |broker| async move {
            broker
                .get_tokenization_request_for_account(account(), other())
                .await
        },
    )
    .await;

    expect_route(
        "GET",
        &format!("/v1/accounts/{ACCOUNT}/documents/w8ben/{OTHER}/download"),
        |broker| async move { broker.download_w8ben_document(account(), other()).await },
    )
    .await;
}

// -------------------------------------------------------- instant funding

#[tokio::test]
async fn the_instant_funding_routes_hang_off_one_prefix() {
    expect_route("GET", "/v1/instant_funding", |broker| async move {
        broker.get_instant_funding(None).await
    })
    .await;

    expect_route("GET", "/v1/instant_funding/fund-1", |broker| async move {
        broker.get_instant_funding_by_id("fund-1").await
    })
    .await;

    expect_route("GET", "/v1/instant_funding/limits", |broker| async move {
        broker.get_instant_funding_limits().await
    })
    .await;

    expect_route("GET", "/v1/instant_funding/reports", |broker| async move {
        broker.get_instant_funding_reports(None).await
    })
    .await;

    expect_route(
        "GET",
        "/v1/instant_funding/settlements",
        |broker| async move { broker.get_instant_funding_settlements(None).await },
    )
    .await;

    expect_route(
        "GET",
        &format!("/v1/instant_funding/settlements/{OTHER}"),
        |broker| async move { broker.get_instant_funding_settlement(other()).await },
    )
    .await;
}

// -------------------------------------------------------------------- JIT

/// The JIT ledgers live under `/transfers/jit` and its settlements under
/// `/jit` — a split that reads like a mistake and is not.
#[tokio::test]
async fn jit_ledgers_and_jit_settlements_are_different_prefixes() {
    expect_route("GET", "/v1/transfers/jit/ledgers", |broker| async move {
        broker.get_jit_ledgers().await
    })
    .await;

    expect_route("GET", "/v1/transfers/jit/limits", |broker| async move {
        broker.get_jit_trading_limits().await
    })
    .await;

    expect_route(
        "GET",
        "/v1/transfers/jit/ledger-1/balances",
        |broker| async move { broker.get_jit_ledger_balances("ledger-1", None).await },
    )
    .await;

    expect_route("GET", "/v1/jit/settlements", |broker| async move {
        broker.get_jit_settlements(None).await
    })
    .await;

    expect_route(
        "GET",
        &format!("/v1/jit/settlements/{OTHER}"),
        |broker| async move { broker.get_jit_settlement(other()).await },
    )
    .await;
}

// ------------------------------------------------------------------- FPSL

#[tokio::test]
async fn the_fpsl_routes_reach_their_own_prefix() {
    expect_route("GET", "/v1/fpsl/loans", |broker| async move {
        broker.get_fpsl_loans(None).await
    })
    .await;

    expect_route("GET", "/v1/fpsl/tiers", |broker| async move {
        broker.get_fpsl_tiers().await
    })
    .await;

    // The account id is in the middle of this one, not at the end.
    expect_route(
        "GET",
        &format!("/v1/fpsl/analytics/{ACCOUNT}/loans"),
        |broker| async move { broker.get_fpsl_analytics(account(), None).await },
    )
    .await;
}

// -------------------------------------------------------------- reporting

#[tokio::test]
async fn the_reporting_routes_reach_their_own_prefix() {
    expect_route(
        "GET",
        "/v1/reporting/eod/cash_interest",
        |broker| async move { broker.get_eod_cash_interest(None).await },
    )
    .await;

    expect_route("GET", "/v1/cash_interest/apr_tiers", |broker| async move {
        broker.get_apr_tiers().await
    })
    .await;
}

// ---------------------------------------------------------------- funding

/// The funding wallet routes are `v1beta`, not the client's `v1`. This is the
/// exact class of mistake that shipped the retired event streams.
#[tokio::test]
async fn the_funding_wallet_routes_are_v1beta_not_v1() {
    expect_route(
        "GET",
        &format!("/v1beta/accounts/{ACCOUNT}/funding_wallet/recipient_bank"),
        |broker| async move { broker.get_recipient_bank(account()).await },
    )
    .await;

    expect_route(
        "GET",
        &format!("/v1beta/accounts/{ACCOUNT}/funding_wallet/transfers"),
        |broker| async move { broker.get_funding_wallet_transfers(account()).await },
    )
    .await;

    expect_route(
        "GET",
        &format!("/v1beta/accounts/{ACCOUNT}/funding_wallet/transfers/{OTHER}"),
        |broker| async move { broker.get_funding_wallet_transfer(account(), other()).await },
    )
    .await;

    expect_route(
        "GET",
        &format!("/v1beta/accounts/{ACCOUNT}/funding_wallet/funding_details"),
        |broker| async move { broker.get_funding_details(account(), None).await },
    )
    .await;
}

// ----------------------------------------------------------- crypto wallets

#[tokio::test]
async fn the_crypto_wallet_routes_nest_under_wallets() {
    expect_route(
        "GET",
        &format!("/v1/accounts/{ACCOUNT}/wallets"),
        |broker| async move { broker.get_crypto_wallets_for_account(account(), None).await },
    )
    .await;

    expect_route(
        "GET",
        &format!("/v1/accounts/{ACCOUNT}/wallets/transfers"),
        |broker| async move { broker.get_crypto_transfers_for_account(account()).await },
    )
    .await;

    expect_route(
        "GET",
        &format!("/v1/accounts/{ACCOUNT}/wallets/transfers/transfer-1"),
        |broker| async move {
            broker
                .get_crypto_transfer_for_account(account(), "transfer-1")
                .await
        },
    )
    .await;

    expect_route(
        "GET",
        &format!("/v1/accounts/{ACCOUNT}/wallets/whitelists"),
        |broker| async move {
            broker
                .get_whitelisted_addresses_for_account(account())
                .await
        },
    )
    .await;
}

// ------------------------------------------------------------------ other

#[tokio::test]
async fn the_remaining_reads_reach_their_documented_paths() {
    expect_route(
        "GET",
        &format!("/v1/rebalancing/portfolios/{OTHER}"),
        |broker| async move { broker.get_portfolio_by_id(other()).await },
    )
    .await;

    expect_route("GET", "/v1/ipos/offer-1", |broker| async move {
        broker.get_ipo_offering("offer-1").await
    })
    .await;

    expect_route(
        "GET",
        &format!("/v1/oauth/clients/{OTHER}"),
        |broker| async move { broker.get_oauth_client(other(), None).await },
    )
    .await;

    // Deprecated by Alpaca and still answering, so it still has to route.
    expect_route(
        "GET",
        &format!("/v1/corporate_actions/announcements/{OTHER}"),
        |broker| async move {
            #[allow(deprecated)]
            let called = broker.get_corporate_announcement_by_id(other()).await;
            called
        },
    )
    .await;
}

// ------------------------------------------------------------ event streams

/// The four streams the reference sweep found, and the versions they live on:
/// activities is `v2beta1` and the other three are `v2`, on a `v1` client.
#[tokio::test]
async fn the_newer_event_streams_carry_their_own_version_segments() {
    expect_route("GET", "/v2beta1/events/activities", |broker| async move {
        broker.get_activity_events(None).await
    })
    .await;

    expect_route("GET", "/v2/events/admin-actions", |broker| async move {
        broker.get_admin_action_events(None).await
    })
    .await;

    expect_route("GET", "/v2/events/ipos", |broker| async move {
        broker.get_ipo_events(None).await
    })
    .await;

    expect_route("GET", "/v2/events/system", |broker| async move {
        broker.get_system_events(None).await
    })
    .await;
}

// ------------------------------------------------------- writes on behalf

/// The account-scoped writes. A `POST` that reaches the right path with the
/// wrong verb is a 405 in production and compiles perfectly.
#[tokio::test]
async fn the_trading_writes_use_the_verb_alpaca_documents() {
    expect_route(
        "DELETE",
        &format!("/v1/trading/accounts/{ACCOUNT}/positions"),
        |broker| async move {
            broker
                .close_all_positions_for_account(account(), Some(true))
                .await
        },
    )
    .await;

    expect_route(
        "DELETE",
        &format!("/v1/trading/accounts/{ACCOUNT}/positions/AAPL"),
        |broker| async move {
            broker
                .close_position_for_account(account(), &AssetIdent::from("AAPL"), None)
                .await
        },
    )
    .await;

    expect_route(
        "DELETE",
        &format!("/v1/trading/accounts/{ACCOUNT}/orders"),
        |broker| async move { broker.cancel_orders_for_account(account()).await },
    )
    .await;

    expect_route(
        "PATCH",
        &format!("/v1/trading/accounts/{ACCOUNT}/orders/{OTHER}"),
        |broker| async move {
            broker
                .replace_order_for_account_by_id(
                    account(),
                    other(),
                    Some(&ReplaceOrderRequest::new()),
                )
                .await
        },
    )
    .await;

    expect_route(
        "POST",
        &format!("/v1/trading/accounts/{ACCOUNT}/orders/estimation"),
        |broker| async move {
            broker
                .estimate_order(account(), &EstimateOrderRequest::default())
                .await
        },
    )
    .await;

    expect_route(
        "POST",
        &format!("/v1/trading/accounts/{ACCOUNT}/positions/AAPL240119C00150000/do-not-exercise"),
        |broker| async move {
            broker
                .do_not_exercise(account(), &AssetIdent::from("AAPL240119C00150000"))
                .await
        },
    )
    .await;
}

/// The four watchlist verbs on one path pair. `POST` adds an asset and `PUT`
/// replaces the list — swapping them silently rewrites a caller's watchlist.
#[tokio::test]
async fn the_watchlist_verbs_are_not_interchangeable() {
    expect_route(
        "POST",
        &format!("/v1/trading/accounts/{ACCOUNT}/watchlists"),
        |broker| async move {
            broker
                .create_watchlist_for_account(
                    account(),
                    &CreateWatchlistRequest::new("mine", vec![]),
                )
                .await
        },
    )
    .await;

    expect_route(
        "PUT",
        &format!("/v1/trading/accounts/{ACCOUNT}/watchlists/{OTHER}"),
        |broker| async move {
            broker
                .update_watchlist_for_account_by_id(
                    account(),
                    other(),
                    &UpdateWatchlistRequest::new().name("renamed"),
                )
                .await
        },
    )
    .await;

    expect_route(
        "POST",
        &format!("/v1/trading/accounts/{ACCOUNT}/watchlists/{OTHER}"),
        |broker| async move {
            broker
                .add_asset_to_watchlist_for_account_by_id(account(), other(), "AAPL")
                .await
        },
    )
    .await;

    expect_route(
        "DELETE",
        &format!("/v1/trading/accounts/{ACCOUNT}/watchlists/{OTHER}/AAPL"),
        |broker| async move {
            broker
                .remove_asset_from_watchlist_for_account_by_id(account(), other(), "AAPL")
                .await
        },
    )
    .await;
}

// ----------------------------------------------------- writes on a prefix
// a read already owns

/// Three routes where a `GET` on the same path is already covered elsewhere and
/// the `POST` was not. That is the worst shape for this to be in: `just
/// coverage` sees the path, a grep for the literal finds a test, and the verb
/// the write actually uses is asserted by nothing. Each of the three has an
/// existing test that stops at `validate` and never reaches the network, so
/// none of them had ever put a request on a wire.
#[tokio::test]
async fn opening_an_account_posts_to_the_path_the_listing_reads() {
    expect_route("POST", "/v1/accounts", |broker| async move {
        broker
            .create_account(&crate::broker_accounts::valid_application())
            .await
    })
    .await;
}

/// The settlement write shares `/v1/jit/settlements` with the listing, and sits
/// on the `/jit` prefix rather than the `/transfers/jit` one the ledgers use.
#[tokio::test]
async fn settling_a_jit_obligation_posts_to_the_jit_prefix() {
    expect_route("POST", "/v1/jit/settlements", |broker| async move {
        // A settlement with no accounts, or one settling nothing, is refused
        // before it is sent — which would leave the mock never called.
        let request = CreateJitSettlementRequest::new(
            vec![JitSettlementAccount::new(
                "9001".to_owned(),
                Decimal::ONE,
                TransmitterInfo::default(),
            )],
            SettlementAssetClass::UsEquity,
            SupportedCurrencies::Usd,
        );
        broker.create_jit_settlement(&request).await
    })
    .await;
}

// -------------------------------------------------- instant funding writes

#[tokio::test]
async fn the_instant_funding_writes_reach_their_paths() {
    expect_route(
        "DELETE",
        "/v1/instant_funding/fund-1",
        |broker| async move { broker.cancel_instant_funding("fund-1").await },
    )
    .await;

    // This one could not be called at all until the list was comma-joined: a
    // bare `Vec` in a query struct fails reqwest's builder with "unsupported
    // value", so the request never left the process and the mock below saw
    // nothing. The 404 it now gets is the improvement.
    let sent = expect_route(
        "GET",
        "/v1/instant_funding/limits/accounts",
        |broker| async move {
            broker
                .get_instant_funding_account_limits(&GetAccountLimitsRequest::new(vec![
                    "9001".to_owned(),
                    "9002".to_owned(),
                ]))
                .await
        },
    )
    .await;
    assert_eq!(sent[0].url.query(), Some("account_numbers=9001%2C9002"));

    // The settlement must name at least one transfer — an empty one is rejected
    // before it is sent, and would leave this mock never called.
    expect_route(
        "POST",
        "/v1/instant_funding/settlements",
        |broker| async move {
            let settlement =
                CreateInstantFundingSettlementRequest::new(vec![SettlementTransfer::new(
                    other(),
                    TransmitterInfo::default(),
                )]);
            broker.create_instant_funding_settlement(&settlement).await
        },
    )
    .await;
}

// ----------------------------------------------------------- funding wallet

/// These are `v1beta` too, and two of them sit on paths that differ only by
/// whether an account id is present.
#[tokio::test]
async fn the_funding_wallet_writes_are_v1beta() {
    expect_route(
        "POST",
        "/v1beta/accounts/funding_wallet",
        |broker| async move {
            broker
                .batch_create_funding_wallets(&BatchCreateFundingWalletsRequest::new(vec![
                    account(),
                ]))
                .await
        },
    )
    .await;

    expect_route(
        "POST",
        &format!("/v1beta/accounts/{ACCOUNT}/funding_wallet"),
        |broker| async move { broker.create_funding_wallet(account()).await },
    )
    .await;

    expect_route(
        "DELETE",
        &format!("/v1beta/accounts/{ACCOUNT}/funding_wallet/recipient_bank"),
        |broker| async move { broker.delete_recipient_bank(account()).await },
    )
    .await;

    expect_route(
        "POST",
        "/v1beta/demo/banking/funding",
        |broker| async move {
            broker
                .create_demo_funding(&DemoFundingRequest::new(
                    Decimal::ONE,
                    SupportedCurrencies::Usd,
                    "9001",
                ))
                .await
        },
    )
    .await;
}

// -------------------------------------------------------------- onboarding

#[tokio::test]
async fn the_onboarding_writes_reach_their_paths() {
    expect_route(
        "POST",
        &format!("/v1/accounts/{ACCOUNT}/options/approval"),
        |broker| async move {
            broker
                .request_options_approval(
                    account(),
                    &RequestOptionsApprovalRequest::new(OptionsLevel::Two).unwrap(),
                )
                .await
        },
    )
    .await;

    expect_route(
        "PATCH",
        &format!("/v1/accounts/{ACCOUNT}/onfido/sdk"),
        |broker| async move {
            broker
                .update_onfido_outcome(
                    account(),
                    &UpdateOnfidoOutcomeRequest::new("tok", "USER_EXITED"),
                )
                .await
        },
    )
    .await;
}

// ------------------------------------------------------------ tokenization

/// Two of these are `:by_…` routes — a colon in the path, not a sub-resource.
/// URL-encoding the colon would 404.
#[tokio::test]
async fn the_tokenization_routes_keep_their_colons() {
    expect_route(
        "POST",
        &format!("/v1/accounts/{ACCOUNT}/tokenization/mint"),
        |broker| async move {
            broker
                .mint_token_for_account(
                    account(),
                    &MintTokenRequest::new(
                        "AAPL",
                        Decimal::ONE,
                        TokenizationIssuer::Xstocks,
                        TokenizationNetwork::Ethereum,
                        "0xabc",
                    ),
                )
                .await
        },
    )
    .await;

    expect_route(
        "GET",
        &format!("/v1/accounts/{ACCOUNT}/tokenization/requests"),
        |broker| async move {
            broker
                .get_tokenization_requests_for_account(account(), None)
                .await
        },
    )
    .await;

    expect_route(
        "GET",
        &format!("/v1/accounts/{ACCOUNT}/tokenization/requests:by_client_request_id"),
        |broker| async move {
            broker
                .get_tokenization_request_by_client_id_for_account(
                    account(),
                    &ByClientRequestId::new("req-1"),
                )
                .await
        },
    )
    .await;

    expect_route(
        "GET",
        &format!("/v1/accounts/{ACCOUNT}/tokenization/requests:by_issuer_request_id"),
        |broker| async move {
            broker
                .get_tokenization_request_by_issuer_id_for_account(account(), "iss-1")
                .await
        },
    )
    .await;

    // The two callbacks used to share one `serde_json::Value` body and so could
    // share a loop. They take different types now — the spec gives them two
    // schemas, `TokenizationMintCallback` and `TokenizationRedeemRequest` —
    // which is the whole reason they are written out twice. Both name an
    // account: neither route is reached without one, since the client
    // validates the exactly-one-of rule before it sends.
    expect_route(
        "POST",
        &format!("/v1/accounts/{ACCOUNT}/tokenization/callback/mint"),
        |broker| async move {
            let mut body = TokenizationMintCallback::new(Uuid::nil(), "0xdead");
            body.client_account_id = Some(account());
            broker.tokenization_mint_callback(account(), &body).await
        },
    )
    .await;

    expect_route(
        "POST",
        &format!("/v1/accounts/{ACCOUNT}/tokenization/callback/redeem"),
        |broker| async move {
            let mut body = TokenizationRedeemRequest::new(
                "iss-1",
                "AAPL",
                "AAPLx",
                Decimal::ONE,
                TokenizationNetwork::Solana,
                "wallet",
                "0xdead",
            );
            body.client_account_id = Some(account());
            broker.tokenization_redeem_callback(account(), &body).await
        },
    )
    .await;
}

// ---------------------------------------------------------- crypto writes

#[tokio::test]
async fn the_crypto_wallet_writes_reach_their_paths() {
    expect_route(
        "POST",
        &format!("/v1/accounts/{ACCOUNT}/wallets/transfers"),
        |broker| async move {
            broker
                .create_crypto_transfer_for_account(
                    account(),
                    &CreateCryptoTransferRequest::new("0xabc", "ETH", Decimal::ONE),
                )
                .await
        },
    )
    .await;

    expect_route(
        "POST",
        &format!("/v1/accounts/{ACCOUNT}/wallets/whitelists"),
        |broker| async move {
            broker
                .create_whitelisted_address_for_account(
                    account(),
                    &CreateWhitelistedAddressRequest::new("0xabc", "ETH", CryptoChain::Eth),
                )
                .await
        },
    )
    .await;

    expect_route(
        "DELETE",
        &format!("/v1/accounts/{ACCOUNT}/wallets/whitelists/addr-1"),
        |broker| async move {
            broker
                .delete_whitelisted_address_for_account(account(), "addr-1")
                .await
        },
    )
    .await;

    expect_route("GET", "/v1/wallets/fees/estimate", |broker| async move {
        broker
            .estimate_crypto_transfer_fee(&TransferFeeEstimateRequest::new(
                "ETH",
                "0xfrom",
                "0xto",
                Decimal::ONE,
            ))
            .await
    })
    .await;
}

// --------------------------------------------------------------- remaining

#[tokio::test]
async fn the_last_few_reads_reach_their_paths() {
    expect_route("GET", "/v1/rebalancing/runs", |broker| async move {
        broker
            .get_all_runs(Some(&GetRunsRequest::default()), Some(1))
            .await
    })
    .await;

    expect_route(
        "GET",
        "/v1/reporting/eod/aggregate_positions",
        |broker| async move {
            broker
                .get_aggregate_positions(&GetAggregatePositionsRequest::new(
                    "2024-04-26".parse().unwrap(),
                ))
                .await
        },
    )
    .await;

    expect_route("POST", "/v1/oauth/authorize", |broker| async move {
        broker
            .authorize_oauth(&OAuthRequest::new(
                account(),
                "client",
                "secret",
                "https://example.test/cb",
                "account:write",
            ))
            .await
    })
    .await;
}
