//! The payloads harvested from `alpaca-trade-api-go`'s test suite.
//!
//! These come from a second SDK's tests rather than alpaca-py's, which is the
//! point: two independent authors wrote down what Alpaca sends, and where they
//! disagree one of them is wrong about the API.
//!
//! Regenerate with `just harvest`. `fixtures/go/index.json` records where each
//! payload came from, and which route the Go test asserted for it.
//!
//! **Every harvested payload is read by something here.** The crate already
//! shipped a fixture nobody parsed — an account list with `"funding_source":
//! null` in it — and it took months and an unrelated task to notice the model
//! could not read it. A payload that no test touches is a payload that proves
//! nothing.

#![cfg(feature = "data")]

use std::collections::HashMap;

use alpaca_sdk::data::{Bar, Quote, Trade};
use serde::Deserialize;

fn harvested() -> Vec<(String, serde_json::Value)> {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/go");
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("fixtures/go should exist") {
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

#[test]
fn every_harvested_payload_is_json_and_non_empty() {
    let all = harvested();
    assert!(
        all.len() > 60,
        "expected the harvest to yield most of the Go suite, got {}",
        all.len()
    );
    for (name, value) in &all {
        let empty = match value {
            serde_json::Value::Object(map) => map.is_empty(),
            serde_json::Value::Array(items) => items.is_empty(),
            _ => true,
        };
        assert!(!empty, "{name} carries no data");
    }
}

#[test]
fn the_index_places_every_payload() {
    // A fixture nobody can place against a route is one nobody will reach for.
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/go");
    let index: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("index.json")).unwrap()).unwrap();
    let entries = index["fixtures"].as_object().expect("fixtures map");

    for (name, _) in harvested() {
        let entry = entries
            .get(&name)
            .unwrap_or_else(|| panic!("{name} is missing from index.json"));
        assert!(
            entry.get("route").is_some() || entry.get("sdk_method").is_some(),
            "{name} has neither a route nor an SDK method"
        );
        assert!(entry.get("source").is_some(), "{name} has no source");
    }
}

/// The `{"bars": {"AAPL": [...]}}` envelope the multi-symbol endpoints use.
#[derive(Deserialize)]
struct MultiBars {
    bars: HashMap<String, Vec<Bar>>,
}

/// The `{"bars": {"AAPL": {...}}}` envelope the "latest" endpoints use.
#[derive(Deserialize)]
struct LatestBars {
    bars: HashMap<String, Bar>,
}

#[derive(Deserialize)]
struct MultiTrades {
    trades: HashMap<String, Vec<Trade>>,
}

#[derive(Deserialize)]
struct LatestTrades {
    trades: HashMap<String, Trade>,
}

#[derive(Deserialize)]
struct MultiQuotes {
    quotes: HashMap<String, Vec<Quote>>,
}

#[derive(Deserialize)]
struct LatestQuotes {
    quotes: HashMap<String, Quote>,
}

/// Reads `name` into `T`, failing with the payload attached.
fn parse_as<T: serde::de::DeserializeOwned>(name: &str, value: &serde_json::Value) -> T {
    serde_json::from_value(value.clone())
        .unwrap_or_else(|e| panic!("{name} does not fit the model: {e}\n{value:#}"))
}

#[test]
fn the_go_suites_bars_fit_our_bar_model() {
    // Cross-SDK verification: these payloads were written by whoever maintains
    // the Go client, against the same API, without reference to alpaca-py.
    let mut checked = 0;
    for (name, value) in harvested() {
        let Some(bars) = value.get("bars") else {
            continue;
        };
        // Both envelope shapes appear, and which one is in play is decided by
        // whether the first symbol maps to a list or an object.
        let Some(first) = bars.as_object().and_then(|m| m.values().next()) else {
            continue;
        };
        if first.is_array() {
            let parsed: MultiBars = parse_as(&name, &value);
            assert!(parsed.bars.values().any(|b| !b.is_empty()), "{name}");
        } else {
            let parsed: LatestBars = parse_as(&name, &value);
            assert!(!parsed.bars.is_empty(), "{name}");
        }
        checked += 1;
    }
    assert!(
        checked >= 10,
        "expected many bar payloads, checked {checked}"
    );
}

#[test]
fn the_go_suites_trades_and_quotes_fit_our_models() {
    let mut checked = 0;
    for (name, value) in harvested() {
        if let Some(trades) = value.get("trades")
            && let Some(first) = trades.as_object().and_then(|m| m.values().next())
        {
            if first.is_array() {
                let parsed: MultiTrades = parse_as(&name, &value);
                assert!(parsed.trades.values().any(|t| !t.is_empty()), "{name}");
            } else {
                let parsed: LatestTrades = parse_as(&name, &value);
                assert!(!parsed.trades.is_empty(), "{name}");
            }
            checked += 1;
        }

        if let Some(quotes) = value.get("quotes")
            && let Some(first) = quotes.as_object().and_then(|m| m.values().next())
        {
            if first.is_array() {
                let parsed: MultiQuotes = parse_as(&name, &value);
                assert!(parsed.quotes.values().any(|q| !q.is_empty()), "{name}");
            } else {
                let parsed: LatestQuotes = parse_as(&name, &value);
                assert!(!parsed.quotes.is_empty(), "{name}");
            }
            checked += 1;
        }
    }
    assert!(
        checked >= 15,
        "expected many trade and quote payloads, checked {checked}"
    );
}

#[test]
fn the_gap_routes_have_payloads_waiting_for_a_model() {
    // Auctions, fixed income and crypto perpetuals are not ported yet. These
    // are the payloads to build those models against when they are — recorded
    // here so the harvest is not silently carrying dead weight, and so this
    // fails loudly if a future harvest stops producing them.
    let names: Vec<String> = harvested().into_iter().map(|(n, _)| n).collect();
    for expected in [
        "auction",
        "fixed_income",
        "crypto_perp",
        "option",
        "news",
        "corporate",
    ] {
        assert!(
            names.iter().any(|n| n.contains(expected)),
            "no harvested payload mentions {expected}; the Go suite covered it before"
        );
    }
}
