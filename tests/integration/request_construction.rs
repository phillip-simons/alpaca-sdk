//! Every type a caller has to *build* can still be built from outside the crate.
//!
//! `#[non_exhaustive]` is applied broadly here, and it is invisible from inside:
//! a struct literal in `src/` compiles whether or not the attribute is present,
//! and only an external crate — which is what an integration test is — sees the
//! difference. Two request-body types were caught unbuildable by this file:
//! `RebalancingCondition`, the element type of a portfolio's
//! `rebalance_conditions`, and `CIPInfo`, the body of the CIP upload — the
//! latter dragging in five nested check types that were unbuildable with it.
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
use serde_json::json;
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
    // These are `#[non_exhaustive]`, so a caller outside the crate builds them
    // through the constructor rather than a struct literal.
    let prices = trading::StopLimit::new(Decimal::new(9500, 2), Decimal::new(9450, 2));
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

    // `Disclosures` is required on an account application, so what it puts on
    // the wire matters. Alpaca rejects an empty one — the exposure booleans are
    // required — but that is its call to make, and inventing ten `null`s to
    // avoid it would be ours.
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

/// The deliberate exception: `AccountConfiguration` has no constructor.
///
/// It is a read-modify-write body — every field but three is required — so the
/// only correct way to get one is from the API. This test exists so the absence
/// reads as a decision rather than an oversight the next sweep should "fix".
#[test]
fn account_configuration_is_obtained_from_the_api_not_built() {
    // The shape a `GET` returns, which is the only source a caller has.
    let fetched: alpaca_sdk::trading::AccountConfiguration = serde_json::from_value(json!({
        "no_shorting": false,
        "suspend_trade": false,
        "fractional_trading": true,
        "max_margin_multiplier": "4",
        "trade_confirm_email": "all",
        "ptp_no_exception_entry": false
    }))
    .unwrap();

    let mut edited = fetched.clone();
    edited.suspend_trade = true;

    // Round-tripping it changes only what was named — no `null`s invented for
    // the three fields the current response shape omits.
    let sent = serde_json::to_value(&edited).unwrap();
    assert_eq!(sent["suspend_trade"], true);
    assert_eq!(sent["fractional_trading"], true);
    assert!(sent.get("dtbp_check").is_none(), "{sent}");
    assert!(sent.get("pdt_check").is_none(), "{sent}");
    assert!(sent.get("max_options_trading_level").is_none(), "{sent}");
}

/// The filter and request types no other integration test builds.
///
/// The tests above cover the types a caller reaches by following a worked
/// example. These are the rest of the input surface: every `#[non_exhaustive]`
/// type a public method takes that no other external test happens to construct.
/// Each is one line, and one line is enough — the assertion is that the code
/// below compiles from outside this crate, which is the only place
/// `#[non_exhaustive]` can be observed.
///
/// A type that grows a required field, or loses its `Default`, fails here rather
/// than in a caller's crate after release.
#[test]
fn every_input_type_is_reachable_from_outside_the_crate() {
    use alpaca_sdk::broker::{
        GetCashInterestRequest, GetEodPositionsRequest, GetFpslAnalyticsRequest,
        GetFpslLoansRequest, GetFundingDetailsRequest, GetInstantFundingReportRequest,
        GetInstantFundingRequest, GetJitBalancesRequest, GetOAuthClientRequest,
        GetOnfidoTokenRequest, GetOptionsApprovalsRequest, GetSettlementsRequest, KycResults,
    };
    use alpaca_sdk::trading::{
        GetCalendarRequest, GetMarketCalendarRequest, GetPortfolioHistoryRequest,
    };

    let _ = GetCashInterestRequest::default();
    let _ = GetEodPositionsRequest::default();
    let _ = GetFpslAnalyticsRequest::default();
    let _ = GetFpslLoansRequest::default();
    let _ = GetFundingDetailsRequest::default();
    let _ = GetInstantFundingReportRequest::default();
    let _ = GetInstantFundingRequest::default();
    let _ = GetJitBalancesRequest::default();
    let _ = GetOAuthClientRequest::default();
    let _ = GetOnfidoTokenRequest::default();
    let _ = GetOptionsApprovalsRequest::default();
    let _ = GetSettlementsRequest::default();
    let _ = KycResults::default();
    let _ = GetCalendarRequest::default();
    let _ = GetMarketCalendarRequest::default();
    let _ = GetPortfolioHistoryRequest::default();
}

/// A chain of setters and a run of field assignments produce the same request.
///
/// The setters are additive: the assignment form is what a caller writes today,
/// it keeps working, and the two must stay interchangeable or the new idiom is
/// a second, subtly different way to build the same request. Equivalence is the
/// behaviour worth pinning, so this asserts on the whole serialized object
/// rather than field by field — a setter writing into the wrong field passes
/// every per-field assertion the test author remembered to write, and fails
/// this one.
///
/// `GetOrdersRequest` because it is the widest: fourteen optional fields across
/// enums, integers, timestamps, `Uuid`, `Decimal`, `Vec` and `String`, which is
/// every shape the macro generates.
#[test]
fn a_setter_chain_and_field_assignment_build_the_same_request() {
    use alpaca_sdk::trading::{AssetClass, GetOrdersRequest, OrderSide, QueryOrderStatus};
    use alpaca_sdk::types::Sort;

    let after: chrono::DateTime<chrono::Utc> = "2024-01-01T00:00:00Z".parse().unwrap();
    let until: chrono::DateTime<chrono::Utc> = "2024-02-01T00:00:00Z".parse().unwrap();
    let before_order = Uuid::from_u128(1);
    let after_order = Uuid::from_u128(2);

    let chained = GetOrdersRequest::default()
        .status(QueryOrderStatus::Open)
        .limit(50)
        .after(after)
        .until(until)
        .direction(Sort::Desc)
        .nested(true)
        .side(OrderSide::Buy)
        .symbols(vec!["AAPL".to_owned(), "SPY".to_owned()])
        .asset_class(vec![AssetClass::UsEquity])
        .before_order_id(before_order)
        .after_order_id(after_order)
        .qty_above(Decimal::ONE)
        .qty_below(Decimal::TEN)
        // A `&str` where the field is `Option<String>`: the `into` half of the
        // macro, and the reason it exists.
        .subtag("desk-7");

    let mut assigned = GetOrdersRequest::default();
    assigned.status = Some(QueryOrderStatus::Open);
    assigned.limit = Some(50);
    assigned.after = Some(after);
    assigned.until = Some(until);
    assigned.direction = Some(Sort::Desc);
    assigned.nested = Some(true);
    assigned.side = Some(OrderSide::Buy);
    assigned.symbols = Some(vec!["AAPL".to_owned(), "SPY".to_owned()]);
    assigned.asset_class = Some(vec![AssetClass::UsEquity]);
    assigned.before_order_id = Some(before_order);
    assigned.after_order_id = Some(after_order);
    assigned.qty_above = Some(Decimal::ONE);
    assigned.qty_below = Some(Decimal::TEN);
    assigned.subtag = Some("desk-7".to_owned());

    assert_eq!(
        serde_json::to_value(&chained).unwrap(),
        serde_json::to_value(&assigned).unwrap(),
    );

    // Fourteen fields set, fourteen fields serialized: a setter that wrote into
    // a field another setter also writes would leave this short, and the
    // equality above would still hold.
    let sent = serde_json::to_value(&chained).unwrap();
    assert_eq!(sent.as_object().unwrap().len(), 14, "{sent}");

    // And the setters stay optional: an untouched request still sends nothing.
    let empty = serde_json::to_value(GetOrdersRequest::default()).unwrap();
    assert!(empty.as_object().unwrap().is_empty(), "{empty}");
}
