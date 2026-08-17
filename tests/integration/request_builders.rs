//! Every market data request builder, asserted on the parameters it produces.
//!
//! A builder is the easiest thing in the crate to get wrong invisibly: a setter
//! that writes the wrong field, or a field whose serde name is not the parameter
//! Alpaca reads, compiles and type-checks perfectly and simply filters nothing.
//! `just parameters` catches a parameter that is *absent*; only this catches one
//! that is present under the wrong name.
//!
//! The assertions are on the serialized object rather than on a query string,
//! because that object is exactly what the market data transport consumes:
//! `pagination::to_param_map` turns a request into `serde_json::Value` and
//! renders each entry from there. Asserting a query string here would mean
//! re-implementing that rendering in the test, which proves nothing.

#![cfg(feature = "data")]

use alpaca_sdk::data::{
    Adjustment, CryptoBarsRequest, CryptoLatestRequest, DataFeed, ForexLatestRatesRequest,
    ForexRatesRequest, MarketMoversRequest, MarketType, MostActivesBy, MostActivesRequest,
    OptionBarsRequest, OptionChainRequest, OptionLatestRequest, OptionsFeed, SingleSymbolRequest,
    StockAuctionsRequest, StockBarsRequest, StockLatestRequest, StockTimeseriesRequest, Symbols,
    TimeFrame, TimeseriesRequest,
};
use alpaca_sdk::types::{ContractType, Sort, SupportedCurrencies};
use serde_json::{Value, json};

/// The parameters a request serializes to.
fn params<T: serde::Serialize>(request: &T) -> Value {
    let value = serde_json::to_value(request).expect("a request must serialize");
    assert!(
        value.is_object(),
        "a request must serialize to an object, got {value}"
    );
    value
}

fn at(text: &str) -> chrono::DateTime<chrono::Utc> {
    text.parse().unwrap()
}

// ----------------------------------------------------------------- symbols

#[test]
fn symbols_accepts_every_shape_a_caller_has() {
    assert_eq!(Symbols::from("AAPL").to_string(), "AAPL");
    assert_eq!(Symbols::from("AAPL".to_owned()).to_string(), "AAPL");
    assert_eq!(Symbols::from(vec!["AAPL", "SPY"]).to_string(), "AAPL,SPY");
    assert_eq!(Symbols::from(vec!["AAPL".to_owned()]).to_string(), "AAPL");
    assert_eq!(Symbols::from(["AAPL", "SPY"]).to_string(), "AAPL,SPY");

    let symbols = Symbols::from(["AAPL", "SPY"]);
    assert_eq!(symbols.as_slice().len(), 2);
    assert!(!symbols.is_empty());
    assert!(Symbols::from(Vec::<String>::new()).is_empty());
}

/// The field is `symbol_or_symbols` in Rust and `symbols` on the wire. A rename
/// that stopped applying would filter nothing and return the whole universe.
#[test]
fn symbols_serialize_under_the_name_alpaca_reads() {
    let request = TimeseriesRequest::new(["AAPL", "SPY"]);
    let params = params(&request);

    assert_eq!(params["symbols"], json!(["AAPL", "SPY"]));
    assert!(params.get("symbol_or_symbols").is_none());
}

// ------------------------------------------------------------- time series

#[test]
fn the_shared_time_series_setters_each_write_their_own_field() {
    let request = TimeseriesRequest::new("AAPL")
        .start(at("2022-01-01T00:00:00Z"))
        .end(at("2022-01-02T00:00:00Z"))
        .limit(500)
        .sort(Sort::Asc)
        .currency(SupportedCurrencies::Gbp);

    let params = params(&request);
    assert_eq!(params["symbols"], json!(["AAPL"]));
    assert_eq!(params["start"], "2022-01-01T00:00:00Z");
    assert_eq!(params["end"], "2022-01-02T00:00:00Z");
    assert_eq!(params["limit"], 500);
    assert_eq!(params["sort"], "asc");
    assert_eq!(params["currency"], "GBP");
}

#[test]
fn an_unset_filter_is_absent_rather_than_null() {
    let params = params(&TimeseriesRequest::new("AAPL"));

    assert_eq!(params.as_object().unwrap().len(), 1);
    for absent in ["limit", "start", "end", "sort", "currency"] {
        assert!(params.get(absent).is_none(), "{absent} should be absent");
    }
}

#[test]
fn stock_bars_carry_the_timeframe_and_their_own_two_filters() {
    let params = params(
        &StockBarsRequest::new("AAPL", TimeFrame::day())
            .adjustment(Adjustment::Split)
            .feed(DataFeed::Iex),
    );

    assert_eq!(params["timeframe"], "1Day");
    assert_eq!(params["adjustment"], "split");
    assert_eq!(params["feed"], "iex");
}

/// The flattened base must not nest. `{"base": {"symbols": …}}` would reach the
/// transport as a parameter named `base` and filter nothing.
#[test]
fn a_flattened_base_serializes_flat() {
    let params = params(&CryptoBarsRequest::new("BTC/USD", TimeFrame::hour()));

    assert_eq!(params["symbols"], json!(["BTC/USD"]));
    assert_eq!(params["timeframe"], "1Hour");
    assert!(params.get("base").is_none());
}

#[test]
fn option_bars_take_a_timeframe_too() {
    let params = params(&OptionBarsRequest::new(
        "AAPL240119C00150000",
        TimeFrame::minute(),
    ));
    assert_eq!(params["timeframe"], "1Min");
}

#[test]
fn stock_time_series_requests_carry_their_own_two_filters() {
    let params = params(
        &StockTimeseriesRequest::new("AAPL")
            .feed(DataFeed::Sip)
            .asof("2022-01-01"),
    );

    assert_eq!(params["feed"], "sip");
    assert_eq!(params["asof"], "2022-01-01");
}

// ------------------------------------------------- the delegated shared five

/// The five shared filters, as they arrive at `base`.
fn written_through(base: &TimeseriesRequest) {
    assert_eq!(base.start, Some(at("2022-01-01T00:00:00Z")));
    assert_eq!(base.end, Some(at("2022-01-02T00:00:00Z")));
    assert_eq!(base.limit, Some(42));
    assert_eq!(base.sort, Some(Sort::Asc));
    assert_eq!(base.currency, Some(SupportedCurrencies::Gbp));
}

/// The same five, as they arrive on the wire.
fn serialized(request: &impl serde::Serialize) {
    let params = params(request);

    assert_eq!(params["start"], "2022-01-01T00:00:00Z");
    assert_eq!(params["end"], "2022-01-02T00:00:00Z");
    assert_eq!(params["limit"], 42);
    assert_eq!(params["sort"], "asc");
    assert_eq!(params["currency"], "GBP");
    // The base is flattened on both sides: a nested `base` object would reach
    // the transport as one parameter of that name and filter nothing.
    assert!(params.get("base").is_none());
}

/// Every wrapper of [`TimeseriesRequest`], with all five delegates called.
///
/// No wrapper's source names these five methods — `#[setters(flatten)]` reads
/// them off `TimeseriesRequest` — so this is where a delegate that writes the
/// wrong field, or one that quietly stopped being generated, becomes visible.
/// A missing delegate fails to compile here; a delegate writing the wrong field
/// compiles and fails the assertion.
///
/// `CryptoBarsRequest` and `OptionBarsRequest` are the two to watch: they have
/// no optional fields of their own, so before flattening every method they had
/// came from the hand-written `timeseries_delegates!` and nothing else. If the
/// delegates stop being generated for them, there is no second impl to hide it.
#[test]
fn every_wrapper_delegates_all_five_shared_filters() {
    let start = at("2022-01-01T00:00:00Z");
    let end = at("2022-01-02T00:00:00Z");

    let stock_bars = StockBarsRequest::new("AAPL", TimeFrame::day())
        .start(start)
        .end(end)
        .limit(42)
        .sort(Sort::Asc)
        .currency(SupportedCurrencies::Gbp);
    written_through(&stock_bars.base);
    serialized(&stock_bars);

    let crypto_bars = CryptoBarsRequest::new("BTC/USD", TimeFrame::hour())
        .start(start)
        .end(end)
        .limit(42)
        .sort(Sort::Asc)
        .currency(SupportedCurrencies::Gbp);
    written_through(&crypto_bars.base);
    serialized(&crypto_bars);

    let option_bars = OptionBarsRequest::new("AAPL240119C00150000", TimeFrame::minute())
        .start(start)
        .end(end)
        .limit(42)
        .sort(Sort::Asc)
        .currency(SupportedCurrencies::Gbp);
    written_through(&option_bars.base);
    serialized(&option_bars);

    let stock_timeseries = StockTimeseriesRequest::new("AAPL")
        .start(start)
        .end(end)
        .limit(42)
        .sort(Sort::Asc)
        .currency(SupportedCurrencies::Gbp);
    written_through(&stock_timeseries.base);
    serialized(&stock_timeseries);

    let stock_auctions = StockAuctionsRequest::new("AAPL")
        .start(start)
        .end(end)
        .limit(42)
        .sort(Sort::Asc)
        .currency(SupportedCurrencies::Gbp);
    written_through(&stock_auctions.base);
    serialized(&stock_auctions);
}

/// A delegate must not be the only way to reach a filter: the base keeps its own
/// setters, and a wrapper's `.base` stays assignable.
#[test]
fn flattening_adds_a_second_route_rather_than_moving_the_first() {
    let mut request = StockBarsRequest::new("AAPL", TimeFrame::day());
    request.base = TimeseriesRequest::new("AAPL").limit(7);

    assert_eq!(request.base.limit, Some(7));
    assert_eq!(request.limit(9).base.limit, Some(9));
}

// ----------------------------------------------------------------- latest

#[test]
fn the_latest_requests_take_a_feed_where_the_api_offers_one() {
    assert_eq!(
        params(&StockLatestRequest::new("AAPL").feed(DataFeed::Otc))["feed"],
        "otc"
    );
    assert_eq!(
        params(&OptionLatestRequest::new("AAPL240119C00150000").feed(OptionsFeed::Opra))["feed"],
        "opra"
    );

    // Crypto has one feed, so its request has no switch to get wrong.
    let crypto = params(&CryptoLatestRequest::new("BTC/USD"));
    assert_eq!(crypto.as_object().unwrap().len(), 1);
}

// ----------------------------------------------------------------- options

#[test]
fn an_option_chain_filters_by_type_and_feed() {
    let params = params(
        &OptionChainRequest::new("AAPL")
            .contract_type(ContractType::Call)
            .feed(OptionsFeed::Indicative),
    );

    // `type`, not `contract_type`: the wire name is a Rust keyword.
    assert_eq!(params["type"], "call");
    assert_eq!(params["feed"], "indicative");
    // The underlying goes in the path, so it must not also be a parameter.
    assert!(params.get("underlying_symbol").is_none());
}

// ---------------------------------------------------------------- auctions

#[test]
fn stock_auctions_carry_a_feed() {
    assert_eq!(
        params(&StockAuctionsRequest::new("AAPL").feed(DataFeed::Sip))["feed"],
        "sip"
    );
}

// -------------------------------------------------------- single symbol

/// The single-symbol request has a setter per field and no shared base, so each
/// one is its own chance to write into the wrong field.
#[test]
fn every_single_symbol_setter_writes_its_own_field() {
    let sent = params(
        &SingleSymbolRequest::new()
            .start(at("2022-01-01T00:00:00Z"))
            .end(at("2022-01-02T00:00:00Z"))
            .limit(25)
            .timeframe(TimeFrame::week())
            .adjustment(Adjustment::All)
            .feed(DataFeed::Iex)
            .sort(Sort::Asc)
            .currency(SupportedCurrencies::Eur),
    );

    assert_eq!(sent["start"], "2022-01-01T00:00:00Z");
    assert_eq!(sent["end"], "2022-01-02T00:00:00Z");
    assert_eq!(sent["limit"], 25);
    assert_eq!(sent["timeframe"], "1Week");
    assert_eq!(sent["adjustment"], "all");
    assert_eq!(sent["feed"], "iex");
    assert_eq!(sent["sort"], "asc");
    assert_eq!(sent["currency"], "EUR");

    // A request with nothing set sends nothing.
    let empty = params(&SingleSymbolRequest::default());
    assert!(empty.as_object().unwrap().is_empty(), "{empty}");
}

// ------------------------------------------------------------------ forex

#[test]
fn forex_rates_take_a_window_and_a_timeframe() {
    let sent = params(
        &ForexRatesRequest::new(["EUR/USD", "GBP/USD"])
            .timeframe(TimeFrame::day())
            .between(at("2022-01-01T00:00:00Z"), at("2022-01-31T00:00:00Z"))
            .limit(100),
    );

    // Keyed by pair, not by symbol — a different parameter name entirely.
    assert_eq!(sent["currency_pairs"], json!(["EUR/USD", "GBP/USD"]));
    assert!(sent.get("symbols").is_none());
    assert_eq!(sent["timeframe"], "1Day");
    assert_eq!(sent["limit"], 100);
    assert!(sent["start"].as_str().unwrap().starts_with("2022-01-01"));
    assert!(sent["end"].as_str().unwrap().starts_with("2022-01-31"));

    let latest = params(&ForexLatestRatesRequest::new("EUR/USD"));
    assert_eq!(latest["currency_pairs"], json!(["EUR/USD"]));
}

// --------------------------------------------------------------- screener

#[test]
fn the_screener_requests_carry_their_defaults() {
    let actives = params(&MostActivesRequest::default());
    assert_eq!(actives["top"], 10);
    assert_eq!(actives["by"], "volume");

    let explicit = params(&MostActivesRequest::new(25, MostActivesBy::Trades));
    assert_eq!(explicit["top"], 25);
    assert_eq!(explicit["by"], "trades");

    let movers = params(&MarketMoversRequest::default());
    assert_eq!(movers["top"], 10);

    // `market_type` picks the path — `/screener/{market_type}/movers` — so it
    // must not also be sent as a parameter.
    let crypto = params(&MarketMoversRequest::new(5, MarketType::Crypto));
    assert_eq!(crypto["top"], 5);
    assert!(crypto.get("market_type").is_none(), "{crypto}");
}
