//! Every type a caller has to *build* can still be built from outside the crate.
//!
//! `#[non_exhaustive]` is applied broadly here, and it is invisible from inside:
//! a struct literal in `src/` compiles whether or not the attribute is present,
//! and only an external crate — which is what an integration test is — sees the
//! difference. Two request-body types were caught unbuildable by this file: `RebalancingCondition`, the element type of a portfolio's
//! `rebalance_conditions`, and `CIPInfo`, the body of the CIP upload.
//!
//! Nothing here asserts on behaviour. It asserts that the *shape* of the public
//! API is reachable, which no unit test in `src/` can do.

#![cfg(all(feature = "broker", feature = "trading"))]

use alpaca_sdk::broker::{
    BankAddress, CIPDocument, CIPIdentity, CIPInfo, CIPKycInfo, CIPPhoto, CIPWatchlist,
    CalendarSubType, CreateBankRequest, CreateWithdrawalRequest, DriftBandSubType, IdentifierType,
    ManualACHRelationship, PlaidACHRelationship, RebalancingCondition, UpdatableContact,
    UpdatableIdentity, Weight,
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

/// The upload body *and* every nested check it can carry.
///
/// Asserting `cip.kyc.is_none()` would pass while the five check types were
/// themselves unconstructible — a guard that misses exactly the case it exists
/// for. Filling a check is what makes it an assertion.
#[test]
fn a_cip_record_can_be_built_for_upload_with_its_checks() {
    let mut cip = CIPInfo::new(Uuid::nil(), Uuid::nil());
    assert!(cip.provider_name.is_empty());

    let mut kyc = CIPKycInfo::new("kyc-1");
    kyc.applicant_name = Some("Jane Doe".to_owned());
    cip.kyc = Some(Box::new(kyc));

    cip.document = Some(Box::new(CIPDocument::new("doc-1")));
    cip.photo = Some(Box::new(CIPPhoto::new("photo-1")));
    cip.identity = Some(Box::new(CIPIdentity::new("id-1")));
    cip.watchlist = Some(Box::new(CIPWatchlist::new("watch-1")));

    let kyc = cip.kyc.as_ref().expect("the check just set");
    assert_eq!(kyc.applicant_name.as_deref(), Some("Jane Doe"));
    assert_eq!(kyc.id, "kyc-1");
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

/// A request body sends the fields it was given, not a null for every field it
/// was not.
///
/// This is the same hazard `AccountConfiguration` was fixed for — it `PATCH`ed
/// `"dtbp_check": null` at a route whose schema does not document the field —
/// and it reaches further than that one type. Two of the constructors above
/// leave an optional field unset by design, so without this every call would
/// put a null on the wire.
#[test]
fn request_bodies_omit_the_fields_they_do_not_set() {
    let drift = serde_json::to_value(RebalancingCondition::drift_band(
        DriftBandSubType::Absolute,
        Decimal::ONE,
    ))
    .unwrap();
    assert!(drift.get("day").is_none(), "{drift}");
    assert_eq!(drift["percent"], "1");

    let calendar = serde_json::to_value(RebalancingCondition::calendar(
        CalendarSubType::Quarterly,
        None,
    ))
    .unwrap();
    assert!(calendar.get("percent").is_none(), "{calendar}");
    assert!(calendar.get("day").is_none(), "{calendar}");

    let cash = serde_json::to_value(Weight::cash(Decimal::ONE)).unwrap();
    assert!(cash.get("symbol").is_none(), "{cash}");

    // `Disclosures` is required on an account application, so an all-default one
    // is a shape Alpaca really sees.
    let disclosures = serde_json::to_value(alpaca_sdk::broker::Disclosures::default()).unwrap();
    assert_eq!(
        disclosures.as_object().map(serde_json::Map::len),
        Some(0),
        "an unset disclosure set should send nothing, got {disclosures}"
    );

    // The upload bodies this file constructs above. These were missed by the
    // first sweep precisely because this test did not look at them: a `CIPInfo`
    // with one check set sent four top-level nulls and eighteen more inside it.
    let mut cip = CIPInfo::new(Uuid::nil(), Uuid::nil());
    let mut kyc = CIPKycInfo::new("kyc-1");
    kyc.applicant_name = Some("Jane Doe".to_owned());
    cip.kyc = Some(Box::new(kyc));

    let encoded = serde_json::to_value(&cip).unwrap();
    assert!(encoded.get("document").is_none(), "{encoded}");
    assert!(encoded.get("photo").is_none(), "{encoded}");
    // The three fields Alpaca assigns. An upload that invented a `created_at`
    // would be putting the client's clock into a KYC record — and these two
    // were the gap the rest of this assertion did not cover.
    assert!(encoded.get("created_at").is_none(), "{encoded}");
    assert!(encoded.get("updated_at").is_none(), "{encoded}");
    assert!(encoded.get("provider_name").is_none(), "{encoded}");
    let kyc = &encoded["kyc"];
    assert_eq!(kyc["applicant_name"], "Jane Doe");
    assert!(kyc.get("risk_score").is_none(), "{kyc}");
    assert!(kyc.get("approved_at").is_none(), "{kyc}");

    // And the nested body on the account application.
    let mut contact = alpaca_sdk::broker::TrustedContact::default();
    contact.given_name = Some("Jane".to_owned());
    let encoded = serde_json::to_value(&contact).unwrap();
    assert_eq!(
        encoded.as_object().map(serde_json::Map::len),
        Some(1),
        "a trusted contact should send only what was set, got {encoded}"
    );
}
