//! The `wire_enum` attribute: a string-valued enum with a catch-all variant.
//!
//! The entry point and its documentation live in `lib.rs`, because a
//! `#[proc_macro_attribute]` has to be defined at the crate root. Everything it
//! does is here.
//!
//! This started as a `macro_rules!`, and **almost everything refused here was
//! already a build failure under it.** Of the seventeen refusals:
//!
//! - **Two are new.** `sorted` with values out of order, and an unrecognised
//!   `#[wire_enum(…)]` option. Both arrived with the attribute form.
//! - **Two catch strictly more than before.** A variant documented with a bare
//!   `///` — `missing_docs` accepts it, and only an *absent* doc failed. And a
//!   `#[cfg_attr]` carrying no `cfg`, which was harmless; it is refused because
//!   this macro runs before `cfg_attr` expands and cannot tell it from one
//!   carrying a `cfg`, which has to be replicated. That is the only refusal
//!   here that rejects something harmless, and it is a deliberate trade.
//! - **Thirteen already failed, in every case they catch.** What they did not do
//!   is say why.
//!
//! One thing is newly *supported* rather than refused, so it is not among the
//! seventeen: a plain `#[cfg]` on a variant. It used to fail — under the old
//! macro and under the first draft of this one — because the gate reached the
//! declaration and not the four uses generated from it. It is copied onto all
//! four now.
//!
//! Compiled against the old macro, every row below verified. The undocumented
//! variant appears here *and* in the second group above: an absent doc was an
//! error, a blank one was not, so that refusal spans both.
//!
//! | Written | Said |
//! | --- | --- |
//! | A variant with fields | ``error: no rules expected `(` `` |
//! | A variant discriminant | ``error: no rules expected `=` `` |
//! | No `#[wire]` | ``error: no rules expected `,` `` |
//! | A second `#[wire]` | ``error: no rules expected `=>` `` |
//! | No variants | ``error: no rules expected `}` `` |
//! | A generic enum, or a `where` clause | ``no rules expected `<` `` / ``keyword `where` `` |
//! | A struct or a union | ``error: no rules expected keyword `struct` `` |
//! | A non-string wire value | `E0308: mismatched types`, in the generated array |
//! | An undocumented variant | `missing documentation for a variant`, spanned in the macro body |
//! | A variant named `Unknown` | `E0428: defined multiple times`, in the expansion |
//! | Two variants of one name | `E0428`, three `E0004`s in the macro body, and an unreachable pattern — five errors |
//! | A `#[cfg_attr]` carrying a `cfg` | `E0599: no variant … named`, at the variant |
//! | `#[serde]` anywhere | ``cannot find attribute `serde` in this scope`` |
//! | `#[wire]` on the enum | ``cannot find attribute `wire` in this scope`` |
//! | A duplicate wire value | `unreachable_patterns`, a *warning* |
//!
//! Most of those are the macro grammar failing to match, which says nothing
//! about wire enums and does not name the rule broken. Two report a missing
//! import for a name only this macro gives meaning to. One is a warning, fatal
//! here only because the crate denies them. The rest are real rustc errors
//! about the generated code, correctly spanned but explaining a rule the
//! author never sees.
//!
//! So this is a diagnostics change, not a detection one, and the honest case for
//! it is that a macro whose whole job is a 702-value vocabulary should say which
//! value is wrong and what being wrong costs. The duplicate is worth singling
//! out twice over: it was only a warning, and its span depends on the toolchain
//! — the call-site literal on stable, the macro body on the 1.88 MSRV.

use std::collections::HashMap;

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{
    Attribute, Expr, ExprLit, Fields, Ident, Item, ItemEnum, Lit, LitStr, Meta, Variant,
    ext::IdentExt as _,
};

use crate::attrs::documented;

/// One message for every shape of generic, so the two spans share a refusal.
const GENERICS: &str = "a wire enum takes no generics: every variant is one \
                        fixed string, and the generated `Deserialize` has \
                        nowhere to put a bound";

/// What the `#[wire_enum(…)]` list asked for.
#[derive(Default)]
struct Options {
    /// Whether the author claims the wire values are in byte order.
    sorted: bool,
}

/// One variant, once its `#[wire = "…"]` has been read off it.
struct WireVariant {
    /// The variant's name.
    ident: Ident,
    /// Every attribute except the `#[wire]` this macro consumes.
    attrs: Vec<Attribute>,
    /// Just the `#[cfg]`s, kept separately because they have to be replicated.
    ///
    /// A `cfg` gates the variant's *declaration*; the `WIRE_VALUES` element and
    /// the three `match` arms built from it are separate items and would
    /// outlive it, giving `E0599: no variant … named`. Attributes on array
    /// elements and on match arms are both stable — checked on this crate's
    /// 1.88 MSRV — so the gate is copied to each of the four rather than the
    /// variant being refused.
    gates: Vec<Attribute>,
    /// The wire literal, kept as a `LitStr` so an error lands on the string
    /// rather than on the variant that carries it.
    wire: LitStr,
}

pub(crate) fn expand(args: TokenStream2, item: TokenStream2) -> syn::Result<TokenStream2> {
    let options = parse_options(args)?;

    // Parsed as `Item` rather than `ItemEnum` so that a struct gets the message
    // below instead of syn's `expected enum`, which says what was wanted but
    // not why.
    let item: Item = syn::parse2(item)?;
    let Item::Enum(input) = item else {
        return Err(syn::Error::new_spanned(
            item,
            "wire_enum applies to enums: it maps each variant to one string on \
             the wire, and this has no variants to map",
        ));
    };

    // `where_clause` as well as `params`: it is not covered by the parameter
    // check, `emit` does not re-emit it, and a silently dropped bound is the
    // same defect as a silently dropped discriminant. Spanned separately,
    // because `Generics::to_tokens` emits nothing when `params` is empty and
    // the error would land on the call site rather than on the `where`.
    let generics = match (
        input.generics.params.is_empty(),
        &input.generics.where_clause,
    ) {
        (true, None) => None,
        (true, Some(where_clause)) => Some(syn::Error::new_spanned(where_clause, GENERICS)),
        (false, _) => Some(syn::Error::new_spanned(&input.generics, GENERICS)),
    };
    if let Some(error) = generics {
        return Err(error);
    }

    // `serde` on the enum itself cannot apply for the same reason it cannot on
    // a variant, so it gets the same message rather than rustc's `cannot find
    // attribute` from the passed-through copy.
    if let Some(error) = input.attrs.iter().find_map(serde_cannot_apply) {
        return Err(error);
    }

    // `#[wire]` on the enum is the likelier slip of the two: it is what an
    // orphaned attribute looks like after the variant under it moved or was
    // deleted. Passed through it is rustc's `cannot find attribute wire in this
    // scope`, which reads as a missing import for a name this macro owns.
    if let Some(attr) = input.attrs.iter().find(|attr| attr.path().is_ident("wire")) {
        return Err(syn::Error::new_spanned(
            attr,
            "`wire` gives one *variant* its string on the wire, so it belongs on \
             a variant — on the enum there is nothing for it to name",
        ));
    }

    // The old `macro_rules!` spelled its variant list `$( … ),+`, so it refused
    // this at the parse. Keeping it refused is part of what makes "the same
    // behaviour as the macro it replaces" true, and an enum that is only ever
    // `Unknown` is a type no caller can name a value of.
    if input.variants.is_empty() {
        return Err(syn::Error::new_spanned(
            &input,
            "a wire enum with no variants is a type that is only ever \
             `Unknown` — give it the values the API sends, or delete it",
        ));
    }

    // Collected rather than returned one at a time, so an enum written fresh
    // with three undocumented variants reports three times rather than three
    // times in a row across three `cargo check` runs.
    let mut variants = Vec::new();
    let mut errors = Vec::new();
    for variant in &input.variants {
        match read_variant(variant) {
            Ok(read) => variants.push(read),
            Err(error) => errors.push(error),
        }
    }
    if let Some(error) = combine(errors) {
        return Err(error);
    }

    // Before the sortedness check, which has nothing useful to say about a list
    // that contains the same value twice.
    if let Some(error) = duplicate_variant_names(&variants) {
        return Err(error);
    }
    if let Some(error) = duplicate_wire_values(&variants) {
        return Err(error);
    }
    if options.sorted
        && let Some(error) = out_of_order(&variants)
    {
        return Err(error);
    }

    Ok(emit(&input, &variants))
}

/// The `#[wire_enum(…)]` options on the enum.
fn parse_options(args: TokenStream2) -> syn::Result<Options> {
    let mut options = Options::default();

    let parser = syn::meta::parser(|meta| {
        if meta.path.is_ident("sorted") {
            options.sorted = true;
            return Ok(());
        }
        Err(meta.error("unknown `wire_enum` option — expected `sorted`"))
    });
    syn::parse::Parser::parse2(parser, args)?;

    Ok(options)
}

/// One variant's wire value, documentation and shape, or everything wrong with
/// it at once.
fn read_variant(variant: &Variant) -> syn::Result<WireVariant> {
    let mut errors = Vec::new();

    // `unraw` so `r#Unknown` is caught too: it names the same variant, and
    // `Ident`'s comparison against a `&str` keeps the `r#`.
    if variant.ident.unraw() == "Unknown" {
        errors.push(syn::Error::new(
            variant.ident.span(),
            "`Unknown` is the catch-all this macro injects, and two variants of \
             one name cannot coexist — name this one for the value it carries",
        ));
    }

    if !matches!(variant.fields, Fields::Unit) {
        errors.push(syn::Error::new_spanned(
            &variant.fields,
            "a wire enum's variants each map to one string, so they carry no \
             fields — there is no `as_str()` this variant could return",
        ));
    }

    // A bare `///` satisfies `missing_docs` while saying nothing, which is the
    // lint passing rather than the variant being documented. `Setters` refuses
    // a blank field doc for the same reason, and the two agree deliberately.
    // `documented` rather than `doc_lines`, so a doc built by `include_str!`
    // is not refused as absent — this macro passes doc attributes through and
    // never needs to read them.
    if !documented(&variant.attrs) {
        errors.push(syn::Error::new_spanned(
            variant,
            "this variant has no documentation, and its documentation is where \
             a caller reads when the value occurs — say what the wire means, \
             not what it spells",
        ));
    }

    if let Some((_, discriminant)) = &variant.discriminant {
        errors.push(syn::Error::new_spanned(
            discriminant,
            "a wire enum variant's value is its `#[wire = \"…\"]` string, and \
             this discriminant would be dropped rather than used — delete it",
        ));
    }

    errors.extend(variant.attrs.iter().filter_map(unhonoured));

    // Deliberately no check that the value is non-empty. `""` looks like an
    // oversight and is not: Alpaca's own schemas list it as an enum value, and
    // `DocumentType`, `BankAccountType` and `AssetExchange` each carry a
    // variant for it. Rejecting it would mean deleting a value the API
    // sends, which turns a working match arm into an `Unknown`. Two variants
    // both claiming `""` is a different matter, and `duplicate_wire_values`
    // already catches it.
    match (combine(errors), wire_value(variant)) {
        (None, Ok(wire)) => Ok(WireVariant {
            ident: variant.ident.clone(),
            attrs: variant
                .attrs
                .iter()
                .filter(|attr| !attr.path().is_ident("wire"))
                .cloned()
                .collect(),
            gates: variant
                .attrs
                .iter()
                .filter(|attr| attr.path().is_ident("cfg"))
                .cloned()
                .collect(),
            wire,
        }),
        (Some(errors), Ok(_)) => Err(errors),
        (None, Err(error)) => Err(error),
        (Some(mut errors), Err(error)) => {
            errors.combine(error);
            Err(errors)
        }
    }
}

/// The message for an attribute this macro cannot honour, or `None` if it can.
///
/// Every other attribute is passed through to the emitted variant untouched,
/// and a plain `#[cfg]` is passed through *and copied* onto the four generated
/// sites that name the variant. These two cannot be handled either way:
///
/// - `#[cfg_attr(…)]`, because it may carry a `cfg`. A `cfg` has to be
///   replicated onto the `WIRE_VALUES` element and the three `match` arms or
///   they outlive the variant; anything else must not be, since most attributes
///   are not valid on a match arm. An attribute macro runs before `cfg_attr`
///   expands, so there is no way to tell which one this is. Refusing it costs a
///   `#[cfg_attr(…, deprecated)]` that would have been fine, and the alternative
///   is a `#[cfg_attr(all(), cfg(…))]` failing inside the expansion.
/// - `#[serde(…)]` looks like it configures the generated `Serialize` and
///   `Deserialize`. It cannot: `serde` is a helper attribute registered by
///   `#[derive(Serialize)]`, and both impls are hand-rolled here precisely so
///   they do not go through derive. Passed through it is rustc's `cannot find
///   attribute serde in this scope` — an error, but one that reads as a missing
///   import rather than as "the wire spelling is `#[wire]` and nothing else".
fn unhonoured(attr: &Attribute) -> Option<syn::Error> {
    if attr.path().is_ident("cfg_attr") {
        return Some(syn::Error::new_spanned(
            attr,
            "`cfg_attr` is refused on a wire enum variant because this macro \
             runs before it expands, so it cannot tell a `cfg` — which has to \
             be copied onto the generated `WIRE_VALUES` element and `match` \
             arms — from something that must not be. Write a plain `#[cfg]`, \
             which is supported, or gate the whole enum",
        ));
    }
    serde_cannot_apply(attr)
}

/// The message for a `#[serde(…)]` anywhere on a wire enum.
///
/// Shared by the variant and the enum itself, because the reason is the same in
/// both places and so the refusal is one refusal.
fn serde_cannot_apply(attr: &Attribute) -> Option<syn::Error> {
    attr.path().is_ident("serde").then(|| {
        syn::Error::new_spanned(
            attr,
            "`serde` attributes cannot apply here: they are read by \
             `#[derive(Serialize)]`, and this macro hand-rolls `Serialize` and \
             `Deserialize` rather than deriving them — the wire spelling is \
             `#[wire = \"…\"]` and nothing else",
        )
    })
}

/// The single `#[wire = "…"]` literal on a variant.
fn wire_value(variant: &Variant) -> syn::Result<LitStr> {
    let mut found: Option<LitStr> = None;

    for attr in &variant.attrs {
        if !attr.path().is_ident("wire") {
            continue;
        }
        let Meta::NameValue(pair) = &attr.meta else {
            return Err(malformed(attr));
        };
        let Expr::Lit(ExprLit {
            lit: Lit::Str(text),
            ..
        }) = &pair.value
        else {
            return Err(malformed(attr));
        };
        if found.is_some() {
            return Err(syn::Error::new_spanned(
                attr,
                "this variant already has a `#[wire]` value — one variant is \
                 one string on the wire, and a second says nothing about which \
                 one wins",
            ));
        }
        found = Some(text.clone());
    }

    found.ok_or_else(|| {
        syn::Error::new_spanned(
            variant,
            "this variant has no `#[wire = \"…\"]`, so nothing on the wire maps \
             to it — it can be constructed but never deserialized",
        )
    })
}

/// The message for a `#[wire]` that is not `= "…"`.
fn malformed(attr: &Attribute) -> syn::Error {
    syn::Error::new_spanned(
        attr,
        "`wire` takes the string the API sends, spelled exactly: \
         `#[wire = \"buy\"]`",
    )
}

/// The variants both duplicate checks can reason about.
///
/// Neither can evaluate a `cfg` — this macro runs long before one is decided —
/// so a gated variant is skipped. Since `#[cfg]` became supported, the
/// idiomatic mutually-exclusive pair is two variants of one name under opposite
/// gates, and checking those would refuse it with a message that is false for
/// every build: there is exactly one of them in each.
///
/// Nothing is lost that matters. A genuine collision between two variants that
/// are *both* live is still caught, by rustc — `E0428` for a repeated name and
/// `unreachable_patterns` for a repeated value, which this crate denies. What
/// the skip costs is the better message, and only for gated variants.
fn ungated(variants: &[WireVariant]) -> impl Iterator<Item = &WireVariant> {
    variants.iter().filter(|variant| variant.gates.is_empty())
}

/// Two variants of one name, reported at both.
///
/// rustc catches this as `E0428` on the emitted enum — but it also emits two
/// `E0004: non-exhaustive patterns` for the generated `match`es, spanned at the
/// `#[wire_enum]` line and suggesting `#[wire_enum], &Dup::One => todo!()`. A
/// fix-it that edits an attribute into a match arm is worse than no fix-it, and
/// it is the shape this macro exists to keep out of the build.
fn duplicate_variant_names(variants: &[WireVariant]) -> Option<syn::Error> {
    let mut seen: HashMap<String, &WireVariant> = HashMap::new();
    let mut errors = Vec::new();

    for variant in ungated(variants) {
        let name = variant.ident.unraw().to_string();
        if let Some(first) = seen.get(&name) {
            let mut error = syn::Error::new(
                variant.ident.span(),
                format!("`{name}` is already a variant of this enum"),
            );
            error.combine(syn::Error::new(
                first.ident.span(),
                format!(
                    "`{name}` is first defined here, carrying {:?}",
                    first.wire.value()
                ),
            ));
            errors.push(error);
        } else {
            seen.insert(name, variant);
        }
    }

    combine(errors)
}

/// Two variants claiming one wire value, reported at both.
///
/// Not a defect rustc misses — `unreachable_patterns` fires on the generated
/// arm, and this crate denies warnings, so it was already fatal. Two things
/// change. It is an error rather than a warning kept fatal by a build flag.
/// And its span no longer depends on the toolchain: on stable rustc does point
/// at the call-site literal, but on this crate's MSRV it points at the macro
/// body — `$($wire => Self::$variant,)+` — naming neither the literal nor the
/// variant, which across 702 values is a search. Reported here it lands on both
/// literals on every toolchain, and says what the collision costs: the second
/// variant can never come back off the wire, however a test constructs it.
fn duplicate_wire_values(variants: &[WireVariant]) -> Option<syn::Error> {
    let mut seen: HashMap<String, &WireVariant> = HashMap::new();
    let mut errors = Vec::new();

    for variant in ungated(variants) {
        let value = variant.wire.value();
        if let Some(first) = seen.get(&value) {
            let mut error = syn::Error::new(
                variant.wire.span(),
                format!(
                    "`{first}` already carries {value:?}, so `From<&str>` \
                     matches it first and `{second}` can never come back off \
                     the wire — one of the two literals is wrong",
                    first = first.ident,
                    second = variant.ident,
                ),
            );
            error.combine(syn::Error::new(
                first.wire.span(),
                format!("{value:?} is first claimed here, by `{}`", first.ident),
            ));
            errors.push(error);
        } else {
            seen.insert(value, variant);
        }
    }

    combine(errors)
}

/// The first wire value that breaks an enum's `sorted` claim.
///
/// Reported at the literal, naming where it belongs, because a message that
/// only says "not sorted" leaves the author diffing the list by eye.
///
/// Read over the source list, `#[cfg]`-gated variants included, because that is
/// what `sorted` is a claim about: the order somebody wrote, not the order a
/// particular build ends up with.
fn out_of_order(variants: &[WireVariant]) -> Option<syn::Error> {
    for (index, variant) in variants.iter().enumerate().skip(1) {
        let value = variant.wire.value();
        let previous = variants[index - 1].wire.value();
        if value >= previous {
            continue;
        }

        // Where it does belong: after the last earlier value it outsorts, or
        // at the front if it outsorts all of them.
        let place = match variants[..index]
            .iter()
            .map(|earlier| earlier.wire.value())
            .filter(|earlier| *earlier < value)
            .max()
        {
            Some(after) => format!("after {after:?}"),
            None => "first".to_owned(),
        };

        return Some(syn::Error::new(
            variant.wire.span(),
            format!(
                "`sorted` says this enum's wire values are in byte order, and \
                 {value:?} is not: it sorts before {previous:?}, so it belongs \
                 {place} — move the variant, or drop `sorted`"
            ),
        ));
    }

    None
}

/// Every error as one, or `None` for none.
fn combine(errors: Vec<syn::Error>) -> Option<syn::Error> {
    let mut errors = errors.into_iter();
    let mut first = errors.next()?;
    for rest in errors {
        first.combine(rest);
    }
    Some(first)
}

/// The enum, its catch-all, and the seven impls that make it a wire type.
///
/// `Serialize`/`Deserialize` are hand-rolled rather than derived. Derive-based
/// catch-alls (`#[serde(other)]`, variant-level `#[serde(untagged)]`) rely on
/// content buffering that behaves differently across formats and, in the case
/// of `other`, discard the unknown string. A plain string visitor behaves
/// identically under `serde_json` and `rmp-serde`, and the live market data
/// stream is msgpack.
fn emit(input: &ItemEnum, variants: &[WireVariant]) -> TokenStream2 {
    let attrs = &input.attrs;
    let vis = &input.vis;
    let ident = &input.ident;

    let declarations = variants.iter().map(|variant| {
        let (attrs, ident) = (&variant.attrs, &variant.ident);
        quote! {
            #(#attrs)*
            #ident,
        }
    });
    // Each of these four names the variant, so each carries its `#[cfg]`s. A
    // gate on the declaration alone would leave four uses of a variant that is
    // not there.
    let wire_values = variants.iter().map(|variant| {
        let (gates, wire) = (&variant.gates, &variant.wire);
        quote! { #(#gates)* #wire }
    });
    let as_str_arms = variants.iter().map(|variant| {
        let (gates, ident, wire) = (&variant.gates, &variant.ident, &variant.wire);
        quote! { #(#gates)* Self::#ident => #wire, }
    });
    // Built twice rather than shared, because a `quote!` repetition consumes
    // the iterator it walks.
    let from_str_arms = variants.iter().map(|variant| {
        let (gates, ident, wire) = (&variant.gates, &variant.ident, &variant.wire);
        quote! { #(#gates)* #wire => Self::#ident, }
    });
    let from_string_arms = variants.iter().map(|variant| {
        let (gates, ident, wire) = (&variant.gates, &variant.ident, &variant.wire);
        quote! { #(#gates)* #wire => Self::#ident, }
    });

    quote! {
        #(#attrs)*
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        #[non_exhaustive]
        #vis enum #ident {
            #(#declarations)*
            /// A value this version of the SDK does not recognize.
            ///
            /// Holds the raw wire string so it can be logged or matched on
            /// without waiting for a crate release.
            ///
            /// **This does not necessarily mean Alpaca has added something
            /// new.** It means only that this crate does not name the value,
            /// which is equally consistent with the crate having omitted one
            /// Alpaca already documents.
            ///
            /// Treating `Unknown` as "the API changed under me" and escalating
            /// on it is therefore not the conservative choice it looks like.
            /// Log it, carry the string, and check it against Alpaca's
            /// documentation before concluding the wire moved.
            Unknown(::std::string::String),
        }

        // `allow(deprecated)` on every generated impl, so a `#[deprecated]`
        // variant — or a `#[deprecated]` enum, which every impl below names —
        // warns at a *caller's* use and not at this macro's own. Without it the
        // expansion emits warnings the author never wrote, and this crate
        // builds under `-D warnings`.
        #[allow(deprecated)]
        impl #ident {
            /// Every wire value this type recognizes, excluding `Unknown`.
            pub const WIRE_VALUES: &'static [&'static str] = &[#(#wire_values),*];

            /// The value as it appears on the wire.
            #[must_use]
            pub fn as_str(&self) -> &str {
                match self {
                    #(#as_str_arms)*
                    Self::Unknown(value) => value.as_str(),
                }
            }

            /// Whether this value is one the SDK does not recognize.
            #[must_use]
            pub fn is_unknown(&self) -> bool {
                ::std::matches!(self, Self::Unknown(_))
            }
        }

        #[allow(deprecated)]
        impl ::std::convert::From<&str> for #ident {
            fn from(value: &str) -> Self {
                match value {
                    #(#from_str_arms)*
                    other => Self::Unknown(::std::borrow::ToOwned::to_owned(other)),
                }
            }
        }

        #[allow(deprecated)]
        impl ::std::convert::From<::std::string::String> for #ident {
            fn from(value: ::std::string::String) -> Self {
                match value.as_str() {
                    #(#from_string_arms)*
                    // Reuse the allocation the caller already made.
                    _ => Self::Unknown(value),
                }
            }
        }

        #[allow(deprecated)]
        impl ::std::str::FromStr for #ident {
            type Err = ::std::convert::Infallible;

            fn from_str(value: &str) -> ::std::result::Result<Self, Self::Err> {
                ::std::result::Result::Ok(Self::from(value))
            }
        }

        #[allow(deprecated)]
        impl ::std::fmt::Display for #ident {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        #[allow(deprecated)]
        impl ::serde::Serialize for #ident {
            fn serialize<__S>(&self, serializer: __S) -> ::std::result::Result<__S::Ok, __S::Error>
            where
                __S: ::serde::Serializer,
            {
                serializer.serialize_str(self.as_str())
            }
        }

        #[allow(deprecated)]
        impl<'de> ::serde::Deserialize<'de> for #ident {
            fn deserialize<__D>(deserializer: __D) -> ::std::result::Result<Self, __D::Error>
            where
                __D: ::serde::Deserializer<'de>,
            {
                struct __WireVisitor;

                impl ::serde::de::Visitor<'_> for __WireVisitor {
                    type Value = #ident;

                    fn expecting(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                        f.write_str(::std::concat!("a ", ::std::stringify!(#ident), " string"))
                    }

                    fn visit_str<__E>(self, value: &str) -> ::std::result::Result<Self::Value, __E>
                    where
                        __E: ::serde::de::Error,
                    {
                        ::std::result::Result::Ok(<#ident as ::std::convert::From<&str>>::from(value))
                    }

                    // Formats that hand over an owned string let `Unknown` take
                    // the allocation rather than copying it.
                    fn visit_string<__E>(
                        self,
                        value: ::std::string::String,
                    ) -> ::std::result::Result<Self::Value, __E>
                    where
                        __E: ::serde::de::Error,
                    {
                        ::std::result::Result::Ok(
                            <#ident as ::std::convert::From<::std::string::String>>::from(value),
                        )
                    }
                }

                deserializer.deserialize_str(__WireVisitor)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn variant(text: &str) -> Variant {
        syn::parse_str(text).expect("a variant")
    }

    fn wire_of(text: &str) -> syn::Result<String> {
        wire_value(&variant(text)).map(|wire| wire.value())
    }

    fn refusal(error: &syn::Error) -> String {
        error.to_string()
    }

    /// `WireVariant`s with the given wire values and nothing else of interest,
    /// for the two checks that only look at the list.
    fn listed(values: &[&str]) -> Vec<WireVariant> {
        values
            .iter()
            .enumerate()
            .map(|(index, value)| WireVariant {
                ident: Ident::new(&format!("V{index}"), proc_macro2::Span::call_site()),
                attrs: Vec::new(),
                gates: Vec::new(),
                wire: LitStr::new(value, proc_macro2::Span::call_site()),
            })
            .collect()
    }

    #[test]
    fn a_wire_attribute_yields_its_literal() {
        assert_eq!(wire_of("#[wire = \"buy\"] Buy").unwrap(), "buy");
        // The value is the API's spelling, which need not resemble the variant.
        assert_eq!(
            wire_of("#[wire = \"ACH_RELATIONSHIP\"] Ach").unwrap(),
            "ACH_RELATIONSHIP"
        );
    }

    /// The empty string is a wire value, not a missing one. Alpaca's schemas
    /// list it, and three enums in `alpaca-sdk` carry a variant for it — a
    /// check that rejected it would be a check that deleted them.
    #[test]
    fn an_empty_wire_value_is_a_value() {
        let read = read_variant(&variant("/// The empty value.\n#[wire = \"\"]\nNull"))
            .expect("`\"\"` is what the API sends for this one");

        assert_eq!(read.wire.value(), "");
    }

    #[test]
    fn a_variant_with_no_wire_attribute_is_refused() {
        let error = wire_of("#[serde(default)] Buy").unwrap_err();
        assert!(refusal(&error).contains("no `#[wire = \"…\"]`"), "{error}");
    }

    /// Every shape of `#[wire]` that is not `= "…"` gets one message, because
    /// the fix for all of them is the same line.
    #[test]
    fn a_malformed_wire_attribute_is_refused() {
        for written in ["#[wire] Buy", "#[wire(\"buy\")] Buy", "#[wire = 5] Buy"] {
            let error = wire_of(written).unwrap_err();
            assert!(
                refusal(&error).contains("takes the string the API sends"),
                "{written}: {error}"
            );
        }
    }

    #[test]
    fn a_repeated_wire_attribute_is_refused() {
        let error = wire_of("#[wire = \"buy\"] #[wire = \"sell\"] Buy").unwrap_err();
        assert!(
            refusal(&error).contains("already has a `#[wire]`"),
            "{error}"
        );
    }

    #[test]
    fn wire_values_in_order_are_sorted() {
        assert!(out_of_order(&listed(&[])).is_none());
        assert!(out_of_order(&listed(&["only"])).is_none());
        assert!(out_of_order(&listed(&["a", "b", "c"])).is_none());
        // Byte order, not case-insensitive order: uppercase sorts first.
        assert!(out_of_order(&listed(&["ACH", "a"])).is_none());
    }

    /// Equal neighbours are in order. They are also a duplicate, which is a
    /// different refusal with a much better message — `sorted` must not steal
    /// the report.
    #[test]
    fn equal_neighbours_are_in_order() {
        assert!(out_of_order(&listed(&["a", "a"])).is_none());
    }

    #[test]
    fn an_out_of_order_value_names_where_it_belongs() {
        let error = out_of_order(&listed(&["a", "c", "b"])).expect("c then b is not sorted");
        let message = refusal(&error);
        assert!(message.contains("\"b\""), "{message}");
        assert!(message.contains("sorts before \"c\""), "{message}");
        assert!(message.contains("belongs after \"a\""), "{message}");
    }

    #[test]
    fn a_value_that_outsorts_everything_belongs_first() {
        let error = out_of_order(&listed(&["b", "c", "a"])).expect("c then a is not sorted");
        assert!(refusal(&error).contains("belongs first"), "{error}");
    }

    /// Reported at both spans, so the author sees which two literals collided
    /// rather than only the survivor.
    #[test]
    fn a_duplicate_wire_value_is_refused_at_both_variants() {
        let error = duplicate_wire_values(&listed(&["a", "b", "a"])).expect("a appears twice");
        assert_eq!(error.into_iter().count(), 2);

        assert!(duplicate_wire_values(&listed(&["a", "b"])).is_none());
    }

    #[test]
    fn wire_is_stripped_and_every_other_attribute_survives() {
        let read = read_variant(&variant(
            "/// Buy.\n#[wire = \"buy\"]\n#[deprecated]\n#[doc(hidden)]\nBuy",
        ))
        .expect("a well-formed variant");

        assert_eq!(read.ident, "Buy");
        assert_eq!(read.wire.value(), "buy");
        assert!(!read.attrs.iter().any(|attr| attr.path().is_ident("wire")));
        let kept: Vec<_> = read
            .attrs
            .iter()
            .map(|attr| attr.path().get_ident().expect("a simple path").to_string())
            .collect();
        assert_eq!(kept, ["doc", "deprecated", "doc"]);
    }

    /// A plain `#[cfg]` is supported: it is copied onto the `WIRE_VALUES`
    /// element and the three `match` arms, so the gate holds across all four.
    #[test]
    fn a_cfg_on_a_variant_is_copied_to_every_use_of_it() {
        let read = read_variant(&variant(
            "/// Buy.\n#[cfg(feature = \"x\")]\n#[wire = \"buy\"]\nBuy",
        ))
        .expect("`cfg` is honoured, not refused");

        assert_eq!(read.gates.len(), 1);
        assert!(read.gates[0].path().is_ident("cfg"));
        // Still on the declaration too — `attrs` is what the variant re-emits.
        assert!(read.attrs.iter().any(|attr| attr.path().is_ident("cfg")));
    }

    /// `cfg_attr` may carry a `cfg`, which would have to be copied, or anything
    /// else, which must not be — and this macro runs before it expands. `serde`
    /// cannot configure impls this macro hand-rolls.
    #[test]
    fn an_attribute_that_cannot_be_honoured_is_refused() {
        let smuggled = read_variant(&variant(
            "/// Buy.\n#[cfg_attr(all(), cfg(feature = \"x\"))]\n#[wire = \"buy\"]\nBuy",
        ))
        .err()
        .expect("`cfg_attr` may carry a `cfg`");
        assert!(
            refusal(&smuggled).contains("cannot tell a `cfg`"),
            "{smuggled}"
        );

        let serde = read_variant(&variant(
            "/// Buy.\n#[serde(alias = \"b\")]\n#[wire = \"buy\"]\nBuy",
        ))
        .err()
        .expect("`serde` cannot be honoured");
        assert!(refusal(&serde).contains("hand-rolls"), "{serde}");
    }

    /// rustc reports this as `E0428` *and* as two `E0004`s spanned at the
    /// `#[wire_enum]` line, suggesting an attribute be edited into a match arm.
    #[test]
    fn two_variants_of_one_name_are_refused_at_both() {
        let error = expand(
            TokenStream2::new(),
            quote! {
                pub enum Dup {
                    /// First.
                    #[wire = "a"]
                    One,
                    /// Second.
                    #[wire = "b"]
                    One,
                }
            },
        )
        .expect_err("one name, twice");

        assert_eq!(error.into_iter().count(), 2);
    }

    /// `#[wire]` on the enum is what an orphaned attribute looks like after the
    /// variant under it moved. Passed through it was rustc's `cannot find
    /// attribute`, for a name this macro owns.
    #[test]
    fn a_wire_attribute_on_the_enum_is_refused() {
        let error = expand(
            TokenStream2::new(),
            quote! {
                #[wire = "side"]
                pub enum Side {
                    /// Buy.
                    #[wire = "buy"]
                    Buy,
                }
            },
        )
        .expect_err("`wire` belongs on a variant");

        assert!(
            refusal(&error).contains("belongs on \na variant")
                || refusal(&error).contains("belongs on a variant"),
            "{error}"
        );
    }

    /// What the attribute injects, pinned as tokens.
    ///
    /// `#[non_exhaustive]` is the one piece of the public contract nothing else
    /// can catch: it constrains only *other* crates, so nothing in this
    /// workspace fails without it, and `cargo semver-checks` does not treat
    /// dropping it as a break. It is on 120 published enums. The derives are
    /// here for the same reason — `Hash` and `Eq` are what the streaming
    /// layer's handler maps rest on, and losing one is a downstream break this
    /// crate would not notice.
    #[test]
    fn the_injected_attributes_and_derives_are_emitted() {
        let emitted = expand(
            TokenStream2::new(),
            quote! {
                pub enum Side {
                    /// Buy.
                    #[wire = "buy"]
                    Buy,
                }
            },
        )
        .expect("a well-formed enum")
        .to_string();

        assert!(emitted.contains("non_exhaustive"), "{emitted}");
        // Matched as a whole list rather than one name at a time, because
        // `contains("Eq")` is satisfied by the `PartialEq` beside it — so
        // dropping `Eq`, which the streaming layer's handler maps need, would
        // have gone unnoticed here.
        assert!(
            emitted.contains("derive (Debug , Clone , PartialEq , Eq , Hash)"),
            "the derive list moved: {emitted}"
        );
        assert!(emitted.contains("Unknown"), "{emitted}");
        // The prose that says not to alarm on `Unknown` is the crate's position
        // on a real operational question, and a `quote!` block is where it is
        // likeliest to get quietly abbreviated.
        assert!(
            emitted.contains("does not necessarily mean Alpaca has added something"),
            "the `Unknown` doc did not survive the expansion: {emitted}"
        );
    }

    /// The enum-level `serde` refusal is a separate `Err` site from the
    /// variant-level one, and only the variant one has a compile-fail case.
    #[test]
    fn serde_on_the_enum_itself_is_refused() {
        let error = expand(
            TokenStream2::new(),
            quote! {
                #[serde(rename_all = "snake_case")]
                pub enum Side {
                    /// Buy.
                    #[wire = "buy"]
                    Buy,
                }
            },
        )
        .expect_err("`serde` cannot apply to a hand-rolled impl");

        assert!(refusal(&error).contains("cannot apply here"), "{error}");
    }

    /// `#[doc = include_str!(…)]` is documentation, just not documentation a
    /// macro can read — refusing it as absent would say the opposite of the
    /// truth. `wire_enum` passes doc attributes through, so it never needs the
    /// text and has no reason to insist on a literal.
    #[test]
    fn a_macro_built_doc_comment_is_documentation() {
        let read = read_variant(&variant(
            "#[doc = concat!(\"Buy\", \".\")]\n#[wire = \"buy\"]\nBuy",
        ))
        .expect("a doc built by a macro is still a doc");

        assert_eq!(read.wire.value(), "buy");
    }

    /// A `where` clause is not covered by the type-parameter check, and `emit`
    /// does not re-emit it — so before this it was silently dropped.
    #[test]
    fn a_where_clause_is_refused_like_any_other_generic() {
        let error = expand(
            TokenStream2::new(),
            quote! {
                pub enum Side where u8: Copy {
                    /// Buy.
                    #[wire = "buy"]
                    Buy,
                }
            },
        )
        .expect_err("a `where` clause would be dropped");

        assert!(refusal(&error).contains("takes no generics"), "{error}");
    }

    /// The old `macro_rules!` said `$( … ),+`, so it refused this at the parse.
    #[test]
    fn a_wire_enum_with_no_variants_is_refused() {
        let error = expand(TokenStream2::new(), quote! { pub enum Side {} })
            .expect_err("an enum that is only ever `Unknown`");

        assert!(refusal(&error).contains("only ever `Unknown`"), "{error}");
    }

    /// `r#Unknown` names the same variant as `Unknown` and collides with the
    /// injected catch-all just as hard, but `Ident`'s comparison against a
    /// `&str` keeps the `r#`.
    #[test]
    fn a_raw_unknown_is_still_unknown() {
        let error = read_variant(&variant("/// Not named here.\n#[wire = \"u\"]\nr#Unknown"))
            .err()
            .expect("`r#Unknown` collides with the catch-all");

        assert!(
            refusal(&error).contains("catch-all this macro injects"),
            "{error}"
        );
    }

    /// A discriminant was silently dropped from the emitted variant, which the
    /// old `macro_rules!` grammar rejected outright at the parse.
    #[test]
    fn a_variant_discriminant_is_refused() {
        let error = read_variant(&variant("/// Buy.\n#[wire = \"buy\"]\nBuy = 3"))
            .err()
            .expect("a discriminant would be dropped");

        assert!(refusal(&error).contains("would be dropped"), "{error}");
    }

    /// A bare `///` satisfies `missing_docs` and documents nothing, which is
    /// what `Setters` already refuses for `#[setters(doc = "")]`.
    #[test]
    fn a_blank_doc_comment_is_not_documentation() {
        let error = read_variant(&variant("///\n#[wire = \"buy\"]\nBuy"))
            .err()
            .expect("a blank doc says nothing");

        assert!(refusal(&error).contains("no documentation"), "{error}");
    }

    /// Everything wrong with a variant is reported at once. A fresh enum with
    /// three undocumented variants should not take three `cargo check` runs to
    /// find out.
    #[test]
    fn a_variant_reports_every_refusal_at_once() {
        let error = read_variant(&variant("Unknown(String)"))
            .err()
            .expect("four things wrong");

        // Named `Unknown`, carries a field, has no documentation, has no wire.
        assert_eq!(error.into_iter().count(), 4);
    }
}
