//! Every captured payload under `fixtures/`, read into the model that claims it.
//!
//! `harvested_go` states the doctrine for `fixtures/go`: "a payload that no test
//! touches is a payload that proves nothing", and the crate has already paid for
//! one — an account list carrying `"funding_source": null` that no test parsed,
//! against a model that could not read it. `fixtures/data` was in the same
//! state: forty-five of its fifty-six payloads were named by no `.rs` file in
//! the repository, so most of what `just capture` recorded was decoration.
//!
//! The three directories are swept two different ways, because they arrive with
//! two different amounts of self-description.
//!
//! **`fixtures/data` dispatches on the envelope key.** The market data API wraps
//! every response in the name of what it carries — `{"bars": …}`,
//! `{"quotes": …}` — and that key is already load-bearing, since the pagination
//! loop merges pages by it. Reading it here means a payload from a route that
//! does not exist yet is still parsed by whichever model matches its envelope.
//!
//! **`fixtures/broker` and `fixtures/trading` dispatch on the file name**, from
//! the table below. They have no envelope: the route decides the model, and two
//! routes that return quite different shapes — `Order` and `ClosePositionResponse`
//! — are both a bare JSON array. The name is not a guess, though. Each fixture is
//! named for the alpaca-py test that produced it, and `fixtures/index.json`
//! records the method and path that test asserted for seventy of the seventy-eight;
//! the table was written from that index, not from reading the names.
//!
//! Either way an unrecognised payload **fails** rather than being quietly
//! skipped. That is the whole point: a fixture this file does not know is a
//! fixture nothing reads.
//!
//! A decode failure here is a fact about the API, not a test to be adjusted. If
//! one of these starts failing, the model is wrong or the wire changed — fixing
//! it by loosening the model or editing the payload destroys the evidence.
//!
//! Seventy-seven of the seventy-eight broker and trading payloads decode. The
//! one that does not is listed in `trading::KNOWN_FAILING` with the reason, and
//! it is a defect in the fixture rather than in the model.

#![cfg(any(feature = "data", feature = "trading"))]

use serde_json::Value;

/// Every JSON payload in `dir`, by file name, sorted.
fn payloads(dir: &str) -> Vec<(String, Value)> {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(dir);
    let mut out = Vec::new();
    for entry in
        std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("{} should exist: {e}", dir.display()))
    {
        let path = entry.expect("readable entry").path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if name == "index.json" {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        let value =
            serde_json::from_str(&text).unwrap_or_else(|e| panic!("{name} is not valid JSON: {e}"));
        out.push((name, value));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// Reads `value` into `T`, failing with the payload attached.
///
/// The payload is in the message on purpose: a decode failure here is a fact
/// about the API, and the field that broke it is not guessable from the error
/// alone.
///
/// Only the data sweep uses this; the name-keyed sweeps collect through
/// [`Sweep`] instead. Hence the gate — without it a `--features trading` build
/// has an unused function and `-D warnings` turns that into a failure.
#[cfg(feature = "data")]
fn parse_as<T: serde::de::DeserializeOwned>(name: &str, value: &Value) {
    let _: T = serde_json::from_value(value.clone())
        .unwrap_or_else(|e| panic!("{name} does not fit the model: {e}\n{value:#}"));
}

/// A sweep that keeps going after a failure, so one bad payload does not hide
/// the rest.
///
/// The data sweep panics on the first failure and that is right for it: an
/// envelope it cannot dispatch means the sweep itself is incomplete. The
/// name-keyed sweeps below are different — they are reading seventy-eight
/// payloads whose models this crate changed in bulk, and the useful output is
/// *every* payload that no longer fits, not the alphabetically first one.
#[cfg(feature = "trading")]
#[derive(Default)]
struct Sweep {
    failures: Vec<(String, String)>,
}

#[cfg(feature = "trading")]
impl Sweep {
    /// Reads `value` into `T`, recording the error rather than unwinding.
    fn check<T: serde::de::DeserializeOwned>(&mut self, name: &str, value: &Value) {
        if let Err(error) = serde_json::from_value::<T>(value.clone()) {
            self.failures.push((name.to_owned(), error.to_string()));
        }
    }

    /// Asserts the failures are exactly `known`, by file name and by reason.
    ///
    /// Both directions matter. A new failure is a model or a wire change and
    /// must fail the build. A `known` entry that now decodes must fail it too:
    /// otherwise the exemption outlives the defect, and the next reader is told
    /// a payload is broken when it is not.
    ///
    /// The recorded reason is checked as well. An entry that still fails, but
    /// for something other than what it documents, is the same lie in a form
    /// the file-name check cannot see.
    fn finish(self, known: &[(&str, &str)]) {
        let failed: Vec<&str> = self.failures.iter().map(|(n, _)| n.as_str()).collect();

        let unexpected: Vec<&(String, String)> = self
            .failures
            .iter()
            .filter(|(name, _)| !known.iter().any(|(k, _)| k == name))
            .collect();
        assert!(
            unexpected.is_empty(),
            "captured payloads stopped fitting their models:\n{}",
            unexpected
                .iter()
                .map(|(n, e)| format!("  {n}: {e}"))
                .collect::<Vec<_>>()
                .join("\n")
        );

        let fixed: Vec<&str> = known
            .iter()
            .map(|(name, _)| *name)
            .filter(|name| !failed.contains(name))
            .collect();
        assert!(
            fixed.is_empty(),
            "these are listed as known-failing but now decode — drop them from \
             the list: {fixed:?}"
        );

        let misreported: Vec<String> = known
            .iter()
            .filter_map(|(name, expected)| {
                let (_, actual) = self.failures.iter().find(|(n, _)| n == name)?;
                (!actual.contains(expected))
                    .then(|| format!("  {name}\n    recorded: {expected}\n    actual:   {actual}"))
            })
            .collect();
        assert!(
            misreported.is_empty(),
            "these still fail, but not for the reason recorded — the exemption \
             is describing a defect that is no longer the one there:\n{}",
            misreported.join("\n")
        );
    }
}

// --------------------------------------------------------------- market data

#[cfg(feature = "data")]
mod data {
    use super::{parse_as, payloads};
    use alpaca_sdk::data::{
        Bar, CorporateActions, MostActives, Movers, NewsSet, OptionsSnapshot, Quote, Snapshot,
        Trade,
    };
    use serde_json::Value;
    use std::collections::HashMap;

    /// Reads a `{"bars": {"AAPL": …}}` envelope, whichever of the two shapes the
    /// symbol maps to.
    ///
    /// Both are live: the time series routes map a symbol to a list, the "latest"
    /// routes map it to one record, and the envelope key is identical. An empty
    /// map is neither and is also real — it is what a symbol Alpaca has nothing
    /// for comes back as.
    fn parse_multi_or_latest<T>(name: &str, inner: &Value)
    where
        T: serde::de::DeserializeOwned,
    {
        match inner.as_object().and_then(|m| m.values().next()) {
            Some(first) if first.is_array() => parse_as::<HashMap<String, Vec<T>>>(name, inner),
            Some(_) => parse_as::<HashMap<String, T>>(name, inner),
            None => assert!(
                inner.as_object().is_some_and(serde_json::Map::is_empty),
                "{name}: envelope is neither a symbol map nor empty"
            ),
        }
    }

    #[test]
    fn every_captured_data_payload_is_read_by_its_model() {
        let all = payloads("fixtures/data");
        assert!(
            all.len() > 50,
            "expected the whole capture, got {}",
            all.len()
        );

        for (name, value) in &all {
            // `next_page_token` rides alongside every envelope and belongs to the
            // pagination loop, not to any model.
            if let Some(inner) = value.get("bars") {
                parse_multi_or_latest::<Bar>(name, inner);
            } else if let Some(inner) = value.get("trades") {
                parse_multi_or_latest::<Trade>(name, inner);
            } else if let Some(inner) = value.get("quotes") {
                parse_multi_or_latest::<Quote>(name, inner);
            } else if let Some(inner) = value.get("snapshots") {
                // Options carry greeks and an implied volatility the equity
                // snapshot has no field for, and the envelope key does not say
                // which is which — the route does.
                if name.contains("option") {
                    parse_as::<HashMap<String, Option<OptionsSnapshot>>>(name, inner);
                } else {
                    parse_as::<HashMap<String, Option<Snapshot>>>(name, inner);
                }
            } else if let Some(inner) = value.get("trade") {
                // Singular, and still a symbol map: the option latest-trade route
                // answers `{"trade": {"AAPL240126P00050000": …}}` where its stock
                // twin answers `{"trades": {…}}`. The singular key on a
                // multi-symbol shape is Alpaca's, not a capture artefact — the
                // only payload in this directory that has it maps a symbol
                // underneath.
                parse_multi_or_latest::<Trade>(name, inner);
            } else if value.get("corporate_actions").is_some() {
                parse_as::<CorporateActions>(name, value);
            } else if value.get("news").is_some() {
                parse_as::<NewsSet>(name, value);
            } else if value.get("most_actives").is_some() {
                parse_as::<MostActives>(name, value);
            } else if value.get("gainers").is_some() {
                parse_as::<Movers>(name, value);
            } else {
                // The stock snapshot route is the one that does not wrap: it maps
                // the symbol straight to the record, so an empty response is `{}`
                // and there is no key to dispatch on.
                assert!(
                    name.contains("snapshot"),
                    "{name} is in an envelope this sweep does not know: {value:#}"
                );
                parse_as::<HashMap<String, Option<Snapshot>>>(name, value);
            }
        }
    }
}

// -------------------------------------------------------------------- broker

#[cfg(feature = "broker")]
mod broker {
    use super::{Sweep, payloads};
    use alpaca_sdk::ApiError;
    use alpaca_sdk::broker::{
        ACHRelationship, Account, AllAccountsPositions, Bank, BatchJournalResponse, Journal, Order,
        Portfolio, RebalancingRun, Subscription, SubscriptionsPage, TradeAccount, TradeDocument,
        Transfer,
    };
    use alpaca_sdk::trading::{
        AccountConfiguration, Activity, Calendar, CancelOrderResponse, Clock,
        ClosePositionResponse, PortfolioHistory, Position, Watchlist,
    };

    /// Payloads that do not decode, with the reason. Empty, and that is a
    /// result: all fifty-five broker captures fit their models.
    const KNOWN_FAILING: &[(&str, &str)] = &[];

    /// `broker` reuses the trading models for orders, positions and watchlists,
    /// exactly as the client does — but **not** for `Order` and `TradeAccount`,
    /// which the broker API answers in its own shape and the crate models
    /// separately. Importing the wrong one of each is the mistake this table is
    /// most likely to make, so both are named explicitly above.
    #[test]
    fn every_captured_broker_payload_is_read_by_its_model() {
        let all = payloads("fixtures/broker");
        assert_eq!(all.len(), 55, "the broker capture changed size");
        let mut sweep = Sweep::default();

        for (name, value) in &all {
            let stem = name.strip_suffix(".json").expect("json extension");
            match stem {
                // Accounts.
                "test_accounts_routes__test_create_account__01"
                | "test_accounts_routes__test_create_ira_account__01"
                | "test_accounts_routes__test_create_lct_account__01"
                | "test_accounts_routes__test_get_account__01"
                | "test_accounts_routes__test_update_account__01" => {
                    sweep.check::<Account>(name, value);
                }
                "test_accounts_routes__test_list_accounts_no_params__01"
                | "test_accounts_routes__test_list_accounts_parses_entities_if_present__01" => {
                    sweep.check::<Vec<Account>>(name, value);
                }
                // Not a model payload at all: alpaca-py captured the *error*
                // body, and it is the one fixture here that a success model must
                // never be asked to read. It goes through the crate's real error
                // parse instead, which is what a caller would see.
                "test_accounts_routes__test_get_account_account_not_found__01" => {
                    let error = ApiError::from_body(401, "/v1/accounts/{id}", value.to_string());
                    assert_eq!(error.code, Some(40_110_000), "{name}");
                    assert!(!error.message.is_empty(), "{name}");
                }
                "test_accounts_routes__test_get_trade_account_by_id__01"
                | "test_accounts_routes__test_get_trade_account_by_id_without_deprecated_pdt_fields__01" =>
                {
                    sweep.check::<TradeAccount>(name, value);
                }
                "test_accounts_routes__test_get_trade_configuration_for_account__01"
                | "test_accounts_routes__test_update_trade_configuration_for_account__01" => {
                    sweep.check::<AccountConfiguration>(name, value);
                }

                // Activities.
                "test_account_activities_routes__test_get_activities_for_account_max_items_and_single_request_date__01" =>
                {
                    sweep.check::<Vec<Activity>>(name, value);
                }

                // Documents.
                "test_documents_routes__test_get_trade_document_for_account_by_id__01" => {
                    sweep.check::<TradeDocument>(name, value);
                }
                "test_documents_routes__test_get_trade_documents_for_account__01" => {
                    sweep.check::<Vec<TradeDocument>>(name, value);
                }

                // Funding.
                "test_funding_routes__test_create_ach_relationship_for_account__01" => {
                    sweep.check::<ACHRelationship>(name, value);
                }
                "test_funding_routes__test_get_ach_relationships_for_account__01"
                | "test_funding_routes__test_get_ach_relationships_for_account_with_statuses__01" =>
                {
                    sweep.check::<Vec<ACHRelationship>>(name, value);
                }
                "test_funding_routes__test_create_bank_for_account__01" => {
                    sweep.check::<Bank>(name, value);
                }
                "test_funding_routes__test_get_banks_for_account__01" => {
                    sweep.check::<Vec<Bank>>(name, value);
                }
                "test_funding_routes__test_create_transfer_for_account__01" => {
                    sweep.check::<Transfer>(name, value);
                }

                // Journals.
                "test_journal_routes__test_create_journal__01"
                | "test_journal_routes__test_create_lct_journal__01"
                | "test_journal_routes__test_get_journal_by_id__01" => {
                    sweep.check::<Journal>(name, value);
                }
                "test_journal_routes__test_get_journals__01" => {
                    sweep.check::<Vec<Journal>>(name, value);
                }
                // A batch journal answers with a per-leg result carrying its own
                // `error_message`, not with a `Journal`.
                "test_journal_routes__test_batch_journal__01"
                | "test_journal_routes__test_reverse_batch_journal__01" => {
                    sweep.check::<Vec<BatchJournalResponse>>(name, value);
                }

                // Clock and calendar, shared with the trading surface.
                "test_misc_routes__test_get_calendar__01" => {
                    sweep.check::<Vec<Calendar>>(name, value);
                }
                "test_misc_routes__test_get_clock__01" => sweep.check::<Clock>(name, value),

                // Rebalancing.
                "test_rebalancing_routes__test_create_portfolio__01"
                | "test_rebalancing_routes__test_get_portfolio_by_id__01"
                | "test_rebalancing_routes__test_update_portfolio_by_id__01" => {
                    sweep.check::<Portfolio>(name, value);
                }
                "test_rebalancing_routes__test_get_all_portfolios__01" => {
                    sweep.check::<Vec<Portfolio>>(name, value);
                }
                "test_rebalancing_routes__test_create_subscription__01"
                | "test_rebalancing_routes__test_get_subscription_by_id__01" => {
                    sweep.check::<Subscription>(name, value);
                }
                // Paged, unlike the portfolio list beside it — the payload is
                // `{"subscriptions": […], "next_page_token": …}`.
                "test_rebalancing_routes__test_get_all_subscriptions__01" => {
                    sweep.check::<SubscriptionsPage>(name, value);
                }
                "test_rebalancing_routes__test_create_manual_run__01"
                | "test_rebalancing_routes__test_get_run_by_id__01" => {
                    sweep.check::<RebalancingRun>(name, value);
                }

                // Trading on behalf of an account.
                "test_trading_routes__test_cancel_orders_for_account__01" => {
                    sweep.check::<Vec<CancelOrderResponse>>(name, value);
                }
                "test_trading_routes__test_close_all_positions_for_account__01" => {
                    sweep.check::<Vec<ClosePositionResponse>>(name, value);
                }
                "test_trading_routes__test_close_position_for_account_with_percentage__01"
                | "test_trading_routes__test_close_position_for_account_with_qty__01" => {
                    sweep.check::<Order>(name, value);
                }
                "test_trading_routes__test_get_all_accounts_positions__01" => {
                    sweep.check::<AllAccountsPositions>(name, value);
                }
                "test_trading_routes__test_get_all_positions_for_account__01" => {
                    sweep.check::<Vec<Position>>(name, value);
                }
                "test_trading_routes__test_get_portfolio_history__01"
                | "test_trading_routes__test_get_portfolio_history_with_filter__01"
                | "test_trading_routes__test_get_portfolio_history_with_null_base_value__01"
                | "test_trading_routes__test_get_portfolio_history_with_null_pl_pct__01" => {
                    sweep.check::<PortfolioHistory>(name, value);
                }

                // Watchlists. Every route here answers with the whole watchlist,
                // including the two that remove an asset.
                "test_watchlist_routes__test_add_asset_to_watchlist__01"
                | "test_watchlist_routes__test_create_watchlist_for_account__01"
                | "test_watchlist_routes__test_get_watchlist_for_account_by_id__01"
                | "test_watchlist_routes__test_remove_asset_from_watchlist_for_account__01"
                | "test_watchlist_routes__test_remove_asset_to_watchlist_for_account__01"
                | "test_watchlist_routes__test_update_watchlist_for_account_by_id__01" => {
                    sweep.check::<Watchlist>(name, value);
                }
                "test_watchlist_routes__test_get_watchlists_for_account__01" => {
                    sweep.check::<Vec<Watchlist>>(name, value);
                }

                other => panic!(
                    "{other} is a broker fixture this sweep has no model for. \
                     Add it to the table — see fixtures/index.json for the route \
                     the capturing test asserted."
                ),
            }
        }

        sweep.finish(KNOWN_FAILING);
    }
}

// ------------------------------------------------------------------- trading

#[cfg(feature = "trading")]
mod trading {
    use super::{Sweep, payloads};
    use alpaca_sdk::trading::{
        AccountConfiguration, Asset, CancelOrderResponse, ClosePositionResponse,
        CorporateActionAnnouncement, OptionContract, OptionContractsResponse, Order, Position,
        TradeAccount,
    };

    /// Payloads that do not decode, with the reason.
    ///
    /// These are **not** models to fix. Each entry is a fact about the fixture,
    /// and the model it fails against is the one the wire justifies.
    const KNOWN_FAILING: &[(&str, &str)] = &[(
        // `"hwm": "string"` — the literal placeholder out of Alpaca's OpenAPI
        // example, not a price. alpaca-py's `test_get_orders` mocked its
        // response with the documentation sample rather than a captured body,
        // so this one field is documentation, not evidence.
        //
        // `Order::hwm` stays `Option<Decimal>`: it is the high-water mark of a
        // trailing stop, every one of the other ten order payloads here sends
        // it as `null`, and widening it to `Option<String>` to accommodate a
        // placeholder would lose the parse on every real fill. Editing the
        // fixture is worse still — it would stop being what alpaca-py shipped.
        "test_order_routes__test_get_orders__01.json",
        "invalid value: string \"string\", expected a decimal as a string or number",
    )];

    #[test]
    fn every_captured_trading_payload_is_read_by_its_model() {
        let all = payloads("fixtures/trading");
        assert_eq!(all.len(), 23, "the trading capture changed size");
        let mut sweep = Sweep::default();

        for (name, value) in &all {
            let stem = name.strip_suffix(".json").expect("json extension");
            match stem {
                "test_account_routes__test_get_account__01" => {
                    sweep.check::<TradeAccount>(name, value);
                }
                "test_account_routes__test_get_account_configurations__01"
                | "test_account_routes__test_get_account_configurations_without_deprecated_pdt_fields__01" =>
                {
                    sweep.check::<AccountConfiguration>(name, value);
                }

                "test_asset_routes__test_get_all_assets__01"
                | "test_asset_routes__test_get_all_assets_params__01" => {
                    sweep.check::<Vec<Asset>>(name, value);
                }
                "test_asset_routes__test_get_asset__01" => sweep.check::<Asset>(name, value),

                "test_corporate_announcements__test_get_announcements__01" => {
                    sweep.check::<Vec<CorporateActionAnnouncement>>(name, value);
                }

                "test_option_routes__test_get_option_contract__01" => {
                    sweep.check::<OptionContract>(name, value);
                }
                // Paged: `{"option_contracts": […], "next_page_token": …}`.
                "test_option_routes__test_get_option_contracts__01"
                | "test_option_routes__test_get_option_contracts_with_multiple_symbols__01" => {
                    sweep.check::<OptionContractsResponse>(name, value);
                }

                "test_order_routes__test_cancel_orders__01" => {
                    sweep.check::<Vec<CancelOrderResponse>>(name, value);
                }
                "test_order_routes__test_get_orders__01" => sweep.check::<Vec<Order>>(name, value),
                "test_order_routes__test_get_order_by_client_id__01"
                | "test_order_routes__test_get_order_by_id__01"
                | "test_order_routes__test_limit_order__01"
                | "test_order_routes__test_market_order__01"
                | "test_order_routes__test_order_position_intent__01"
                | "test_order_routes__test_order_position_intent__02"
                | "test_order_routes__test_replace_order__01" => sweep.check::<Order>(name, value),

                "test_position_routes__test_close_all_positions__01" => {
                    sweep.check::<Vec<ClosePositionResponse>>(name, value);
                }
                "test_position_routes__test_close_position_with_percentage__01"
                | "test_position_routes__test_close_position_with_qty__01" => {
                    sweep.check::<Order>(name, value);
                }
                "test_position_routes__test_get_all_positions__01" => {
                    sweep.check::<Vec<Position>>(name, value);
                }

                other => panic!(
                    "{other} is a trading fixture this sweep has no model for. \
                     Add it to the table — see fixtures/index.json for the route \
                     the capturing test asserted."
                ),
            }
        }

        sweep.finish(KNOWN_FAILING);
    }
}
