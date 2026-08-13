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
| 6.5 — Exceed alpaca-py | ✅ | 251/253 spec routes, 2 deliberate skips |
| 7 — Polish | ✅ | `blocking` and `polars` built, exponential retry, `Error::Stream`, two coverage checks, the migration guide |

Ported against alpaca-py `cc4cb3b`. `just pinned` reports drift against a local
alpaca-py checkout — though alpaca-py is no longer the target; see below.

**Published:** `0.1.0-alpha.1` is on crates.io and docs.rs built it. It is a
rehearsal rather than a milestone: it exists to prove the release path end to
end before a version anyone depends on goes out. That path is
`.github/workflows/release.yml` with crates.io trusted publishing — no API token
anywhere — and `RELEASING.md` has the procedure. It has now run once, so `0.1.0`
is a version bump and a tag, not an experiment.

### Blocked on credentials this account does not have

Three separate things, so that a session picking this up does not spend an hour
rediscovering which:

| Wanted | Blocks |
|---|---|
| **Broker sandbox key** | All 153 broker routes are verified against captured payloads, the reference and the spec — never against a server. It would settle the two undocumented routes below, whether `commission` is accepted as a string on an order request, and the 69 routes phase 6.5 added from the reference alone. |
| **Nothing will settle CIP** | alpaca-py's own comment says the sandbox answers 404 for the CIP routes, which is why its two methods are stubs. A sandbox key probably leaves the six `CIP*` models exactly as unverified as they are now. |
| **Forex / indices / logos grants** | Each answers 403 `insufficient grants` on a paid plan that reaches SIP, so they are per-product entitlements. Porting them is possible from the spec; *verifying* them is not. |
| **A key paper does not gate** | Locates, tokenization and crypto funding answer **404** on the paper trading API — not 403. See "What the live capture found": that is a different kind of unverified, and worth separating. |

**Everything else has been checked against something real**, and "real" is
ranked: a captured payload beats a harvested one, a harvested one beats a
reference example, and a reference example beats a schema. `just capture`
upgrades whatever paper keys can reach; what is left is the table above.

## Phase 6, as built

All 76 of alpaca-py's broker routes are ported, bar one: `delete_account`, which
alpaca-py deprecates and forwards to `close_account`. One route, one method.

The broker spec has **154 operations**; alpaca-py has 76. The remaining 78 were
Phase 6.5's business, and are now done — 153 implemented and one skipped.

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
- ~~**The REST retry ignores Alpaca's own advice**~~ — fixed in phase 7. It
  waited a flat 3 seconds where the rate-limit page asks for exponential backoff
  with jitter; it now calls the same `backoff.rs` curve the stream reconnect
  uses.
- ~~**`Error` has no variant for a stream that breaks mid-flight**~~ — fixed in
  phase 7 as `Error::Stream`, additively, since `Error` is `#[non_exhaustive]`.

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

### Where coverage stands — done

`just coverage` diffs every route this crate calls against the OpenAPI specs and
writes `COVERAGE.md`. **Every documented route is now either implemented or
recorded as a deliberate skip.**

| Surface | Implemented | Skipped | Spec operations |
|---|---|---|---|
| trading | 56 | 1 | 57 |
| data | 42 | 0 | 42 |
| broker | 153 | 1 | 154 |
| **total** | **251** | **2** | **253** |

It started at 130 of 253. Two independent numbers corroborated the extraction at
the time: broker's 154 operations is the count this file already carried, and
data's 26 was exactly the method count Phase 3 landed.

**The scanner under-reported, and fixing it was worth four routes.** A route
whose path is bound to a local first — because it interpolates an id, or two
calls share it — was read by the binding alone and recorded as a GET whatever
method actually used it. `PATCH /v2/orders/{id}`, `DELETE /v2/positions/{id}`
and their broker equivalents had been implemented since phases 2 and 6 and were
being reported as gaps. `scripts/coverage.py` now walks each file in source
order so the call decides the method. The guard that a looser matcher has not
started *inventing* routes is the "called by the crate but not in any spec"
section, which still lists exactly the two known ones.

**What the counts still do not prove.** A ✅ means the route is called, not that
it is called at the right version — precisely the distinction the event streams
got wrong, and one the matcher cannot check because the version lives in the
client rather than in the path literal. What stands in for it:
`RestClient::at_version` puts the version at the call site, and every route
whose version differs from its client's has a test asserting the segment it
requests. Nor do the counts say anything about *parameter* coverage; see
"Also open".

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

Every reference page has a `.md` twin at the same slug, and the twin is better
than markdown: it embeds a **one-operation OpenAPI document** carrying the
versioned path, the `operationId`, which of the three APIs it belongs to, and
the deprecation flags. That makes the reference machine-readable after all.

**Done, and automated.** `just reference` fetches all 256 pages and writes
`specs/reference.json`; `just coverage` reads it and annotates every route with
what the reference says. Re-run it rather than re-reading the site.

### The reconciliation, done

All 123 gaps were joined against the reference. **122 of them are documented
there**, at the version the spec gives, and are real work rather than spec
noise. The exception is `POST /v1/jit/settlements`, which the spec has and the
reference does not list — the same footing as the two undocumented routes above,
so it is implemented with the same warning in its rustdoc.

**Alpaca flags exactly eight routes across the whole reference**, and the crate's
position on each is now settled:

| Route | Flag | Position |
|---|---|---|
| `GET /v1/events/transfers/status` | deprecated + legacy | **skipped** — we call `/v2/events/funding/status` |
| `POST /v2/wallets/transfers` | sunset 2026-10-09 | **skipped** — the replacement is the web app, not a route |
| `GET /v1/events/journals/status` | legacy | already migrated to `/v2` in Phase 6 |
| `GET /v{1,2}/corporate_actions/announcements{,/{id}}` (4) | deprecated | implemented; the replacement `/v1/corporate-actions` is too |
| `GET /v1/accounts/positions` | deprecated | implemented; no replacement documented |

Everything else in the reference is current. The two skips are recorded in
`SKIP` in `scripts/coverage.py` with their reasons, so a skipped route reads as
a decision rather than as an unfilled gap, and "not implemented" can reach zero.

**The single-symbol market data routes are not legacy.** `/v2/stocks/{symbol}/bars`
and its seven siblings looked like aliases the multi-symbol routes had replaced.
The reference documents them as current, with their own pages and their own
response shape — unwrapped, with `symbol` beside the array rather than as the
map key. They are ported as separate methods returning the unwrapped shape.

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

### The coverage checklist — every group is covered

`alpacahq/alpaca-java` is **generated from the OpenAPI specs**, vendors them at
`specs/{broker,data,trading}/openapi.yaml`, and runs an `openapi-drift.yml` CI
job to catch the spec moving under it. That makes its API-group list the closest
thing to an authoritative statement of what the API *is* — better than alpaca-py,
which is hand-written and demonstrably stale.

Every group is now implemented. What each of the eighteen that were missing
turned into, so the mapping from group to module survives:

| Group | Where it landed |
|---|---|
| CashInterest, Reporting | `broker/reporting.rs` |
| CountryInfo, Ira, options approval, Onfido, trading limits, order estimation | `broker/onboarding.rs` |
| CryptoFunding | `trading/wallets.rs`, reused by the broker client |
| FpslProgram | `broker/fpsl.rs` |
| FundingWallets | `broker/funding_wallet.rs` |
| InstantFunding, JIT | `broker/instant_funding.rs`, `broker/jit.rs`, `broker/settlements.rs` |
| Ipo | `broker/ipos.rs` |
| OAuth | `broker/oauth.rs` |
| Tokenization | `trading/tokenization.rs`, reused by the broker client |
| Logos | `types/logo.rs` — both surfaces document the route |
| Forex | `data/historical.rs` (`ForexDataClient`) |
| Locates | `trading/locates.rs` |
| Events | the SSE streams, now in `src/sse.rs` |

**Fixed income was the only group with real payloads behind it.** `just harvest`
had already pulled the Go SDK's `us_corporates` and `us_treasuries` responses
into `fixtures/go/`, and parsing them found a divergence the spec would never
have shown: the captured corporate bond **omits `fractionable`, which the spec
marks required**. The field defaults rather than being required, because a
required-field model would reject the only real corporate bond anyone has
captured. Third time rule zero has paid.

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
`fixtures/live/` and recording refusals as well as successes. It grew from 11
routes to 26 once phase 6.5 landed routes that paper keys can reach, and
**16 of 26 came back**.

**Captured:** stock exchanges, stock trade and quote conditions, option
exchanges, option trade conditions, auctions, a SIP bars sample, all eight
single-symbol stock routes, and the `v3` per-market calendar.

Every one of those is now what its test parses, rather than a body hand-written
out of the reference. That is the point: the reference example is the weakest
tier of evidence this repo recognises, and the capture upgrades it wherever
paper keys allow.

**Refused with a 403, on an account whose paid plan reaches SIP** — so these are
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

**Refused with a 404 — a different finding, and a new one.** Six trading routes
the spec and the reference both document answer `404 endpoint not found` on the
paper API:

| Route | Answer |
|---|---|
| `/v1/locates`, `/v1/locates/quotes` | 404 |
| `/v2/tokenization/requests` | 404 |
| `/v2/wallets`, `/v2/wallets/transfers`, `/v2/wallets/whitelists` | 404 |

A 403 says "the route is there and you cannot see it". A 404 says the paper
endpoint does not serve it at all — so locates, tokenization and crypto funding
are live-only, entitlement-gated at the routing layer, or served somewhere the
reference does not say. The methods stay: the reference documents all six as
current, and paper is not the whole API. But they are **unverified for a
different reason than forex and logos**, and the distinction is worth keeping,
because a 404 is also what a wrong path looks like. Re-running the capture
against a live or entitled key is what would separate the two.

### Deprecated routes warn at the call site, and that is now checked

The five routes Alpaca has flagged and this crate still calls carry
`#[deprecated]`, so a caller learns from the compiler rather than from
`COVERAGE.md`. `just coverage` now verifies it: it finds each route's enclosing
method and reports whether that method is marked, printing the unmarked ones to
stderr. The check was confirmed by removing an attribute and watching it fail,
because a check nobody has seen fail is not a check.

**Only one route in the whole reference carries a sunset date** —
`POST /v2/wallets/transfers`, 2026-10-09 — and it is the one deliberately
skipped, so no function can carry that date. The other five are deprecated with
no sunset published, which their notes say. If Alpaca ever adds one, it lands in
`specs/reference.json` on the next `just reference` and `COVERAGE.md` prints it
beside the route.

One note was stale: `get_all_accounts_positions` said its replacement
"this crate does not wrap yet". Phase 6.5 wrapped it — the note now names
`BrokerClient::get_eod_positions`.

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
| Activities: `category` xor `activity_types` | "Cannot be used with `activity_types` parameter" |

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
- `date` alongside `after`/`until` on account activities. There is now a test
  asserting this combination still *reaches* the server, so a re-port from
  alpaca-py cannot quietly reinstate the rule.

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

**Found while checking, now done:** the reference documents that `category` and
`activity_types` are mutually exclusive on account activities. The crate had
neither the field nor the rule; it now has both, along with `order_id` — the way
to fetch the fills that made up one order — which was also missing.

One doc comment was found asserting a rule the code no longer had: it claimed
`validate` rejected `date` alongside `after`/`until`, which had been removed as
undocumented. A stale comment about a rule is worse than no comment, because the
next reader trusts it. Fixed.

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

~~The lib.rs framing ("a port of the official Python SDK") needs rewriting
too.~~ This contradicted the top of this section, which already said the framing
was fixed. It was: `lib.rs` opens with what the crate targets, and the port
history is a qualified section below it. Phase 7 confirmed rather than repeated
the work.

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
their payloads. Nothing lands unread — the account-list fixture that sat unparsed
for months is the reason that rule exists.

**The rest now have models, and reading them paid.** `tests/data_meta.rs` parses
the auction payloads and `tests/broker_extended.rs` the fixed income ones. The
corporate bond immediately contradicted the spec: it omits `fractionable`, which
the spec marks required, so the field defaults rather than being required. A
model built from the spec alone would have rejected it.

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

**Follow-on, done:** `meta/conditions` and `meta/exchanges` are the official
decoder for the single-letter exchange codes and the opaque
`conditions: Vec<String>` on trades and quotes, so they return
[`Codes`](../src/data/meta.rs) rather than a raw map. `Codes::name` takes the
code verbatim and `Codes::names` maps a whole `conditions` list, degrading per
code where the table has fallen behind the tape. The wrapper exists for one
reason: `" "` is `"Regular Sale"`, a bare `HashMap` invites `.trim()` at the call
site, and the ordinary case is the one that would break. There is a test that
only that distinction passes.

## Phase 7 — polish, and the decisions 1.0 closes the door on

Phase 6.5 was measurable: a route was implemented or it was not, and
`just coverage` said which. Phase 7 mostly is not. What is left divides into
**one thing that is wrong today**, **changes that stop being possible at 1.0**,
documentation, and two checks that would have caught by machine what hand-
checking caught by luck.

Read the version numbers as the constraint. `0.x` may break anything; `1.0`
may not. Everything below is sorted by whether it survives that line.

### ✅ The two features that did not exist

`blocking` and `polars` were declared in `Cargo.toml`, listed in the `lib.rs`
feature table, and implemented nowhere — the crate claiming something untrue on
docs.rs, not a gap in a plan. The decision was to build both rather than drop
the claims, and both are built. The feature table is now true.

**`polars` is built.** `.df()` arrives as the [`ToFrame`] extension trait in
`src/data/frame.rs`, over `BarSet`, `QuoteSet`, `TradeSet`, `AuctionSet`,
`ForexRateSet` and a plain slice of any of their records. The type-alias problem
named here was real — `pub type BarSet = HashMap<…>` cannot take an inherent
`impl` — and the extension trait is the additive way out, so the newtype
question does not need answering before 1.0 after all.

What the sketch did not anticipate:

- **The feature had to imply `data`.** `polars = ["dep:polars"]` on its own
  still compiled all of polars and exposed nothing, which is the same
  dishonesty in a new place. It is `polars = ["data", "dep:polars"]` now, with a
  `just features` line pinning it.
- **`lazy` is gone from the polars dependency.** Nothing here needs it, it is
  the expensive half to compile, and a caller doing frame work depends on polars
  directly anyway — so they enable it and cargo unifies the two.
- **The crate re-exports `polars`.** `.df()` returns a `polars::DataFrame`, and a
  caller on a different polars version gets two incompatible types with the same
  name and a bewildering error. `alpaca_sdk::polars` is the one that matches.
- **`DailyAuctions` is not a row.** It carries two lists of prints, so it
  flattens to one row per print with a `session` column. The internal column
  trait has to allow one record to become many rows, which is why it reports a
  row count rather than assuming one.
- **Dtypes are the thing worth testing.** Timestamps are
  `Datetime(Nanoseconds, "UTC")` and conditions are `List(String)` even when
  every row is null — building that column the obvious way infers `List(Null)`
  from an all-null input, and crypto quotes routinely carry no conditions. A
  frame with the right numbers under the wrong types is worse than no frame:
  the arithmetic still works and the joins silently do not.

**`blocking` is built**, as `blocking::Blocking<C>` — one generic wrapper that
owns a runtime, rather than a synchronous copy of every method. That was the
decision worth making carefully: mirroring 251 routes would double the surface
that has to stay correct, and the copy would drift from the original the first
time a route was added to one and not the other. The wrapper reaches every route
the day it is added, and there is nothing to keep in sync. The cost is one
closure at the call site: `client.call(|client| client.get_account())?`.

The trap named here was real and there were **two** of them, not one:

- `block_on` inside an async context panics. `call` checks for an ambient
  runtime and returns an error instead. This is the one that was written down.
- **Dropping a runtime inside an async context also panics**, and it is the
  easier of the two to hit, because it needs no call at all — constructing a
  `Blocking` in an async fn and letting it fall out of scope is enough. The
  runtime is shut down in the background on drop, which is allowed everywhere.
  Both have a test; the second one found the bug rather than confirming it.

Streams stay async, deliberately. A blocking iterator over a live market data
feed deadlocks as soon as the caller is slower than the socket's read buffer.

[`ToFrame`]: https://docs.rs/alpaca-sdk/latest/alpaca_sdk/data/trait.ToFrame.html

### Breaking changes, so `0.x` or never

**`RetryConfig` is now `#[non_exhaustive]`** — done, and it was the only entry
here that had a deadline. It cost three builder methods (`attempts`, `wait`,
`status_codes`) and one struct literal in `tests/rest_transport.rs`, because
`..Default::default()` is not available on a non-exhaustive struct either. What
it bought is the row below: a wait strategy can now arrive as a *new field*
rather than as a replacement for `wait`, which makes it additive.

That is the general lesson and not a one-off. Any public struct a caller is
expected to build is one field away from a breaking change for as long as it is
exhaustive; `RestConfig` is the other one, and has the same window.

**The retry default no longer contradicts Alpaca's own documentation** — done.
It waited a flat 3 seconds, three times, on 429 and 504, inherited from
alpaca-py, where the [rate-limit page][rate-limits] says to retry "using
exponential backoff". This was never a missing capability: `backoff.rs` had
`reconnect_delay` — doubling from 1s to a 30s cap with equal jitter — and the
REST path simply did not call it. Two subsystems disagreeing, and the one that
was wrong is the one Alpaca wrote about.

The strategy arrived as a new `RetryBackoff` field rather than as a replacement
for `wait`, which is exactly what the `#[non_exhaustive]` change above bought.
`RetryBackoff::Flat` keeps alpaca-py's behaviour available by name. The default
`wait` is now 1s, since it is a base rather than the whole delay.

**This is a behaviour change with no compile error attached to it**, which makes
it the one item in this phase that must appear in the release notes: a caller
who set `wait` and expected it flat now gets it doubled, and a caller who set
nothing waits 1s before the first retry rather than 3. It wants a release of its
own.

Not done, and deliberately: **`Retry-After` is ignored.** A 429 that carries the
header is telling us the exact answer that the backoff curve is guessing at.
Whether Alpaca sends it is unverified — it does not appear in the reference, and
provoking a 429 to find out has not been done. Reading it when present, and
falling back to the curve when absent, is additive, so it is not a 1.0 deadline.

[rate-limits]: https://docs.alpaca.markets/us/docs/broker-api-rate-limits

### Additive, so it need not wait for 1.0

**`Error::Stream` now exists** — done. Both surfaces used to report a stream
that broke mid-flight as `InvalidRequest`, which was a lie in both directions:
nothing about the request was invalid, and the failure happened long after the
request was accepted. `Error` is `#[non_exhaustive]`, so the variant was
additive; what changes observably is what an existing failure maps to, which is
a release-notes line.

Two things the two-line sketch of this item did not show:

- **It was 26 call sites, not 2.** Every websocket connect, send, handshake and
  frame-decode failure in `data/live/mod.rs` and `trading/stream.rs`, plus all
  three arms of `sse::stream_error`. The boundary drawn is *where* the failure
  happened, not what caused it: a failure the crate determines locally before
  any network call stays `InvalidRequest` — an empty subscription set, a
  non-positive timeout, an invalid feed — and everything on the wire is
  `Stream`.
- **`is_fatal` reads the message out of the variant.** The market data stream
  decides whether to reconnect by matching `Error::InvalidRequest(message)` and
  looking for "insufficient subscription" or "auth failed" in it. Moving the
  handshake rejection to `Stream` without moving that match would have turned a
  permanent entitlement failure into an infinite reconnect loop. The data-stream
  test now asserts the variant and the message rather than `is_err()`.

The trading stream's rejected authorization is `Error::Credentials`, not
`Stream`, and stays that way: the socket and the handshake both worked and the
server said no. Its test now says so.

Not changed, and it is the same class of lie: `data/pagination.rs` and
`data/historical.rs` report a *response* that does not match the expected shape
as `InvalidRequest` — "the response carried no `bars`". That is a decode
failure, and `Error::Decode` already exists for it. Also additive, also not a
1.0 deadline.

### Documentation — done

- **The migration guide exists**, as a table in the `lib.rs` "Coming from
  alpaca-py" section rather than a `MIGRATING.md`. Someone arriving from the
  Python SDK finds this on docs.rs, not in the repository tree.

  Collecting the marked comments was not enough on its own: most of what a
  migrating caller needs to know was created by the rest of this phase and
  existed in no comment. The retry default now diverges from alpaca-py's, `.df`
  became a trait that needs a `use`, synchronous use became a wrapper type, and
  a broken stream stopped being an `InvalidRequest`. Four of the thirteen rows
  are things that were true for a matter of hours before the table was written.
- **The `lib.rs` framing was already fixed**, in phase 6.5. This item described
  a file that no longer existed: `lib.rs` opens "targets the Alpaca API itself"
  and the port history is a qualified section below it. Phase 6.5's own section
  said the framing was fixed and then ended by asking for it again; that
  contradiction is resolved rather than acted on.
- **The remainder of the doc-rule audit is checked.** Seven comments describe an
  alpaca-py *default* rather than an enforced rule: the broker page size of 100,
  the data page limit of 10,000, most-actives' top 10 by volume, corporate
  actions' 1,000 ascending, option contracts defaulting to active, and the two
  stream staleness timeouts being off. All seven were verified against the
  pinned checkout at `cc4cb3b` and all seven are accurate, so nothing changed.
  That is the result, not an absence of one — the point of the audit was that
  nobody had looked.

### Two checks, because hand-checking does not scale

Both of these are the same lesson twice: 6.5 found real divergences by reading,
and reading does not survive contact with 251 routes.

- **Parameter coverage is measured now** — `just parameters`, backed by
  `scripts/parameters.py`. `just coverage` compares paths and methods only, and
  hand-checking three routes in 6.5 turned up four missing parameters. Three
  routes out of 251 is not a sample.

  This section used to say `specs/reference.json` already carried every
  parameter of every operation. **It did not** — it carried route metadata and
  nothing else. The parameters are in the cached reference pages, so
  `reference.py` now records them and the file has a `parameters` field; a
  reference.json written before that is reported as stale rather than passing
  silently.

  The check is one-directional on purpose. There is no mechanical path from a
  route to the struct that serializes its query string, so it widens each route
  to the module implementing it — `src/trading/client.rs` to `src/trading/`,
  plus `src/types/`, plus `src/trading/` again for anything in `src/broker/`,
  which reuses it — and asks whether the parameter's name appears there at all.
  A name it does not find is definitely not sent; a name it finds might belong
  to a different struct. False positives cost a minute; a false negative costs a
  parameter nobody notices for a year.

  **It found twelve on its first run**, and they are real:

  | Route | Missing |
  |---|---|
  | `GET /v1/accounts/positions` | `page` — the call passes `&Empty` |
  | `GET /v1/events/nta` | `group_id`, `include_preprocessing` — NTA-only, and `GetEventsRequest` is shared across all five streams |
  | `GET /v1/options/contracts`, `GET /v2/options/contracts` | `ppind` |
  | `GET /v1/trading/accounts/{account_id}/orders` | `qty_above`, `qty_below`, `subtag` — broker-only extras on a trading-shaped request |
  | `GET /v2/account/activities`, `…/{activity_type}` | `activity_types`, `category`, `page_size` |

  The last row is the interesting one. Those two methods take a free-form
  `&[(&str, String)]` query, so the parameters *can* be sent — they are just not
  named anywhere, and the broker's equivalent route has a typed request with a
  `page_size` field. That asymmetry is the gap, not the parameters.
- **Enum drift is measured now** — `just enums-drift`, backed by
  `scripts/enum_drift.py`. It reads the checked-in `enums.rs` files rather than
  regenerating them, which is why it is a separate script and not a step in
  `gen_enums.py` as this section originally asked: `gen_enums.py` needs an
  alpaca-py checkout to run at all, and a check that can only run during a
  regeneration is a check that runs once a year. The generated files are what
  ships, and reading them also catches a hand edit that should not be there.

  It confirms the count this section quoted from a manual survey — 7 of the 19
  with a same-named schema agree exactly — and names the rest. The report has
  two halves, and only one is work:

  - **In the spec, not in the crate.** A value no caller can name. Nine enums:
    `AccountStatus` is missing `ACCOUNT_CLOSED_PENDING`, `ActivityType` twelve
    values, `AssetClass` six, `OrderSide` seven (`sell_short`, `cross`,
    `buy_minus` and friends), `JournalStatus` one, and `OrderClass` the **empty
    string** — which Alpaca documents as a synonym for `simple` in the schema's
    own description, so it is a real wire value and not a parsing artifact.
  - **In the crate, not in the spec.** Reported quietly and never to be acted
    on. alpaca-py carries values Alpaca still serves and has stopped
    documenting; deleting one turns a working match arm into an `Unknown`.

  Two entries are decisions rather than findings, and both live in the script so
  the report converges. `Exchange` is not drift: the spec's same-named schema is
  venue names and alpaca-py's is the tape codes the data API actually sends —
  different vocabularies, same word. `TaxIdType::ARG_AR_CUIT` against the spec's
  `ARG_AG_CUIT` is a typo in one of the two, and no diff can say which; the
  report prints the pair with what would settle it.

### What 1.0 itself needs

The release path already works — `0.1.0-alpha.1` went out through
`.github/workflows/release.yml` with crates.io trusted publishing, docs.rs built
it, and `RELEASING.md` has the procedure. `just publish-dry` runs the whole
thing including `cargo semver-checks`. So 1.0 is not a mechanics problem.

It is a promise problem. Publishing 1.0 says the public surface is one worth
keeping, and it was a surface with two features that did nothing and a retry
default contradicting the vendor's own advice. Both are fixed, so the version
number is no longer a claim the crate cannot back.

What is left before tagging it is a release, not a decision:

- **The release notes carry real behaviour changes**, none of which a compiler
  will point at. Retries wait 1 second and double rather than a flat 3;
  `Error::InvalidRequest` no longer means a dead stream; `RetryConfig` can no
  longer be built with a struct literal. That last one is the only compile
  error, and it is the intended kind.
- **`0.2.0` before `1.0`.** The breaking change is already made, so it should go
  out under a version that is allowed to make it and be lived with for a while.
  `1.0` is then a promise about a surface that has been used, rather than one
  published the same day it changed.
- **Twelve missing parameters and the enum gaps are now on a list** rather than
  in someone's memory. None of them break a caller — an absent parameter is one
  a caller cannot set, and an unknown enum value degrades to `Unknown(String)` —
  so they are 1.x work, not 1.0 blockers.

**`just semver` cannot help yet, and it is worth knowing why before relying on
it.** Run against this phase's changes it reports "no semver update required"
having skipped all 254 checks: within `0.x` every version bump is a major
change, and a major change permits anything, so `cargo semver-checks` has
nothing to assert. It was silent about `RetryConfig` becoming non-exhaustive for
exactly that reason. The tool starts earning its place in `just publish-dry` at
`1.0` — until then the release notes are the only mechanism, which is an
argument for writing them as the change is made rather than at tag time.

## Also open

Facts a reader needs rather than work to schedule. The work lives in
"Phase 7" above.

- **CIP is spec-derived and unverified.** alpaca-py's two CIP methods are empty
  stubs — its own comment says the sandbox 404s them — so the six `CIP*` models
  have never met a real response. They follow `models/cip.py` and the broker
  spec. First real payload wins; treat a decode failure there as expected work,
  not a regression. Note `CIPPhoto.face_comparison` reads Alpaca's
  `face_comparision`, which is a typo on the wire and so is load-bearing.
- **The 69 broker routes added in phase 6.5 have never met a real response.**
  Instant funding, JIT, FPSL, funding wallets, IPOs, reporting, OAuth,
  tokenization and the crypto wallets are spec-and-reference derived, exactly as
  CIP is, and for the same reason: no broker sandbox key. Their tests use the
  reference's own examples and say so. Treat a first-payload decode failure as
  expected work. Fixed income is the exception — it has Go-harvested payloads,
  and they already corrected the spec once.
- **The wire vocabularies do not line up across families**, and each divergence
  is a separate type rather than a shared one: `COMPLETE` (funding wallets)
  against `COMPLETED` (instant funding), `incoming` against `INCOMING`,
  `checking` against `CHECKING`, `ASC` against `asc`. Each pair has a test
  naming both, because the temptation to unify them is exactly what would break
  decoding. `parnter_fee` is Alpaca's own typo and is load-bearing, like
  `face_comparision`.
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
just reference    # index the published API reference into specs/reference.json
just coverage     # regenerate COVERAGE.md from the specs and that index
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
