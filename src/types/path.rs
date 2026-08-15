//! Percent-encoding for values interpolated into a request path.
//!
//! Every route in this crate builds its path with `format!`, and most of what
//! gets interpolated is caller-supplied: a ticker symbol, an Alpaca-issued
//! reference string, or a [`wire_enum!`](crate::types::wire) value, whose
//! `Unknown(String)` variant means even the enum-typed segments carry whatever
//! the caller put in them. Interpolating those raw is how `BTC/USD` becomes two
//! path segments and how `..` reaches a route the caller never named.
//!
//! Alpaca's own reference states the requirement — *"Since the slash is a
//! special character in HTTP, use the URL encoded version instead, e.g.
//! `/v2/assets/BTC%2FUSDT`"* — so encoding is what the API asks for, not only
//! what safety asks for.
//!
//! # Why `.` is rejected rather than encoded
//!
//! Encoding handles every dangerous character except the dot, and the dot is
//! the one that cannot be handled that way. The URL parser reqwest hands the
//! string to implements the WHATWG rules, under which a segment of `..`,
//! `%2e%2e`, `.%2e` or `%2e.` is a double-dot segment and is removed — the
//! percent-encoded forms included. There is no spelling of a lone `.` or `..`
//! that survives as a literal segment.
//!
//! So a segment that is exactly `.`, `..`, or empty is refused with
//! [`Error::InvalidRequest`]. None of the three is a real symbol or identifier,
//! and refusing is the only option that does not silently address a different
//! route than the caller wrote.

use std::fmt;

use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};

use crate::error::{Error, Result};

/// Everything outside RFC 3986's *unreserved* set, which is
/// `A-Z a-z 0-9 - . _ ~`.
///
/// `.` is deliberately absent — see the module docs. `BRK.A` is a real ticker,
/// and encoding its dot would change a segment the API matches literally.
const SEGMENT: &AsciiSet = &CONTROLS
    // The delimiters that would end the segment, or the path, outright.
    .add(b'/')
    .add(b'?')
    .add(b'#')
    // `%` first among the rest: encoding it is what stops a caller's `%2E`
    // from arriving as a dot.
    .add(b'%')
    .add(b'\\')
    // RFC 3986 gen-delims and sub-delims.
    .add(b':')
    .add(b'@')
    .add(b'[')
    .add(b']')
    .add(b'!')
    .add(b'$')
    .add(b'&')
    .add(b'\'')
    .add(b'(')
    .add(b')')
    .add(b'*')
    .add(b'+')
    .add(b',')
    .add(b';')
    .add(b'=')
    // Characters a URL parser may rewrite or reject rather than carry.
    .add(b' ')
    .add(b'"')
    .add(b'<')
    .add(b'>')
    .add(b'`')
    .add(b'{')
    .add(b'}')
    .add(b'|')
    .add(b'^');

/// Percent-encodes `value` for use as one path segment.
///
/// # Errors
/// Returns [`Error::InvalidRequest`] if `value` is empty or is a dot segment
/// (`.` or `..`), neither of which can be expressed as a literal path segment.
pub(crate) fn segment(value: impl fmt::Display) -> Result<String> {
    let raw = value.to_string();

    if raw.is_empty() {
        return Err(Error::InvalidRequest(
            "a path segment cannot be empty".to_owned(),
        ));
    }
    if raw == "." || raw == ".." {
        return Err(Error::InvalidRequest(format!(
            "`{raw}` cannot be used as a path segment: a URL parser removes it \
             rather than sending it, so the request would address a different route"
        )));
    }

    Ok(utf8_percent_encode(&raw, SEGMENT).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_crypto_pair_is_one_segment_not_two() {
        // The case that made every crypto path route unusable, and the exact
        // form Alpaca's reference asks for.
        assert_eq!(segment("BTC/USD").unwrap(), "BTC%2FUSD");
    }

    #[test]
    fn a_dotted_ticker_is_left_alone() {
        // `.` is not encoded, because real tickers carry one and the API
        // matches the segment literally.
        assert_eq!(segment("BRK.A").unwrap(), "BRK.A");
    }

    #[test]
    fn a_plain_symbol_is_unchanged() {
        assert_eq!(segment("AAPL").unwrap(), "AAPL");
        assert_eq!(
            segment("b0b6dd9d-8b9b-48a9-ba46-b9d54906e415").unwrap(),
            "b0b6dd9d-8b9b-48a9-ba46-b9d54906e415"
        );
    }

    #[test]
    fn query_and_fragment_delimiters_cannot_escape_the_segment() {
        assert_eq!(segment("AAPL?foo=bar").unwrap(), "AAPL%3Ffoo%3Dbar");
        assert_eq!(segment("AAPL#frag").unwrap(), "AAPL%23frag");
    }

    #[test]
    fn a_percent_is_encoded_so_a_caller_cannot_spell_a_dot() {
        // Without this, `%2E%2E` would arrive at the parser as `..`.
        assert_eq!(segment("%2E%2E").unwrap(), "%252E%252E");
    }

    #[test]
    fn traversal_is_neutralized_by_encoding_the_slash() {
        // `..%2Fpositions` is a single literal segment, not a walk upwards.
        assert_eq!(segment("../positions").unwrap(), "..%2Fpositions");
    }

    #[test]
    fn the_dot_segments_are_refused_rather_than_encoded() {
        // These are the two that no encoding can express, so they are the two
        // that have to be errors. `..` reached `DELETE /v2/` and `.` reached
        // `DELETE /v2/positions/` — the close-all route.
        for value in [".", ".."] {
            let error = segment(value).unwrap_err();
            assert!(
                matches!(error, Error::InvalidRequest(_)),
                "expected InvalidRequest for {value:?}, got {error:?}"
            );
        }
    }

    #[test]
    fn an_empty_segment_is_refused() {
        // An empty symbol would collapse `/positions/{asset}` to `/positions`,
        // which is a different route with a far larger blast radius.
        assert!(matches!(segment("").unwrap_err(), Error::InvalidRequest(_)));
    }

    #[test]
    fn a_space_and_non_ascii_survive_as_an_encoded_segment() {
        assert_eq!(segment("a b").unwrap(), "a%20b");
        assert_eq!(segment("é").unwrap(), "%C3%A9");
    }
}
