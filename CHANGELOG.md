# Changelog

Notable changes to this crate, newest first.

**Inside `0.x`, these notes are the only mechanism there is.** Every `0.x` bump
is permitted to break, so `cargo-semver-checks` — which runs in the release
pipeline — has nothing to assert until `1.0`. A breaking change that no
compiler will point at gets written down here or it is not communicated at all.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

A pre-release audit of the whole crate, and the fixes it produced. Nothing here
has shipped, so none of it is a break from a published version — but the
behaviour changes are worth reading before the first release rather than after.

### Fixed — correctness

- **Path segments are percent-encoded.** Every route interpolated caller-supplied
  text into its path raw, so a crypto pair split into two segments and *no crypto
  position could be read or closed*: `close_position("BTC/USD")` addressed
  `/v2/positions/BTC/USD` and 404'd. Alpaca's reference asks for the encoded form
  (`/v2/assets/BTC%2FUSDT`) and now gets it. The same defect let `..` in a symbol
  reach a route the caller never named — `close_position("../positions")` issued
  `DELETE /v2/positions`, the close-all route, and returned a normal-looking
  success. A segment that is exactly `.`, `..`, or empty is now refused with
  `Error::InvalidRequest`, because no encoding survives a URL parser's dot-segment
  removal.
- **A 504 no longer replays a `POST` or `PATCH`.** The retry policy was a set of
  status codes and nothing else, so one gateway timeout on `submit_order` sent
  four orders and then handed back `RetriesExhausted` — telling the caller none
  had been placed. Retries are now gated on the method as well: idempotent
  methods replay, `POST` and `PATCH` do not, and a 429 replays whatever the
  method because the rate limiter refuses the request before anything acts on it.
- **Credentials no longer follow a cross-host redirect.** The event-stream client
  put the Alpaca key pair in `default_headers` and allowed ten redirects. reqwest
  strips `Authorization` on a cross-host hop and nothing else, so the custom
  `APCA-API-*` headers rode along. **No stream client follows a redirect now** —
  no SSE route redirects, so following one was surface without upside. The broker
  keeps a second client that does follow them, for the one route that needs it:
  the document download answers `301` to a presigned storage URL. That is safe
  because those credentials are basic auth, which reqwest does strip.
- **`RestConfig::timeout` is no longer applied to event streams.** It is a total
  deadline on a whole request, and an event stream's body never finishes, so
  setting it gave every subscription a fixed lifespan: events until the deadline,
  then a timeout, forever. The broker keeps a second client for the document
  download, which *is* an ordinary request/response call and still honours the
  deadline — the two were one client before, and only one of them wanted it.
- **`data_timeout` is measured, not guessed.** Both websocket loops treated one
  elapsed 5-second socket read as the staleness signal, so any `data_timeout`
  above five seconds behaved as exactly five — an overnight stock stream
  reconnected every five seconds against an endpoint that allows one connection
  per account. There is now a real clock, reset by market data.
- **A `null` symbol no longer fails the whole market-data response.** Alpaca
  answers `null` for a symbol it has nothing for, and that error propagated and
  discarded every good symbol beside it. A request takes up to 100 symbols, so
  one delisted ticker made the batch unusable. The crate's own shipped fixture
  `fixtures/go/marketdata__test_snapshots__01.json` is exactly this shape.
- **`CorporateActionsRequest` walks every page again.** Its `limit` defaulted to
  1,000 — the endpoint's own page size — and `limit` caps the *total* across all
  pages, so page one filled the cap and the walk ended there with the
  `next_page_token` discarded and unrecoverable.
- **`Calendar` round-trips.** `Serialize` was derived over a hand-written
  `Deserialize`, so `to_string` → `from_str` failed and caching a calendar did
  not work. It is now the deserializer's inverse.
- **`AccountConfiguration` no longer PATCHes `null`.** Three optional fields had
  no `skip_serializing_if`, so the only possible usage pattern —
  read-modify-write, forced because every other field is non-`Option` — sent two
  fields the current schema does not document and a `null` into an integer enum.
- **`JitReport` can fail again.** `JitReportInline` accepted any JSON object and
  produced an all-`None` value, so the untagged enum could never fail and a
  settlement report came back silently empty. It now errors when no known report
  key is present, and carries the three report types that had no field at all.
- **The crypto funding list routes decode.** Three broker routes and their three
  trading twins were typed as a single object against specs that say "an array
  of…", so every call failed to decode. They now return `Vec` and accept both
  shapes, since no payload has ever been observed — see the note below.
- **`capped_delay` no longer panics.** `Duration::MAX` is reachable through the
  public `RetryBackoff::Exponential { max }`, and the arithmetic produced a value
  `Duration::from_secs_f64` rejects — a panic inside an async task.
- **The blocking façade works from `spawn_blocking`.** Its guard tested for an
  ambient runtime handle, which is present both inside an async fn (where
  blocking panics) and inside `spawn_blocking` (where it is fine). The supported
  bridge from an async program into the façade was the one path it rejected.

  The guard is now tokio's own answer, caught and converted. **On a
  `panic = "abort"` profile that recovery does not exist**, so calling the façade
  from an async context aborts the process where it used to return an error.
  There is no cheaper pre-check — nothing distinguishes an async fn from a
  `spawn_blocking` closure — so this is a deliberate trade of a misuse-path
  error for the supported path working. It is written up on `Blocking` itself.
- **`oto_take_profit` and `oto_stop_loss` replace rather than accumulate.**
  Chaining both produced an `oto` order carrying two exit legs that `validate`
  accepted.
- **Empty `Symbols` is refused locally** instead of issuing `?symbols=`.
- **`Error::Decode` carries the route and the payload** everywhere in the
  market-data client, including the three single-symbol "latest" routes and the
  news route, where it used to carry an empty body. The payload is now borrowed
  rather than cloned, so a successful multi-symbol response no longer pays for an
  error path it did not take.
- **The reconnect curve resets on a session that did its job.** The counter was
  incremented immediately *after* a successful connect and reset only by an
  inbound message, so a few clean server-side recycles pushed the delay to its
  30-second ceiling — and on the trade-update stream, whose own docs call a
  silent account normal, it never reset at all. Resetting on *connect* would go
  too far the other way: it pins the delay at its minimum against a server that
  accepts and immediately hangs up, about one connection a second at an endpoint
  that allows one per account. So a session clears the count when it delivered
  data or stayed up past `stable_session`.
- **`get_option_chain` and `get_market_movers` encode their path segments.** Both
  interpolate a request *field* rather than a bare argument, so the first
  encoding sweep missed them.
- **`JitReport` reports the real decode error too.** It was the other untagged
  enum on an unobserved route, and it was discarding the carefully worded error
  its own inline arm produces.
- **The broker document download retries like every other route.** Its
  hand-rolled loop ignored `Retry-After` and waited a flat interval instead of
  the backoff curve.
- **`get_latest_*_for_symbol` reports a `null` payload as an absent record**
  rather than a decode mismatch. It returns one record, so it cannot skip the way
  the three sibling helpers do — but the error now says the response carried
  nothing for that symbol instead of blaming the shape.

### Added

- `TradingClient::get_all_orders` and `BrokerClient::get_all_orders_for_account`,
  which walk `/v2/orders` and its broker twin with the `before_order_id` cursor.
  `get_orders` returns one page — 50 by default, 500 at most — and said so
  nowhere, so an account with more history than that reconciled against a
  silently truncated list. The walk deduplicates on order id rather than assuming
  the cursor is exclusive, because Alpaca's cursors are inclusive on some routes
  and the reference does not say which this is.
- `StreamConfig::stable_session` and `TradingStream::stable_session`, which set
  how long a connection must stay up before it clears the reconnect failure
  count. Defaults to 30 seconds.

### Changed — API surface

Decisions that become expensive after the first release, settled now.

- **Response models are `#[non_exhaustive]`,** across every module rather than
  the three model files — 88 further types, including all fourteen corporate
  action shapes and the funding, JIT, reporting, IPO, OAuth and locate responses.
  Request structs carry it too, as CONTRIBUTING states — the exemptions are
  clients, and the caller-constructed value types where the attribute costs
  something and buys nothing: `OrderAmount`, `Trail`, `StopLimit`, `Symbols`,
  `TimeFrame`, `Codes`, `SettlementTransfer`, `JitSettlementAccount`,
  `TransmitterInfo`, `W8BenDocument` and `StreamConfig`. `CHANGELOG` stated the policy as a guarantee; `trading/models.rs`, `broker/models.rs` and `data/models.rs` had it
  on nothing. Alpaca adds fields without a version bump, and without the
  attribute the crate cannot follow an *additive* upstream change without a major
  release of its own. Several request-body components gain constructors as a
  result: `Agreement::new`, `ManualACHRelationship::new`,
  `PlaidACHRelationship::new`; `Contact`, `Identity`, `Disclosures`,
  `UpdatableContact`, `UpdatableIdentity` and `BankAddress` are built from
  `Default` and assigned by name. `W8BenDocument` is deliberately left exhaustive:
  it transcribes an IRS form, and eleven required fields would make a constructor
  a row of interchangeable strings.
- **`Channel::ALL` is `&'static [Channel]`,** not `[Channel; 11]`. The array put
  the variant count in a public type signature.
- **Several public enums are now `#[non_exhaustive]` too,** which is a separate
  break from the struct sweep: `Channel`, `Activity`, `ClosePositionBody`,
  `JitReport` and `RebalancingSubType` now require a wildcard arm in an external
  `match`. Alpaca has added stream channels and activity kinds without a version
  bump before, and each addition would otherwise be a major release here.
- **`OrderRequest::stop_limit` takes a `StopLimit { stop, limit }`** instead of
  two adjacent bare `Decimal`s in the reverse of the field order used everywhere
  else. Transposing them compiled and produced a legal order Alpaca accepted, so
  nothing errored anywhere — a protective sell stop-limit simply armed a dollar
  late and rested above its trigger.
- **`Error::Transport` carries `TransportError`,** an opaque newtype, rather than
  `reqwest::Error`. reqwest is a `0.x` crate, so exposing it made every
  `0.13 → 0.14` bump a breaking change here, for a dependency unrelated to
  Alpaca. `is_timeout`, `is_connect`, `is_body`, `is_decode`, `status`, `url` and
  `source` are forwarded.
- **`Error::is_retryable` is now `Error::is_transient`.** The classification is
  unchanged; the name was the problem. "Retryable" reads as "safe to send again",
  which no error value can answer — a timed-out `POST` and a 504 on one are both
  transient and both indistinguishable from a request the server accepted. The
  safety question belongs to the method, and the docs now say so.
  `ApiError::is_retryable` is now `ApiError::is_retried_by_default`, which is
  what it measured — the default status set, not the policy the client was built
  with.
- **`EventStreamRequest::after_id` is now `from_id`**, and
  `GetEventsRequest::after_ulid` is now `from_ulid`. "After" asserted an
  exclusivity only some of these streams have: Alpaca documents `since_id` as
  inclusive for corporate actions and exclusive for IPO events. Deduplicate on
  the event id.
- **`currency` is `SupportedCurrencies` everywhere,** rather than the enum on
  some types and `String` on others — including on the one you read back after
  setting it, where the mismatch turned a compile error into a string comparison
  that quietly evaluated false. `CreateWithdrawalRequest::desired_currency` and
  `FundingWalletTransfer::original_currency` are included; the first is the write
  path for money leaving an account.
- **`strike_price_gte` / `strike_price_lte` are `Decimal`** on the market-data
  request, matching the trading request and the model they filter. They were the
  only `f64` money fields in the request surface.
- **`GetAggregatePositionsRequest::firm_accounts` is `Option<bool>`.** It was a
  comma-separated id list; Alpaca parses it as a boolean, so the report came back
  silently missing the firm inventory.
- **`Disclosures::employment_status` is `EmploymentStatus`**, and the market-data
  exchange fields are `data::Exchange`. Both enums existed, were exported, and
  were referenced by nothing.
- **`trading::AllAccountsPositions` is removed.** It duplicated the broker type,
  which is the one any route actually returns — and the broker's is the more
  tolerant of the two, carrying `#[serde(default)]` on `positions` where the
  trading copy did not.
- **`StreamConfig`'s knobs are all private,** set through `StreamConfig::backoff`
  and `StreamConfig::data_timeout`, which reject a zero value; read back through
  `min_backoff()`, `max_backoff()`, `data_timeout_after()` and
  `stable_session_after()`. Zero `min_backoff` looked like "reconnect
  immediately" and was a hot loop, and a validator a field assignment can step
  around is not a validator — `data_timeout` was still `pub` and had the same
  hole. `TradingStream` gained `backoff` and the data streams gained `backoff`
  and `stable_session`, so the two stream surfaces configure the same way.
- **`CryptoDataStream::new` and `OptionDataStream::new` return `Result`,**
  matching `StockDataStream::new`. A `wire_enum`'s `Unknown(String)` variant is
  publicly constructible, so the feed name reached the endpoint URL unchecked.
- **`AssetIdent::as_path_segment` returns `Result<String>`** and encodes. It was
  `self.to_string()` — a no-op that no route called.
- `Event` and `JitReportInline` are `#[non_exhaustive]`.

### Documented

- `Event` now states what the SSE transport cannot tell you: Alpaca's
  "dropped 10000 messages" and "internal server error" notices arrive as comment
  lines, which the parser discards before this crate sees them — and a stream
  ending is indistinguishable from a stream failing, so `None` does not mean
  "you have everything".
- `types::decimal` states the real precision limit. Alpaca sends derived figures
  — a `cost_basis` with thirty significant digits — beyond `Decimal`'s
  twenty-eight, and those are rounded rather than refused, because refusing them
  would make an already-captured response undecodable. The exactness promise
  holds for what Alpaca *quotes*.
- The fixture corpus is 227 files and 232KiB, not the 135 recorded in three
  places. `RELEASING.md` names `all checks` rather than "the 9 required checks".

### Dependencies

- **`percent-encoding` added**, for the path segment encoder.
- **polars gains its `fmt_no_tty` feature** on the optional `polars` feature,
  which pulls in `comfy-table` and `unicode-width`. Without it a `DataFrame`
  printed its shape and then "to see more, compile with the 'fmt' or
  'fmt_no_tty' feature" — every value suppressed, including in this crate's own
  `ToFrame` example. `DataFrame`'s `Display` output therefore changes for anyone
  using the `polars` feature.

### Known gaps

- **The crypto funding routes are still unverified against a live payload.**
  Nothing in this repository has ever decoded one — the route smoke tests mount a
  404, and the live capture is recorded as `refused`. The `OneOrMany` decoder
  accepts both documented shapes so that no guess can be wrong, but the field
  models behind them remain spec-derived.
- **No `Idempotency-Key`.** Not replaying a `POST` closes the defect this crate
  introduced; it does not protect a caller who retries one themselves. Alpaca's
  reference asks for the header on journals, and there is no way to send one yet.

## [0.1.0] — unreleased

The first real release.

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
  OAuth, plus nine server-sent-event streams (account status, trades, journals,
  transfers, non-trading activity, activities, admin actions, IPOs and system
  events).

**251 of the 253 routes the vendored specs document.** The two exceptions are
deliberate skips, each recorded with its reason in
[COVERAGE.md](COVERAGE.md) — a route decided against must not keep reading as a
gap.

### Behaviour worth knowing before you depend on it

- **Money that crosses the wire as a string is `rust_decimal::Decimal`.**
  Alpaca sends order quantities and prices as strings and market data as JSON
  numbers, so the deserializer accepts both and market-data floats stay `f64`.
  Reading a string price as a float loses precision.
- **Unknown enum values deserialize into `Unknown` rather than failing.** Alpaca
  adds values without warning, and a new order status should cost a caller a
  match arm rather than a decode.
- **Unknown response fields are ignored.** Alpaca sends fields no model declares.
- **Paginated endpoints offer two methods** — `get_x` for one page, `get_all_x`
  to walk every page with an optional cap.
- **Retries follow Alpaca's own rate-limit guidance**: 429 and 504, three
  retries after the first request, waiting about a second and doubling to a
  30-second ceiling, jittered. A response carrying `Retry-After` overrides the
  curve, clamped to that ceiling; only the delta-seconds form is read, and an
  HTTP-date is treated as absent.
- **Request structs and `RestConfig` are `#[non_exhaustive]`.** Build with `new`
  or `default` and assign fields. This is what lets a newly documented query
  parameter arrive as a field rather than as a breaking change — which has
  already happened five times.
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

### If you pinned `0.1.0-alpha.1`

That version was a rehearsal of the release pipeline, published before most of
this crate existed — forty commits and three development phases separate it from
`0.1.0`, including the entire broker API expansion. It is not a useful baseline,
and it will be yanked once `0.1.0` is out. Upgrade rather than diff.

The changes most likely to surprise you, none of which a compiler will point at:

- Retries wait 1 second and double, rather than a flat 3 seconds three times.
- `Error::InvalidRequest` no longer means a dead stream. A websocket or SSE
  failure on the wire is `Error::Stream`; a failure the crate determines locally
  before any network call — an empty subscription set, a non-positive timeout —
  stays `InvalidRequest`.
- A malformed market data *response* is now `Error::Decode`, carrying the
  offending payload, where it used to be `InvalidRequest`. Code matching
  `InvalidRequest` to catch a response it could not read needs `Decode` now.

And the ones a compiler will:

- `RetryConfig`, `RestConfig` and every request struct are `#[non_exhaustive]`,
  so struct literals and `..Default::default()` no longer work from outside this
  crate. Construct with `new` or `default` and assign fields.
- Two client methods changed signature and five request structs gained fields
  when twelve documented query parameters that were never being sent were added.

### Packaging

The published tarball ships `src/`, `tests/` and `fixtures/` — the tests are
shipped runnable, which is why the 232KiB of captured payloads they read ship
with them. `scripts/`, `RELEASING.md` and `.github/` are excluded: they cannot
run, or have no meaning, outside a clone. `ROADMAP.md` was a working document
for building the crate and has been removed; it is in the git history.

[Unreleased]: https://github.com/phillip-simons/alpaca-sdk/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/phillip-simons/alpaca-sdk/releases/tag/v0.1.0
