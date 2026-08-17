//! The `wire_enum` attribute behind every string enum in this crate.
//!
//! The attribute itself lives in `alpaca-sdk-macros`, because a procedural
//! macro cannot live in the crate that uses it. This module is where the
//! *convention* is written down, and it re-exports the attribute so call sites
//! say `use crate::types::wire::wire_enum;` rather than naming the macro crate —
//! which is an implementation detail, pinned with `=` and published in
//! lockstep, and which no call site should have to know about.
//!
//! What it generates, and the rules it enforces, are documented on the
//! attribute itself, which is public on docs.rs. What follows is the part that
//! is about this crate's wire vocabulary rather than about the macro.
//!
//! # Why `Unknown` exists
//!
//! Alpaca introduces new enum values without a version bump, and an SDK that
//! models them as a closed set rejects the whole payload the first time it meets
//! one — a new order status breaking deserialization in production. The
//! generated `Unknown(String)` variant keeps the raw wire value instead, so an
//! unrecognized status is inspectable rather than fatal.
//!
//! # The cost of that tolerance
//!
//! A value this crate simply forgot is indistinguishable from one Alpaca has
//! just invented, and neither fails a test. `TradeEvent` shipped in 0.1.0
//! carrying twelve of the twenty-one values Alpaca documents for its trade
//! events, because it had been transcribed from another SDK rather than from
//! Alpaca's own list; the nine it omitted — two of which,
//! `order_replace_rejected` and `order_cancel_rejected`, occur in routine
//! trading — arrived as `Unknown` for a whole release without anything noticing.
//!
//! **Whenever a variant is added here, check the whole list against a source
//! rather than adding the one value that prompted the visit.**
//! `just enums-drift` does that against Alpaca's own schemas and is the report
//! to read before believing a list is complete.
//!
//! That report is also why `Unknown` is not an alarm. Treating it as "the API
//! changed under me" is not the conservative reading it looks like: it is at
//! least as likely to mean this crate omitted a value Alpaca already documents.
//!
//! # Grammar
//!
//! ```
//! // A doctest compiles as its own crate, so it cannot use the `pub(crate)`
//! // re-export below and names the macro crate directly. Call sites inside
//! // this crate should use `crate::types::wire::wire_enum` instead.
//! use alpaca_sdk_macros::wire_enum;
//!
//! /// Which side of the market an order is on.
//! #[wire_enum(sorted)]
//! pub enum OrderSide {
//!     /// Buy.
//!     #[wire = "buy"]
//!     Buy,
//!     /// Sell.
//!     #[wire = "sell"]
//!     Sell,
//! }
//!
//! assert_eq!(OrderSide::WIRE_VALUES, &["buy", "sell"]);
//! assert_eq!(OrderSide::from("short"), OrderSide::Unknown("short".to_owned()));
//! ```
//!
//! The grammar is checked in two further places, so a change to it cannot rot
//! quietly: `wire_tests.rs` declares enums through the attribute and exercises
//! them under both wire formats, and
//! `macros/tests/compile_fail/a_whole_wire_enum.rs` compiles one in a crate of
//! its own.
//!
//! `sorted` is an opt-in claim that the wire values are in byte order, checked
//! at compile time. Plenty of these enums are deliberately ordered by something
//! else — `ActivityType` leads with `Fill` because that is the one anybody
//! reads for — and those simply do not carry it. A claim nobody meant is worse
//! than silence.

pub(crate) use alpaca_sdk_macros::wire_enum;
