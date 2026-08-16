//! [Fixed income assets](https://docs.alpaca.markets/us/reference/uscorporates-1):
//! the US corporate and treasury master lists, and entry requirements.
//!
//! Unlike most of the broker surface, these are verified against real payloads: `just harvest` lifted them out of
//! the Go SDK's tests, where they are raw JSON pasted into backtick literals,
//! so the wire's quirks survived the trip. See `fixtures/go/`.
//!
//! Prices and yields stay `f64`. They arrive as JSON numbers, not as the
//! strings the order and account money fields use — the same split the market
//! data models make. `regt_long` and `regt_short` on an entry requirement *do*
//! arrive as strings, and are [`Decimal`] accordingly.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::types::serde_util::comma_separated;
use crate::types::setters::Setters;
use crate::types::wire::wire_enum;

wire_enum! {
    /// Where a bond is in its life.
    pub enum BondStatus {
        /// Issued and not yet matured.
        Outstanding => "outstanding",
        /// Past its maturity date.
        Matured => "matured",
        /// Announced but not yet issued.
        PreIssuance => "pre_issuance",
    }
}

wire_enum! {
    /// How often a bond pays.
    pub enum CouponFrequency {
        /// Once a year.
        Annual => "annual",
        /// Twice a year, the US corporate norm.
        SemiAnnual => "semi_annual",
        /// Four times a year.
        Quarterly => "quarterly",
        /// Every month.
        Monthly => "monthly",
        /// Never — a discount instrument.
        Zero => "zero",
    }
}

wire_enum! {
    /// What kind of coupon a bond pays.
    pub enum CouponType {
        /// A fixed rate.
        Fixed => "fixed",
        /// A rate that resets against a benchmark.
        Floating => "floating",
        /// None; the return is the discount.
        Zero => "zero",
    }
}

wire_enum! {
    /// The day-count convention accrued interest is figured on.
    pub enum DayCount {
        /// Actual/360.
        Actual360 => "A/360",
        /// Actual/365.
        Actual365 => "A/365",
        /// 30/360.
        Thirty360 => "30/360",
        /// 30/365.
        Thirty365 => "30/365",
        /// Actual/actual.
        ActualActual => "A/A",
        /// 30E/360, the European variant.
        ThirtyE360 => "30E/360",
        /// Business/252.
        Business252 => "B/252",
        /// Actual/364.
        Actual364 => "A/364",
    }
}

wire_enum! {
    /// How an issuer may call a bond early.
    pub enum CallType {
        /// At a schedule of prices.
        Ordinary => "ordinary",
        /// At a price that compensates for lost coupons.
        MakeWhole => "make_whole",
        /// On a regulatory event.
        Regulatory => "regulatory",
        /// On some other named event.
        Special => "special",
    }
}

wire_enum! {
    /// Which way S&P expects a rating to move.
    pub enum CreditOutlook {
        /// Upward.
        Positive => "positive",
        /// Downward.
        Negative => "negative",
        /// Either way.
        Developing => "developing",
        /// Neither way.
        Stable => "stable",
        /// No rating.
        NotRated => "not_rated",
        /// A rating that says nothing useful.
        NotMeaningful => "not_meaningful",
    }
}

wire_enum! {
    /// Which kind of treasury instrument.
    pub enum TreasurySubtype {
        /// Long-dated, coupon-paying.
        Bond => "bond",
        /// Short-dated, discount.
        Bill => "bill",
        /// Medium-dated, coupon-paying.
        Note => "note",
        /// A stripped coupon or principal.
        Strips => "strips",
        /// Inflation-protected.
        Tips => "tips",
        /// Floating rate.
        Floating => "floating",
    }
}

/// A US corporate bond.
///
/// Nearly every field beyond the identifiers is optional, and that is the
/// captured payload's doing rather than caution: the Go SDK's own fixture omits
/// the whole liquidity block on one bond and the maturity date on another.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct UsCorporate {
    /// The CUSIP.
    pub cusip: String,
    /// The ISIN.
    pub isin: String,
    /// The issuer's ticker.
    pub ticker: String,
    /// Who issued it.
    pub issuer: String,
    /// The full description.
    pub description: String,
    /// The abbreviated description.
    pub description_short: String,
    /// Where the bond is in its life.
    pub bond_status: BondStatus,
    /// The industry sector.
    pub sector: String,
    /// Where the issuer is domiciled.
    pub country_domicile: String,
    /// Where the bond sits in the capital structure.
    pub seniority: String,
    /// The coupon rate, as a percentage.
    pub coupon: f64,
    /// How often it pays.
    pub coupon_frequency: CouponFrequency,
    /// What kind of coupon.
    pub coupon_type: CouponType,
    /// The day-count convention.
    pub day_count: DayCount,
    /// When interest starts accruing.
    pub dated_date: NaiveDate,
    /// When the bond was issued.
    pub issue_date: NaiveDate,
    /// The issue price.
    pub issue_price: f64,
    /// How much was issued.
    pub issue_size: f64,
    /// The smallest tradable denomination.
    pub issue_minimum_denomination: f64,
    /// The face value.
    pub par_value: f64,
    /// Whether the issuer may call it early.
    pub callable: bool,
    /// Whether it converts to equity.
    pub convertible: bool,
    /// Whether the holder may put it back.
    pub puttable: bool,
    /// Whether it never matures.
    pub perpetual: bool,
    /// Whether it was issued under Regulation S.
    pub reg_s: bool,
    /// Whether Alpaca will trade it.
    pub tradable: bool,
    /// Whether it can be traded in fractions.
    ///
    /// **The spec marks this required and a real payload omits it.** The Go
    /// SDK's captured corporate bond has no `fractionable` at all, so this
    /// defaults rather than failing the whole response — the same call the
    /// repo's rule zero describes, and the reason a fixture is worth more than
    /// a schema.
    #[serde(default)]
    pub fractionable: bool,
    /// Whether it can be bought on margin.
    pub marginable: bool,
    /// When it matures. Absent on a perpetual.
    #[serde(default)]
    pub maturity_date: Option<NaiveDate>,
    /// Interest accrued since the last coupon.
    #[serde(default)]
    pub accrued_interest: Option<f64>,
    /// How the issuer may call it.
    #[serde(default)]
    pub call_type: Option<CallType>,
    /// The next date it may be called.
    #[serde(default)]
    pub next_call_date: Option<NaiveDate>,
    /// The price it would be called at.
    #[serde(default)]
    pub next_call_price: Option<f64>,
    /// The first coupon date.
    #[serde(default)]
    pub first_coupon_date: Option<NaiveDate>,
    /// The most recent coupon date.
    #[serde(default)]
    pub last_coupon_date: Option<NaiveDate>,
    /// The next coupon date.
    #[serde(default)]
    pub next_coupon_date: Option<NaiveDate>,
    /// The last close price.
    #[serde(default)]
    pub close_price: Option<f64>,
    /// When that close was.
    #[serde(default)]
    pub close_price_date: Option<NaiveDate>,
    /// Yield to maturity at that close.
    #[serde(default)]
    pub close_yield_to_maturity: Option<f64>,
    /// Yield to worst at that close.
    #[serde(default)]
    pub close_yield_to_worst: Option<f64>,
    /// When it was reissued, if it was.
    #[serde(default)]
    pub reissue_date: Option<NaiveDate>,
    /// The reissue price.
    #[serde(default)]
    pub reissue_price: Option<f64>,
    /// How much was reissued.
    #[serde(default)]
    pub reissue_size: Option<f64>,
    /// S&P's rating.
    #[serde(default)]
    pub sp_rating: Option<String>,
    /// When that rating was set.
    #[serde(default)]
    pub sp_rating_date: Option<NaiveDate>,
    /// S&P's outlook.
    #[serde(default)]
    pub sp_outlook: Option<CreditOutlook>,
    /// When that outlook was set.
    #[serde(default)]
    pub sp_outlook_date: Option<NaiveDate>,
    /// Whether S&P has it on credit watch.
    #[serde(default)]
    pub sp_creditwatch: Option<String>,
    /// When it was put on credit watch.
    #[serde(default)]
    pub sp_creditwatch_date: Option<NaiveDate>,
    /// Aggregate liquidity score for institutional size.
    #[serde(default)]
    pub liquidity_institutional_aggregate: Option<f64>,
    /// Buy-side liquidity score for institutional size.
    #[serde(default)]
    pub liquidity_institutional_buy: Option<f64>,
    /// Sell-side liquidity score for institutional size.
    #[serde(default)]
    pub liquidity_institutional_sell: Option<f64>,
    /// Aggregate liquidity score for retail size.
    #[serde(default)]
    pub liquidity_retail_aggregate: Option<f64>,
    /// Buy-side liquidity score for retail size.
    #[serde(default)]
    pub liquidity_retail_buy: Option<f64>,
    /// Sell-side liquidity score for retail size.
    #[serde(default)]
    pub liquidity_retail_sell: Option<f64>,
    /// Aggregate liquidity score for micro size.
    #[serde(default)]
    pub liquidity_micro_aggregate: Option<f64>,
    /// Buy-side liquidity score for micro size.
    #[serde(default)]
    pub liquidity_micro_buy: Option<f64>,
    /// Sell-side liquidity score for micro size.
    #[serde(default)]
    pub liquidity_micro_sell: Option<f64>,
}

/// A US treasury instrument.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct UsTreasury {
    /// The CUSIP.
    pub cusip: String,
    /// The ISIN.
    pub isin: String,
    /// The full description.
    pub description: String,
    /// The abbreviated description.
    pub description_short: String,
    /// Which kind of instrument.
    pub subtype: TreasurySubtype,
    /// Where it is in its life.
    pub bond_status: BondStatus,
    /// The coupon rate. Zero on a bill.
    pub coupon: f64,
    /// How often it pays.
    pub coupon_frequency: CouponFrequency,
    /// What kind of coupon.
    pub coupon_type: CouponType,
    /// When it was issued.
    pub issue_date: NaiveDate,
    /// When it matures.
    pub maturity_date: NaiveDate,
    /// Whether Alpaca will trade it.
    pub tradable: bool,
    /// Whether it can be traded in fractions.
    #[serde(default)]
    pub fractionable: bool,
    /// The first coupon date.
    #[serde(default)]
    pub first_coupon_date: Option<NaiveDate>,
    /// The most recent coupon date.
    #[serde(default)]
    pub last_coupon_date: Option<NaiveDate>,
    /// The next coupon date.
    #[serde(default)]
    pub next_coupon_date: Option<NaiveDate>,
    /// The last close price.
    #[serde(default)]
    pub close_price: Option<f64>,
    /// When that close was.
    #[serde(default)]
    pub close_price_date: Option<NaiveDate>,
    /// Yield to maturity at that close.
    #[serde(default)]
    pub close_yield_to_maturity: Option<f64>,
    /// Yield to worst at that close.
    #[serde(default)]
    pub close_yield_to_worst: Option<f64>,
}

/// The corporates response, which nests its list under a key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct UsCorporates {
    /// The bonds.
    #[serde(
        default,
        deserialize_with = "crate::types::serde_util::null_as_default"
    )]
    pub us_corporates: Vec<UsCorporate>,
}

/// The treasuries response, which nests its list under a key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct UsTreasuries {
    /// The instruments.
    #[serde(
        default,
        deserialize_with = "crate::types::serde_util::null_as_default"
    )]
    pub us_treasuries: Vec<UsTreasury>,
}

/// What Regulation T requires to hold one symbol.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct EntryRequirement {
    /// The symbol.
    pub symbol: String,
    /// The margin requirement for a long position, as a fraction.
    #[serde(default, with = "crate::types::option_decimal")]
    pub regt_long: Option<Decimal>,
    /// The margin requirement for a short position, as a fraction.
    #[serde(default, with = "crate::types::option_decimal")]
    pub regt_short: Option<Decimal>,
}

/// Filters for the corporates list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Setters)]
#[non_exhaustive]
pub struct GetUsCorporatesRequest {
    /// Only bonds in this state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bond_status: Option<BondStatus>,
    /// Only these ISINs.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "comma_separated"
    )]
    #[setters(into)]
    pub isins: Option<Vec<String>>,
    /// Only these CUSIPs.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "comma_separated"
    )]
    #[setters(into)]
    pub cusips: Option<Vec<String>>,
    /// Only these issuer tickers.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "comma_separated"
    )]
    #[setters(into)]
    pub tickers: Option<Vec<String>>,
}

impl GetUsCorporatesRequest {
    /// A request with no filters.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

/// Filters for the treasuries list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Setters)]
#[non_exhaustive]
pub struct GetUsTreasuriesRequest {
    /// Only this kind of instrument.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtype: Option<TreasurySubtype>,
    /// Only instruments in this state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bond_status: Option<BondStatus>,
    /// Only these ISINs.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "comma_separated"
    )]
    #[setters(into)]
    pub isins: Option<Vec<String>>,
    /// Only these CUSIPs.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "comma_separated"
    )]
    #[setters(into)]
    pub cusips: Option<Vec<String>>,
}

impl GetUsTreasuriesRequest {
    /// A request with no filters.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

/// A request for entry requirements.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Setters)]
#[non_exhaustive]
pub struct GetEntryRequirementsRequest {
    /// The symbols to ask about, sent as one comma-separated parameter.
    #[serde(serialize_with = "crate::types::serde_util::comma_separated_required")]
    pub symbols: Vec<String>,
}

impl GetEntryRequirementsRequest {
    /// Requirements for `symbols`.
    #[must_use]
    pub fn new(symbols: Vec<String>) -> Self {
        Self { symbols }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bill_has_no_coupon_dates_and_still_decodes() {
        // Straight out of the Go SDK's fixture: a zero-coupon bill omits every
        // coupon date, and requiring them would fail on the commonest treasury.
        let treasury: UsTreasury = serde_json::from_value(serde_json::json!({
            "bond_status": "outstanding",
            "close_price": 99.6459,
            "coupon": 0,
            "coupon_frequency": "zero",
            "coupon_type": "zero",
            "cusip": "912797KJ5",
            "description": "test bill",
            "description_short": "tb",
            "isin": "US912797KJ59",
            "issue_date": "2026-04-03",
            "maturity_date": "2026-07-03",
            "subtype": "bill",
            "tradable": true,
        }))
        .unwrap();

        assert_eq!(treasury.subtype, TreasurySubtype::Bill);
        assert_eq!(treasury.first_coupon_date, None);
        assert_eq!(treasury.coupon, 0.0);
    }

    #[test]
    fn day_counts_keep_their_slashes() {
        // `30/360` and `A/360` are the wire values. A Rust-friendly renaming
        // here would send something Alpaca does not recognise.
        assert_eq!(DayCount::Thirty360.as_str(), "30/360");
        assert_eq!(DayCount::ActualActual.as_str(), "A/A");
    }

    #[test]
    fn entry_requirements_are_decimals_because_they_arrive_as_strings() {
        // Unlike the prices and yields on a bond, which are JSON numbers.
        let requirement: EntryRequirement = serde_json::from_value(serde_json::json!({
            "symbol": "AAPL",
            "regt_long": "0.5",
            "regt_short": "1.5",
        }))
        .unwrap();

        assert_eq!(requirement.regt_long, Some(Decimal::new(5, 1)));
    }
}
