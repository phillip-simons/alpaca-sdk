//! Every market data payload under `fixtures/data`, read into the model that
//! claims it.
//!
//! `harvested_go` states the doctrine for `fixtures/go`: "a payload that no test
//! touches is a payload that proves nothing", and the crate has already paid for
//! one — an account list carrying `"funding_source": null` that no test parsed,
//! against a model that could not read it. `fixtures/data` was in the same
//! state: forty-five of its fifty-six payloads were named by no `.rs` file in
//! the repository, so most of what `just capture` recorded was decoration.
//!
//! This sweeps the directory rather than listing the files, for the reason
//! `harvested_go` does: a payload added by a later capture is covered the moment
//! it lands, instead of when someone remembers to name it.
//!
//! Dispatch is by envelope key, not by file name. The market data API wraps
//! every response in the name of what it carries — `{"bars": …}`,
//! `{"quotes": …}` — and that key is already load-bearing, since the pagination
//! loop merges pages by it. Reading it here means a payload from a route that
//! does not exist yet is still parsed by whichever model matches its envelope,
//! and a payload in an envelope this file does not know fails rather than being
//! quietly skipped.
//!
//! `fixtures/broker` and `fixtures/trading` have thirty unnamed payloads
//! between them and are **not** swept here. They have no envelope to dispatch
//! on — the route decides the model and the file name is the only clue to the
//! route — so covering them means a hand-written table mapping a hundred-odd
//! names to models, which is its own change and a wider one than this.

#![cfg(feature = "data")]

use std::collections::HashMap;

use alpaca_sdk::data::{
    Bar, CorporateActions, MostActives, Movers, NewsSet, OptionsSnapshot, Quote, Snapshot, Trade,
};
use serde_json::Value;

/// Every JSON payload under `fixtures/data`, by file name.
fn payloads() -> Vec<(String, Value)> {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/data");
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("fixtures/data should exist") {
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
fn parse_as<T: serde::de::DeserializeOwned>(name: &str, value: &Value) {
    let _: T = serde_json::from_value(value.clone())
        .unwrap_or_else(|e| panic!("{name} does not fit the model: {e}\n{value:#}"));
}

/// Reads a `{"bars": {"AAPL": …}}` envelope, whichever of the two shapes the
/// symbol maps to.
///
/// Both are live: the time series routes map a symbol to a list, the "latest"
/// routes map it to one record, and the envelope key is identical. An empty map
/// is neither and is also real — it is what a symbol Alpaca has nothing for
/// comes back as.
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
    let all = payloads();
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
            // multi-symbol shape is Alpaca's, not a capture artefact — the only
            // payload in this directory that has it maps a symbol underneath.
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
