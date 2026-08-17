//! The `Validated` trait, and the reason the transport takes it as a bound.
//!
//! The derive lives in `alpaca-sdk-macros`, for the same reason
//! [`Setters`](crate::types::setters) does: a procedural macro cannot live in
//! the crate that uses it. This module is where the *convention* is written
//! down.
//!
//! # The convention
//!
//! **Every type the transport sends implements [`Validated`].** There are two
//! ways to satisfy that and they are mutually exclusive:
//!
//! - `#[derive(Validated)]` emits `impl Validated for T {}`, which takes the
//!   defaulted no-op. It says this type has no rules.
//! - A type with rules hand-writes `impl Validated for T { fn validate(…) }`.
//!
//! Doing **both** is `E0119`, conflicting implementations — a compile error.
//! Doing **neither** is a compile error too, but only *at a call site that
//! sends the type*. There are three, and each calls `validate` itself:
//! [`RestClient`](crate::RestClient) on every body and query;
//! `sse::subscribe` on every event stream filter, before it is flattened into
//! query pairs; and the market data pagination loop on every data request,
//! before it is flattened into a parameter map — that surface reaches the
//! transport as a [`Raw`] map, so `RestClient`'s own bound never sees its
//! request types.
//!
//! A request type that nothing sends *yet* is the gap the compiler cannot
//! close, and it is the first of the four things `just validated` checks. The
//! others are a type doing both halves, a type that derives the no-op while
//! holding a field whose type has rules, and a `to_query` that flattens a
//! request with rules into pairs that have none.
//!
//! # Why the rule is a bound and not a review comment
//!
//! Twenty-six request types carried hand-written validation rules, and roughly
//! thirty client methods called `request.validate()?` before sending. Every one
//! of them was wired up correctly. Nothing enforced it.
//!
//! The failure mode is silent in every direction that matters: a new route that
//! forgets the call compiles, passes `just check`, passes CI, passes the
//! wiremock routing test — and sends a body Alpaca rejects, or worse, accepts
//! as something the caller did not mean. The rule was held by review alone, and
//! review is the one mechanism this repository has already documented as
//! insufficient: `TradeEvent` shipped 0.1.0 missing nine wire values for
//! exactly that reason.
//!
//! # Why there is no `#[validated(…)]` attribute
//!
//! The obvious design is a derive that emits a no-op unless the type is marked
//! `#[request(validate)]`, in which case it defers to a hand-written body. That
//! recreates the original bug one level up: write a validator, forget the
//! attribute, and it never runs — while everything still compiles.
//!
//! Coherence removes that hole without a lint, a script, or a reviewer. It is
//! also why the derive is allowed to be as thin as one line: it earns its place
//! by sitting in the `#[derive(…)]` list beside `Setters`, where the next
//! request type will pick it up by habit, and by giving `just validated`
//! something uniform to look for.

use crate::error::Result;
use crate::rest::{Empty, Raw};

/// The rules Alpaca applies to a request, checked before the request is sent.
///
/// [`RestClient`](crate::RestClient) takes this as a bound on every body and
/// every query it sends, and calls [`validate`](Validated::validate) itself.
/// Validation therefore happens once, in one place, and always before a socket
/// is opened — a request Alpaca would reject costs no round trip, and a caller
/// who never heard of this trait cannot skip it.
///
/// # Satisfying the bound
///
/// Inside this crate, a request type with no rules derives it and one with
/// rules writes the impl by hand. Both spellings are one item, and writing both
/// is a conflicting implementation rather than a silent winner.
///
/// From outside this crate — the raw [`RestClient`](crate::RestClient) methods
/// are public so a route this crate has not wrapped is still one call away —
/// there are two options. Implement the trait, which for a type with no rules
/// is one line:
///
/// ```
/// use alpaca_sdk::Validated;
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct CustomBody {
///     symbol: String,
/// }
///
/// impl Validated for CustomBody {}
/// ```
///
/// Or wrap the value in [`Raw`], which says the same thing at the call site
/// rather than at the type.
///
/// # The guarantee
///
/// A type that serializes but carries no `Validated` impl cannot reach the
/// wire. This does not compile:
///
/// ```compile_fail,E0277
/// use alpaca_sdk::{RestClient, Result};
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct Unchecked {
///     symbol: String,
/// }
///
/// async fn send(rest: &RestClient) -> Result<()> {
///     rest.post("/orders", &Unchecked { symbol: "AAPL".to_owned() })
///         .await
/// }
/// # let _ = send;
/// ```
///
/// The same code with the impl added does, and the impl is the only difference
/// between the two — which is what stops the case above from passing for some
/// unrelated reason:
///
/// ```
/// use alpaca_sdk::{RestClient, Result, Validated};
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct Checked {
///     symbol: String,
/// }
///
/// impl Validated for Checked {}
///
/// async fn send(rest: &RestClient) -> Result<()> {
///     rest.post("/orders", &Checked { symbol: "AAPL".to_owned() })
///         .await
/// }
/// # let _ = send;
/// ```
pub trait Validated {
    /// Checks the combinations Alpaca rejects, before the request is sent.
    ///
    /// The default is `Ok(())`, which is the honest answer for most request
    /// types: their invalid states are already unrepresentable, either because
    /// the field is not optional or because the choice is an enum rather than
    /// two optional fields.
    ///
    /// # Errors
    /// Returns [`Error::InvalidRequest`](crate::Error::InvalidRequest) if the
    /// request cannot be sent as built.
    fn validate(&self) -> Result<()> {
        Ok(())
    }
}

/// A request with no query parameters or body has nothing to check.
impl Validated for Empty {}

/// The other spelling of [`Empty`], which a caller reaching for "no query"
/// writes at least as often. It carries no data, so there is nothing a rule
/// could be about and nothing this could hide.
impl Validated for () {}

/// [`Raw`] is the opt-out, so it checks nothing by construction.
impl<T> Validated for Raw<T> {}

/// A query parameter is a pair of scalars, and a pair of scalars has no rules.
///
/// Several routes take a handful of ad-hoc parameters rather than a request
/// struct — `&[("symbols", symbols)]` — and a pair is the shape they arrive in.
/// These three cover every such call site and no more: an array literal of
/// borrowed strings, `&[("cancel_orders", cancel)]`, and a `to_query` returning
/// `Vec<(&'static str, String)>`.
///
/// There is no `(String, String)`. The market data pagination loop builds owned
/// pairs and would have needed one, but it wraps them in [`Raw`] instead —
/// having already validated the request they were flattened from — so the impl
/// would be dead. In a list whose whole argument is that each exemption is
/// individually justified, one that nothing needs is the beginning of the
/// general case this is here to avoid.
///
/// **Deliberately not `impl<K, V> Validated for (K, V)`.** That is the obvious
/// spelling and it is a hole: it makes *any* two-tuple validate as a no-op, so
/// `rest.post(path, &("key", order_request))` compiles and skips
/// `OrderRequest`'s rules — a type with real rules hiding inside a shape whose
/// exemption was only ever justified for strings. Nothing in this crate writes
/// that, which is precisely why nobody would have noticed. Naming the concrete
/// pairs costs nothing and makes the hole a compile error; a new parameter
/// shape costs one line here, which is the right price for it.
///
/// A pair-flattening `to_query` is still a way past the bound, because what
/// reaches the transport is this no-op rather than the request. That is what
/// `just validated` has a rule about, and why
/// `GetCorporateAnnouncementsRequest::to_query` returns a `Result`. Not linked
/// from here: this module is unconditional and that type is behind the
/// `trading` feature.
impl Validated for (&str, String) {}
impl Validated for (&str, &str) {}
impl Validated for (&str, bool) {}

/// Validates every element.
///
/// `upload_documents_to_account` takes a `&[UploadDocument]` and each element
/// carries its own rules. That loop used to live in the client, where
/// forgetting it was invisible; here the bound reaches the elements through the
/// slice.
impl<T: Validated> Validated for [T] {
    fn validate(&self) -> Result<()> {
        for item in self {
            item.validate()?;
        }
        Ok(())
    }
}

/// An array literal at a call site — `&[("qty", qty)]` — is this shape, not a
/// slice, until something coerces it.
impl<T: Validated, const N: usize> Validated for [T; N] {
    fn validate(&self) -> Result<()> {
        self.as_slice().validate()
    }
}

/// Defers to the slice impl, so a `Vec` of requests is checked element by
/// element like a borrowed one.
impl<T: Validated> Validated for Vec<T> {
    fn validate(&self) -> Result<()> {
        self.as_slice().validate()
    }
}

/// A borrowed request is the request.
///
/// The transport takes `&Q`, so this only matters where a call site already
/// holds a reference and would otherwise have to dereference it to satisfy the
/// bound.
impl<T: Validated + ?Sized> Validated for &T {
    fn validate(&self) -> Result<()> {
        T::validate(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Error;

    /// A type whose rules fail, to prove the container impls actually reach
    /// their elements rather than defaulting to `Ok(())`.
    struct AlwaysInvalid;

    impl Validated for AlwaysInvalid {
        fn validate(&self) -> Result<()> {
            Err(Error::InvalidRequest("no".to_owned()))
        }
    }

    struct NoRules;
    impl Validated for NoRules {}

    /// Valid or not per value, because a `Vec` is homogeneous: proving that a
    /// container reaches past its first element needs one type that can be
    /// both, not two types.
    struct Maybe(bool);

    impl Validated for Maybe {
        fn validate(&self) -> Result<()> {
            if self.0 {
                Ok(())
            } else {
                Err(Error::InvalidRequest("no".to_owned()))
            }
        }
    }

    #[test]
    fn the_default_passes() {
        NoRules.validate().unwrap();
        Empty.validate().unwrap();
        Raw(AlwaysInvalid).validate().unwrap();
    }

    /// The whole point of the slice impl. A container that returned `Ok(())`
    /// without asking its elements would satisfy the bound and check nothing,
    /// which is the failure this change exists to remove.
    #[test]
    fn containers_ask_every_element() {
        assert!([AlwaysInvalid].validate().is_err());
        assert!([AlwaysInvalid].as_slice().validate().is_err());
        assert!(vec![AlwaysInvalid].validate().is_err());
        // Spelled as a call rather than `(&x).validate()`, which auto-derefs
        // straight past the reference impl and would test nothing.
        assert!(Validated::validate(&&AlwaysInvalid).is_err());

        // The failing element second, so an impl that checked only `first()`
        // would pass this and be wrong. An earlier version of this line put two
        // `NoRules` in the vector and held for any impl at all — including one
        // that returned `Ok(())` without looking.
        assert!(vec![Maybe(true), Maybe(false)].validate().is_err());
        assert!(vec![Maybe(true), Maybe(true)].validate().is_ok());
    }

    #[test]
    fn an_empty_container_has_nothing_to_refuse() {
        let none: [AlwaysInvalid; 0] = [];
        none.validate().unwrap();
        Vec::<AlwaysInvalid>::new().validate().unwrap();
    }

    /// The ad-hoc query idiom, which has to keep compiling under the bound.
    #[test]
    fn a_query_pair_list_is_a_no_op() {
        [("qty", "1.5")].validate().unwrap();
        vec![("since", "2024-01-01".to_owned())].validate().unwrap();
    }
}
