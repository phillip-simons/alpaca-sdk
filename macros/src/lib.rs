//! Macros for [`alpaca-sdk`](https://docs.rs/alpaca-sdk).
//!
//! This crate exists because a procedural macro cannot live in the crate that
//! uses it. Nothing here is meant to be named directly: `alpaca-sdk` re-exports
//! what it needs, and this crate's version is pinned to it exactly.
//!
//! Three macros, one per shape the SDK repeats:
//!
//! - [`macro@Setters`], a derive, generates a consuming setter for every
//!   `Option` field on a request type.
//! - [`macro@Validated`], a derive, gives a request type the no-op validator
//!   that makes a hand-written one impossible to shadow.
//! - [`macro@wire_enum`], an attribute, defines a string-valued enum with a
//!   catch-all `Unknown` variant, and refuses the ways such an enum can be
//!   written wrong.
//!
//! All three are here for the same reason: the alternative is a list repeated
//! beside the thing it describes, and a repeated list drifts silently.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{
    Attribute, Data, DeriveInput, Fields, GenericArgument, Ident, LitStr, PathArguments, Type,
    parse_macro_input, spanned::Spanned,
};

mod attrs;
mod wire_enum;

use crate::attrs::doc_lines;

/// Generates a consuming setter for every `Option` field on a request type.
///
/// Every request type in `alpaca-sdk` is `#[non_exhaustive]`, so a downstream
/// caller cannot use struct-literal or `..Default::default()` construction. The
/// fields stay public and assignable, but `let mut r = X::default(); r.limit =
/// Some(50);` is three lines and a `Some` where `X::default().limit(50)` is one
/// and none.
///
/// A field of type `Option<T>` gets:
///
/// ```ignore
/// #[must_use]
/// pub fn field(mut self, field: T) -> Self {
///     self.field = Some(field);
///     self
/// }
/// ```
///
/// Fields that are not `Option` are left alone. A required field is a
/// constructor argument, and a setter for it would be a second way to say the
/// same thing.
///
/// # The field list is not repeated
///
/// This is the whole reason the derive is worth a second published crate. The
/// alternative — a declarative macro listing each field beside the struct —
/// works, but the list can fall behind the struct silently, which is a class of
/// drift a reviewer has to catch by eye. Reading the real fields means a field
/// added tomorrow has a setter today.
///
/// That holds for the types that *wrap* a shared base too, which is what
/// `flatten` is for. It was the last place a field list was still written down
/// twice.
///
/// # Attributes
///
/// | Attribute | Effect |
/// |---|---|
/// | `#[setters(into)]` | Takes `impl Into<T>` and calls `.into()` |
/// | `#[setters(skip = "why")]` | Generates nothing, and records why in the source |
/// | `#[setters(doc = "…")]` | Uses this as the setter's documentation |
/// | `#[setters(flatten)]` | Delegates this field's base's setters to the wrapper |
///
/// And one on the struct itself:
///
/// | Attribute | Effect |
/// |---|---|
/// | `#[setters(flattenable)]` | Makes this struct a base another type can `flatten` |
///
/// `into` is for the types a caller would otherwise have to name while only
/// passing through: `String`, so a `&str` works, and `Vec<T>`, so an array
/// works. Everything else takes `T` exactly — an enum, a `Decimal`, a `Uuid`
/// and a `DateTime<Utc>` each have one obvious spelling, and `impl Into` there
/// buys nothing and costs inference at the call site.
///
/// `skip` takes a reason rather than being a bare flag, because the fields that
/// need it are not oversights: something else already holds the name — a
/// constructor, or a setter that writes this field together with the one it
/// only makes sense beside — and two `pub fn` of one name cannot coexist in one
/// impl. The reason belongs next to the field, where it stays arguable, rather
/// than in a list somewhere that reads as settled.
///
/// # Documentation
///
/// Every generated method carries a doc comment, because `alpaca-sdk` denies
/// `missing_docs`. By default that is the field's own documentation, which for
/// a request field is usually already the right sentence. Where it is not —
/// where the field reads as a noun and the method should read as an action —
/// `#[setters(doc = "…")]` overrides it.
///
/// A field with neither is a compile error rather than a `missing_docs` error
/// pointing at a line nobody wrote.
///
/// # Flattening a shared base
///
/// Several request types hold one `TimeseriesRequest` and offer its filters as
/// their own, so a caller writes `.limit(50)` rather than `.base.limit(50)`.
/// Writing those delegates out is writing the base's field list down a second
/// time, once per wrapper, which is the drift this derive exists to prevent.
///
/// ```ignore
/// #[derive(Setters)]
/// #[setters(flattenable)]
/// pub struct TimeseriesRequest {
///     /// Caps the total number of items returned across all pages.
///     pub limit: Option<u32>,
/// }
///
/// #[derive(Setters)]
/// pub struct StockBarsRequest {
///     /// The shared time series filters.
///     #[setters(flatten)]
///     pub base: TimeseriesRequest,
///     /// The bar interval.
///     pub timeframe: TimeFrame,
/// }
///
/// StockBarsRequest::new("AAPL", TimeFrame::day()).limit(50);
/// ```
///
/// `flattenable` emits an unexported `macro_rules!` carrying one delegate per
/// optional field, generated from the same reading of the struct the inherent
/// setters come from — so `into`, `skip` and `doc` all apply to the delegates
/// exactly as they apply to the originals, and each delegate calls the base's
/// own setter rather than assigning the field. `flatten` emits an invocation of
/// that macro. The wrapper's source names no field of the base.
///
/// ## The base must be declared first
///
/// `macro_rules!` is textually scoped, so the base has to appear **before** the
/// wrapper, in the same module or an ancestor of it. A module declared after the
/// base inherits the scope; one declared before it does not.
///
/// Violating this fails loudly, spanned at the offending field:
///
/// ```text
/// error: cannot find macro `__setters_flatten_TimeseriesRequest` in this scope
///  --> src/data/requests.rs
///   |
///   |     pub base: TimeseriesRequest,
///   |               ^^^^^^^^^^^^^^^^^
/// ```
///
/// (The line and column are elided here so this example does not rot every time
/// the file above it moves.)
///
/// The same `error:` line appears when the base exists but is not marked
/// `flattenable`, which is the other way to reach it. Only the ordering case
/// carries rustc's trailing `#[macro_use]` suggestion, since only there does a
/// macro of that name exist — and it is not the right advice in either case.
///
/// ## A `skip` on the base reaches every wrapper
///
/// `skip` applies to the delegates as it applies to the inherent setters, which
/// is the point — but it is the one attribute whose effect is felt somewhere
/// other than where it is written. A `#[setters(skip = "…")]` added to a base
/// field removes that method from *every* type flattening the base, and the
/// reason is recorded only at the base. The wrappers lose a method with nothing
/// beside them saying so, and nothing reports it.
///
/// A skip's reason is usually a fact about the base — a constructor holding the
/// name — and then this is right. Where it is not, the base is the wrong place
/// for it.
///
/// ## A wrapper must not repeat one of the base's names
///
/// A wrapper's own `Option` field sharing a name with one of the base's asks for
/// two `pub fn` of that name on one type — one from the derive's own impl, one
/// from the helper's. That is rustc's `E0592`, "duplicate definitions with name
/// `limit`": duplicates *across* a type's impls rather than a plain redefinition
/// inside one, which is why it is spanned at the two `#[derive(Setters)]`
/// attributes and never mentions either field.
///
/// The derive cannot catch it — it reads one struct at a time and does not know
/// what the base's fields are called. Adding a field to a wrapper is the way to
/// reach it.
///
/// ## The base's types resolve at the wrapper
///
/// This is the sharp edge. A delegate's signature carries the field's type
/// written as it appears in the *base's* source — `DateTime<Utc>`, `Sort` — and
/// `macro_rules!` is not hygienic for type paths, so those names have to resolve
/// where the **wrapper** is rather than where the base is. Same module, and this
/// costs nothing. Move a wrapper to a module that does not import what the base
/// imports and it stops compiling.
///
/// ## The helper is not exported
///
/// The natural design is `#[macro_export]` and an absolute path, and it does not
/// compile: a `macro_export` macro produced *by* macro expansion — which this
/// one is, it comes out of a derive — cannot be referred to by absolute path
/// from within its own crate. That is
/// [rust-lang/rust#52234](https://github.com/rust-lang/rust/issues/52234), a
/// deny-by-default `future_incompatible` lint.
///
/// Bare-name invocation is the way around it, and it turns out to be the better
/// design anyway: no export is needed at all, so the helper adds nothing to the
/// public API — nothing at the crate root, nothing for `cargo-semver-checks` to
/// see, and no question about what a `0.x` bump means for it.
///
/// Neither side accepts generics: the helper takes a bare `$wrapper:ident`, and
/// a generic base would have to carry its parameters to every wrapper. Both are
/// refused at the attribute.
///
/// # Example
///
/// ```ignore
/// #[derive(Debug, Default, Serialize, Setters)]
/// #[non_exhaustive]
/// pub struct GetOrdersRequest {
///     /// Restricts the response to orders in this status.
///     pub status: Option<QueryOrderStatus>,
///     /// Restricts the response to orders carrying this subtag.
///     #[setters(into)]
///     pub subtag: Option<String>,
/// }
///
/// GetOrdersRequest::default()
///     .status(QueryOrderStatus::Open)
///     .subtag("desk-7");
/// ```
#[proc_macro_derive(Setters, attributes(setters))]
pub fn derive_setters(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Implements `Validated` with its defaulted no-op, for a request with no rules.
///
/// `alpaca-sdk`'s transport takes `Validated` as a bound on every body and
/// every query it sends, so a request type that does not implement it cannot be
/// sent at all. Most have nothing to check — their invalid states are already
/// unrepresentable — and this derive is how they say so:
///
/// ```ignore
/// #[derive(Debug, Default, Serialize, Setters, Validated)]
/// #[non_exhaustive]
/// pub struct GetOrdersRequest { /* … */ }
/// ```
///
/// It expands to an empty `impl Validated for GetOrdersRequest {}`, under an
/// `#[automatically_derived]` attribute, and nothing further.
///
/// The trait name in that expansion is **unqualified**, unlike the `::core`
/// paths the `Setters` derive is careful to spell out, so it resolves to
/// whatever `Validated` is in scope at the use site. That is deliberate and it
/// is also the reason this derive is re-exported `pub(crate)` rather than
/// publicly: inside `alpaca-sdk` the trait is in scope in every file that has
/// request types, and outside it a downstream `Validated` of someone else's
/// would be silently implemented instead. A downstream caller satisfying the
/// bound writes the one-line impl by hand, or reaches for `Raw`.
///
/// Qualifying it is not available: this crate cannot name `alpaca-sdk`, which
/// is the crate that depends on *it*.
///
/// # There are no options
///
/// A type with rules hand-writes `impl Validated for T { fn validate(…) }`
/// instead, and doing both is `E0119` — a conflicting implementation, refused
/// by the compiler.
///
/// That is the whole design. An attribute — `#[request(validate)]`, say,
/// switching the derive between a no-op and a hand-written body — would
/// reintroduce the failure the bound exists to remove: write a validator,
/// forget the attribute, and it silently never runs. Coherence cannot be
/// forgotten, so `#[validated(…)]` is a compile error rather than a no-op that
/// reads as configuration.
#[proc_macro_derive(Validated, attributes(validated))]
pub fn derive_validated(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_validated(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Defines a string-valued enum with a catch-all `Unknown` variant.
///
/// ```
/// use alpaca_sdk_macros::wire_enum;
///
/// /// Which side of the market an order is on.
/// #[wire_enum(sorted)]
/// pub enum OrderSide {
///     /// Buy.
///     #[wire = "buy"]
///     Buy,
///     /// Sell.
///     #[wire = "sell"]
///     Sell,
/// }
///
/// assert_eq!(OrderSide::WIRE_VALUES, &["buy", "sell"]);
/// assert_eq!(OrderSide::Buy.as_str(), "buy");
/// assert_eq!(OrderSide::from("short"), OrderSide::Unknown("short".to_owned()));
/// ```
///
/// The enum keeps its own documentation and visibility and gains
/// `#[derive(Debug, Clone, PartialEq, Eq, Hash)]`, `#[non_exhaustive]`, an
/// `Unknown(String)` variant, an `as_str`, an `is_unknown`, a `WIRE_VALUES`
/// constant in declaration order, and impls of `From<&str>`, `From<String>`,
/// `FromStr`, `Display`, `Serialize` and `Deserialize`.
///
/// # Attributes
///
/// | Attribute | Where | Effect |
/// |---|---|---|
/// | `#[wire = "…"]` | Variant | The string this variant is, on the wire. Required |
/// | `#[wire_enum(sorted)]` | Enum | Asserts the wire values are in byte order |
///
/// `sorted` is opt-in because plenty of these enums are deliberately ordered by
/// something else — significance, or the shape of the API's own list — and a
/// check that rejected those would only teach people to write the attribute
/// that turns it off. Where an enum *is* alphabetical, saying so is what keeps
/// a value inserted in the wrong place from reading as intentional.
///
/// # Serde is hand-rolled
///
/// Alpaca introduces new enum values without a version bump, and an SDK that
/// models them as a closed set rejects the whole payload the first time it
/// meets one — a new order status breaking deserialization in production. The
/// `Unknown(String)` variant keeps the raw wire value instead, so an
/// unrecognized status is inspectable rather than fatal.
///
/// The derive-based ways of spelling that catch-all do not survive contact with
/// two formats. `#[serde(other)]` discards the string, which is the one thing
/// worth keeping. `#[serde(untagged)]` on a variant relies on content
/// buffering, which behaves differently across formats. A plain string visitor
/// behaves identically under `serde_json` and `rmp-serde`, and the live market
/// data stream is msgpack.
///
/// # What it refuses, and why
///
/// Almost none of this is newly *detected*. Thirteen of the seventeen were
/// already build failures under the `macro_rules!` this replaces — as
/// ``no rules expected `(` ``, as a diagnostic inside the expansion, or as
/// `cannot find attribute`. Two more catch strictly wider: a bare `///`,
/// which `missing_docs` accepts, and a `#[cfg_attr]` carrying no `cfg`. Only
/// `sorted` and its option parsing are new outright.
///
/// What each one buys is a message that names the rule and says what breaking
/// it costs, at the value that broke it.
///
/// | Refusal | What it prevents |
/// |---|---|
/// | Two variants with one wire value | The second is an unreachable `match` arm, so it can never come back off the wire. `unreachable_patterns` catches it, but as a warning, and on the MSRV it is spanned at the macro body rather than at either literal |
/// | A variant with no `#[wire = "…"]` | No arm at all, so the same unreachability by another route |
/// | A second `#[wire]` on one variant | Two literals, and nothing saying which one wins |
/// | A `#[wire]` that is not `= "…"` | A value that is not a string is not something the wire carries |
/// | A variant with no documentation | The docs are where a caller reads to learn when a value occurs; a bare `///` counts as none |
/// | A variant with fields | A wire enum's variants are strings; a payload-carrying one has no `as_str()` |
/// | A variant named `Unknown` | Collides with the injected catch-all |
/// | A variant discriminant | `Buy = 3` would be dropped, not used — the value is the `#[wire]` string |
/// | `#[cfg_attr]` on a variant | It may carry a `cfg`, which has to be copied onto the generated arms, or anything else, which must not be — and this macro runs before it expands. The only refusal here that rejects something harmless |
/// | Two variants of one name | `E0428`, plus two `E0004`s spanned at the attribute suggesting it be edited into a match arm |
/// | `#[serde(…)]`, on a variant or the enum | It cannot apply: `serde` is a helper attribute of `#[derive(Serialize)]`, and both impls are hand-rolled here. Passed through it is rustc's `cannot find attribute`, which reads as a missing import |
/// | `#[wire = "…"]` on the enum | It names one variant's string, so on the enum it names nothing — usually a leftover from a variant that moved |
/// | `sorted` with values out of order | An ordering claim nobody holds is worse than no claim |
/// | An unknown `#[wire_enum(…)]` option | A misspelled option is silence, and silence reads as applied |
/// | A struct or a union | Neither has variants to map to a string at all |
/// | An enum with no variants | A type that is only ever `Unknown`, and no caller can name a value of it |
/// | A generic enum, or one with a `where` clause | The generated `Deserialize` has nowhere to put a bound, and its visitor cannot name a type parameter |
///
/// Every other attribute on a variant — `#[deprecated]`, `#[doc(hidden)]` —
/// passes through to the generated enum untouched. A plain `#[cfg]` passes
/// through *and* is copied onto the `WIRE_VALUES` element and the three
/// `match` arms that name the variant, so gating one variant works.
///
/// The class of bug has a history here. `TradeEvent` shipped in 0.1.0 carrying
/// twelve of the twenty-one values Alpaca documents for its trade events, and
/// the nine it omitted arrived as `Unknown` for a whole release without
/// anything noticing. No macro catches *that* one — a value nobody wrote down
/// is not visible from the source — which is what `scripts/enum_drift.py` is
/// for. These refusals cover the routes that are visible from the source.
///
/// One thing it deliberately does **not** refuse is an empty wire value.
/// `#[wire = ""]` reads like an oversight and is not: Alpaca's schemas list the
/// empty string as an enum value, and rejecting it would mean deleting a value
/// the API sends. Two variants both claiming `""` is still a duplicate.
#[proc_macro_attribute]
pub fn wire_enum(args: TokenStream, item: TokenStream) -> TokenStream {
    wire_enum::expand(args.into(), item.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn expand_validated(input: &DeriveInput) -> syn::Result<TokenStream2> {
    // The attribute is *declared* so that this error is the one a caller sees.
    // Without declaring it, `#[validated(nested)]` is rustc's "cannot find
    // attribute" — which is true, and says nothing about why there is none.
    if let Some(attr) = validated_attribute(input) {
        return Err(syn::Error::new_spanned(
            attr,
            "`Validated` takes no options: a type either has no rules, which is \
             what this derive says, or it has rules, which are hand-written as \
             `impl Validated for T`. Deriving it and implementing it are \
             conflicting implementations, so the two cannot drift apart — an \
             attribute switching between them could be forgotten, which is the \
             failure the trait exists to remove",
        ));
    }

    let ty = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    Ok(quote! {
        #[automatically_derived]
        impl #impl_generics Validated for #ty #ty_generics #where_clause {}
    })
}

/// The first `#[validated(…)]` anywhere on the item, its fields or its variants.
///
/// All three positions are checked because all three are where someone would
/// plausibly reach for one: `#[validated(nested)]` on a field holding a request
/// of its own is the obvious next feature to ask for, and it should say no
/// rather than be ignored.
fn validated_attribute(input: &DeriveInput) -> Option<&Attribute> {
    fn named(attrs: &[Attribute]) -> Option<&Attribute> {
        attrs.iter().find(|attr| attr.path().is_ident("validated"))
    }

    if let Some(attr) = named(&input.attrs) {
        return Some(attr);
    }

    match &input.data {
        Data::Struct(data) => data.fields.iter().find_map(|field| named(&field.attrs)),
        Data::Enum(data) => data.variants.iter().find_map(|variant| {
            named(&variant.attrs).or_else(|| variant.fields.iter().find_map(|f| named(&f.attrs)))
        }),
        Data::Union(data) => data
            .fields
            .named
            .iter()
            .find_map(|field| named(&field.attrs)),
    }
}

/// What a field's `#[setters(…)]` attributes asked for.
#[derive(Default)]
struct Options {
    /// The span of `into`, kept so a misplaced one can be reported at itself.
    into: Option<proc_macro2::Span>,
    /// The reason this field takes no setter, if it takes none.
    skip: Option<(String, proc_macro2::Span)>,
    /// Documentation to use in place of the field's own, one entry per line.
    doc: Vec<String>,
    /// The span of the first `doc`, kept so a misplaced one can be reported at
    /// itself rather than at the field.
    doc_span: Option<proc_macro2::Span>,
    /// The span of `flatten`, kept for the same reason `into`'s is.
    flatten: Option<proc_macro2::Span>,
}

/// What a struct's own `#[setters(…)]` attributes asked for.
#[derive(Default)]
struct ContainerOptions {
    /// The span of `flattenable`, kept so a refusal lands on the word.
    flattenable: Option<proc_macro2::Span>,
}

fn expand(input: &DeriveInput) -> syn::Result<TokenStream2> {
    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            input,
            "Setters applies to structs: an enum has no fields to set, and a \
             union cannot have a safe one",
        ));
    };

    let Fields::Named(fields) = &data.fields else {
        return Err(syn::Error::new_spanned(
            &data.fields,
            "Setters needs named fields — a setter is named after the field it \
             writes, and neither a tuple struct nor a unit struct has one",
        ));
    };

    // Struct-level `#[setters(…)]` used to be refused outright, on the grounds
    // that there was no whole-type option and one written there parses cleanly,
    // applies to nothing and looks configured — an `#[setters(into)]` a line too
    // high being the plausible typo. `flattenable` is now that whole-type
    // option, so the refusal moves into `parse_container_options` rather than
    // going away: it accepts exactly one word and rejects every other, which
    // keeps the typo loud while letting the one real option through.
    let container = parse_container_options(&input.attrs)?;
    let generic = !input.generics.params.is_empty();

    // The helper macro takes `$wrapper:ident` and writes the base's field types
    // out verbatim. Neither survives a type parameter, and no request type in
    // this workspace has one.
    if let Some(span) = container.flattenable
        && generic
    {
        return Err(syn::Error::new(
            span,
            "`flattenable` writes this struct's field types into a helper macro, \
             and a generic base would have to carry its parameters to every \
             wrapper — write the delegating setters by hand",
        ));
    }

    let mut setters = Vec::new();
    let mut delegates = Vec::new();
    let mut flattens = Vec::new();
    let mut flattened_bases: Vec<Ident> = Vec::new();

    for field in &fields.named {
        // Guaranteed by `Fields::Named`.
        let name = field.ident.as_ref().expect("a named field has an ident");
        let options = parse_options(&field.attrs)?;
        let optional = option_inner(&field.ty);

        // A flattened field takes no setter of its own: it takes the base's,
        // one per optional field the base has, which is the entire point.
        if let Some(span) = options.flatten {
            if let Some(into) = options.into {
                return Err(syn::Error::new(
                    into,
                    "`into` configures a setter, and a flattened field gets none \
                     — the delegates follow whatever the base's own fields say",
                ));
            }
            if let Some((_, skip)) = &options.skip {
                return Err(syn::Error::new(
                    *skip,
                    "`flatten` and `skip` contradict each other: the field either \
                     delegates the base's setters or it does not",
                ));
            }
            if let Some(doc) = options.doc_span {
                return Err(syn::Error::new(
                    doc,
                    "`doc` documents one setter, and `flatten` generates one per \
                     optional field of the base — each takes its documentation \
                     from that field",
                ));
            }
            if optional.is_some() {
                return Err(syn::Error::new(
                    span,
                    "`flatten` writes through to a base this type always holds, \
                     and an `Option` one can be absent — make the field required, \
                     or drop `flatten`",
                ));
            }
            if generic {
                return Err(syn::Error::new(
                    span,
                    "`flatten` generates an impl naming this type, so a generic \
                     one would lose its parameters — write the delegating setters \
                     by hand",
                ));
            }
            // Flattening does not chain. A delegate is built from the loop below,
            // which this `continue` skips, so a `flattenable` struct that also
            // flattens would emit a helper carrying its own optional fields and
            // silently not the inner base's — and every wrapper of it would be
            // missing setters with nothing to say so. That is the exact silence
            // this attribute exists to delete, so it is refused rather than
            // documented.
            if let Some(flattenable) = container.flattenable {
                return Err(syn::Error::new(
                    flattenable,
                    "a `flattenable` base cannot itself flatten another: the \
                     helper it emits would carry its own fields and not the \
                     inner base's, so a wrapper would silently be missing \
                     setters — flatten the inner base at each wrapper instead",
                ));
            }
            let Some(base) = base_ident(&field.ty) else {
                return Err(syn::Error::new_spanned(
                    &field.ty,
                    "`flatten` delegates to a base named by a plain path with \
                     no generic arguments, and this type is not one — only a \
                     struct carrying `#[setters(flattenable)]` can be \
                     flattened, and the helper it emits carries no parameters",
                ));
            };
            // Two fields flattening one base means two invocations of one helper
            // and so two `pub fn` of every one of its names. That is `E0592`
            // from rustc, spanned at the *base's* derive — the one item in the
            // program that is right. This is the only collision the derive can
            // see for itself, because both fields are in the struct in front of
            // it, so it is the only one worth refusing here rather than leaving
            // to a message that points somewhere else.
            if flattened_bases.iter().any(|seen| seen == base) {
                return Err(syn::Error::new(
                    span,
                    "this base is already flattened by another field, and one \
                     type cannot have two delegates of every one of its names — \
                     flatten it once and reach the other through its own field",
                ));
            }
            flattened_bases.push(base.clone());

            let helper = helper_ident(base);
            let ty = &input.ident;
            flattens.push(quote! { #helper!(#ty, #name); });
            continue;
        }

        if let Some((_, span)) = &options.skip {
            // A skip on a field that would never have had a setter anyway is
            // not harmless: it reads as a decision about a real collision, and
            // the next person believes it.
            if optional.is_none() {
                return Err(syn::Error::new(
                    *span,
                    "this field is not an `Option`, so it takes no setter with \
                     or without `skip` — drop the attribute",
                ));
            }
            if let Some(into) = options.into {
                return Err(syn::Error::new(
                    into,
                    "`into` and `skip` contradict each other: the field either \
                     takes a setter or it does not",
                ));
            }
            if let Some(doc) = options.doc_span {
                return Err(syn::Error::new(
                    doc,
                    "`doc` documents a setter, and `skip` says there is none — \
                     if the prose is worth keeping, it belongs on the field",
                ));
            }
            continue;
        }

        // `into` and `doc` on a required field configure a setter that is never
        // generated, which is worse than doing nothing: it looks configured.
        let Some(inner) = optional else {
            if let Some(into) = options.into {
                return Err(syn::Error::new(
                    into,
                    "`into` applies to an `Option` field — this one is required, \
                     so it belongs in the constructor rather than in a setter",
                ));
            }
            if let Some(doc) = options.doc_span {
                return Err(syn::Error::new(
                    doc,
                    "`doc` documents a setter, and a required field gets none — \
                     it is a constructor argument, so document it on the field",
                ));
            }
            continue;
        };

        let docs = if options.doc.is_empty() {
            let inherited = doc_lines(&field.attrs);
            // Blank as well as absent. A bare `///` on the field satisfies
            // `missing_docs` on the generated setter while saying nothing,
            // which is the same hole `#[setters(doc = "")]` is refused for one
            // branch down. `wire_enum` refuses it too, and the two macros
            // should not disagree about the same question.
            //
            // "the setter can inherit" rather than "the field has", because a
            // `#[doc = include_str!(…)]` is documentation this cannot copy:
            // the value is still an unexpanded expression here.
            if inherited.iter().all(|line| line.trim().is_empty()) {
                return Err(syn::Error::new_spanned(
                    field,
                    "this field has no documentation the setter can inherit — \
                     document it with a plain `///`, or write the setter's own \
                     with `#[setters(doc = \"…\")]`",
                ));
            }
            inherited
        } else {
            options.doc
        };

        let signature = if options.into.is_some() {
            quote! { #name: impl ::core::convert::Into<#inner> }
        } else {
            quote! { #name: #inner }
        };
        let value = if options.into.is_some() {
            quote! { ::core::convert::Into::into(#name) }
        } else {
            quote! { #name }
        };

        setters.push(quote! {
            #(#[doc = #docs])*
            #[must_use]
            pub fn #name(mut self, #signature) -> Self {
                self.#name = ::core::option::Option::Some(#value);
                self
            }
        });

        if container.flattenable.is_some() {
            // The delegate calls the setter directly above rather than
            // assigning `self.$field.#name = Some(…)`, so `into` and every
            // other decision about this field is made in exactly one place and
            // a wrapper cannot disagree with the base.
            delegates.push(quote! {
                #(#[doc = #docs])*
                #[must_use]
                pub fn #name(mut self, #signature) -> Self {
                    self.$field = self.$field.#name(#name);
                    self
                }
            });
        }
    }

    let ty = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    // Not `#[macro_export]`: a `macro_export` macro produced by macro expansion
    // cannot be reached by absolute path from inside its own crate
    // (rust-lang/rust#52234), so the invocation is by bare name — which means
    // the helper needs no export at all, and adds nothing to the public API.
    //
    // `unused_macros` because a base may be marked `flattenable` before the
    // first wrapper that flattens it exists, and this workspace denies warnings.
    // No `#[doc(hidden)]`: rustdoc does not document a textually-scoped
    // `macro_rules!` in the first place, so it would be decoration standing
    // where a reason should be.
    let helper = container.flattenable.map(|_| {
        let helper = helper_ident(ty);
        quote! {
            #[allow(unused_macros)]
            macro_rules! #helper {
                ($wrapper:ident, $field:ident) => {
                    // Deliberately no `#[automatically_derived]`: on an inherent
                    // impl written by a `macro_rules!` expansion it is a
                    // future-incompatibility warning, and warnings are denied.
                    impl $wrapper {
                        #(#delegates)*
                    }
                };
            }
        }
    });

    Ok(quote! {
        #[automatically_derived]
        impl #impl_generics #ty #ty_generics #where_clause {
            #(#setters)*
        }

        #helper

        #(#flattens)*
    })
}

/// The `#[setters(…)]` options on one field.
fn parse_options(attrs: &[Attribute]) -> syn::Result<Options> {
    let mut options = Options::default();

    for attr in attrs {
        if !attr.path().is_ident("setters") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            // Every other ambiguity here is a hard refusal, and a repeat is one:
            // last-wins is invisible, and two `skip` reasons would leave the
            // derive using the last while `scripts/setters.py` reports the
            // first — two sources disagreeing about why a field has no setter.
            if meta.path.is_ident("into") {
                if options.into.is_some() {
                    return Err(meta.error("`into` is already set on this field"));
                }
                // `into` is a flag. Without this the value in
                // `#[setters(into = "yes")]` is left for the surrounding parser
                // to trip over, and the caller gets syn's bare "expected `,`"
                // where every other mistake here gets a sentence.
                if meta.input.peek(syn::Token![=]) {
                    return Err(meta.error(
                        "`into` takes no value — it is a flag, and the type it \
                         converts to is the field's own",
                    ));
                }
                options.into = Some(meta.path.span());
                return Ok(());
            }
            if meta.path.is_ident("skip") {
                if options.skip.is_some() {
                    return Err(meta.error(
                        "`skip` is already set on this field, with a different \
                         reason — keep the one that is true",
                    ));
                }
                let reason: LitStr = meta.value()?.parse()?;
                if reason.value().trim().is_empty() {
                    return Err(meta.error(
                        "`skip` needs a reason: a bare skip is indistinguishable \
                         from an oversight",
                    ));
                }
                options.skip = Some((reason.value(), meta.path.span()));
                return Ok(());
            }
            // Repeatable, one call per line, because a setter's documentation
            // is sometimes a paragraph and a `\n` inside a string literal
            // renders as a space rather than a break.
            if meta.path.is_ident("doc") {
                let text: LitStr = meta.value()?.parse()?;
                // A blank first line would satisfy `missing_docs` with nothing
                // in it, which is the lint passing rather than the method being
                // documented. Later lines may be blank — that is a paragraph
                // break — so only the first is checked.
                if options.doc.is_empty() && text.value().trim().is_empty() {
                    return Err(meta.error(
                        "`doc` needs something to say: an empty one satisfies \
                         `missing_docs` and documents nothing",
                    ));
                }
                options.doc.push(text.value());
                options.doc_span.get_or_insert(meta.path.span());
                return Ok(());
            }
            if meta.path.is_ident("flatten") {
                options.flatten = Some(meta.path.span());
                return Ok(());
            }
            Err(meta.error(
                "unknown `setters` option on a field — expected `into`, \
                 `skip = \"…\"`, `doc = \"…\"` or `flatten`, and `flattenable` \
                 marks the base itself rather than the field holding it",
            ))
        })?;
    }

    Ok(options)
}

/// The `#[setters(…)]` options on the struct itself.
///
/// Struct-level `#[setters(…)]` went unread until `flattenable` existed, so
/// anything written there was silently accepted and silently did nothing. This
/// takes the field-level parser's stance instead: an attribute that looks
/// configured has to be.
fn parse_container_options(attrs: &[Attribute]) -> syn::Result<ContainerOptions> {
    let mut options = ContainerOptions::default();

    for attr in attrs {
        if !attr.path().is_ident("setters") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("flattenable") {
                options.flattenable = Some(meta.path.span());
                return Ok(());
            }
            Err(meta.error(
                "unknown `setters` option on a struct — `flattenable` is the \
                 only one, and `into`, `skip`, `doc` and `flatten` configure a \
                 field rather than a type",
            ))
        })?;
    }

    Ok(options)
}

/// The name of the helper macro a `flattenable` struct emits.
///
/// It is unexported and textually scoped, so a collision is confined to the
/// module it is declared in — and the name is what a wrapper sees when it
/// flattens a base that is not `flattenable`, or one declared after it, so it
/// should read as an instruction rather than as an internal symbol that leaked.
///
/// A raw identifier loses its `r#`: `__setters_flatten_r#struct` is not a valid
/// identifier and `Ident::new` *panics* on one, which is the one way out of this
/// derive that would not be a sentence explaining itself. Both sides of the
/// invocation come through here, so they still agree on the name.
fn helper_ident(ty: &Ident) -> Ident {
    let name = ty.to_string();
    let name = name.strip_prefix("r#").unwrap_or(&name);
    Ident::new(&format!("__setters_flatten_{name}"), ty.span())
}

/// The name a flattened field's type is known by, or `None` if it has none.
///
/// The last path segment, so both `TimeseriesRequest` and a qualified spelling
/// of it find the same helper — mirroring how `option_inner` treats `Option`.
/// A reference, a tuple, a slice or a generic base has no plain name to look
/// for and is refused at the field.
fn base_ident(ty: &Type) -> Option<&Ident> {
    let Type::Path(path) = ty else {
        return None;
    };
    if path.qself.is_some() {
        return None;
    }
    let segment = path.path.segments.last()?;
    if !matches!(segment.arguments, PathArguments::None) {
        return None;
    }
    Some(&segment.ident)
}

/// The `T` of an `Option<T>`, or `None` for any other type.
///
/// Matches on the last path segment, so `std::option::Option<T>` and a plain
/// `Option<T>` both resolve. A type *named* `Option` that is not the standard
/// one would fool this — and would also make the generated `Some(…)` fail to
/// compile, which is the outcome that matters.
fn option_inner(ty: &Type) -> Option<&Type> {
    let Type::Path(path) = ty else {
        return None;
    };
    if path.qself.is_some() {
        return None;
    }
    let segment = path.path.segments.last()?;
    if segment.ident != "Option" {
        return None;
    }
    let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return None;
    };
    if arguments.args.len() != 1 {
        return None;
    }
    match arguments.args.first()? {
        GenericArgument::Type(inner) => Some(inner),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::ToTokens;

    /// The expansion is the whole design: because it is a trait impl, a type
    /// that also hand-writes one gets `E0119`. A change that emitted an
    /// inherent method instead would keep every trybuild case passing and lose
    /// the property silently, so the shape is asserted here rather than only
    /// implied by the compile-fail file that depends on it.
    #[test]
    fn the_expansion_is_an_empty_trait_impl() {
        let input: DeriveInput = syn::parse_quote!(
            pub struct Request {
                pub limit: Option<u32>,
            }
        );

        assert_eq!(
            expand_validated(&input).unwrap().to_string(),
            quote! {
                #[automatically_derived]
                impl Validated for Request {}
            }
            .to_string()
        );
    }

    /// Generics, a `where` clause and a lifetime all have to survive into the
    /// impl header, and `split_for_impl` is easy to use three-quarters right.
    #[test]
    fn generics_reach_the_impl_header() {
        let input: DeriveInput = syn::parse_quote!(
            pub struct Request<'a, T>
            where
                T: Clone,
            {
                pub name: &'a T,
            }
        );

        assert_eq!(
            expand_validated(&input).unwrap().to_string(),
            quote! {
                #[automatically_derived]
                impl<'a, T> Validated for Request<'a, T> where T: Clone, {}
            }
            .to_string()
        );
    }

    /// A union is the fourth position `validated_attribute` walks and the one
    /// no other test reaches. The derive has no reason to refuse a union — it
    /// generates nothing that touches fields — so what is asserted is that the
    /// attribute is still seen there.
    #[test]
    fn a_union_field_attribute_is_still_refused() {
        let input: DeriveInput = syn::parse_quote!(
            pub union Request {
                #[validated(nested)]
                pub limit: u32,
            }
        );

        assert!(expand_validated(&input).is_err());
    }

    fn ty(text: &str) -> Type {
        syn::parse_str(text).expect("a type")
    }

    fn inner_of(text: &str) -> Option<String> {
        option_inner(&ty(text)).map(|found| found.to_token_stream().to_string())
    }

    #[test]
    fn an_option_yields_its_argument() {
        assert_eq!(inner_of("Option<u32>").as_deref(), Some("u32"));
        assert_eq!(
            inner_of("Option<Vec<String>>").as_deref(),
            Some("Vec < String >")
        );
        assert_eq!(
            inner_of("Option<DateTime<Utc>>").as_deref(),
            Some("DateTime < Utc >")
        );
    }

    /// The path-qualified spellings a field may legitimately use.
    #[test]
    fn a_qualified_option_is_still_an_option() {
        assert_eq!(inner_of("std::option::Option<u32>").as_deref(), Some("u32"));
        assert_eq!(
            inner_of("core::option::Option<u32>").as_deref(),
            Some("u32")
        );
    }

    /// A required field takes no setter, and every one of these is a shape a
    /// request struct in this workspace actually has.
    #[test]
    fn anything_else_yields_nothing() {
        for required in [
            "u32",
            "String",
            "Vec<Option<u32>>",
            "Decimal",
            "TimeseriesRequest",
            "&'a str",
            "[u8; 4]",
            "(Option<u32>, u32)",
        ] {
            assert!(inner_of(required).is_none(), "{required} is not an Option");
        }
    }

    /// `Option` with no argument, or with a lifetime rather than a type, is not
    /// something `Some(value)` can be generated for.
    #[test]
    fn a_malformed_option_yields_nothing() {
        assert!(inner_of("Option").is_none());
        assert!(inner_of("Option<'a>").is_none());
        assert!(inner_of("Option<u32, u32>").is_none());
    }

    fn base_of(text: &str) -> Option<String> {
        base_ident(&ty(text)).map(ToString::to_string)
    }

    /// The spellings a `#[setters(flatten)]` field may legitimately use.
    ///
    /// The qualified case is the one that carries weight beyond this function:
    /// `NAMED_FIELD` in `scripts/setters.py` allows an optional path prefix
    /// *because* of it, so a wrapper spelling its base that way is one the gate
    /// still checks. Without this test that regex rests on a promise in a doc
    /// comment and nothing else.
    #[test]
    fn a_plain_or_qualified_path_yields_its_last_segment() {
        assert_eq!(
            base_of("TimeseriesRequest").as_deref(),
            Some("TimeseriesRequest")
        );
        assert_eq!(
            base_of("crate::data::TimeseriesRequest").as_deref(),
            Some("TimeseriesRequest")
        );
        assert_eq!(base_of("self::Base").as_deref(), Some("Base"));
    }

    /// Each of these reaches a different `None` in `base_ident`, and every one
    /// is refused at the field rather than generating an invocation of a helper
    /// that could not exist. `Option<Base>` is deliberately absent: the
    /// `optional.is_some()` guard fires before `base_ident` is ever called, so
    /// it gets the message about an absent base instead of this one.
    #[test]
    fn anything_without_a_plain_name_yields_nothing() {
        // Not a path at all.
        assert!(base_of("(Base, u32)").is_none());
        assert!(base_of("&'a Base").is_none());
        assert!(base_of("[Base; 2]").is_none());
        // A qualified-self path, whose last segment names an associated type
        // rather than a struct that could carry `flattenable`.
        assert!(base_of("<S as Trait>::Out").is_none());
        // A generic base: the helper takes a bare `$wrapper:ident` and the
        // parameters would have nowhere to go.
        assert!(base_of("Base<u32>").is_none());
        assert!(base_of("crate::data::Base<u32>").is_none());
    }
}
