//! Captures payloads for the routes no SDK's tests cover.
//!
//! `fixtures/` and `fixtures/go/` come from other SDKs' test suites. Between
//! them they miss forex, logos, the stock and option metadata, and indices —
//! nobody tests those, so no SDK can supply a payload for them. This
//! asks Alpaca directly.
//!
//! Phase 6.5 widened it. Most of the routes added there could only be tested
//! against payloads written out of the reference, which is the weakest tier of
//! evidence this repo recognises. The ones reachable with **paper keys** are
//! captured here instead, so they move up a tier: the single-symbol stock
//! routes, the `v3` per-market calendar, and the read-only trading routes for
//! locates, tokenization and crypto funding.
//!
//! `#[ignore]`d like the rest of the live tests, and **read-only**: every route
//! here is a GET. Nothing that moves money or creates state is a candidate, and
//! nothing here should ever become one — minting a token or requesting a locate
//! is not something a capture run may do by accident.
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

/// Which endpoint a candidate lives on.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Host {
    /// The market data API.
    Data,
    /// The paper trading API. Live keys are refused by [`credentials`].
    Trading,
}

/// A route to try, with the API version it lives at.
struct Candidate {
    /// What the fixture is called.
    name: &'static str,
    /// Which endpoint it is on.
    host: Host,
    /// The version segment, which differs per route rather than per client.
    ///
    /// This is the whole reason the field exists: `/v3/calendar/{market}` sits
    /// beside a `v2` client and `/locates` beside the same one at `v1`. A live
    /// capture is the only check that a version segment is right, because a
    /// mock answers whatever it is pointed at.
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
        host: Host::Data,
        version: "v2",
        path: "/stocks/meta/exchanges",
        query: &[],
    },
    // `tape` is required here and *not* on the option equivalent below — an
    // asymmetry the spec's `/stocks/meta/conditions/{tick_type}` does not hint
    // at. A is NYSE, B is NYSE Arca and American, C is Nasdaq.
    Candidate {
        name: "stocks_meta_conditions_trade",
        host: Host::Data,
        version: "v2",
        path: "/stocks/meta/conditions/trade",
        query: &[("tape", "A")],
    },
    Candidate {
        name: "stocks_meta_conditions_quote",
        host: Host::Data,
        version: "v2",
        path: "/stocks/meta/conditions/quote",
        query: &[("tape", "A")],
    },
    Candidate {
        name: "options_meta_exchanges",
        host: Host::Data,
        version: "v1beta1",
        path: "/options/meta/exchanges",
        query: &[],
    },
    Candidate {
        name: "options_meta_conditions_trade",
        host: Host::Data,
        version: "v1beta1",
        path: "/options/meta/conditions/trade",
        query: &[],
    },
    // Forex.
    Candidate {
        name: "forex_latest_rates",
        host: Host::Data,
        version: "v1beta1",
        path: "/forex/latest/rates",
        query: &[("currency_pairs", "EURUSD,GBPUSD")],
    },
    Candidate {
        name: "forex_rates",
        host: Host::Data,
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
        host: Host::Data,
        version: "v1beta1",
        path: "/indices/latest/values",
        query: &[("symbols", "SPX")],
    },
    // Logos answer with an image rather than JSON, which the capture records
    // as `not_json` rather than pretending it is a fixture.
    Candidate {
        name: "logos_aapl",
        host: Host::Data,
        version: "v1beta1",
        path: "/logos/AAPL",
        query: &[],
    },
    // Doubles as an entitlement probe. The free tier serves IEX only, so a SIP
    // response proves a paid market-data plan and narrows any 403 below to a
    // per-product grant rather than the plan as a whole.
    Candidate {
        name: "stocks_bars_sip",
        host: Host::Data,
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
        host: Host::Data,
        version: "v2",
        path: "/stocks/auctions",
        query: &[
            ("symbols", "AAPL"),
            ("start", "2026-08-10"),
            ("end", "2026-08-11"),
            ("limit", "5"),
        ],
    },
    // ------------------------------------------------ single-symbol routes
    //
    // Documented as current, with their own response shape: a bare list and
    // the symbol beside it rather than a map keyed by symbol. Nobody's tests
    // cover them — the Go SDK's own single-symbol helpers call the *multi*
    // route — so these are the only payloads there will be.
    Candidate {
        name: "stocks_bars_single",
        host: Host::Data,
        version: "v2",
        path: "/stocks/AAPL/bars",
        query: &[
            ("start", "2026-08-10"),
            ("end", "2026-08-11"),
            ("timeframe", "1Day"),
        ],
    },
    Candidate {
        name: "stocks_quotes_single",
        host: Host::Data,
        version: "v2",
        path: "/stocks/AAPL/quotes",
        query: &[
            ("start", "2026-08-10"),
            ("end", "2026-08-11"),
            ("limit", "5"),
        ],
    },
    Candidate {
        name: "stocks_trades_single",
        host: Host::Data,
        version: "v2",
        path: "/stocks/AAPL/trades",
        query: &[
            ("start", "2026-08-10"),
            ("end", "2026-08-11"),
            ("limit", "5"),
        ],
    },
    Candidate {
        name: "stocks_auctions_single",
        host: Host::Data,
        version: "v2",
        path: "/stocks/AAPL/auctions",
        query: &[
            ("start", "2026-08-10"),
            ("end", "2026-08-11"),
            ("limit", "5"),
        ],
    },
    // The three "latest" siblings nest one record under a *singular* key, and
    // the snapshot has no wrapping key at all — four shapes across five routes.
    Candidate {
        name: "stocks_latest_bar_single",
        host: Host::Data,
        version: "v2",
        path: "/stocks/AAPL/bars/latest",
        query: &[],
    },
    Candidate {
        name: "stocks_latest_quote_single",
        host: Host::Data,
        version: "v2",
        path: "/stocks/AAPL/quotes/latest",
        query: &[],
    },
    Candidate {
        name: "stocks_latest_trade_single",
        host: Host::Data,
        version: "v2",
        path: "/stocks/AAPL/trades/latest",
        query: &[],
    },
    Candidate {
        name: "stocks_snapshot_single",
        host: Host::Data,
        version: "v2",
        path: "/stocks/AAPL/snapshot",
        query: &[],
    },
    // -------------------------------------------------------- trading API
    //
    // Read-only, and every one of them is a version check as much as a
    // payload: the trading client is v2 and none of these are.
    Candidate {
        name: "trading_calendar_market_v3",
        host: Host::Trading,
        version: "v3",
        path: "/calendar/XNYS",
        query: &[("start", "2026-08-10"), ("end", "2026-08-14")],
    },
    Candidate {
        name: "trading_locates",
        host: Host::Trading,
        version: "v1",
        path: "/locates",
        query: &[("limit", "5")],
    },
    Candidate {
        name: "trading_locate_quotes",
        host: Host::Trading,
        version: "v1",
        path: "/locates/quotes",
        query: &[("symbols", "TSLA,AAPL")],
    },
    Candidate {
        name: "trading_tokenization_requests",
        host: Host::Trading,
        version: "v2",
        path: "/tokenization/requests",
        query: &[],
    },
    // The reference gives these list-titled routes a *singular* response
    // schema, which reads like a documentation error. One live answer settles
    // it, and settling it is the point of capturing them.
    Candidate {
        name: "trading_wallets",
        host: Host::Trading,
        version: "v2",
        path: "/wallets",
        query: &[],
    },
    Candidate {
        name: "trading_wallets_transfers",
        host: Host::Trading,
        version: "v2",
        path: "/wallets/transfers",
        query: &[],
    },
    Candidate {
        name: "trading_wallets_whitelists",
        host: Host::Trading,
        version: "v2",
        path: "/wallets/whitelists",
        query: &[],
    },
];

fn credentials() -> Credentials {
    Credentials::from_env().unwrap_or_else(|e| {
        panic!("{e}\n\nSet APCA_API_KEY_ID and APCA_API_SECRET_KEY (paper keys) and re-run.")
    })
}

fn client(credentials: &Credentials, host: Host, version: &str) -> RestClient {
    let base = match host {
        Host::Data => BaseUrl::Data,
        // Paper, always. `just live` refuses a key that is not `PK`-prefixed
        // and this runs under the same rule; a capture against a live account
        // would be reading someone's real positions.
        Host::Trading => BaseUrl::trading(true),
    };
    RestClient::new(credentials, RestConfig::from(base).api_version(version)).expect("a client")
}

#[tokio::test]
#[ignore = "hits the live market data API; run with `just capture`"]
async fn capture_the_routes_no_sdk_tests_cover() {
    let credentials = credentials();
    let out = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/live");
    std::fs::create_dir_all(&out).expect("fixtures/live");

    let mut index: BTreeMap<String, serde_json::Value> = BTreeMap::new();

    for candidate in CANDIDATES {
        let rest = client(&credentials, candidate.host, candidate.version);
        let host = match candidate.host {
            Host::Data => "data",
            Host::Trading => "paper-trading",
        };
        let route = format!("/{}{}", candidate.version, candidate.path);

        let result = rest
            .request_raw(
                reqwest::Method::GET,
                alpaca_sdk::Replay::ByMethod,
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
                    json!({ "host": host, "route": route, "query": candidate.query, "status": "captured" })
                }
                Err(e) => {
                    // A 200 that is not JSON is itself worth recording: the
                    // logo route answers with an image, for instance.
                    println!("not json  {route}: {e}");
                    json!({
                        "host": host,
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
                    "host": host,
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
