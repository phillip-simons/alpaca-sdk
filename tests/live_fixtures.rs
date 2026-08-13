//! The payloads captured from the live API by `just capture`.
//!
//! Unlike `fixtures/` and `fixtures/go/`, which come from two SDKs' test
//! suites, these came off the wire. They cover the routes nobody tests:
//! the stock and option metadata, and auctions.
//!
//! Read here for the same reason as everything else in `fixtures/` — a payload
//! no test touches proves nothing. `fixtures/live/index.json` also records the
//! routes that *refused*, which this checks too: a 403 is a finding about the
//! account's grants, and losing it silently would waste the trip.

#![cfg(feature = "data")]

use std::collections::BTreeMap;

fn live_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/live")
}

fn read(name: &str) -> serde_json::Value {
    let path = live_dir().join(name);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// The metadata endpoints answer with a flat map of code to description.
fn decoder(name: &str) -> BTreeMap<String, String> {
    serde_json::from_value(read(name)).unwrap_or_else(|e| panic!("{name} is not a code map: {e}"))
}

#[test]
fn the_exchange_codes_decode_to_names() {
    // These are the official decoder for the single-letter `exchange` field on
    // trades and quotes, which is otherwise unreadable.
    let stocks = decoder("stocks_meta_exchanges.json");
    assert!(stocks.len() > 10, "got {} stock exchanges", stocks.len());
    assert_eq!(stocks.get("V").map(String::as_str), Some("IEX"));
    assert!(stocks.contains_key("P"), "NYSE Arca should be in there");

    let options = decoder("options_meta_exchanges.json");
    assert!(options.len() > 5, "got {} option exchanges", options.len());
    // The same letter means different venues on the two maps, which is the
    // whole reason they are separate endpoints.
    assert_ne!(stocks.get("A"), options.get("A"));
}

#[test]
fn a_single_space_is_a_trade_condition() {
    // `" ": "Regular Sale"` — the most common condition on the tape is spelled
    // with a space. Anything that trims or splits on whitespace loses it, and
    // an empty-string-means-absent rule would drop the ordinary case.
    let conditions = decoder("stocks_meta_conditions_trade.json");
    assert_eq!(
        conditions.get(" ").map(String::as_str),
        Some("Regular Sale")
    );
    assert!(conditions.len() > 20);
}

#[test]
fn quote_and_trade_conditions_are_different_tables() {
    let trade = decoder("stocks_meta_conditions_trade.json");
    let quote = decoder("stocks_meta_conditions_quote.json");
    assert!(!quote.is_empty());
    // Same code, different meaning depending on which it decodes.
    let overlap = trade
        .keys()
        .filter(|k| quote.contains_key(*k))
        .filter(|k| trade.get(*k) != quote.get(*k))
        .count();
    assert!(
        overlap > 0,
        "if no code disagreed, one table would do for both"
    );
}

#[test]
fn the_option_conditions_need_no_tape_and_the_stock_ones_do() {
    // Captured proof of an asymmetry no spec mentions: the stock conditions
    // route requires a `tape` parameter and returns 400 without it, while the
    // option one takes none. The index records the query each capture used.
    let index = read("index.json");
    let routes = &index["routes"];

    let stock_query = routes["stocks_meta_conditions_trade"]["query"]
        .as_array()
        .expect("recorded query");
    assert!(
        stock_query.iter().any(|p| p[0] == "tape"),
        "the stock capture should have needed a tape"
    );

    let option_query = routes["options_meta_conditions_trade"]["query"]
        .as_array()
        .expect("recorded query");
    assert!(
        option_query.is_empty(),
        "the option capture needed no parameters"
    );
}

#[test]
fn the_refusals_are_kept_as_findings() {
    // forex, indices and logos answered 403 on an account whose paid plan
    // reaches SIP — so those are per-product grants rather than the plan as a
    // whole. That is worth keeping; it is the difference between "we did not
    // get to it" and "it exists and costs extra".
    let index = read("index.json");
    let routes = index["routes"].as_object().expect("routes");

    let refused: Vec<&String> = routes
        .iter()
        .filter(|(_, v)| v["status"] == "refused")
        .map(|(k, _)| k)
        .collect();
    assert!(
        !refused.is_empty(),
        "if everything now succeeds, the README's account of the grants is stale"
    );

    for name in ["forex_latest_rates", "indices_latest_values", "logos_aapl"] {
        assert_eq!(
            routes[name]["status"], "refused",
            "{name} was refused when captured; if that changed, update the README"
        );
        assert_eq!(routes[name]["http_status"], 403);
    }

    // And the SIP probe that proves the refusals are not just an unpaid plan.
    assert_eq!(routes["stocks_bars_sip"]["status"], "captured");
}

#[test]
fn every_captured_file_is_in_the_index() {
    let index = read("index.json");
    let routes = index["routes"].as_object().expect("routes");

    for entry in std::fs::read_dir(live_dir()).expect("fixtures/live") {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if name == "index.json" || path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let stem = name.trim_end_matches(".json");
        assert!(routes.contains_key(stem), "{name} is not in index.json");
        assert_eq!(routes[stem]["status"], "captured", "{name}");
    }
}
