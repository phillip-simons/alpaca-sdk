# Roadmap and working notes

Where the port stands, what is left, and the things that are easy to get wrong.
Written to be picked up cold.

## Status

| Phase | State | Contents |
|---|---|---|
| 0 — Foundation | ✅ | Transport, auth, retry, errors, backoff |
| 1 — Type vocabulary | ✅ | 71 generated enums, `Decimal`, `TimeFrame`, `AssetIdent` |
| 2 — Trading REST | ✅ | 32 routes, 20 models, request builders |
| 3 — Historical data | ✅ | 6 clients, 26 methods, the pagination loop |
| 4 — Live market data | ✅ | msgpack websocket, 4 streams, reconnect machine |
| 5 — Trade updates | ✅ | JSON websocket, single channel |
| 6 — Broker | ✅ | 75 routes, 20 models, 4 pagination schemes, 5 SSE streams |
| **6.5 — Exceed alpaca-py** | **⬜** | API gaps; see below |
| 7 — Polish | ⬜ | polars, blocking façade, docs, migration guide, 1.0 |

Ported against alpaca-py `cc4cb3b`. `just pinned` reports drift against a local
alpaca-py checkout.

## Phase 6, as built

All 76 of alpaca-py's broker routes are ported, bar one: `delete_account`, which
alpaca-py deprecates and forwards to `close_account`. One route, one method.

The broker spec has **154 operations**; alpaca-py has 76. The remaining 78 are
Phase 6.5's business.

**Count the routes, not the sections.** This section once claimed accounts and
trading-on-behalf were finished while 16 order, asset, announcement and
account-lifecycle routes were missing. The check that would have caught it, and
the one to re-run before believing this file:

```sh
diff <(grep -oE '^    def [a-z_0-9]+' ../alpaca-py/alpaca/broker/client.py \
        | sed 's/    def //' | grep -v '^_' | sort) \
     <(grep -oE 'pub (async )?fn [a-z_0-9]+' src/broker/client.rs \
        | sed 's/.*fn //' | sort -u)
```

### Still open from Phase 6 (nothing route-shaped)

- `Weight::percent` is rounded to 2dp by the two constructors only. alpaca-py
  rounds in a field validator, so it also rounds percentages *Alpaca sent*;
  this port does not, on the grounds that editing a server's own numbers on the
  way in is worse than the divergence. A `percent` assigned directly to the
  field is likewise not rounded.
- **The REST retry ignores Alpaca's own advice.** The rate-limit documentation
  says to retry 429s "using exponential backoff", doubling from ~1s with jitter.
  `RetryConfig` waits a flat 3 seconds, inherited from alpaca-py. `backoff.rs`
  already implements exponential-with-jitter for stream reconnects, so the
  machinery exists; the REST path just does not use it. Changing the default
  changes how callers behave under load, so it wants a deliberate release rather
  than a drive-by fix.
- `Error` has no variant for a stream that breaks mid-flight. Both the SSE
  streams and the websocket code report those as `InvalidRequest`, which is
  wrong in the same way for both. Worth one variant covering the two.

### The broker's models are not the trading models

Three of them extend their trading counterparts, and returning the trading type
silently drops the extra fields:

- `broker::Order` = `trading::Order` + `commission`
- `broker::TradeAccount` = `trading::TradeAccount` + 12 fields (`cash_withdrawable`,
  the `last_*` previous-session values, `clearing_broker`)
- `broker::OrderRequest` = `trading::OrderRequest` + `commission` + `currency`,
  and a non-USD order must be a market order
- `BatchJournalResponse` = `Journal` + `error_message`

They use `#[serde(flatten)]` over the trading struct rather than transcribing it,
which is the closest thing to alpaca-py's subclassing. Check for this whenever a
broker route appears to reuse a trading model.

### Four pagination schemes, not one

The broker API pages four different ways, and the fixtures disagree with each
other about which:

- **Offset**, no envelope, empty array to stop: transfers.
- **Page token**, envelope (`{"subscriptions": [...], "next_page_token": …}`),
  absent token to stop: rebalancing subscriptions and runs.
- **Cursor**, no envelope, where the cursor is the last item's own `id`: account
  activities. Nothing in the response says there is more; an empty array stops it.
- **None at all**, bare array: rebalancing portfolios.

### The event streams are not the websocket streams

The five SSE endpoints are plain HTTP streams of `text/event-stream`: no
subscribe message, no auth handshake, no reconnect machine. They live in
`broker/events.rs` and share nothing with `data/live`.

Two details the SSE specification decides, not Alpaca:

- **The last event id persists.** An event that sends no `id` line keeps the
  previous one, which is what makes `GetEventsRequest::after_id` a usable way to
  resume a dropped stream.
- **The event type does not.** It resets every dispatch and falls back to the
  spec default `"message"`, so `BrokerEvent::name` always has a value and that
  value is often meaningless.

The subscription is awaited before the stream is handed back, so a rejected one
is an error rather than a stream that mysteriously says nothing.

Each paginated route has both a single-page method and a `get_all_*` that walks,
which covers alpaca-py's `PaginationType.NONE` and `.FULL`. The lazy `.ITERATOR`
mode is not ported; a `Stream` is its Rust equivalent if a caller ever wants one.
Every walk stops on an empty page even when a token says otherwise — a token
pointing at an empty page would otherwise loop forever.

`commission` arrives as a JSON *number* on order responses and as a *string* in
the spec's trade-update events; `Decimal` reads both and writes a string. If a
live broker sandbox ever rejects a commission on an order request, that is the
thing to look at first.

## Phase 6.5 — the API gaps

Scope changed on 2026-08-12 from *alpaca-py parity* to *API coverage*. alpaca-py
is the least complete of Alpaca's five official SDKs for market data. Evidence:
the OpenAPI specs carry 18 trading routes it lacks, and diffing the C#, Node, Go
and Java clients found ~25 more non-broker gaps.

### Where coverage actually stands

`just coverage` diffs every route this crate calls against the OpenAPI specs and
writes `COVERAGE.md`. As of the first run:

| Surface | Implemented | Spec operations |
|---|---|---|
| trading | 30 | 57 |
| data | 26 | 42 |
| broker | 74 | 154 |
| **total** | **130** | **253** |

Two independent numbers corroborate the extraction: broker's 154 operations is
the count this file already carried, and data's 26 is exactly the method count
Phase 3 landed.

**Two routes this crate calls appear in no spec and in no reference page:**

- `GET /v1/accounts/{account_id}/documents/{document_id}` — the reference
  documents the collection, the upload and the download, but no fetch-one.
- `GET /v1/trading/accounts/{account_id}/account/configurations` — the `PATCH`
  is documented; the `GET` is not.

Both are here because alpaca-py calls them. That is the same footing the retired
`/v1/events/trades` stream stood on, so both are now marked undocumented in
their rustdoc rather than quietly trusted. Neither is removed: undocumented is
not the same as absent, and alpaca-py has a captured payload for the second.
Confirming or removing them wants a live broker sandbox.

The percentages are a floor, not a verdict. A ✅ means the route is called, not
that it is called at the right version — precisely the distinction the event
streams got wrong — so version drift needs the reference, which is what the rest
of this section is for.

### Start by reconciling against the published reference

Three sources disagree about what the API is, and the published reference is the
one that says which endpoints are *current*. The OpenAPI specs list routes
without saying which are legacy; the other SDKs show what someone bothered to
implement. Neither tells you a route has been switched off. **The reference
does**, and the first pass over it found exactly that (see the SSE table below).

The reference at <https://docs.alpaca.markets/us/reference/> is a JavaScript
application and cannot be scraped. It publishes a machine-readable index instead:

```sh
curl -s https://docs.alpaca.markets/us/llms.txt          # every doc page, grouped
curl -s https://docs.alpaca.markets/us/reference/<slug>.md   # one endpoint, as markdown
```

Every reference page has a `.md` twin at the same slug, carrying the method,
path, parameters, and — the part that matters — the deprecation notes. Work the
index group by group against `src/`, and for each endpoint record one of: ported,
gap, or deliberately skipped.

**Surface areas the index shows that the lists below do not mention at all.**
Surfaced by one read of `llms.txt`, not yet checked against the port or costed:

- **Broker JIT** — reports, daily limits, ledgers, ledger balances, settlements
  (7 routes)
- **Broker funding wallets** — create, batch create, transfers, recipient banks,
  withdrawals (11 routes)
- **Broker instant funding** (3 routes)
- **Options approval** — request options trading for an account, list approval
  requests (2 routes, BETA)
- **Order estimation** — `/v1/trading/accounts/{id}/orders/estimation`
- **Trading limits** — `/v1/account/trading/limits`
- **W-8BEN download** — a separate route from the document download we have
- **Asset entry requirements**
- **A single activity event by ULID** — `getaccountactivityevent`
- **More SSE streams than alpaca-py knows about** — admin actions, funding
  status, system events, IPO events, activities, corporate actions
- **OAuth token issuance**

Confirmed already covered, so the reference is not all gaps: news, market movers,
most actives, tokenisation, locates.

### What the first pass already found and fixed

The reference check paid for itself immediately: of the five SSE streams shipped
in Phase 6 — ported faithfully from alpaca-py — one was calling a route Alpaca
had switched off, and two more were legacy. **Fixed**, and worth reading as the
argument for doing the rest of this reconciliation:

| Stream | alpaca-py calls | We now call | Why |
|---|---|---|---|
| Account status | `/v1/events/accounts/status` | unchanged | current, no v2 exists |
| Trades | `/v1/events/trades` | `/v2/events/trades` | v1 is "fully deprecated and no longer available" |
| Journals | `/v1/events/journals/status` | `/v2/events/journals/status` | v1 is legacy; ids are *not* compatible across the two |
| Transfers | `/v1/events/transfers/status` | `/v2/events/funding/status` | v1 is deprecated and closed to new broker partners; v2 also covers banks and wallets |
| Non-trading activity | `/v1/events/nta` | unchanged | current, no v2 exists |

**The cursor parameter is a trap.** The v1 streams take the ULID as
`since_ulid`/`until_ulid`; their `since_id`/`until_id` are a legacy *integer*
form, deprecated 2023-08-01 and sunsetting 2027-02-15. The v2 streams take the
ULID as `since_id`/`until_id`. Same names, different meanings, opposite
versions. `GetEventsRequest` names the field for the concept and the client
renders it for the stream it is calling; the deprecated integer form is not
exposed at all.

Still not exposed: `include_preprocessing` and `group_id`, which only the
non-trading-activity stream takes.

This is what porting an SDK rather than reading the API costs, and it is the
reason the reconciliation above is Phase 6.5's first job rather than its last.

### The coverage checklist

`alpacahq/alpaca-java` is **generated from the OpenAPI specs**, vendors them at
`specs/{broker,data,trading}/openapi.yaml`, and runs an `openapi-drift.yml` CI
job to catch the spec moving under it. That makes its API-group list the closest
thing to an authoritative statement of what the API *is* — better than alpaca-py,
which is hand-written and demonstrably stale.

Its groups, with where this crate stands:

| Surface | Covered | Not covered |
|---|---|---|
| **broker** (25) | Accounts, Assets, Calendar, CorporateActions, Documents, Events, Funding, Journals, Kyc, PortfolioHistory, Rebalancing, Trading, Watchlist | CashInterest, CountryInfo, CryptoFunding, FpslProgram, FundingWallets, InstantFunding, Ipo, Ira, Logos, OAuth, Reporting, Tokenization |
| **data** (8) | CorporateActions, Crypto, News, Option, Screener, Stock | Forex, Logos |
| **trading** (15) | AccountActivities, AccountConfigurations, Accounts, Assets, Calendar, Clock, CorporateActions, Orders, PortfolioHistory, Positions, Watchlists | CryptoFunding, Events, Locates, Tokenization |

Six of those were in neither the route lists below nor the reference sweep:
**CashInterest, CountryInfo, FpslProgram** (fully-paid securities lending),
**Ipo, Ira, Reporting**. Diff against the vendored YAML directly — it is a file,
not a website.

### What building the account requests from the reference changed

The three account request types were built from the reference rather than from
alpaca-py, and the two disagree enough to be worth recording:

- **The required-field set is different.** alpaca-py's create validator requires
  `phone_number`, which the reference does not list; misses `street_address`,
  `city`, `tax_id_type`, `country_of_tax_residence` and `funding_source`, which
  it does; and loses two of its own four disclosure checks to a duplicate key in
  a dict literal, so they never run. `CreateAccountRequest::validate` follows the
  reference. It deliberately does *not* require a phone number: refusing a
  request Alpaca would accept is the worse of the two failures.
- **More is updatable than alpaca-py exposes.** The reference lists ten
  updatable top-level fields to alpaca-py's four, and an identity field list that
  includes `tax_id`, `tax_id_type` and the `country_of_*` fields — which
  alpaca-py's *docstring* promises and its *code* omits. Not yet modelled, because
  each needs types this crate lacks: `beneficiaries`, `cash_interest`, `fpsl`,
  `allow_instant_ach`, and identity's `marital_status`,
  `investment_experience_with_{options,stocks}`.
- **`primary_account_holder_id`** exists on both create and update, for
  multi-live accounts, and is absent from alpaca-py entirely.

### What the live capture found

`just capture` asks the API directly for the routes no SDK tests, writing to
`fixtures/live/` and recording refusals as well as successes. Seven of eleven
came back.

**Captured:** stock exchanges, stock trade and quote conditions, option
exchanges, option trade conditions, auctions, and a SIP bars sample.

**Refused, on an account whose paid plan reaches SIP** — so these are
per-product grants, not the plan as a whole:

| Route | Answer |
|---|---|
| `/v1beta1/forex/{rates,latest/rates}` | 403 `forbidden: insufficient grants` |
| `/v1beta1/indices/latest/values` | 403 `forbidden: insufficient grants` |
| `/v1beta1/logos/{symbol}` | 403 `Subscription does not permit querying logos` |

A 403 rather than a 404 settles a question the spec could not: **indices exist**,
which until now rested on the Node SDK alone. Porting any of these three needs
the matching entitlement before it can be verified, and `stocks_bars_sip` in the
same run is the control that proves the plan itself is fine.

**Two findings worth keeping:**

- **`/v2/stocks/meta/conditions/{tick_type}` requires a `tape` parameter** and
  answers 400 without one. The option equivalent takes none. Nothing in the spec
  or the gap list above hints at the asymmetry, and a port written from the spec
  would have shipped a route that always fails.
- **A single space is a trade condition.** `" ": "Regular Sale"` — the most
  common condition on the tape. Any helper that trims, splits on whitespace, or
  treats the empty string as absent loses the ordinary case. That is the trap
  waiting for the `conditions` lookup helper this file keeps asking for.

### The validation rules, checked against the reference

Every client-side rule this crate enforces was read back against its reference
page. The rules split four ways, and the split is the useful part — a future
port of anything else should sort its rules the same way before enforcing them.

**Confirmed by the reference — enforced.**

| Rule | Where |
|---|---|
| Announcement window ≤ 90 days | "The date range is limited to 90 days." |
| W-8BEN: `content` xor `content_data` | "required unless content_data is provided" |
| W-8BEN: `content_data` implies `application/json` | upload schema |
| `ftin_not_required` when neither tax id is set | "Required if foreign_tax_id and tax_id_ssn are empty." |
| Bank: address fields only on a BIC bank | "Only for international banks, ie if bank_code_type = BIC" |
| Journals: `amount` ↔ JNLC, `symbol`+`qty` ↔ JNLS | "Required if entry_type = JNLC" / "= JNLS" |
| Account creation: the fifteen required fields | `createaccount` schema |

**Contradicted by the reference — removed.**

- **Local currency orders are not market-only.** alpaca-py rejects any non-USD
  order that is not a market order. The LCT page: "Alpaca currently supports LCT
  trading for market, limit, stop & stop limit orders with a time in force=Day".
  The rule refused orders Alpaca accepts. Its time-in-force constraint is *not*
  enforced in its place — that sentence describes what is supported today, and
  enforcing it would recreate the same bug one field over.
- **International banks do not need all five address fields.** The reference
  marks every one of them optional.

**Undocumented business rules — not enforced, documented instead.**

A limit or combination that encodes Alpaca's policy, which only Alpaca can
confirm. The server's rejection says more than a guess of ours, and a guess can
refuse a request that would have worked.

- The ten-document cap on an upload. Alpaca documents a 10MB ceiling on each
  document's contents and no count at all. `DOCUMENT_UPLOAD_LIMIT` is still
  exported for a caller who wants alpaca-py's behaviour.
- `date` alongside `after`/`until` on account activities.

**Coherence rules — kept without needing the reference.**

A request that contradicts itself, or is degenerate, cannot be one Alpaca
accepts, so these cannot wrongly refuse anything: transfer `amount > 0`, weight
`percent > 0`, an asset weight naming a symbol, `start <= end` on a date window,
a watchlist update changing something, and the trading request's `> 0` checks.
`amount > 0` guards a money-movement route and catches a sign error before it
becomes a transfer.

Each removed rule has a test asserting the request now *reaches* the server, the
same shape as the `expect(0)` test on the retired event streams: a re-port from
alpaca-py cannot quietly reinstate a stale rule.

**Found while checking, not yet done:** the reference documents that `category`
and `activity_types` are mutually exclusive on account activities. This crate has
no `category` field, so it implements neither the field nor its rule.

### Documentation: cite the API, not the Python SDK

**Partly done.** The framing is fixed — `lib.rs`, `README.md` and `NOTICE` now
say the crate targets the API and diverges from alpaca-py where alpaca-py is
stale, and 18 module headers lead with what the module is rather than which
Python file it came from. 25 links to `docs.alpaca.markets` were added where
there were none. Doing it turned up a real divergence: the REST retry uses a
flat wait where Alpaca's rate-limit page asks for exponential backoff.

**The count barely moved, and that is the honest result.** Most of the ~140
references were already in the right form: they lead with the wire fact and cite
alpaca-py as contrast or as the source of a divergence, which is what should
happen. What remains falls into three groups, and only the third is work:

1. *Provenance* — "Ported from `alpaca/broker/client.py`". Keep. `just regen`,
   `just pinned` and the end-of-port upstream diff all need the mapping.
2. *alpaca-py as the subject* — "alpaca-py registers a callback per symbol and
   dispatches from a task", "alpaca-py fragments one message at 32 KiB". These
   are about alpaca-py's design, and rewriting them to be about the API would
   make them false.
3. *Rules attributed to alpaca-py* — "alpaca-py enforces this in a model
   validator", "alpaca-py defaults this to active". Roughly 40 of these back a
   `validate()` or a default in this crate. They are claims about what **Alpaca**
   requires, resting on what alpaca-py believes. That is the same footing the
   retired event streams stood on. They are left attributed rather than promoted
   to API facts — visible hearsay beats laundered hearsay — and verifying them
   is part of the reference reconciliation above.

Do not mass-rewrite group 3 without checking each against the reference. An
unverified claim that *says* it is unverified is worth more than a confident one
that is wrong.

The doc comments explain this crate in terms of alpaca-py: 140 references across
`src/`, another ~53 across `tests/`. That was reasonable while the goal was
parity and is now a liability — the retired event streams shipped precisely
because a comment said "alpaca-py does X" and nobody asked whether Alpaca still
did. A reader should be able to check a claim against the API, and "alpaca-py
sets `allow_redirects=False`" cannot be checked against anything.

Not a blanket replacement. Three kinds of reference, three rules:

1. **Wire facts → cite the reference.** "alpaca-py sends `DELETE` parameters in
   the query string", "the retry defaults reproduce alpaca-py exactly",
   "alpaca-py sets `allow_redirects=False`". These are claims about the API, so
   they should cite the API and be verifiable. Most of the 140.
2. **Migration guidance → keep, and say that is what it is.** "the typed
   replacement for alpaca-py's `raw_data=True`" helps someone arriving from the
   Python SDK and is true regardless of what Alpaca does. Phase 7's migration
   guide is where this belongs; mark it rather than delete it.
3. **Generator provenance → keep verbatim.** "Generated by `gen_enums.py` from
   `alpaca/trading/enums.py` at revision `cc4cb3b`" is a build-reproducibility
   fact, and `just pinned` parses that revision string. Do not touch.

The lib.rs framing ("a port of the official Python SDK") needs rewriting too:
the crate targets the Alpaca API and alpaca-py is now one source among four.

### Harvest fixtures from the other SDKs — done, and narrower than expected

`just harvest` pulls **73 payloads** out of `alpaca-trade-api-go`'s tests into
`fixtures/go/`, with `fixtures/go/index.json` recording the source test and the
route it asserted for each.

**Only one of the four SDKs was worth reading, and the reason is not the one this
section assumed.** The distinction is not captured-versus-constructed — the
alpaca-py fixtures are test-authored too, and they still found three real bugs.
It is that Go pastes **raw JSON strings** into backtick literals, so the wire's
quirks survive: numbers as strings, nulls, empty strings, misspelled fields. C#
builds payloads with `JObject` and TypeScript with `JSON.stringify(object)` —
both go *through the SDK's own types*, which normalizes away exactly the quirks
a fixture exists to catch. A payload that has been through a model is evidence
about the model, not about the API.

What the harvest covers, against the gap list below: **auctions, fixed income
latest prices, us_treasuries, us_corporates, option bars/trades/quotes/snapshots
/chain, and crypto perpetuals** — the last being a gap this file recorded as
"not in the spec, verify against the live API", now with six real payloads
showing the shape.

**What no SDK's tests can supply:** forex, logos, `meta/exchanges`,
`meta/conditions`, indices. Nobody tests them. Those need live capture, and the
tool already exists — `just live` with paper keys, and they are all cheap
GET-only market-data routes.

Half the harvest is routes already implemented, so `tests/harvested_go.rs`
deserializes those through the real `Bar`, `Trade` and `Quote` models: a second
SDK's authors, writing down the same API independently, and the models read
their payloads. The rest are placed and parsed but await their models. Nothing
lands unread — the account-list fixture that sat unparsed for months is the
reason that rule exists.

### The original plan for harvesting

`fixtures/` holds 135 real API responses lifted out of alpaca-py's test suite by
`scripts/extract_fixtures.py`, and they have caught more bugs than any schema.
The problem for Phase 6.5 is that **alpaca-py has no fixtures for the routes it
does not implement** — auctions, forex, fixed income, logos, meta/conditions —
which is exactly the surface being added. The other SDKs test those routes and
their payloads are just as real:

| Repo | Language | Where the payloads are |
|---|---|---|
| `alpacahq/alpaca-trade-api-csharp` | C# | ~100 test files, incl. `AlpacaDataClientTest.Auctions.cs` — auctions being gap number one |
| `alpacahq/alpaca-trade-api-js` | TypeScript | mock server and fixtures under the test tree |
| `alpacahq/alpaca-trade-api-go` | Go | JSON literals inside `*_test.go` |
| `alpacahq/alpaca-java` | Java | generated from the specs; vendored `specs/*.yaml` are worth more than its tests |

Four languages means four extractors, so scope it by value: take the routes with
no fixture today first, and stop at the point where a fifth payload for a model
we already verify adds nothing. Land them in `fixtures/` beside the existing
ones, keep `fixtures/SOURCE` honest about which SDK and revision each came from,
and extend `just fixtures` to re-run every extractor rather than only alpaca-py's.

Note `alpacahq/alpaca-trade-api-python` is archived — alpaca-py's own predecessor
is already dead, which is the pattern this whole section exists to get ahead of.

**Market data** — all confirmed present in `market-data-api.json`:

- Auctions: `/v2/stocks/auctions` — the only route all four other SDKs carry and
  we do not
- Stock meta: `/v2/stocks/meta/exchanges`, `/v2/stocks/meta/conditions/{tick_type}`
- Option meta: `/v1beta1/options/meta/conditions/{tick_type}`
- Forex: `/v1beta1/forex/rates`, `/v1beta1/forex/latest/rates`
- Fixed income: `/v1beta1/fixed_income/latest/{prices,quotes}`, and
  `/v2/assets/fixed_income/us_{corporates,treasuries}`
- Logos: `/v1beta1/logos/{symbol}`

**Not in the spec — now verified against the live API:**

- Indices: `/v1beta1/indices/{values,latest/values}` (Node only). **Exists** —
  answers 403 `insufficient grants`, not 404.
- Crypto perpetuals: `/v1beta1/crypto-perps/{feed}/latest/*` (C#, Go, Node).
  Six payloads harvested from the Go suite; see `fixtures/go/`.

**Trading** — confirmed in `trading-api.json`:

- `/v2/positions/{id}/do-not-exercise`
- `/v2/account/activities/{activity_type}`
- `/v2/watchlists:by_name` (GET, PUT, DELETE)
- `/v3/calendar/{market}`
- `/v1/locates`, `/locates/quotes`, `/locates/{id}`
- `/v2/wallets/*` (7 routes), `/v2/tokenization/*` (4 routes)
- `/v2beta1/events/activities`

**Follow-on worth doing properly:** `meta/conditions` and `meta/exchanges` are the
official decoder for the single-letter exchange codes and the opaque
`conditions: Vec<String>` on trades and quotes. Worth a lookup helper, not just
a raw map.

## Also open

- **Enum drift.** Of 71 generated enums, only 7 of the 19 with a same-named spec
  schema agree exactly. Missing values land in `Unknown(String)` — degrading, not
  breaking — but should become real variants. Add a spec cross-check to
  `scripts/gen_enums.py`. Do **not** remove values we have that the spec lacks;
  they may be deprecated but still served. `Exchange` is not drift: the spec's
  same-named schema is venue names, alpaca-py's is tape codes. Verify
  `TaxIdType::ARG_AR_CUIT` (ours) against the spec's `ARG_AG_CUIT` on a live
  response — one is a typo.
- **docs.rs build** for 0.0.0 has not been checked.
- **CIP is spec-derived and unverified.** alpaca-py's two CIP methods are empty
  stubs — its own comment says the sandbox 404s them — so the six `CIP*` models
  have never met a real response. They follow `models/cip.py` and the broker
  spec. First real payload wins; treat a decode failure there as expected work,
  not a regression. Note `CIPPhoto.face_comparison` reads Alpaca's
  `face_comparision`, which is a typo on the wire and so is load-bearing.
- **`BrokerClient` carries a second `reqwest::Client`**, only for the document
  download: that route answers `301` to a presigned storage URL, and
  `RestClient` refuses redirects on purpose. The second client follows them and
  sheds the credentials when one crosses hosts. That shedding is reqwest's
  behaviour, not ours, so `broker_documents.rs` asserts it against a pair of
  mock servers rather than trusting it to stay true across upgrades.

## How this port is built

The method that has actually found bugs, in order of value:

0. **A fixture only helps if something parses it.** The account-list payload sat
   in `fixtures/` from the start with `"funding_source": null` in it, and nothing
   deserialized it as a list until the account request work needed to. It failed
   immediately: `#[serde(default)]` covers an *absent* field, not a present-and-
   null one. Nine `Vec` fields had the same hole. Having the payload was not
   enough — a test had to read it.
1. **Captured fixtures beat schemas.** `fixtures/` holds 135 real API responses
   extracted from alpaca-py's test suite by `scripts/extract_fixtures.py`. Every
   model is verified against them. Three bugs came from this that no schema would
   have shown: a string where an `int` was declared, money-as-strings in `float`
   fields, and a pagination loop that never terminated.
2. **The live smoke test beats mocks.** `tests/live_smoke.rs` found a bug all 15
   stream tests had passed — the mocks encoded msgpack timestamps as strings, a
   form the real server never sends.
3. **Generators for the mechanical parts.** `scripts/gen_enums.py` produces the
   71 wire enums and their parity test from alpaca-py's AST.
4. **The published reference beats the SDK you ported from.** Added after it
   caught three event streams pointing at routes Alpaca had retired. A spec says
   what exists; another SDK says what someone implemented; only the reference
   says what is still *current*. See Phase 6.5.

The limit of 1 is that a fixture can only verify a route the source SDK
implements, which is why Phase 6.5 harvests payloads from the other four SDKs
rather than only alpaca-py's.

## Running things

```sh
just              # = just check: fmt, clippy, rustdoc, test, feature combos
just ci           # + msrv, cargo-deny
just live         # the #[ignore]d tests against the real paper API
just regen        # re-run both generators against ../alpaca-py
just pinned       # is the generated code stale vs. the local alpaca-py?
just hooks        # install the pre-commit credential guard (once per clone)
```

**`just live` needs credentials in the environment.** They are not exported by
the login shell; they come from `.envrc` (`dotenv_if_exists secrets.env`) via
direnv. Either allow direnv for the directory, or scope it to one command:

```sh
direnv exec . just live
```

Use **paper** keys. The tests refuse to run against a key that is not `PK`-prefixed.

## Conventions worth keeping

- **Never `git add -A`.** An unrelated `secrets.env` was swept into a commit and
  pushed that way once. Stage explicit paths; `just hooks` installs a guard, and
  there is a second one at the Claude Code tool boundary.
- **Money that crosses the wire as a string is `Decimal`**; market-data floats
  that arrive as JSON numbers stay `f64`.
- **Unknown enum values must degrade**, never fail — Alpaca adds them.
- **Unknown response fields are ignored.** Alpaca sends fields no model declares
  (`Asset.last_price`, order `commission`, the calendar session fields).
- **Generated files are never hand-edited.** Enum methods go in `enums_ext.rs`.
