//! Captures payloads for the routes no SDK's tests cover.
//!
//! `fixtures/` came from alpaca-py's tests and `fixtures/go/` from the Go SDK's.
//! Between them they miss forex, logos, the stock and option metadata, and
//! indices — nobody tests those, so no SDK can supply a payload for them. This
//! asks Alpaca directly.
//!
//! `#[ignore]`d like the rest of the live tests, and read-only: every route here
//! is a GET against market data.
//!
//! ```text
//! just capture
//! ```
//!
//! Writes to `fixtures/live/`, and writes `fixtures/live/index.json` recording
//! what each route answered — **including the ones that refused**. A 403 is a
//! finding: it says the route exists and this account cannot see it, which is
//! worth more than a gap in a list. Several of these are plan-gated, so a clean
//! run is not expected to capture everything.

#![cfg(feature = "data")]

use std::collections::BTreeMap;

use alpaca_sdk::rest::{Empty, RestClient, RestConfig};
use alpaca_sdk::{BaseUrl, Credentials};
use serde_json::json;

/// A route to try, with the API version it lives at.
struct Candidate {
    /// What the fixture is called.
    name: &'static str,
    /// The version segment, which differs per route rather than per client.
    version: &'static str,
    /// The path below the version.
    path: &'static str,
    /// Query parameters, if the route needs any to answer usefully.
    query: &'static [(&'static str, &'static str)],
}

const CANDIDATES: &[Candidate] = &[
    // The official decoder for the single-letter exchange codes and the opaque
    // `conditions` lists on trades and quotes. Static reference data, so most
    // likely to be reachable.
    Candidate {
        name: "stocks_meta_exchanges",
        version: "v2",
        path: "/stocks/meta/exchanges",
        query: &[],
    },
    // `tape` is required here and *not* on the option equivalent below — an
    // asymmetry the spec's `/stocks/meta/conditions/{tick_type}` does not hint
    // at. A is NYSE, B is NYSE Arca and American, C is Nasdaq.
    Candidate {
        name: "stocks_meta_conditions_trade",
        version: "v2",
        path: "/stocks/meta/conditions/trade",
        query: &[("tape", "A")],
    },
    Candidate {
        name: "stocks_meta_conditions_quote",
        version: "v2",
        path: "/stocks/meta/conditions/quote",
        query: &[("tape", "A")],
    },
    Candidate {
        name: "options_meta_exchanges",
        version: "v1beta1",
        path: "/options/meta/exchanges",
        query: &[],
    },
    Candidate {
        name: "options_meta_conditions_trade",
        version: "v1beta1",
        path: "/options/meta/conditions/trade",
        query: &[],
    },
    // Forex.
    Candidate {
        name: "forex_latest_rates",
        version: "v1beta1",
        path: "/forex/latest/rates",
        query: &[("currency_pairs", "EURUSD,GBPUSD")],
    },
    Candidate {
        name: "forex_rates",
        version: "v1beta1",
        path: "/forex/rates",
        query: &[
            ("currency_pairs", "EURUSD"),
            ("start", "2026-08-10"),
            ("end", "2026-08-11"),
            ("timeframe", "1Day"),
        ],
    },
    // Indices — in no spec, and only the Node SDK claims they exist.
    Candidate {
        name: "indices_latest_values",
        version: "v1beta1",
        path: "/indices/latest/values",
        query: &[("symbols", "SPX")],
    },
    // Logos answer with an image rather than JSON, which the capture records
    // as `not_json` rather than pretending it is a fixture.
    Candidate {
        name: "logos_aapl",
        version: "v1beta1",
        path: "/logos/AAPL",
        query: &[],
    },
    // Doubles as an entitlement probe. The free tier serves IEX only, so a SIP
    // response proves a paid market-data plan and narrows any 403 below to a
    // per-product grant rather than the plan as a whole.
    Candidate {
        name: "stocks_bars_sip",
        version: "v2",
        path: "/stocks/bars",
        query: &[
            ("symbols", "AAPL"),
            ("start", "2026-08-10"),
            ("end", "2026-08-11"),
            ("timeframe", "1Day"),
            ("feed", "sip"),
        ],
    },
    // Auctions, to sit beside the Go SDK's harvested pages as a live sample.
    Candidate {
        name: "stocks_auctions",
        version: "v2",
        path: "/stocks/auctions",
        query: &[
            ("symbols", "AAPL"),
            ("start", "2026-08-10"),
            ("end", "2026-08-11"),
            ("limit", "5"),
        ],
    },
];

fn credentials() -> Credentials {
    Credentials::from_env().unwrap_or_else(|e| {
        panic!("{e}\n\nSet APCA_API_KEY_ID and APCA_API_SECRET_KEY (paper keys) and re-run.")
    })
}

fn client(credentials: &Credentials, version: &str) -> RestClient {
    RestClient::new(
        credentials,
        RestConfig::from(BaseUrl::Data).api_version(version),
    )
    .expect("a data client")
}

#[tokio::test]
#[ignore = "hits the live market data API; run with `just capture`"]
async fn capture_the_routes_no_sdk_tests_cover() {
    let credentials = credentials();
    let out = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/live");
    std::fs::create_dir_all(&out).expect("fixtures/live");

    let mut index: BTreeMap<String, serde_json::Value> = BTreeMap::new();

    for candidate in CANDIDATES {
        let rest = client(&credentials, candidate.version);
        let route = format!("/{}{}", candidate.version, candidate.path);

        let result = rest
            .request_raw(
                reqwest::Method::GET,
                candidate.path,
                Some(candidate.query),
                None::<&Empty>,
            )
            .await;

        let entry = match result {
            Ok(body) => match serde_json::from_str::<serde_json::Value>(&body) {
                Ok(value) => {
                    std::fs::write(
                        out.join(format!("{}.json", candidate.name)),
                        serde_json::to_string_pretty(&value).unwrap() + "\n",
                    )
                    .expect("write fixture");
                    println!("captured  {route}");
                    json!({ "route": route, "query": candidate.query, "status": "captured" })
                }
                Err(e) => {
                    // A 200 that is not JSON is itself worth recording: the
                    // logo route answers with an image, for instance.
                    println!("not json  {route}: {e}");
                    json!({
                        "route": route,
                        "status": "not_json",
                        "detail": e.to_string(),
                        "bytes": body.len(),
                    })
                }
            },
            Err(e) => {
                // Refusals are the point as much as successes. A 403 says the
                // route is there and this account cannot reach it.
                println!("refused   {route}: {e}");
                json!({
                    "route": route,
                    "query": candidate.query,
                    "status": "refused",
                    "http_status": e.status(),
                    "detail": e.to_string(),
                })
            }
        };
        index.insert(candidate.name.to_owned(), entry);
    }

    std::fs::write(
        out.join("index.json"),
        serde_json::to_string_pretty(&json!({
            "note": "Captured from the live market data API by tests/live_capture.rs \
                     (`just capture`). Routes that refused are recorded too: a 403 \
                     means the route exists and the account's plan does not reach it.",
            "captured_by": "tests/live_capture.rs",
            "routes": index,
        }))
        .unwrap()
            + "\n",
    )
    .expect("write index");

    let captured = index.values().filter(|v| v["status"] == "captured").count();
    println!("\n{captured}/{} routes captured", index.len());
    assert!(
        captured > 0,
        "nothing captured at all — check the credentials rather than the routes"
    );
}
