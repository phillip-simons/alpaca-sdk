# Changelog

Notable changes to this crate, newest first.

**Inside `0.x`, these notes are the only mechanism there is.** Every `0.x` bump
is permitted to break, so `cargo-semver-checks` — which runs in the release
pipeline — has nothing to assert until `1.0`. A breaking change that no
compiler will point at gets written down here or it is not communicated at all.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

Nothing yet.

## [0.1.0] — 2026-08-14

The first real release: an unofficial Rust SDK for the Alpaca trading, market
data and broker APIs, in one crate with three API surfaces behind cargo
features.

### Surfaces

- **Trading REST** — orders, positions, assets, watchlists, the clock and
  calendar, account configuration and activities, corporate actions, options
  contracts, crypto funding, locates and tokenization.
- **Historical market data** — stocks, options, crypto, forex, news, screeners
  and metadata, with the pagination cursor walked for you.
- **Live market data streams** — the msgpack websocket for stocks, options,
  crypto and news, with a reconnect machine that replays subscriptions.
- **Trade update stream** — the JSON websocket for order lifecycle events.
- **Broker API** — accounts, onboarding, documents, funding, journals,
  rebalancing, instant funding, JIT, FPSL, funding wallets, IPOs, reporting and
  OAuth, plus nine server-sent-event streams.

**251 of the 253 routes the vendored specs document.** The two exceptions are
deliberate skips, each recorded with its reason in
[COVERAGE.md](COVERAGE.md) — a route decided against must not keep reading as a
gap.

### Behaviour worth knowing before you depend on it

- **Money that crosses the wire as a string is `rust_decimal::Decimal`.** Alpaca
  sends order quantities and prices as strings and market data as JSON numbers,
  so market-data floats stay `f64`. Reading a string price as a float loses
  precision.
- **Unknown enum values deserialize into `Unknown` rather than failing**, and
  **unknown response fields are ignored.** Alpaca adds both without warning, and
  a new order status should cost a caller a match arm rather than a decode.
- **Most paginated endpoints offer two methods** — `get_x` for one page,
  `get_all_x` to walk every page with an optional cap. Not every paginated route
  has a walker, and `get_all_x` does not always mean "walk": some are
  single-request routes named for the endpoint. Each method's own documentation
  says which it is.
- **Retries follow Alpaca's own rate-limit guidance**: 429 and 504, three
  retries after the first request, waiting about a second and doubling to a
  30-second ceiling, jittered. A response carrying `Retry-After` overrides the
  curve, clamped to that ceiling; only the delta-seconds form is read, and an
  HTTP-date is treated as absent.
- **Request structs and `RestConfig` are `#[non_exhaustive]`.** Build with the
  constructor the type provides — `new`, `default`, or a named one where the
  shape depends on the choice, as with `OrderRequest::limit` and
  `CreateJournalRequest::cash` — then assign fields. This is what lets a newly
  documented query parameter arrive as a field rather than as a breaking change.
- **`request_raw` is the escape hatch** for routes this crate does not wrap.

### Features

`trading` and `data` are on by default, with `rustls-tls`. `broker`, `blocking`,
`polars` and `native-tls` are opt-in. Streams stay async even under `blocking`:
a blocking iterator over a live feed deadlocks as soon as the caller is slower
than the socket's read buffer.

### Minimum supported Rust version

**1.88.** Enabling `polars` raises it to 1.95, which is why that feature is off
by default — a convenience feature does not get to set the crate's floor.

### Known limits

These are properties of what could be verified, not of what was implemented.

- **The broker routes have never met a live server.** This account has no broker
  sandbox key, so all 153 are verified against captured payloads, the published
  reference and the vendored specs — never against a response. Treat a decode
  failure on a first real payload as expected work rather than a regression.
- **The `CIP*` models are unverified and probably unverifiable.** alpaca-py's own
  comment says the sandbox answers 404 for those routes.
- **Forex, indices and logos answer `403 insufficient grants`** on a plan that
  reaches SIP, so they are per-product entitlements. The models follow the
  published reference.
- **Locates, tokenization and crypto funding answer 404 on the paper API**,
  which is a different kind of unverified from a 403.
- **The crypto funding routes are still unverified against a live payload.**
  Nothing in this repository has ever decoded one — the route smoke tests mount a
  404, and the live capture is recorded as `refused`. The `OneOrMany` decoder
  accepts both documented shapes so that no guess can be wrong, but the field
  models behind them remain spec-derived.
- **No `Idempotency-Key`.** The crate declining to replay a `POST` does not
  protect a caller who retries one themselves. Alpaca's reference asks for the
  header on journals, and there is no way to send one yet.

### If you pinned `0.1.0-alpha.1`

That version was a rehearsal of the release pipeline, published before most of
this crate existed — forty commits and three development phases separate it from
`0.1.0`, including the entire broker API expansion. It is not a useful baseline,
and it will be yanked once `0.1.0` is out. Upgrade rather than diff.

### Packaging

The published tarball ships the crate and everything needed to check it: `src/`
and `build.rs`, `examples/`, and `tests/` with `fixtures/` — the tests are
shipped runnable, which is why the 232KiB of captured payloads they read ship
with them. `Cargo.lock` pins a build that is known to work, `justfile` and
`deny.toml` are the commands and the licence policy those checks run under, and
`LICENSE`, `NOTICE`, `README.md`, `CHANGELOG.md` and `COVERAGE.md` are the
crate's own record.

Excluded are `scripts/`, `RELEASING.md`, `.github/` and `.githooks/`: they
cannot run, or have no meaning, outside a clone — `scripts/` needs other SDKs'
checkouts and downloaded specs, and the rest describe publishing this crate or
developing against it.

[Unreleased]: https://github.com/phillip-simons/alpaca-sdk/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/phillip-simons/alpaca-sdk/releases/tag/v0.1.0
