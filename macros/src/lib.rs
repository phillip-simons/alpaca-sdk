//! Derive macros for [`alpaca-sdk`](https://docs.rs/alpaca-sdk).
//!
//! This crate exists because a procedural macro cannot live in the crate that
//! uses it. Nothing here is meant to be named directly: `alpaca-sdk` re-exports
//! what it needs, and this crate's version is pinned to it exactly.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{
    Attribute, Data, DeriveInput, Expr, ExprLit, Fields, GenericArgument, Lit, LitStr, Meta,
    PathArguments, Type, parse_macro_input, spanned::Spanned,
};

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
/// # Attributes
///
/// | Attribute | Effect |
/// |---|---|
/// | `#[setters(into)]` | Takes `impl Into<T>` and calls `.into()` |
/// | `#[setters(skip = "why")]` | Generates nothing, and records why in the source |
/// | `#[setters(doc = "…")]` | Uses this as the setter's documentation |
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

    // `#[setters(…)]` is a per-field attribute, and one written on the struct
    // parses cleanly, applies to nothing, and looks configured — the exact
    // failure the `into`-on-a-required-field and `doc`-on-a-skipped-field
    // refusals below exist to prevent. An `#[setters(into)]` one line too high
    // is a plausible typo, and silence is the worst answer to it.
    for attribute in &input.attrs {
        if attribute.path().is_ident("setters") {
            return Err(syn::Error::new_spanned(
                attribute,
                "`setters` applies to a field, not to the struct — there is no \
                 whole-type option, so this configures nothing",
            ));
        }
    }

    let mut setters = Vec::new();

    for field in &fields.named {
        // Guaranteed by `Fields::Named`.
        let name = field.ident.as_ref().expect("a named field has an ident");
        let options = parse_options(&field.attrs)?;
        let optional = option_inner(&field.ty);

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
            if inherited.is_empty() {
                return Err(syn::Error::new_spanned(
                    field,
                    "this field has no documentation to give its setter — \
                     document the field, or write the setter's own with \
                     `#[setters(doc = \"…\")]`",
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
    }

    let ty = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    Ok(quote! {
        #[automatically_derived]
        impl #impl_generics #ty #ty_generics #where_clause {
            #(#setters)*
        }
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
            if meta.path.is_ident("into") {
                options.into = Some(meta.path.span());
                return Ok(());
            }
            if meta.path.is_ident("skip") {
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
            Err(meta.error(
                "unknown `setters` option — expected `into`, `skip = \"…\"` or \
                 `doc = \"…\"`",
            ))
        })?;
    }

    Ok(options)
}

/// The text of a field's `///` comments, one entry per line.
fn doc_lines(attrs: &[Attribute]) -> Vec<String> {
    attrs
        .iter()
        .filter_map(|attr| {
            if !attr.path().is_ident("doc") {
                return None;
            }
            let Meta::NameValue(pair) = &attr.meta else {
                return None;
            };
            let Expr::Lit(ExprLit {
                lit: Lit::Str(text),
                ..
            }) = &pair.value
            else {
                return None;
            };
            Some(text.value())
        })
        .collect()
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

    #[test]
    fn doc_lines_reads_every_line_and_nothing_else() {
        let field: syn::Field = syn::parse_quote!(
            /// First.
            ///
            /// Second.
            #[serde(default)]
            #[setters(into)]
            pub name: Option<String>
        );

        assert_eq!(doc_lines(&field.attrs), [" First.", "", " Second."]);
    }

    #[test]
    fn a_field_with_no_documentation_reads_as_empty() {
        let field: syn::Field = syn::parse_quote!(
            #[serde(default)]
            pub name: Option<String>
        );

        assert!(doc_lines(&field.attrs).is_empty());
    }
}
