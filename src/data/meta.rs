//! The market data API's own decoder rings.
//!
//! Trades and quotes carry an [exchange code](crate::data::Trade::exchange) and a
//! list of [condition codes](crate::data::Trade::conditions), both single
//! characters, and neither means anything without a lookup. These are the
//! endpoints that publish those lookups:
//!
//! - `GET /v2/stocks/meta/exchanges`
//! - `GET /v2/stocks/meta/conditions/{ticktype}`
//! - `GET /v1beta1/options/meta/conditions/{ticktype}`
//!
//! No other SDK ports them, so the shapes here are the ones `just capture`
//! recorded from the live API, in `fixtures/live/`.
//!
//! # A single space is a condition code
//!
//! `" "` is `"Regular Sale"` — the most common condition on the tape. [`Codes`]
//! exists so the lookup goes through something that cannot trim it away: a bare
//! `HashMap` invites `.trim()` at the call site, and the ordinary case is the
//! one that would break.
//!
//! See <https://docs.alpaca.markets/us/reference/stockmetaconditions-1>.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::types::Validated;
use crate::types::wire::wire_enum;

/// Which kind of tick a condition-code table describes.
#[wire_enum]
pub enum TickType {
    /// Conditions that appear on trades.
    #[wire = "trade"]
    Trade,
    /// Conditions that appear on quotes.
    #[wire = "quote"]
    Quote,
}

/// The consolidated tape a stock reports on.
///
/// Required on the stock condition-code route and rejected with a 400 when
/// absent — a requirement the vendored spec states and no other SDK
/// implements. The option route takes no tape at all.
#[wire_enum(sorted)]
pub enum Tape {
    /// NYSE-listed securities.
    #[wire = "A"]
    A,
    /// NYSE American, NYSE Arca, and other regional listings.
    #[wire = "B"]
    B,
    /// Nasdaq-listed securities.
    #[wire = "C"]
    C,
}

/// A code-to-name table, as the `meta` endpoints publish it.
///
/// Wraps the map rather than exposing it directly so that [`Codes::name`] is the
/// obvious way to read one. The keys are exactly what the wire sent, including
/// the single space that means an ordinary trade.
///
/// ```
/// # use alpaca_sdk::data::Codes;
/// let codes: Codes = serde_json::from_str(r#"{" ": "Regular Sale", "I": "Odd Lot Trade"}"#)?;
///
/// assert_eq!(codes.name(" "), Some("Regular Sale"));
/// assert_eq!(codes.name("I"), Some("Odd Lot Trade"));
/// // Not a code the table knows, rather than a panic or a silent default.
/// assert_eq!(codes.name("zzz"), None);
/// # Ok::<(), serde_json::Error>(())
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Codes(HashMap<String, String>);

impl Codes {
    /// The name for a code, or `None` if the table does not list it.
    ///
    /// `code` is used verbatim. Alpaca's most common trade condition is a
    /// single space, so trimming here would lose the ordinary case.
    #[must_use]
    pub fn name(&self, code: &str) -> Option<&str> {
        self.0.get(code).map(String::as_str)
    }

    /// Names for a record's `conditions` list, in the same order.
    ///
    /// Each entry is `None` where the table has no such code, so a table that
    /// has fallen behind the tape degrades per code rather than as a whole.
    pub fn names<'a>(&'a self, codes: &'a [String]) -> impl Iterator<Item = Option<&'a str>> {
        codes.iter().map(|code| self.name(code))
    }

    /// The underlying map.
    #[must_use]
    pub fn as_map(&self) -> &HashMap<String, String> {
        &self.0
    }

    /// How many codes the table carries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the table is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<HashMap<String, String>> for Codes {
    fn from(map: HashMap<String, String>) -> Self {
        Self(map)
    }
}

impl<'a> IntoIterator for &'a Codes {
    type Item = (&'a String, &'a String);
    type IntoIter = std::collections::hash_map::Iter<'a, String, String>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

/// The `tape` parameter, which the stock condition route requires.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validated)]
pub(crate) struct TapeQuery {
    pub tape: Tape,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_single_space_survives_the_lookup() {
        // The most common condition on the tape. Any helper that trims, splits
        // on whitespace, or treats the empty string as absent loses it.
        let codes: Codes = serde_json::from_str(r#"{" ": "Regular Sale"}"#).unwrap();

        assert_eq!(codes.name(" "), Some("Regular Sale"));
        assert_eq!(codes.name(""), None);
        assert_eq!(codes.name("  "), None);
    }

    #[test]
    fn unknown_codes_read_as_absent_rather_than_erroring() {
        let codes: Codes = serde_json::from_str(r#"{"I": "Odd Lot Trade"}"#).unwrap();
        let conditions = vec![" ".to_owned(), "I".to_owned()];

        let names: Vec<_> = codes.names(&conditions).collect();
        assert_eq!(names, vec![None, Some("Odd Lot Trade")]);
    }

    #[test]
    fn tape_is_required_by_the_query_type() {
        // Not an Option: the route answers 400 without it.
        let query = TapeQuery { tape: Tape::A };
        assert_eq!(serde_json::to_value(&query).unwrap()["tape"], "A");
    }
}
