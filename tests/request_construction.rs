//! Every type a caller has to *build* can still be built from outside the crate.
//!
//! `#[non_exhaustive]` is applied broadly here, and it is invisible from inside:
//! a struct literal in `src/` compiles whether or not the attribute is present,
//! and only an external crate — which is what an integration test is — sees the
//! difference. Two request-body types shipped unbuildable before this file
//! existed: `RebalancingCondition`, the element type of a portfolio's
//! `rebalance_conditions`, and `CIPInfo`, the body of the CIP upload.
//!
//! Nothing here asserts on behaviour. It asserts that the *shape* of the public
//! API is reachable, which no unit test in `src/` can do.

#![cfg(all(feature = "broker", feature = "trading"))]

use alpaca_sdk::broker::{
    BankAddress, CIPInfo, CalendarSubType, CreateBankRequest, CreateWithdrawalRequest,
    DriftBandSubType, IdentifierType, ManualACHRelationship, PlaidACHRelationship,
    RebalancingCondition, UpdatableContact, UpdatableIdentity, Weight,
};
use alpaca_sdk::types::SupportedCurrencies;
use alpaca_sdk::{Decimal, trading};
use uuid::Uuid;

#[test]
fn rebalancing_conditions_can_be_built() {
    let drift = RebalancingCondition::drift_band(DriftBandSubType::Absolute, Decimal::ONE);
    assert_eq!(drift.percent, Some(Decimal::ONE));
    assert!(drift.day.is_none());

    let calendar = RebalancingCondition::calendar(CalendarSubType::Quarterly, Some("1".to_owned()));
    assert_eq!(calendar.day.as_deref(), Some("1"));
    assert!(calendar.percent.is_none());
}

#[test]
fn a_cip_record_can_be_built_for_upload() {
    let cip = CIPInfo::new(Uuid::nil(), Uuid::nil());
    assert!(cip.provider_name.is_empty());
    assert!(cip.kyc.is_none());
}

#[test]
fn portfolio_weights_can_be_built() {
    assert!(Weight::cash(Decimal::new(1000, 2)).percent > Decimal::ZERO);
    assert!(Weight::asset("AAPL", Decimal::new(9000, 2)).percent > Decimal::ZERO);
}

#[test]
fn ach_and_bank_bodies_can_be_built() {
    let manual = ManualACHRelationship::new(
        "Jane Doe",
        alpaca_sdk::broker::BankAccountType::Checking,
        "123456789",
        "021000021",
    )
    .nickname("main");
    assert_eq!(manual.nickname.as_deref(), Some("main"));

    let plaid = PlaidACHRelationship::new("processor-sandbox-abc");
    assert_eq!(plaid.processor_token, "processor-sandbox-abc");

    // Five same-typed `String`s, so it is filled in by name rather than
    // positionally — which is the reason it has no constructor.
    let mut address = BankAddress::default();
    address.country = "USA".to_owned();
    address.city = "San Mateo".to_owned();

    let bank = CreateBankRequest::international("My Bank", "BOFAUS3N", "123456789", address);
    assert!(bank.validate().is_ok());
    assert_eq!(bank.bank_code_type, IdentifierType::Bic);
}

#[test]
fn account_update_bodies_can_be_built() {
    let mut contact = UpdatableContact::default();
    contact.email_address = Some("jane@example.com".to_owned());
    assert!(contact.email_address.is_some());

    let mut identity = UpdatableIdentity::default();
    identity.given_name = Some("Jane".to_owned());
    assert!(identity.given_name.is_some());
}

#[test]
fn money_moving_bodies_can_be_built() {
    let withdrawal = CreateWithdrawalRequest::new(Decimal::ONE, SupportedCurrencies::Gbp);
    assert!(withdrawal.validate().is_ok());
}

#[test]
fn the_order_value_types_can_be_built() {
    // These are deliberately *exhaustive*, so a struct literal must keep working.
    let prices = trading::StopLimit {
        stop: Decimal::new(9500, 2),
        limit: Decimal::new(9450, 2),
    };
    assert!(prices.stop > prices.limit);

    let order = trading::OrderRequest::stop_limit(
        "AAPL",
        trading::OrderSide::Sell,
        trading::OrderAmount::Qty(Decimal::ONE),
        trading::TimeInForce::Day,
        prices,
    );
    assert!(order.validate().is_ok());
}
