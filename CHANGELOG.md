# Changelog

Notable changes to this crate, newest first.

**Inside `0.x`, these notes are the only mechanism there is.** Cargo treats a
`0.x.y` bump as compatible, so it reaches dependants without them choosing it,
and `cargo-semver-checks` — which runs in the release pipeline — only sees the
type level. A change that alters behaviour without altering a signature is
invisible to both. It gets written down here or it is not communicated at all.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

**Slated for `0.1.1`.** `cargo semver-checks` agrees there is no type-level
break. Read "The semver call" below before upgrading anyway: one behaviour
change here is invisible to the compiler, and a `0.1.1` arrives without you
asking for it.

### Added

- **A setter for every optional field on every request type** — 531 of the 547,
  across 129 types. `GetOrdersRequest` had fourteen filters and a setter for
  none of them; `UpdatableIdentity` had twenty-one and none; the five check
  types inside a CIP payload had sixty-eight between them.

  ```rust
  let orders = client
      .get_orders(
          GetOrdersRequest::default()
              .status(QueryOrderStatus::Open)
              .limit(50)
              .symbols(vec!["AAPL".to_owned()]),
      )
      .await?;
  ```

  **The assignment form still works, and always will.** These types are
  `#[non_exhaustive]` with public fields, so nothing was ever unbuildable:

  ```rust
  let mut request = GetOrdersRequest::default();
  request.status = Some(QueryOrderStatus::Open);
  request.limit = Some(50);
  ```

  Both build the same request — `tests/integration/request_construction.rs`
  asserts the two serialize identically, because a second way to build a
  request is only worth having if it is the same request.

  `String` and `Vec<T>` fields take `impl Into<T>`, so `.subtag("desk-7")` works
  without a `to_owned()`. Everything else takes its type exactly.

  Sixteen fields have no setter, deliberately, and assignment remains the way
  to reach them. Three because a *constructor* already holds the name and two
  `pub fn` of one name cannot coexist in one impl:
  `GetEventsRequest::since`, `EventStreamRequest::since` and
  `EstimateOrderRequest::notional`.

  The other thirteen because one setter writes them as a group, and offering one
  per field would make an incoherent request easy to build by accident. Ten are
  on `OrderRequest`, whose module documentation names three combinations the
  *type* makes unrepresentable — a per-field setter would have quietly undone
  two of them:

  - `qty` / `notional` are `OrderAmount`, and `trail_price` / `trail_percent`
    are `Trail`. Both exist so "both at once" cannot be expressed, and
    `OrderRequest::validate` does not reject either pair — precisely because
    the type made them unreachable.
  - `limit_price` / `stop_price` are set by the constructors for the shapes
    that have them. "`limit_price` is not supported for market orders" is
    enforced, in that module's own words, "by there being no way to set it on
    one".
  - `order_class`, `take_profit`, `stop_loss` and `legs` belong to `bracket`,
    `oco`, `oto_take_profit`, `oto_stop_loss` and `multi_leg`. An exit leg with
    no order class passes `validate` and is not a bracket — it is a plain order
    carrying a field Alpaca ignores.

  Two more are `CreateRecipientBankRequest`'s `routing_code` and
  `routing_code_type`, which go together because a routing code without its
  scheme is ambiguous. The last is `EventStreamRequest::since_id`, documented
  "mutually exclusive with `since`" and reachable through the `from_id`
  constructor.

  What earns a skip is narrower than "these fields interact". Exclusivity the
  type already *checks* does not: `GetAccountActivitiesRequest`'s `category` and
  `activity_types` are as mutually exclusive as `qty` and `notional`, and both
  keep their setters, because `validate` rejects the pair and the client calls
  it before sending. Ordering does not either — `start` and `end` keep their
  setters everywhere, including on the four types that also offer a fallible
  `between(start, end)`, because a one-sided window is ordinary and `between`
  cannot express one. Nor does a field that merely *requires* a companion:
  `EventStreamRequest::until` needs `since`, and says so in its own
  documentation, which the derive carries onto the setter.

  Purely additive. The 79 setters that already existed were written by hand and
  are now generated, keeping their names, their documentation and their
  behaviour. Nine *widened*, from `Vec<T>` to `impl Into<Vec<T>>` —
  `GetUsCorporatesRequest::{cusips, tickers}`,
  `GetAggregatePositionsRequest::symbols`, `GetSettlementsRequest::statuses`,
  `CorporateActionEventsRequest::types`, `NewsRequest::symbols`,
  `CorporateActionsRequest::{symbols, types}` and
  `UpdateWatchlistRequest::symbols` — so an array or a boxed slice works where
  only a `Vec` did before.

  **Those nine can break a call site that relies on inference**, even though
  nothing narrowed and `cargo semver-checks` reports no break — it models types,
  not inference. An argument whose type was previously deduced *from* the
  parameter now has nothing to deduce it from:

  ```rust
  // Compiled on 0.1.0, needs a type annotation now:
  request.symbols(boxed_slice.into())
  request.symbols(Default::default())

  // Both fine:
  request.symbols(vec!["AAPL".to_owned()])
  request.symbols(Vec::<String>::new())
  ```

  Written down here because this is the class of change `cargo-semver-checks`
  cannot see and a `0.1.1` reaches you without your asking.

  One parameter was renamed — `GetAggregatePositionsRequest::firm_accounts` took
  `include` and now takes `firm_accounts`, since the derive names a parameter
  after its field. Rust has no named arguments, so no call site changes; it is
  noted because it is visible in the documentation.

### Changed

- **`alpaca-sdk` is now a workspace, and publishes a second crate** —
  `alpaca-sdk-macros`, holding the `Setters` derive above. A procedural macro
  cannot live in the crate that uses it, which is the only reason it exists;
  nothing in it is meant to be named directly, and `alpaca-sdk` pins it with `=`
  so the two always resolve as the pair they were built as.

  **This costs a caller nothing to compile.** `syn`, `quote` and `proc-macro2`
  were already in the dependency tree by way of `serde`'s `derive` feature, so
  cargo unifies them and the only new work is the macros crate's own
  compilation.

  The alternative was a declarative macro listing each field beside its struct,
  which needs no second crate — and needs a script to check the list against the
  struct, because it can fall behind silently. Reading the real fields deletes
  that class of drift rather than reporting on it.

### Fixed

- **`TradeEvent` named twelve of the twenty-one documented `trade_updates`
  events**, and was ordered alphabetically because it had been ported from
  alpaca-py's `TradeEvent` rather than from Alpaca's own list. The port is the
  bug; the nine absent values were its symptom. Now added: `done_for_day`,
  `stopped`, `calculated`, `suspended`, `order_replace_rejected`,
  `order_cancel_rejected`, `trade_bust`, `trade_correct` and `held`.

  `order_replace_rejected` and `order_cancel_rejected` are the two that matter
  in practice. Alpaca files them under "rarer events", but a refused replace is
  routine for anything that reprices a resting limit order, and a refused
  cancel is routine for anything that races a fill. Both previously decoded to
  `TradeEvent::Unknown`, so a caller treating `Unknown` as "the API changed
  under me" — a conservative-looking reading — could halt an execution system
  on an ordinary event. Note the `order_` prefix on both: the wire values are
  not bare `replace_rejected` and `cancel_rejected`.

  That is what happens to a frame that decodes. Whether these two decode at all
  is a separate and still-open question — see the known limit below.

- **The variants are now in the order of Alpaca's `TradeUpdateEventType`
  schema**, matching how `OrderStatus` already followed its own. That is
  broadly lifecycle order rather than the "common, then rarer" split the prose
  uses — `accepted` is documented common but follows `rejected` and
  `pending_new` — so a variant's position is not a claim about frequency.
  `WIRE_VALUES` changes both contents and order.

- **`TradeEvent`'s documentation said "the values accepted by the API"**, which
  describes a request-side parameter list rather than the set of events the
  stream emits. It now says what it is, and points at the distinction from
  `OrderStatus` that the original port got wrong: `fill` is an event and
  `filled` is a status.

### Changed

- **`Unknown(String)` no longer documents itself as "Alpaca added something
  new."** It has always had a second meaning — this crate omitted a value
  Alpaca already documents — and `TradeEvent` is the proof that the second
  meaning is the likelier one. The `wire_enum!` documentation now says so, and
  says that escalating on `Unknown` is not the safe default it appears to be.

### Known limits

- **`TradeUpdate::timestamp` is required, and it is unconfirmed that every event
  carries one.** A `trade_updates` frame without it fails the whole decode.
  Alpaca's prose gives a `timestamp` meaning for six events — `fill`,
  `partial_fill`, `canceled`, `expired`, `replaced`, `rejected` — and so for
  neither `order_replace_rejected` nor `order_cancel_rejected`, while
  introducing the list with "the meanings have been specified here for which
  types the timestamp field will be present."

  The evidence points mostly the other way, and is recorded here so the limit is
  not read as worse than it is. `TradeUpdateEventV2`, the schema for the
  server-sent trade events endpoint, declares `timestamp` and every other field
  `TradeUpdate` carries — that struct's fields are a strict subset of it — and
  Alpaca's own worked example for that schema, `TradeUpdateEventV2New`, emits a
  `timestamp` for `new`, an event outside the documented six. So the prose most
  likely describes what the field *means* per event rather than when it is sent.
  Against that, the schema declares no `required:` list at all.

  It stays a known limit because none of that is the websocket message. That
  schema is `$ref`'d once, from the server-sent endpoint, and describes itself
  as "sent over the events streaming api"; no vendored source models the
  websocket frame directly, and nothing under `fixtures/` carries an `event`
  field. Pre-existing either way: `timestamp` was required before these variants
  existed, so such a frame failed identically on `0.1.0`, and naming the events
  does not change it.

- **The server-sent trade events stream does not model `reason`**, which the
  published reference documents on `TradeUpdateEventV2` and the vendored
  specification lacks. Its known values include `TOO_LATE_TO_CANCEL` for exactly
  the two rejection events named above — the cancel or replace lost the race
  against a fill — which is the discrimination a repricing strategy wants.
  `get_trade_events` yields the generic `BrokerEvent`, which keeps the payload
  as it arrived, so the field is unmodelled rather than lost — `SseEvent::json`
  into your own struct reads it today. Tracked rather than added: the reference
  says the field is not a closed vocabulary, so the choice between `String` and
  a `wire_enum!` deserves its own look. Note this is the server-sent payload,
  not the websocket one — no source seen here says `trade_updates` carries
  `reason`.

### Tooling

Not shipped in the crate, but it is why the bug above survived to `0.1.0`.

- **`just enums-drift` compared enums by name only**, so an enum this crate
  spells differently from Alpaca's schema was not reported as drifting — it was
  not reported at all. `TradeEvent` against `TradeUpdateEventType` is exactly
  that shape, so the report that exists to catch this had no opinion on it.
  `scripts/enum_drift.py` now carries an `ALIASES` map for such pairs, and
  fails loudly if either side of an alias stops resolving, since a stale alias
  and no alias produce the same silence.
- **`just enums-drift` could not run on Python 3.9**, which is what ships on
  current macOS: a backslash inside an f-string expression is a syntax error
  before 3.12. The step `.github/CONTRIBUTING.md` requires after touching a
  wire enum was therefore not performable, which is the other half of why
  nothing caught this.

  It is runnable now, not enforced. It needs `specs/`, which is gitignored and
  fetched by `just specs`, so it cannot join `just check` or CI without putting
  a network download in the gate. It remains a step someone has to take.

Those two were what the `TradeEvent` fix needed. Reviewing the report while
verifying that fix turned up more, all of the same shape — a partial answer
presented as a whole one:

- **It only read files named `*enums*.rs`**, 73 of the crate's 120 shipping
  `wire_enum!` blocks; the rest sit beside the models that use them. Ten enums
  that do have a spec schema had therefore never been compared to it. None had a
  missing value, but `TokenizationNetwork` carries `cronos` and `hyperevm`
  beyond its schema and had simply never been looked at. Reading every file
  means excluding `#[cfg(test)]` ones, or the undercount is merely traded for an
  overcount: `wire_tests.rs` declares a `Side` that ships to nobody.
- **A name declared twice was silently collapsed**, on both sides.
  `ActivityCategory` and `TransferDirection` are each declared on two surfaces,
  and six schema names — including `OrderSide`, 9 values in `broker.yaml` against
  2 in `trading.yaml` — are defined differently in two specs. Each half covered
  the other's gaps: deleting a value from one `TransferDirection` left it
  reported as agreeing exactly. Two `wire_enum!`s under one name now get no
  verdict, since the report cannot hold both; an ordinary `pub enum` that merely
  shares a name is not a collision, because withholding a verdict over that
  would be worse than the narrow glob it replaced. Spec-side collisions are
  compared but flagged, because a union can only add values Alpaca documents
  somewhere, so a gap against it is still real while surplus is not
  trustworthy — and an enum with both a gap and a suppressed surplus says so,
  rather than printing the gap as though it were the whole verdict.
- **The report now lists what it could not check.** Of 118 enums, 28 get a
  verdict and 90 do not — 88 with no schema this parser can use, plus
  `Exchange`, suppressed by `NOT_DRIFT`, and `TransferDirection`, declared
  twice. The 88 split again by what a reader could do about them: 83 have no
  schema of that name, so an `ALIASES` pair would start checking them, while 5
  have one that carries no readable value list — four documenting their values
  in `description` prose, `LocateQuoteError` on a property — where aliasing is
  a no-op. It also separates "agree exactly" from "agree apart from values
  recorded below", which two enums only qualified for, and prints the buckets
  summed against the compared count. That sum is a guard against a future edit
  rather than a finding: as the branches stand each compared enum reaches
  exactly one bucket, so it cannot currently fail.
- **Deliberate crate-only values can be recorded.** `restated` and `held` were
  listed under "do not delete these, Alpaca still serves values it has stopped
  documenting" — true in general and not why those two are there. A `CRATE_ONLY`
  map carries the real reason. `TaxIdType`'s `ARG_AR_CUIT` sat under the same
  wrong sentence and now carries its own: a suspected typo, not a value Alpaca
  stopped documenting.
- **The suppression maps go stale silently**, so `ALIASES`, `CRATE_ONLY` and
  `UNRESOLVED` now fail the run or stop printing once the state they describe
  no longer holds. `DECIDED` and `NOT_DRIFT` can only have their keys checked —
  the first names a value the crate deliberately does not carry, and the
  second's claim is that two vocabularies are unrelated, which no diff
  confirms — so both fail on an enum name that has gone, and a `NOT_DRIFT` pair
  that comes to match value for value asks to be rechecked.
- **None of it had a test**, which is how four of the defects above reached a
  commit. `scripts/tests/test_enum_drift.py` covers each one, and it is in
  `just check` and CI even though `just enums-drift` is in neither. That is not
  a contradiction: running the report needs Alpaca's `specs/`, gitignored and
  fetched over the network, while testing the parser needs a synthetic tree the
  tests write for themselves. The logic deciding which enums the report can see
  at all does not have to stay unverified because the report does. Stdlib
  `unittest`, so the crate's gate gains no Python dependency.

### The semver call

**The call is `0.1.1`.** `cargo semver-checks` reports no breakage, and is right
not to: the enum is `#[non_exhaustive]`, so every caller already needs a
wildcard arm, and adding variants is additive at the type level.

One behaviour change is not additive, and no compiler will point at it. On
`0.1.0` the only way to handle any of these nine events was to match the
catch-all by string:

```rust
TradeEvent::Unknown(s) if s == "done_for_day" => { … }
```

That arm still compiles and now never fires, because the value arrives as
`TradeEvent::DoneForDay`. The same applies to all nine, `order_replace_rejected`
and `order_cancel_rejected` included. `WIRE_VALUES` also changes both contents
and order, so anything asserting on it by index will move.

`0.1.1` reaches every `alpaca-sdk = "0.1"` dependant without them choosing it,
which is exactly why this note is here and stated this plainly: per the preamble
at the top of this file, inside `0.x` these notes are the only mechanism there
is. **If you match on `TradeEvent::Unknown` by string, grep for it before
taking this upgrade.**

### On the two values with weaker evidence

`restated` and `held` are described in **prose** and appear in neither list of
trade event values: the `TradeUpdateEventType` schema and the published
reference both enumerate the same nineteen, and neither of these is among them.
(`held` does appear in the `OrderStatus` schema — as a status, which is part of
why it is the weaker of the two.)

They are not corroborated by two sources. The published reference re-serves the
same OpenAPI document the specification comes from, so the agreement is one text
quoted twice. `restated` appears in two prose passages of it; `held` appears in
exactly one, and in none of the schema's own description. `held` is the thinnest
claim in the enum and its doc comment says so.

They are carried on that basis: a value the crate does not name is one no caller
can match on, whereas a variant Alpaca never sends costs a dead match arm. Each
says it is prose-only in its own documentation rather than being presented as
wire-verified.

`just enums-drift` records the same reasoning through its `CRATE_ONLY` map, so
the pair reads as a decision rather than as unexplained drift on every run. They
had been appearing under "in the crate, not in the spec", whose heading tells you
not to delete a value because Alpaca still serves ones it has stopped
documenting — true in general, and not the reason these two are there.

`held` is also an `OrderStatus` value, so it may be a status that leaked into an
event list rather than an event in its own right.

**The port was wider than this enum.** `TradeEvent` was reconciled because it
had a reported defect, but it is one of twenty-four enums in
`src/trading/enums.rs`, and all twenty-four sit in alpaca-py's declaration
order, contiguously — the only divergence is `ContractType`, which this crate
keeps in `types`. The file was transcribed, not just this type. Most of its
neighbours have no spec schema for `just enums-drift` to check them against, so
they sit in the same silence `TradeEvent` did. Nothing here says they are wrong;
it says they are unverified, and the one that was checked turned out to be
missing nine values. That is tracked separately rather than fixed here.

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
