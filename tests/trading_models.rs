//! Trading models against captured API responses.
//!
//! These payloads came off the real API, so they carry the quirks a synthetic
//! fixture would not: string-typed integers, fields absent from the documented
//! models, `qty` as a bare number in one response and a string in the next.
//! Extracted by `scripts/extract_fixtures.py`; see `fixtures/index.json` for
//! which test suite each one was captured from.

#![cfg(feature = "trading")]

use std::path::PathBuf;

use alpaca_sdk::trading::{
    Activity, Asset, AssetClass, AssetExchange, AssetStatus, Calendar, ClosePositionBody,
    ClosePositionResponse, Order, OrderClass, OrderStatus, OrderType, PositionIntent, TimeInForce,
    TradeAccount, Watchlist,
};
use rust_decimal::Decimal;

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

fn parse<T: serde::de::DeserializeOwned>(name: &str) -> T {
    let body = fixture(name);
    serde_json::from_str(&body).unwrap_or_else(|e| panic!("{name}: {e}\n{body}"))
}

// ---------------------------------------------------------------- account

#[test]
fn account_deserializes_from_the_captured_response() {
    let account: TradeAccount = parse("trading/test_account_routes__test_get_account__01.json");

    assert_eq!(account.account_number, "010203ABCD");
    assert_eq!(account.currency.as_deref(), Some("USD"));
    assert_eq!(account.cash, Some(Decimal::new(-231_402, 1)));
    assert_eq!(account.equity, Some(Decimal::new(10_382_056, 2)));
    assert_eq!(account.shorting_enabled, Some(true));
}

#[test]
fn account_accepts_integers_sent_as_strings() {
    // The captured response has "options_approved_level": "1" alongside
    // "daytrade_count": 0. A plain i64 field would reject the whole payload.
    let account: TradeAccount = parse("trading/test_account_routes__test_get_account__01.json");

    assert_eq!(account.options_approved_level, Some(1));
    assert_eq!(account.options_trading_level, Some(1));
    assert_eq!(account.daytrade_count, Some(0));
}

#[test]
fn account_money_keeps_full_precision() {
    let account: TradeAccount = parse("trading/test_account_routes__test_get_account__01.json");

    // 262113.632 through an f64 and back is not guaranteed to render the same.
    assert_eq!(
        account.buying_power.unwrap().to_string(),
        "262113.632",
        "buying power lost precision"
    );
}

#[test]
fn account_configuration_deserializes() {
    let config: alpaca_sdk::trading::AccountConfiguration =
        parse("trading/test_account_routes__test_get_account_configurations__01.json");

    assert!(!config.no_shorting);
    assert_eq!(config.max_margin_multiplier, "4");
}

#[test]
fn account_configuration_tolerates_removed_deprecated_fields() {
    // dtbp_check and pdt_check were dropped from Alpaca responses on 2026-07-06.
    let config: alpaca_sdk::trading::AccountConfiguration = parse(
        "trading/test_account_routes__test_get_account_configurations_without_deprecated_pdt_fields__01.json",
    );

    assert_eq!(config.dtbp_check, None);
    assert_eq!(config.pdt_check, None);
}

// ------------------------------------------------------------------ asset

#[test]
fn asset_maps_the_class_field() {
    let asset: Asset = parse("trading/test_asset_routes__test_get_asset__01.json");

    // Sent on the wire as "class", hence the serde rename.
    assert_eq!(asset.asset_class, AssetClass::UsEquity);
    assert_eq!(asset.exchange, AssetExchange::Nasdaq);
    assert_eq!(asset.symbol, "AAPL");
    assert_eq!(asset.status, AssetStatus::Active);
}

#[test]
fn asset_ignores_fields_absent_from_the_model() {
    // The captured response carries last_price and last_close_pct_change, which
    // no specification declares. Rejecting unknown fields would break here.
    let asset: Asset = parse("trading/test_asset_routes__test_get_asset__01.json");

    assert_eq!(
        asset.attributes.as_deref(),
        Some(["attribute1".to_owned(), "attribute2".to_owned()].as_slice())
    );
}

#[test]
fn asset_list_deserializes() {
    let assets: Vec<Asset> = parse("trading/test_asset_routes__test_get_all_assets__01.json");

    assert_eq!(assets.len(), 1);
    assert!(assets[0].tradable);
}

// ----------------------------------------------------------------- orders

#[test]
fn market_order_deserializes_with_qty_as_a_bare_number() {
    // This response sends "qty": 1 as a JSON number and "filled_qty": "0" as a
    // string, in the same object.
    let order: Order = parse("trading/test_order_routes__test_market_order__01.json");

    assert_eq!(order.status, OrderStatus::Accepted);
    assert_eq!(order.qty, Some(Decimal::from(1)));
    assert_eq!(order.filled_qty, Some(Decimal::from(0)));
    assert_eq!(order.order_type, Some(OrderType::Market));
    assert_eq!(order.time_in_force, TimeInForce::Day);
    assert_eq!(order.order_class, OrderClass::Simple);
}

#[test]
fn order_ignores_the_undocumented_commission_field() {
    // The captured payload carries "commission": 1.25, which the trading
    // specification does not declare.
    let order: Order = parse("trading/test_order_routes__test_market_order__01.json");

    assert_eq!(
        order.client_order_id,
        "eb9e2aaa-f71a-4f51-b5b4-52a6c565dad4"
    );
}

#[test]
fn limit_order_carries_its_limit_price() {
    let order: Order = parse("trading/test_order_routes__test_limit_order__01.json");

    assert_eq!(order.order_type, Some(OrderType::Limit));
    assert!(order.limit_price.is_some());
}

#[test]
fn order_list_deserializes() {
    // This fixture is an OpenAPI documentation sample rather than a real
    // capture: several values are the literal placeholder "string", including
    // `hwm`, which is a price. A client that keeps prices as strings never
    // notices.
    //
    // Rejecting a non-numeric price is the behavior we want, so the placeholder
    // is patched out here rather than the type being weakened to match. See
    // `a_non_numeric_price_is_rejected` for the other half of this.
    let body = fixture("trading/test_order_routes__test_get_orders__01.json")
        .replace(r#""hwm": "string""#, r#""hwm": null"#);
    let orders: Vec<Order> = serde_json::from_str(&body).unwrap();

    assert!(!orders.is_empty());
    assert_eq!(orders[0].symbol.as_deref(), Some("SPY"));
    assert_eq!(orders[0].qty, Some(Decimal::from(1)));
    // order_type and type disagree in this payload; both are preserved.
    assert_eq!(orders[0].order_type_deprecated, Some(OrderType::Market));
    assert_eq!(orders[0].order_type, Some(OrderType::Stop));
}

#[test]
fn a_non_numeric_price_is_rejected() {
    // Typing a price as a string and handing the caller whatever arrived would
    // accept this; parsing it is what catches it.
    let json = r#"{
        "id": "61e69015-8549-4bfd-b9c3-01e75843f47d",
        "client_order_id": "x",
        "created_at": "2021-03-16T18:38:01.942282Z",
        "updated_at": "2021-03-16T18:38:01.942282Z",
        "submitted_at": "2021-03-16T18:38:01.937734Z",
        "order_class": "simple",
        "time_in_force": "day",
        "status": "new",
        "extended_hours": false,
        "hwm": "not-a-number"
    }"#;

    assert!(serde_json::from_str::<Order>(json).is_err());
}

#[test]
fn order_position_intent_is_parsed() {
    let order: Order = parse("trading/test_order_routes__test_order_position_intent__01.json");

    assert!(matches!(
        order.position_intent,
        Some(PositionIntent::BuyToOpen | PositionIntent::SellToOpen)
    ));
}

#[test]
fn replaced_order_deserializes() {
    let order: Order = parse("trading/test_order_routes__test_replace_order__01.json");
    assert!(!order.client_order_id.is_empty());
}

#[test]
fn order_by_client_id_deserializes() {
    let order: Order = parse("trading/test_order_routes__test_get_order_by_client_id__01.json");
    assert!(!order.client_order_id.is_empty());
}

// -------------------------------------------------------------- positions

#[test]
fn position_deserializes_with_usd_values() {
    let positions: Vec<alpaca_sdk::trading::Position> =
        parse("trading/test_position_routes__test_get_all_positions__01.json");

    let position = &positions[0];
    assert_eq!(position.symbol, "AAPL");
    assert_eq!(position.qty, Decimal::from(5));
    assert_eq!(position.avg_entry_price, Decimal::new(1000, 1));

    let usd = position.usd.as_ref().expect("usd values present");
    assert_eq!(usd.market_value, Decimal::new(6000, 1));
}

#[test]
fn close_all_positions_response_distinguishes_success_from_failure() {
    let responses: Vec<ClosePositionResponse> =
        parse("trading/test_position_routes__test_close_all_positions__01.json");

    assert!(!responses.is_empty());

    // The body is an order when the close succeeded and a failure detail when
    // it did not, so both shapes have to decode.
    for response in &responses {
        match &response.body {
            ClosePositionBody::Order(order) => assert!(!order.client_order_id.is_empty()),
            ClosePositionBody::Failed(failure) => assert!(!failure.message.is_empty()),
        }
    }
}

#[test]
fn close_position_with_qty_returns_an_order() {
    let order: Order = parse("trading/test_position_routes__test_close_position_with_qty__01.json");
    assert_eq!(order.status, OrderStatus::Accepted);
}

// ------------------------------------------------------- options and corp

#[test]
fn option_contract_deserializes() {
    let contract: alpaca_sdk::trading::OptionContract =
        parse("trading/test_option_routes__test_get_option_contract__01.json");

    assert!(!contract.symbol.is_empty());
    assert!(contract.strike_price > Decimal::ZERO);
}

#[test]
fn option_contracts_page_deserializes() {
    let page: alpaca_sdk::trading::OptionContractsResponse =
        parse("trading/test_option_routes__test_get_option_contracts__01.json");

    assert!(page.option_contracts.is_some_and(|c| !c.is_empty()));
}

#[test]
fn corporate_action_announcements_deserialize() {
    let announcements: Vec<alpaca_sdk::trading::CorporateActionAnnouncement> =
        parse("trading/test_corporate_announcements__test_get_announcements__01.json");

    assert!(!announcements.is_empty());
    assert!(!announcements[0].initiating_symbol.is_empty());
}

// ------------------------------------------------------ constructed cases

#[test]
fn multi_leg_order_empty_strings_become_none() {
    // An mleg parent order: the fields describing a single leg come back as ""
    // because the parent has several. All seven read as absent.
    let json = r#"{
        "id": "61e69015-8549-4bfd-b9c3-01e75843f47d",
        "client_order_id": "mleg-parent",
        "created_at": "2021-03-16T18:38:01.942282Z",
        "updated_at": "2021-03-16T18:38:01.942282Z",
        "submitted_at": "2021-03-16T18:38:01.937734Z",
        "asset_id": "",
        "symbol": "",
        "asset_class": "",
        "side": "",
        "position_intent": "",
        "type": "",
        "order_type": "",
        "order_class": "mleg",
        "time_in_force": "day",
        "status": "accepted",
        "extended_hours": false,
        "legs": []
    }"#;

    let order: Order = serde_json::from_str(json).unwrap();

    assert_eq!(order.asset_id, None);
    assert_eq!(order.symbol, None);
    assert_eq!(order.asset_class, None);
    assert_eq!(order.side, None);
    assert_eq!(order.position_intent, None);
    assert_eq!(order.order_type, None);
    assert_eq!(order.order_type_deprecated, None);
    assert_eq!(order.order_class, OrderClass::Mleg);
}

#[test]
fn missing_or_empty_order_class_defaults_to_simple() {
    let base = r#"{
        "id": "61e69015-8549-4bfd-b9c3-01e75843f47d",
        "client_order_id": "x",
        "created_at": "2021-03-16T18:38:01.942282Z",
        "updated_at": "2021-03-16T18:38:01.942282Z",
        "submitted_at": "2021-03-16T18:38:01.937734Z",
        "time_in_force": "day",
        "status": "accepted",
        "extended_hours": false"#;

    let absent: Order = serde_json::from_str(&format!("{base}}}")).unwrap();
    let empty: Order = serde_json::from_str(&format!(r#"{base}, "order_class": ""}}"#)).unwrap();

    assert_eq!(absent.order_class, OrderClass::Simple);
    assert_eq!(empty.order_class, OrderClass::Simple);
}

#[test]
fn nested_legs_deserialize_recursively() {
    let json = r#"{
        "id": "61e69015-8549-4bfd-b9c3-01e75843f47d",
        "client_order_id": "bracket-parent",
        "created_at": "2021-03-16T18:38:01.942282Z",
        "updated_at": "2021-03-16T18:38:01.942282Z",
        "submitted_at": "2021-03-16T18:38:01.937734Z",
        "order_class": "bracket",
        "time_in_force": "day",
        "status": "accepted",
        "extended_hours": false,
        "legs": [{
            "id": "71e69015-8549-4bfd-b9c3-01e75843f47d",
            "client_order_id": "bracket-leg",
            "created_at": "2021-03-16T18:38:01.942282Z",
            "updated_at": "2021-03-16T18:38:01.942282Z",
            "submitted_at": "2021-03-16T18:38:01.937734Z",
            "order_class": "simple",
            "time_in_force": "day",
            "status": "held",
            "extended_hours": false,
            "qty": "1"
        }]
    }"#;

    let order: Order = serde_json::from_str(json).unwrap();
    let legs = order.legs.expect("legs present");

    assert_eq!(legs.len(), 1);
    assert_eq!(legs[0].client_order_id, "bracket-leg");
    assert_eq!(legs[0].qty, Some(Decimal::from(1)));
}

#[test]
fn an_unknown_order_status_does_not_break_the_response() {
    // A strict decoder fails the whole call here.
    let json = r#"{
        "id": "61e69015-8549-4bfd-b9c3-01e75843f47d",
        "client_order_id": "x",
        "created_at": "2021-03-16T18:38:01.942282Z",
        "updated_at": "2021-03-16T18:38:01.942282Z",
        "submitted_at": "2021-03-16T18:38:01.937734Z",
        "order_class": "simple",
        "time_in_force": "day",
        "status": "some_new_status_alpaca_added",
        "extended_hours": false
    }"#;

    let order: Order = serde_json::from_str(json).unwrap();

    assert_eq!(
        order.status,
        OrderStatus::Unknown("some_new_status_alpaca_added".to_owned())
    );
}

#[test]
fn calendar_combines_the_date_with_the_time_strings() {
    // The API sends open and close as bare HH:MM, joined onto the date here so
    // the caller does not re-parse them.
    let calendars: Vec<Calendar> =
        serde_json::from_str(r#"[{"date": "2022-04-13", "open": "09:30", "close": "16:00"}]"#)
            .unwrap();

    let day = &calendars[0];
    assert_eq!(day.date.to_string(), "2022-04-13");
    assert_eq!(day.open.to_string(), "2022-04-13 09:30:00");
    assert_eq!(day.close.to_string(), "2022-04-13 16:00:00");
    // Absent from this payload, and from the specification.
    assert_eq!(day.session_open, None);
    assert_eq!(day.settlement_date, None);
}

#[test]
fn calendar_reads_the_session_fields_the_real_api_sends() {
    // Exactly what /v2/calendar returns, which the specification omits.
    // The session times are HHMM with no separator, unlike open and close.
    let calendars: Vec<Calendar> = serde_json::from_str(
        r#"[{
            "date": "2026-08-12",
            "open": "09:30",
            "close": "16:00",
            "session_open": "0400",
            "session_close": "2000",
            "settlement_date": "2026-08-13"
        }]"#,
    )
    .unwrap();

    let day = &calendars[0];
    assert_eq!(day.open.to_string(), "2026-08-12 09:30:00");
    assert_eq!(
        day.session_open.unwrap().to_string(),
        "2026-08-12 04:00:00",
        "extended-hours open"
    );
    assert_eq!(
        day.session_close.unwrap().to_string(),
        "2026-08-12 20:00:00",
        "extended-hours close"
    );
    assert_eq!(day.settlement_date.unwrap().to_string(), "2026-08-13");
}

#[test]
fn calendar_rejects_a_session_time_in_the_wrong_format() {
    // If the separator ever appears here, that is a wire change worth failing on
    // rather than silently dropping the field.
    let result = serde_json::from_str::<Vec<Calendar>>(
        r#"[{"date":"2026-08-12","open":"09:30","close":"16:00","session_open":"04:00"}]"#,
    );

    assert!(
        result.is_err(),
        "expected the HH:MM session time to be rejected"
    );
}

#[test]
fn clock_deserializes() {
    let clock: alpaca_sdk::trading::Clock = serde_json::from_str(
        r#"{
            "timestamp": "2022-04-28T14:07:04.451420928-04:00",
            "is_open": true,
            "next_open": "2022-04-29T09:30:00-04:00",
            "next_close": "2022-04-28T16:00:00-04:00"
        }"#,
    )
    .unwrap();

    assert!(clock.is_open);
}

#[test]
fn watchlist_deserializes_with_and_without_assets() {
    let without: Watchlist = serde_json::from_str(
        r#"{
            "id": "fb306d55-2d64-4b8b-8c2a-3d0d9e0b7d47",
            "account_id": "3f2504e0-4f89-11d3-9a0c-0305e82c3301",
            "name": "Primary",
            "created_at": "2022-04-28T14:07:04.451420Z",
            "updated_at": "2022-04-28T14:07:04.451420Z"
        }"#,
    )
    .unwrap();

    assert_eq!(without.name, "Primary");
    assert!(without.assets.is_none());
}

#[test]
fn activities_dispatch_on_activity_type() {
    // The endpoint returns a heterogeneous array; a fill is a TradeActivity and
    // a dividend is a NonTradeActivity.
    let activities: Vec<Activity> = serde_json::from_str(
        r#"[
            {
                "id": "20220419000000000::fd5c8a1e",
                "account_id": "3f2504e0-4f89-11d3-9a0c-0305e82c3301",
                "activity_type": "FILL",
                "transaction_time": "2022-04-19T14:30:00Z",
                "type": "fill",
                "price": 170.5,
                "qty": 10,
                "side": "buy",
                "symbol": "AAPL",
                "leaves_qty": 0,
                "order_id": "61e69015-8549-4bfd-b9c3-01e75843f47d",
                "cum_qty": 10,
                "order_status": "filled"
            },
            {
                "id": "20220419000000000::aa5c8a1e",
                "account_id": "3f2504e0-4f89-11d3-9a0c-0305e82c3301",
                "activity_type": "DIV",
                "date": "2022-04-19",
                "net_amount": 12.5,
                "description": "dividend",
                "symbol": "AAPL",
                "qty": 10,
                "per_share_amount": 1.25
            }
        ]"#,
    )
    .unwrap();

    assert_eq!(activities.len(), 2);
    assert!(matches!(activities[0], Activity::Trade(_)));
    assert!(matches!(activities[1], Activity::NonTrade(_)));
}
