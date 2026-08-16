//! The `Setters` derive behind the fluent setters on every request type.
//!
//! The derive itself lives in `alpaca-sdk-macros`, because a procedural macro
//! cannot live in the crate that uses it. This module is where the *convention*
//! is written down, and it re-exports the derive so call sites say
//! `use crate::types::setters::Setters;` rather than naming the macro crate —
//! which is an implementation detail, pinned with `=` and published in
//! lockstep, and which no call site should have to know about.
//!
//! # The convention
//!
//! **Every request type a caller builds derives `Setters`.** These types are
//! all `#[non_exhaustive]`, so struct-literal construction is unavailable from
//! outside the crate; the fields stay public and assignable, but
//! `let mut r = X::default(); r.limit = Some(50);` is three lines and a `Some`
//! where `X::default().limit(50)` is one and none. The assignment form still
//! works and is not going away — it is the fallback, not the idiom.
//!
//! `just setters` names the types that should derive it and do not. A missing
//! setter is not a compile error, not a failing test, and not visible in the
//! diff that adds the field, so without that report the only way to see one is
//! to read a struct and its impl side by side and notice a name in one and not
//! the other. `GetOrdersRequest` shipped 0.1.0 with fourteen filters and a
//! setter for none of them inside exactly that silence.
//!
//! # Which fields take `into`
//!
//! A field takes `#[setters(into)]` when a caller would otherwise have to name
//! a type they are only passing through: `String`, so `&str` works, and
//! `Vec<T>`, so an array or an iterator collected elsewhere works. Everything
//! else takes `T` exactly — an enum, a `Decimal`, a `Uuid` and a
//! `DateTime<Utc>` each have one obvious spelling, and `impl Into` there buys
//! nothing and costs inference at the call site. This is the convention
//! `ReplaceOrderRequest::client_order_id` already followed by hand.
//!
//! # Which fields take no setter
//!
//! Those carrying `#[setters(skip = "…")]`, which are of two kinds. `just
//! setters` lists them on every run, with the reason each gives for itself.
//!
//! **A constructor already holds the name.** `GetEventsRequest::since`,
//! `EventStreamRequest::since` and `EstimateOrderRequest::notional`; two
//! `pub fn` of one name cannot coexist in one impl, so this is a fact about the
//! type rather than a decision.
//!
//! **The field is only coherent set alongside another**, and one setter writes
//! the group. Thirteen fields, all of them cases where setting one alone is
//! *always* wrong rather than sometimes:
//!
//! - `OrderRequest`'s `qty`/`notional` are `OrderAmount` and its
//!   `trail_price`/`trail_percent` are `Trail`. Both types exist to make "both
//!   at once" unrepresentable, and `validate` does not catch either pair —
//!   precisely because the type made it unreachable.
//! - `OrderRequest`'s `limit_price` and `stop_price` come from the constructor
//!   for the shape that has one. That module's own documentation says
//!   "`limit_price` is not supported for market orders" is enforced "by there
//!   being no way to set it on one", and a setter is that way.
//! - `OrderRequest`'s `order_class`, `take_profit`, `stop_loss` and `legs` are
//!   written by `bracket`, `oco`, `oto_take_profit`, `oto_stop_loss` and
//!   `multi_leg`. An exit leg with no order class passes `validate` and is not
//!   a bracket.
//! - `CreateRecipientBankRequest`'s `routing_code` and `routing_code_type` go
//!   together because a routing code without its scheme is ambiguous.
//! - `EventStreamRequest`'s `since_id` is documented mutually exclusive with
//!   `since`, and `from_id` is its constructor.
//!
//! The test is not "could a caller misuse this" — the fields are public, so
//! they always could. It is whether the incoherent state is one the API
//! *offers*, in a documented method a reader would reasonably take as blessed.
//!
//! # What does not earn a skip
//!
//! **A field that merely *requires* a companion keeps its setter.**
//! `EventStreamRequest::until` documents "Alpaca requires `since` whenever this
//! is set", and `GetEventsRequest::until_id` says the same of its lower bound.
//! Neither is skipped, and the distinction from `since_id` above is the one
//! that matters: mutually exclusive fields make *every* chain that sets both
//! wrong, while a required companion makes only the chain that omits it wrong —
//! and the chain that includes it, `EventStreamRequest::since(t).until(u)`, is
//! the ordinary correct usage. Skipping would take the setter away from the
//! case it is for.
//!
//! What makes that safe rather than merely convenient is that the derive gives
//! each setter the *field's* documentation, so the requirement travels onto the
//! method: `.until()`'s own rustdoc says `since` is required. A precondition
//! stated on the method is a different thing from an incoherent pair the API
//! silently blesses.
//!
//! **Mutual exclusion earns a skip; ordering does not.** `start` and `end` keep
//! their setters on every type that has them, including the four that also
//! offer a fallible `between(start, end)` — `GetLocatesRequest`,
//! `GetFpslLoansRequest`, `GetJitBalancesRequest` and
//! `GetMarketCalendarRequest`. Both fields set is the ordinary case rather than
//! the broken one, only the ordering can be wrong, and a one-sided window is
//! something `between` cannot express at all. `TimeseriesRequest` has shipped
//! unchecked `start`/`end` since 0.1.0, so this is the existing shape of the
//! crate and not a new hole; `between` remains the checked path for callers who
//! have both values at once.
//!
//! # Documentation
//!
//! The derive gives each setter the field's own doc comment, which for a
//! request field is usually already the right sentence. Where the field reads
//! as a noun and the method should read as an action, `#[setters(doc = "…")]`
//! overrides it. A field with neither is a compile error — deliberately, since
//! the alternative is a `missing_docs` failure pointing at a generated line
//! nobody wrote.

pub(crate) use alpaca_sdk_macros::Setters;
