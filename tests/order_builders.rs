//! Every order shape, asserted on the JSON body it posts.
//!
//! This is the highest-consequence code in the crate. A filter that serializes
//! under the wrong name returns the wrong rows; a bracket order whose stop-loss
//! leg serializes under the wrong name is a **real position with no exit**, and
//! it compiles, type-checks and looks right at the call site. Nothing but the
//! body catches it.
//!
//! So the assertions are whole-body `assert_eq!` rather than field probes: an
//! extra key is as wrong as a missing one — Alpaca rejects an unexpected field
//! on an order — and a probe cannot see one.

#![cfg(feature = "trading")]

use alpaca_sdk::trading::{
    GetCorporateAnnouncementsRequest, OptionLegRequest, OrderAmount, OrderClass, OrderRequest,
    OrderSide, OrderType, PositionIntent, StopLimit, StopLossRequest, TakeProfitRequest,
    TimeInForce, Trail,
};
use rust_decimal::Decimal;
use serde_json::{Value, json};

fn body(order: &OrderRequest) -> Value {
    order
        .validate()
        .expect("the order under test must be valid");
    serde_json::to_value(order).expect("an order must serialize")
}

fn qty(n: i64) -> OrderAmount {
    OrderAmount::Qty(Decimal::from(n))
}

fn price(n: i64) -> Decimal {
    Decimal::from(n)
}

// --------------------------------------------------------------- the shapes

#[test]
fn a_market_order_sends_only_what_a_market_order_needs() {
    let order = OrderRequest::market("AAPL", OrderSide::Buy, qty(1), TimeInForce::Day);

    assert_eq!(
        body(&order),
        json!({
            "symbol": "AAPL",
            "qty": "1",
            "side": "buy",
            "type": "market",
            "time_in_force": "day"
        })
    );
}

/// A notional order is a dollar amount, and the two are mutually exclusive —
/// which is why `OrderAmount` is an enum. Sending both is what Alpaca rejects.
#[test]
fn a_notional_order_sends_notional_and_no_qty() {
    let order = OrderRequest::market(
        "AAPL",
        OrderSide::Buy,
        OrderAmount::Notional(Decimal::new(15_050, 2)),
        TimeInForce::Day,
    );

    let body = body(&order);
    assert_eq!(body["notional"], "150.50");
    assert!(body.get("qty").is_none(), "{body}");
}

#[test]
fn a_limit_order_carries_its_limit_price_and_nothing_else() {
    let order = OrderRequest::limit("AAPL", OrderSide::Buy, qty(2), TimeInForce::Gtc, price(150));

    assert_eq!(
        body(&order),
        json!({
            "symbol": "AAPL",
            "qty": "2",
            "side": "buy",
            "type": "limit",
            "time_in_force": "gtc",
            "limit_price": "150"
        })
    );
}

/// `stop_price`, not `limit_price`. The two are one keystroke apart and a stop
/// order sent with a limit price is a different order at a different price.
#[test]
fn a_stop_order_carries_a_stop_price_and_no_limit_price() {
    let order = OrderRequest::stop(
        "AAPL",
        OrderSide::Sell,
        qty(3),
        TimeInForce::Day,
        price(140),
    );

    assert_eq!(
        body(&order),
        json!({
            "symbol": "AAPL",
            "qty": "3",
            "side": "sell",
            "type": "stop",
            "time_in_force": "day",
            "stop_price": "140"
        })
    );
}

#[test]
fn a_stop_limit_order_carries_both_prices_under_their_own_names() {
    // Named fields rather than two adjacent `Decimal` arguments: transposing
    // positional ones compiled and produced a legal-but-different order.
    let order = OrderRequest::stop_limit(
        "AAPL",
        OrderSide::Sell,
        qty(1),
        TimeInForce::Day,
        StopLimit {
            stop: price(140),
            limit: price(139),
        },
    );

    let body = body(&order);
    assert_eq!(body["type"], "stop_limit");
    assert_eq!(body["stop_price"], "140");
    assert_eq!(body["limit_price"], "139");
}

/// The trail is a price *or* a percent, never both — `Trail` is an enum for the
/// same reason `OrderAmount` is. A trailing stop carrying both is rejected, and
/// one carrying the wrong one of the two trails by the wrong amount.
#[test]
fn a_trailing_stop_sends_exactly_one_of_the_two_trail_fields() {
    let by_price = body(&OrderRequest::trailing_stop(
        "AAPL",
        OrderSide::Sell,
        qty(1),
        TimeInForce::Day,
        Trail::Price(Decimal::new(150, 1)),
    ));
    assert_eq!(by_price["type"], "trailing_stop");
    assert_eq!(by_price["trail_price"], "15.0");
    assert!(by_price.get("trail_percent").is_none(), "{by_price}");

    let by_percent = body(&OrderRequest::trailing_stop(
        "AAPL",
        OrderSide::Sell,
        qty(1),
        TimeInForce::Day,
        Trail::Percent(Decimal::new(25, 1)),
    ));
    assert_eq!(by_percent["trail_percent"], "2.5");
    assert!(by_percent.get("trail_price").is_none(), "{by_percent}");
}

// --------------------------------------------------------------- the exits

/// The one that costs money if it is wrong. A bracket order is an entry plus
/// two exits, and the exits are nested objects — `take_profit.limit_price` and
/// `stop_loss.stop_price`. Flatten either by accident and Alpaca accepts the
/// entry and silently drops the exit.
#[test]
fn a_bracket_order_nests_both_exits_under_their_own_keys() {
    let order = OrderRequest::limit("AAPL", OrderSide::Buy, qty(1), TimeInForce::Gtc, price(150))
        .bracket(
            TakeProfitRequest::new(price(160)),
            StopLossRequest::new(price(140)),
        );

    assert_eq!(
        body(&order),
        json!({
            "symbol": "AAPL",
            "qty": "1",
            "side": "buy",
            "type": "limit",
            "time_in_force": "gtc",
            "limit_price": "150",
            "order_class": "bracket",
            "take_profit": {"limit_price": "160"},
            "stop_loss": {"stop_price": "140"}
        })
    );
}

/// A stop-loss leg may carry a limit price of its own, making the exit a
/// stop-limit rather than a stop. Dropping it turns a bounded exit into a
/// market order in a falling market.
#[test]
fn a_stop_loss_leg_can_carry_its_own_limit_price() {
    let order = OrderRequest::limit("AAPL", OrderSide::Buy, qty(1), TimeInForce::Gtc, price(150))
        .bracket(
            TakeProfitRequest::new(price(160)),
            StopLossRequest::new(price(140)).limit_price(price(139)),
        );

    assert_eq!(
        body(&order)["stop_loss"],
        json!({"stop_price": "140", "limit_price": "139"})
    );
}

#[test]
fn an_oco_order_carries_both_exits_and_says_oco() {
    let order = OrderRequest::limit(
        "AAPL",
        OrderSide::Sell,
        qty(1),
        TimeInForce::Gtc,
        price(160),
    )
    .oco(
        TakeProfitRequest::new(price(160)),
        StopLossRequest::new(price(140)),
    );

    let body = body(&order);
    assert_eq!(body["order_class"], "oco");
    assert_eq!(body["take_profit"], json!({"limit_price": "160"}));
    assert_eq!(body["stop_loss"], json!({"stop_price": "140"}));
}

/// OTO takes one exit, and which one is the caller's choice — so there are two
/// constructors, and each must set only its own leg.
#[test]
fn each_oto_constructor_sets_only_its_own_exit() {
    let entry =
        || OrderRequest::limit("AAPL", OrderSide::Buy, qty(1), TimeInForce::Gtc, price(150));

    let take_profit = body(&entry().oto_take_profit(TakeProfitRequest::new(price(160))));
    assert_eq!(take_profit["order_class"], "oto");
    assert_eq!(take_profit["take_profit"], json!({"limit_price": "160"}));
    assert!(take_profit.get("stop_loss").is_none(), "{take_profit}");

    let stop_loss = body(&entry().oto_stop_loss(StopLossRequest::new(price(140))));
    assert_eq!(stop_loss["order_class"], "oto");
    assert_eq!(stop_loss["stop_loss"], json!({"stop_price": "140"}));
    assert!(stop_loss.get("take_profit").is_none(), "{stop_loss}");
}

// ------------------------------------------------------------- multi-leg

/// A multi-leg order has no symbol and no side of its own — the legs carry
/// both — and `ratio_qty` is what makes a spread a spread. Getting the ratios
/// wrong builds a different position from the one the caller described.
#[test]
fn a_multi_leg_order_puts_the_symbol_and_side_on_the_legs() {
    let order = OrderRequest::multi_leg(
        Decimal::ONE,
        TimeInForce::Day,
        vec![
            OptionLegRequest::new("AAPL240119C00150000", Decimal::ONE, OrderSide::Buy),
            OptionLegRequest::new("AAPL240119C00160000", Decimal::from(2), OrderSide::Sell),
        ],
        None,
    );

    assert_eq!(
        body(&order),
        json!({
            "qty": "1",
            "type": "market",
            "time_in_force": "day",
            "order_class": "mleg",
            "legs": [
                {"symbol": "AAPL240119C00150000", "ratio_qty": "1", "side": "buy"},
                {"symbol": "AAPL240119C00160000", "ratio_qty": "2", "side": "sell"}
            ]
        })
    );
}

/// The limit price is what decides the order type here — there is no separate
/// constructor — so passing one has to flip `type` as well as add the field.
#[test]
fn a_priced_multi_leg_order_becomes_a_limit_order() {
    let legs = vec![
        OptionLegRequest::new("AAPL240119C00150000", Decimal::ONE, OrderSide::Buy),
        OptionLegRequest::new("AAPL240119C00160000", Decimal::ONE, OrderSide::Sell),
    ];

    let market = body(&OrderRequest::multi_leg(
        Decimal::ONE,
        TimeInForce::Day,
        legs.clone(),
        None,
    ));
    assert_eq!(market["type"], "market");
    assert!(market.get("limit_price").is_none(), "{market}");

    let limit = body(&OrderRequest::multi_leg(
        Decimal::ONE,
        TimeInForce::Day,
        legs,
        Some(Decimal::new(250, 2)),
    ));
    assert_eq!(limit["type"], "limit");
    assert_eq!(limit["limit_price"], "2.50");
}

/// A leg can name a position intent instead of a side, which is how a caller
/// says "close this" rather than "sell this".
#[test]
fn a_leg_can_carry_a_position_intent_instead_of_a_side() {
    let by_intent = OptionLegRequest::with_position_intent(
        "AAPL240119C00150000",
        Decimal::ONE,
        PositionIntent::SellToClose,
    );
    let value = serde_json::to_value(&by_intent).unwrap();
    assert_eq!(value["position_intent"], "sell_to_close");
    assert!(value.get("side").is_none(), "{value}");

    // Or both, when the caller wants to be explicit about each.
    let both = OptionLegRequest::new("AAPL240119C00150000", Decimal::ONE, OrderSide::Sell)
        .position_intent(PositionIntent::SellToClose);
    let value = serde_json::to_value(&both).unwrap();
    assert_eq!(value["side"], "sell");
    assert_eq!(value["position_intent"], "sell_to_close");
}

// ------------------------------------------------------------- modifiers

#[test]
fn the_modifiers_each_write_their_own_field() {
    let order = OrderRequest::market("AAPL", OrderSide::Buy, qty(1), TimeInForce::Day)
        .extended_hours(true)
        .client_order_id("my-order-1")
        .position_intent(PositionIntent::BuyToOpen);

    let body = body(&order);
    assert_eq!(body["extended_hours"], true);
    assert_eq!(body["client_order_id"], "my-order-1");
    assert_eq!(body["position_intent"], "buy_to_open");
}

/// `extended_hours(false)` is not the same as not setting it: Alpaca reads an
/// absent field as its own default, and a caller who says `false` means it.
#[test]
fn an_explicit_false_is_sent_rather_than_skipped() {
    let body = body(
        &OrderRequest::market("AAPL", OrderSide::Buy, qty(1), TimeInForce::Day)
            .extended_hours(false),
    );

    assert_eq!(body["extended_hours"], false);
}

// -------------------------------------------------------------- validation

/// Every rule `validate` enforces, each with the shape that trips it. These
/// are the combinations Alpaca rejects, caught before a request is sent.
#[test]
fn a_bracket_or_oco_order_without_both_exits_is_refused() {
    let entry =
        || OrderRequest::limit("AAPL", OrderSide::Buy, qty(1), TimeInForce::Gtc, price(150));

    // A bracket built by hand, missing the stop-loss.
    let mut half = entry().oto_take_profit(TakeProfitRequest::new(price(160)));
    half.order_class = Some(OrderClass::Bracket);
    let error = half.validate().unwrap_err();
    assert!(error.to_string().contains("stop_loss"), "{error}");

    let mut half = entry().oto_stop_loss(StopLossRequest::new(price(140)));
    half.order_class = Some(OrderClass::Bracket);
    assert!(
        half.validate()
            .unwrap_err()
            .to_string()
            .contains("take_profit")
    );

    let mut half = entry().oto_stop_loss(StopLossRequest::new(price(140)));
    half.order_class = Some(OrderClass::Oco);
    assert!(half.validate().is_err());

    // Both present is fine.
    assert!(
        entry()
            .bracket(
                TakeProfitRequest::new(price(160)),
                StopLossRequest::new(price(140))
            )
            .validate()
            .is_ok()
    );
}

#[test]
fn an_oto_order_with_neither_exit_is_refused() {
    let mut order =
        OrderRequest::limit("AAPL", OrderSide::Buy, qty(1), TimeInForce::Gtc, price(150));
    order.order_class = Some(OrderClass::Oto);

    let error = order.validate().unwrap_err();
    assert!(error.to_string().contains("take_profit"), "{error}");
    assert!(error.to_string().contains("stop_loss"), "{error}");
}

/// The multi-leg rules, all four of them. Two to four legs, unique symbols, a
/// quantity, and market or limit only.
#[test]
fn the_multi_leg_rules_are_each_enforced() {
    let leg = |symbol: &str| OptionLegRequest::new(symbol, Decimal::ONE, OrderSide::Buy);
    let build = |legs: Vec<OptionLegRequest>| {
        OrderRequest::multi_leg(Decimal::ONE, TimeInForce::Day, legs, None)
    };

    // Too few.
    assert!(
        build(vec![])
            .validate()
            .unwrap_err()
            .to_string()
            .contains("legs is required")
    );
    assert!(
        build(vec![leg("A")])
            .validate()
            .unwrap_err()
            .to_string()
            .contains("at least 2")
    );

    // Too many.
    let five = ["A", "B", "C", "D", "E"].map(leg).to_vec();
    assert!(
        build(five)
            .validate()
            .unwrap_err()
            .to_string()
            .contains("at most 4")
    );

    // Repeated symbol: two legs on the same contract is not a spread.
    assert!(
        build(vec![leg("A"), leg("A")])
            .validate()
            .unwrap_err()
            .to_string()
            .contains("unique symbols")
    );

    // A quantity is required.
    let mut no_qty = build(vec![leg("A"), leg("B")]);
    no_qty.qty = None;
    assert!(no_qty.validate().unwrap_err().to_string().contains("qty"));

    // Only market and limit.
    let mut stop = build(vec![leg("A"), leg("B")]);
    stop.order_type = OrderType::Stop;
    assert!(
        stop.validate()
            .unwrap_err()
            .to_string()
            .contains("market and limit")
    );

    assert!(build(vec![leg("A"), leg("B")]).validate().is_ok());
    assert!(
        build(["A", "B", "C", "D"].map(leg).to_vec())
            .validate()
            .is_ok()
    );
}

/// Every class but `mleg` needs a symbol and a side, and a multi-leg order must
/// not carry either — the legs do.
#[test]
fn a_single_leg_order_without_a_symbol_or_a_side_is_refused() {
    let mut order = OrderRequest::market("AAPL", OrderSide::Buy, qty(1), TimeInForce::Day);

    order.symbol = None;
    assert!(order.validate().unwrap_err().to_string().contains("symbol"));

    order.symbol = Some("AAPL".to_owned());
    order.side = None;
    assert!(order.validate().unwrap_err().to_string().contains("side"));
}

// ------------------------------------------------- the other trading filters

/// `ca_types` is emitted once per value — `?ca_types=x&ca_types=y` — rather
/// than comma-separated, which is why this request builds its query by hand.
/// It cannot go through the normal serializer at all: `serde_urlencoded` has no
/// representation for a sequence and fails the whole request.
#[test]
fn corporate_announcements_repeat_their_type_parameter() {
    use alpaca_sdk::trading::{CorporateActionDateType, CorporateActionType};

    let request = GetCorporateAnnouncementsRequest::new(
        vec![CorporateActionType::Dividend, CorporateActionType::Split],
        "2022-01-01".parse().unwrap(),
        "2022-03-01".parse().unwrap(),
    )
    .cusip("037833100")
    .date_type(CorporateActionDateType::ExDate);

    let query = request.to_query();

    assert_eq!(
        query
            .iter()
            .filter(|(key, _)| *key == "ca_types")
            .map(|(_, value)| value.as_str())
            .collect::<Vec<_>>(),
        ["dividend", "split"],
        "a repeated parameter, not a comma-joined one"
    );

    let named = |key: &str| {
        query
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v.as_str())
    };
    assert_eq!(named("since"), Some("2022-01-01"));
    assert_eq!(named("until"), Some("2022-03-01"));
    assert_eq!(named("cusip"), Some("037833100"));
    assert_eq!(named("date_type"), Some("ex_date"));
}

/// A locate has two optional terms, and both are the difference between a
/// borrow a caller wanted and one they did not: a fee cap and a refusal to
/// partially fill.
#[test]
fn a_locate_carries_its_price_cap_and_its_all_or_none_flag() {
    use alpaca_sdk::trading::CreateLocateRequest;

    let bare = serde_json::to_value(CreateLocateRequest::new("AAPL", 100)).unwrap();
    assert_eq!(bare, json!({"symbol": "AAPL", "qty": 100}));

    let bounded = serde_json::to_value(
        CreateLocateRequest::new("AAPL", 100)
            .limit_price(Decimal::new(25, 2))
            .all_or_none(true),
    )
    .unwrap();

    assert_eq!(bounded["limit_price"], "0.25");
    assert_eq!(bounded["all_or_none"], true);
}

#[test]
fn the_wallet_and_tokenization_filters_write_their_own_fields() {
    use alpaca_sdk::trading::{
        CryptoChain, GetCryptoWalletsRequest, GetTokenizationRequestsRequest, TokenizationType,
    };

    let wallets =
        serde_json::to_value(GetCryptoWalletsRequest::new().chain(CryptoChain::Eth)).unwrap();
    assert_eq!(wallets["chain"], "ETH");

    let tokenization = serde_json::to_value(
        GetTokenizationRequestsRequest::new().request_type(TokenizationType::Mint),
    )
    .unwrap();
    // `type` on the wire, `request_type` in Rust — the wire name is a keyword.
    assert_eq!(tokenization["type"], "mint");
    assert!(tokenization.get("request_type").is_none(), "{tokenization}");
}

/// Only an account update resets the staleness clock. A control frame that
/// counted would make a silent connection look healthy indefinitely.
#[test]
fn only_a_trade_update_counts_as_stream_activity() {
    use alpaca_sdk::trading::TradeStreamMessage;

    let control = TradeStreamMessage::Other {
        stream: "listening".to_owned(),
        raw: json!({"streams": ["trade_updates"]}),
    };
    assert!(!control.is_trade_update());
}

// ------------------------------------------------------ the vocabulary itself

/// The point of carrying a value is that a caller can *name* it. Every one of
/// these was reachable before as `AssetClass::from("us_index")` — an `Unknown`
/// that serializes correctly and reads at the call site like a bug.
#[test]
fn the_filters_can_name_every_documented_value() {
    use alpaca_sdk::trading::{ActivityType, AssetClass, GetOrdersRequest};

    let mut filter = GetOrdersRequest::default();
    filter.side = Some(OrderSide::SellShort);
    filter.asset_class = Some(vec![AssetClass::UsIndex, AssetClass::Treasury]);

    let sent = serde_json::to_value(&filter).unwrap();
    assert_eq!(sent["side"], "sell_short");
    assert_eq!(sent["asset_class"], "us_index,treasury");

    for value in [
        OrderSide::BuyMinus,
        OrderSide::SellPlus,
        OrderSide::SellShortExempt,
        OrderSide::Undisclosed,
        OrderSide::Cross,
        OrderSide::CrossShort,
    ] {
        assert!(!value.is_unknown(), "{value}");
    }

    for value in [
        ActivityType::Cgd,
        ActivityType::Divfee,
        ActivityType::Divft,
        ActivityType::Divtw,
        ActivityType::Fopt,
        ActivityType::Intnra,
        ActivityType::Inttw,
        ActivityType::Jnl,
        ActivityType::Misc,
        ActivityType::Opca,
        ActivityType::Ptr,
        ActivityType::Trans,
    ] {
        assert!(!value.is_unknown(), "{value}");
    }
}

/// An empty `order_class` decodes as `simple`, which is why there is no variant
/// for it — the drift report records that as a decision rather than a gap.
#[test]
fn an_empty_order_class_decodes_as_simple() {
    use alpaca_sdk::trading::Order;

    let order: Order = serde_json::from_value(json!({
        "id": "61e69015-8549-4bfd-b9c3-01e75843f47d",
        "client_order_id": "x",
        "created_at": "2021-03-16T18:38:01.942282Z",
        "updated_at": "2021-03-16T18:38:01.942282Z",
        "submitted_at": "2021-03-16T18:38:01.937734Z",
        "order_class": "",
        "time_in_force": "day",
        "status": "filled",
        "extended_hours": false,
        "symbol": "AAPL"
    }))
    .unwrap();

    assert_eq!(order.order_class, OrderClass::Simple);
}

// ------------------------------------------------------------- OTO exits
//
// An OTO order carries exactly one exit. The builders used to *accumulate*, so
// chaining both — or building one conditionally — produced an `oto` order with
// two legs that `validate` accepted, and the position exited at a take-profit
// the caller thought they had replaced.

/// Chaining both builders leaves the last one called, not both.
#[test]
fn chaining_both_oto_exits_keeps_only_the_last() {
    let take_profit_last = OrderRequest::market("AAPL", OrderSide::Buy, qty(1), TimeInForce::Day)
        .oto_stop_loss(StopLossRequest::new(price(90)))
        .oto_take_profit(TakeProfitRequest::new(price(110)));

    let json = body(&take_profit_last);
    assert_eq!(json["order_class"], "oto");
    assert_eq!(json["take_profit"]["limit_price"], "110");
    assert!(
        json.get("stop_loss").is_none(),
        "the earlier stop-loss leg should have been replaced: {json}"
    );
    assert!(take_profit_last.validate().is_ok());

    // And the other order of the two.
    let stop_loss_last = OrderRequest::market("AAPL", OrderSide::Buy, qty(1), TimeInForce::Day)
        .oto_take_profit(TakeProfitRequest::new(price(110)))
        .oto_stop_loss(StopLossRequest::new(price(90)));

    let json = body(&stop_loss_last);
    assert_eq!(json["stop_loss"]["stop_price"], "90");
    assert!(json.get("take_profit").is_none(), "{json}");
}

/// The fields are public, so the builders are not the only way in. Two legs on
/// an `oto` is a bracket written the wrong way round, and `validate` says so
/// rather than letting the API take it as something else.
#[test]
fn an_oto_order_carrying_both_exits_is_refused() {
    let mut order = OrderRequest::market("AAPL", OrderSide::Buy, qty(1), TimeInForce::Day)
        .oto_take_profit(TakeProfitRequest::new(price(110)));
    order.stop_loss = Some(StopLossRequest::new(price(90)));

    let error = order.validate().unwrap_err();
    assert!(
        matches!(error, alpaca_sdk::Error::InvalidRequest(ref m) if m.contains("bracket")),
        "expected the error to point at the bracket class, got {error:?}"
    );
}

/// The rule it must not over-reach: one exit is still valid, either side.
#[test]
fn an_oto_order_with_exactly_one_exit_is_accepted() {
    let with_take_profit = OrderRequest::market("AAPL", OrderSide::Buy, qty(1), TimeInForce::Day)
        .oto_take_profit(TakeProfitRequest::new(price(110)));
    assert!(with_take_profit.validate().is_ok());

    let with_stop_loss = OrderRequest::market("AAPL", OrderSide::Buy, qty(1), TimeInForce::Day)
        .oto_stop_loss(StopLossRequest::new(price(90)));
    assert!(with_stop_loss.validate().is_ok());
}
