//! Bar aggregation intervals.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use crate::error::{Error, Result};
use crate::types::wire::wire_enum;

/// The unit of time a [`TimeFrame`] is measured in.
#[wire_enum]
pub enum TimeFrameUnit {
    /// Minutes.
    #[wire = "Min"]
    Minute,
    /// Hours.
    #[wire = "Hour"]
    Hour,
    /// Days.
    #[wire = "Day"]
    Day,
    /// Weeks.
    #[wire = "Week"]
    Week,
    /// Months.
    #[wire = "Month"]
    Month,
}

/// A bar interval: a positive multiple of a [`TimeFrameUnit`].
///
/// Alpaca constrains which multiples are legal per unit, and rejects the rest at
/// request time. [`TimeFrame::new`] applies the same rules up front, so an
/// invalid interval is a local error rather than a round trip.
///
/// ```
/// # use alpaca_sdk::data::{TimeFrame, TimeFrameUnit};
/// assert_eq!(TimeFrame::minute().to_string(), "1Min");
/// assert_eq!(TimeFrame::new(15, TimeFrameUnit::Minute).unwrap().to_string(), "15Min");
/// assert!(TimeFrame::new(60, TimeFrameUnit::Minute).is_err());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TimeFrame {
    amount: u32,
    unit: TimeFrameUnit,
}

impl TimeFrame {
    /// Builds an interval, validating the amount against the unit.
    ///
    /// # Errors
    /// Returns [`Error::InvalidRequest`] when the combination is one Alpaca
    /// rejects: a non-positive amount, more than 59 minutes, more than 23 hours,
    /// any day or week count other than 1, or a month count outside 1, 2, 3, 6,
    /// and 12.
    pub fn new(amount: u32, unit: TimeFrameUnit) -> Result<Self> {
        Self::validate(amount, &unit)?;
        Ok(Self { amount, unit })
    }

    /// The multiple of [`TimeFrame::unit`].
    #[must_use]
    pub fn amount(&self) -> u32 {
        self.amount
    }

    /// The unit of time.
    #[must_use]
    pub fn unit(&self) -> &TimeFrameUnit {
        &self.unit
    }

    /// A one-minute interval.
    #[must_use]
    pub fn minute() -> Self {
        Self {
            amount: 1,
            unit: TimeFrameUnit::Minute,
        }
    }

    /// A one-hour interval.
    #[must_use]
    pub fn hour() -> Self {
        Self {
            amount: 1,
            unit: TimeFrameUnit::Hour,
        }
    }

    /// A one-day interval.
    #[must_use]
    pub fn day() -> Self {
        Self {
            amount: 1,
            unit: TimeFrameUnit::Day,
        }
    }

    /// A one-week interval.
    #[must_use]
    pub fn week() -> Self {
        Self {
            amount: 1,
            unit: TimeFrameUnit::Week,
        }
    }

    /// A one-month interval.
    #[must_use]
    pub fn month() -> Self {
        Self {
            amount: 1,
            unit: TimeFrameUnit::Month,
        }
    }

    fn validate(amount: u32, unit: &TimeFrameUnit) -> Result<()> {
        let invalid = |reason: &str| Err(Error::InvalidRequest(reason.to_owned()));

        if amount == 0 {
            return invalid("amount must be a positive integer value");
        }

        match unit {
            TimeFrameUnit::Minute if amount > 59 => {
                invalid("minute units can only be used with amounts between 1 and 59")
            }
            TimeFrameUnit::Hour if amount > 23 => {
                invalid("hour units can only be used with amounts between 1 and 23")
            }
            TimeFrameUnit::Day | TimeFrameUnit::Week if amount != 1 => {
                invalid("day and week units can only be used with amount 1")
            }
            TimeFrameUnit::Month if !matches!(amount, 1 | 2 | 3 | 6 | 12) => {
                invalid("month units can only be used with amount 1, 2, 3, 6, or 12")
            }
            // An unrecognized unit came from the wire, so let the API judge it.
            _ => Ok(()),
        }
    }
}

impl fmt::Display for TimeFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.amount, self.unit.as_str())
    }
}

impl FromStr for TimeFrame {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        let split = value
            .find(|c: char| !c.is_ascii_digit())
            .ok_or_else(|| Error::InvalidRequest(format!("{value:?} has no time frame unit")))?;

        let (amount, unit) = value.split_at(split);
        let amount = amount
            .parse::<u32>()
            .map_err(|_| Error::InvalidRequest(format!("{value:?} has no time frame amount")))?;

        Self::new(amount, TimeFrameUnit::from(unit))
    }
}

impl Serialize for TimeFrame {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for TimeFrame {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::from_str(&raw).map_err(de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shorthand_constructors_build_the_documented_units() {
        assert_eq!(TimeFrame::minute().to_string(), "1Min");
        assert_eq!(TimeFrame::hour().to_string(), "1Hour");
        assert_eq!(TimeFrame::day().to_string(), "1Day");
        assert_eq!(TimeFrame::week().to_string(), "1Week");
        assert_eq!(TimeFrame::month().to_string(), "1Month");
    }

    #[test]
    fn valid_multiples_are_accepted() {
        for (amount, unit) in [
            (1, TimeFrameUnit::Minute),
            (59, TimeFrameUnit::Minute),
            (23, TimeFrameUnit::Hour),
            (1, TimeFrameUnit::Day),
            (1, TimeFrameUnit::Week),
            (12, TimeFrameUnit::Month),
        ] {
            assert!(
                TimeFrame::new(amount, unit.clone()).is_ok(),
                "{amount}{unit} should be valid"
            );
        }
    }

    #[test]
    fn invalid_multiples_are_rejected_locally() {
        for (amount, unit) in [
            (0, TimeFrameUnit::Minute),
            (60, TimeFrameUnit::Minute),
            (24, TimeFrameUnit::Hour),
            (2, TimeFrameUnit::Day),
            (2, TimeFrameUnit::Week),
            (4, TimeFrameUnit::Month),
            (5, TimeFrameUnit::Month),
        ] {
            assert!(
                TimeFrame::new(amount, unit.clone()).is_err(),
                "{amount}{unit} should be rejected"
            );
        }
    }

    #[test]
    fn parses_from_its_own_rendering() {
        for text in ["1Min", "15Min", "1Hour", "4Hour", "1Day", "1Week", "3Month"] {
            let parsed: TimeFrame = text.parse().unwrap();
            assert_eq!(parsed.to_string(), text);
        }
    }

    #[test]
    fn parsing_rejects_malformed_input() {
        for text in ["Min", "", "15", "0Min", "60Min"] {
            assert!(text.parse::<TimeFrame>().is_err(), "{text:?} should fail");
        }
    }

    #[test]
    fn serializes_to_the_query_parameter_form() {
        let frame = TimeFrame::new(5, TimeFrameUnit::Minute).unwrap();
        assert_eq!(serde_json::to_string(&frame).unwrap(), r#""5Min""#);

        let decoded: TimeFrame = serde_json::from_str(r#""5Min""#).unwrap();
        assert_eq!(decoded, frame);
    }

    #[test]
    fn an_unknown_unit_is_left_for_the_api_to_reject() {
        // Forward compatibility: if Alpaca adds a unit, parsing should not fail
        // before the request is even made.
        let frame = TimeFrame::new(2, TimeFrameUnit::from("Quarter")).unwrap();
        assert_eq!(frame.to_string(), "2Quarter");
    }
}
