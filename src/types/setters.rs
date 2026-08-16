//! The `Setters` derive behind the fluent setters on every request type.
//!
//! The derive itself lives in `alpaca-sdk-macros`, because a procedural macro
//! cannot live in the crate that uses it. This module is where the *convention*
//! is written down, and it re-exports the derive so call sites say
//! `use crate::types::Setters;` rather than naming the macro crate — which is
//! an implementation detail, pinned with `=` and published in lockstep.
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
//! Five, each with `#[setters(skip = "…")]` saying what already holds the name.
//! Three are constructors — `GetEventsRequest::since`,
//! `EventStreamRequest::since`, `EstimateOrderRequest::notional` — and two
//! `pub fn` of one name cannot coexist in one impl. The other two are
//! `CreateRecipientBankRequest::routing_code` and `routing_code_type`, which
//! one setter writes together because a routing code without its scheme is
//! ambiguous. Facts about the types rather than oversights, and the reason sits
//! next to the field where it stays arguable.
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
