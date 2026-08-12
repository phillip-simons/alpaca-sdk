//! Hand-ported behavior that `wire_enum!` cannot generate.
//!
//! Kept separate from `enums.rs` so regenerating that file never drops it.

use crate::trading::enums::ActivityType;

impl ActivityType {
    /// Whether this activity type belongs to a trade activity rather than a
    /// non-trade activity.
    ///
    /// Currently that means exactly [`ActivityType::Fill`]. alpaca-py keeps this
    /// as a method rather than an inline comparison because the set may grow.
    #[must_use]
    pub fn is_trade_activity(&self) -> bool {
        matches!(self, Self::Fill)
    }

    /// The same check against a raw wire value, for use before deserializing.
    ///
    /// The account-activities endpoint returns a heterogeneous array whose
    /// element type is decided by this field.
    #[must_use]
    pub fn is_str_trade_activity(value: &str) -> bool {
        value == Self::Fill.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_fill_is_a_trade_activity() {
        assert!(ActivityType::Fill.is_trade_activity());
        assert!(!ActivityType::Div.is_trade_activity());
        assert!(!ActivityType::Unknown("NEWTHING".to_owned()).is_trade_activity());
    }

    #[test]
    fn raw_value_check_matches_the_typed_one() {
        assert!(ActivityType::is_str_trade_activity("FILL"));
        assert!(!ActivityType::is_str_trade_activity("DIV"));
        assert!(!ActivityType::is_str_trade_activity("fill"));
    }
}
