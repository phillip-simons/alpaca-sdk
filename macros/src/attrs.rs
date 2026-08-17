//! Reading documentation off an item.
//!
//! Both macros refuse an item whose documentation is absent or blank, and this
//! is where that question is answered — but they ask two different versions of
//! it, so there are two functions and neither is shared.
//!
//! `wire_enum` passes the original `#[doc]` attributes straight through and
//! only needs to know whether any is there, so a `#[doc = include_str!(…)]`
//! counts. `Setters` copies the text onto the setter it generates, so the same
//! attribute is documentation it cannot use, and it says so in those words.
//! They live together because they are one concern read two ways.

use syn::{Attribute, Expr, ExprLit, Lit, Meta};

/// Whether an item carries documentation at all.
///
/// Not the same question as "is `doc_lines` non-empty". A
/// `#[doc = include_str!("blurb.md")]` or `#[doc = concat!(…)]` is real
/// documentation that no macro can read: at this point the value is an
/// unexpanded expression, not a string. Counting it as absent would refuse an
/// item that is in fact documented, with a message saying the opposite.
pub(crate) fn documented(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        if !attr.path().is_ident("doc") {
            return false;
        }
        let Meta::NameValue(pair) = &attr.meta else {
            return false;
        };
        match &pair.value {
            Expr::Lit(ExprLit {
                lit: Lit::Str(text),
                ..
            }) => !text.value().trim().is_empty(),
            // Built by a macro: documentation, just not legible from here.
            _ => true,
        }
    })
}

/// The text of an item's `///` comments, one entry per line.
///
/// Only the lines spelled as string literals. See [`documented`] for why the
/// two questions are separate.
pub(crate) fn doc_lines(attrs: &[Attribute]) -> Vec<String> {
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

#[cfg(test)]
mod tests {
    use super::*;

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

    /// The `_ => true` arm is the whole reason this function exists apart from
    /// `doc_lines`: a doc built by a macro is documentation, and calling it
    /// absent would refuse a documented item with a message saying the opposite.
    #[test]
    fn documentation_a_macro_built_still_counts() {
        let built: syn::Field = syn::parse_quote!(
            #[doc = concat!("Restricts the response to ", "orders")]
            pub status: Option<u32>
        );

        assert!(documented(&built.attrs));
        // Unreadable from here, which is why the two questions are separate.
        assert!(doc_lines(&built.attrs).is_empty());
    }

    #[test]
    fn absent_and_blank_documentation_both_read_as_undocumented() {
        let none: syn::Field = syn::parse_quote!(
            #[serde(default)]
            pub status: Option<u32>
        );
        let blank: syn::Field = syn::parse_quote!(
            ///
            pub status: Option<u32>
        );
        let real: syn::Field = syn::parse_quote!(
            /// Restricts the response.
            pub status: Option<u32>
        );

        assert!(!documented(&none.attrs));
        assert!(!documented(&blank.attrs));
        assert!(documented(&real.attrs));
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
