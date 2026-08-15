//! Corporate action models.
//!
//! The endpoint groups actions by type under `corporate_actions`, with one array
//! per kind. Each kind is its own field rather than one map keyed by type name,
//! so reading an entry needs no downcast and the compiler knows which fields it
//! has.
//!
//! Every record carries an `id` that the specs do not declare.
//!
//! Rates and cash amounts are `f64` here rather than [`Decimal`], even though
//! [`CashDividend::rate`] models the same quantity as the trading surface's
//! `CorporateActionAnnouncement::cash`, which is a [`Decimal`]. Both follow
//! the rule; the two endpoints simply do not agree on
//! the wire. `fixtures/data/test_corporate_actions__test_get_corporate_actions__02.json`
//! sends `"rate": 0.086928` as a bare JSON number, while
//! `fixtures/trading/test_corporate_announcements__test_get_announcements__01.json`
//! sends `"cash": "0.018"` as a string. Reading a string amount as a float
//! loses precision, so that one is [`Decimal`]; a number that arrived as a
//! float gains nothing from being widened after the fact.
//!
//! [`Decimal`]: rust_decimal::Decimal

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

/// A stock split that increases the share count.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ForwardSplit {
    /// Alpaca's identifier for the action.
    #[serde(default)]
    pub id: Option<String>,
    /// The symbol being split.
    pub symbol: String,
    /// The security's CUSIP.
    pub cusip: String,
    /// Numerator of the split ratio.
    pub new_rate: f64,
    /// Denominator of the split ratio.
    pub old_rate: f64,
    /// When the action was processed.
    pub process_date: NaiveDate,
    /// First date on which buying does not confer entitlement.
    pub ex_date: NaiveDate,
    /// Date a settled position must be held to receive the entitlement.
    #[serde(default)]
    pub record_date: Option<NaiveDate>,
    /// When the action takes effect on balances.
    #[serde(default)]
    pub payable_date: Option<NaiveDate>,
    /// When any due bill is redeemed.
    #[serde(default)]
    pub due_bill_redemption_date: Option<NaiveDate>,
}

/// A stock split that reduces the share count.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ReverseSplit {
    /// Alpaca's identifier for the action.
    #[serde(default)]
    pub id: Option<String>,
    /// The symbol being split.
    pub symbol: String,
    /// CUSIP before the split.
    pub old_cusip: String,
    /// CUSIP after the split.
    pub new_cusip: String,
    /// Numerator of the split ratio.
    pub new_rate: f64,
    /// Denominator of the split ratio.
    pub old_rate: f64,
    /// When the action was processed.
    pub process_date: NaiveDate,
    /// First date on which buying does not confer entitlement.
    pub ex_date: NaiveDate,
    /// Date a settled position must be held to receive the entitlement.
    #[serde(default)]
    pub record_date: Option<NaiveDate>,
    /// When the action takes effect on balances.
    #[serde(default)]
    pub payable_date: Option<NaiveDate>,
}

/// A unit separating into its component securities.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct UnitSplit {
    /// Alpaca's identifier for the action.
    #[serde(default)]
    pub id: Option<String>,
    /// Symbol of the unit being split.
    pub old_symbol: String,
    /// CUSIP of the unit being split.
    pub old_cusip: String,
    /// Rate of the unit being split.
    pub old_rate: f64,
    /// Symbol of the primary resulting security.
    pub new_symbol: String,
    /// CUSIP of the primary resulting security.
    pub new_cusip: String,
    /// Rate of the primary resulting security.
    pub new_rate: f64,
    /// Symbol of the secondary resulting security.
    pub alternate_symbol: String,
    /// CUSIP of the secondary resulting security.
    pub alternate_cusip: String,
    /// Rate of the secondary resulting security.
    pub alternate_rate: f64,
    /// When the action was processed.
    pub process_date: NaiveDate,
    /// When the action takes effect.
    pub effective_date: NaiveDate,
    /// When the action takes effect on balances.
    #[serde(default)]
    pub payable_date: Option<NaiveDate>,
}

/// A dividend paid in shares.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct StockDividend {
    /// Alpaca's identifier for the action.
    #[serde(default)]
    pub id: Option<String>,
    /// The paying symbol.
    pub symbol: String,
    /// The security's CUSIP.
    pub cusip: String,
    /// Shares paid per share held.
    pub rate: f64,
    /// When the action was processed.
    pub process_date: NaiveDate,
    /// First date on which buying does not confer entitlement.
    pub ex_date: NaiveDate,
    /// Date a settled position must be held to receive the entitlement.
    #[serde(default)]
    pub record_date: Option<NaiveDate>,
    /// When the action takes effect on balances.
    #[serde(default)]
    pub payable_date: Option<NaiveDate>,
}

/// A dividend paid in cash.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct CashDividend {
    /// Alpaca's identifier for the action.
    #[serde(default)]
    pub id: Option<String>,
    /// The paying symbol.
    pub symbol: String,
    /// The security's CUSIP.
    pub cusip: String,
    /// Cash paid per share held.
    pub rate: f64,
    /// Whether this is a special dividend.
    pub special: bool,
    /// Whether this is a foreign dividend.
    pub foreign: bool,
    /// When the action was processed.
    pub process_date: NaiveDate,
    /// First date on which buying does not confer entitlement.
    pub ex_date: NaiveDate,
    /// Date a settled position must be held to receive the entitlement.
    #[serde(default)]
    pub record_date: Option<NaiveDate>,
    /// When the action takes effect on balances.
    #[serde(default)]
    pub payable_date: Option<NaiveDate>,
    /// When a due bill attaches.
    #[serde(default)]
    pub due_bill_on_date: Option<NaiveDate>,
    /// When a due bill detaches.
    #[serde(default)]
    pub due_bill_off_date: Option<NaiveDate>,
}

/// A subsidiary distributed to shareholders as a separate security.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct SpinOff {
    /// Alpaca's identifier for the action.
    #[serde(default)]
    pub id: Option<String>,
    /// Symbol of the parent.
    pub source_symbol: String,
    /// CUSIP of the parent.
    pub source_cusip: String,
    /// Rate of the parent security.
    pub source_rate: f64,
    /// Symbol of the spun-off security.
    pub new_symbol: String,
    /// CUSIP of the spun-off security.
    pub new_cusip: String,
    /// Rate of the spun-off security.
    pub new_rate: f64,
    /// When the action was processed.
    pub process_date: NaiveDate,
    /// First date on which buying does not confer entitlement.
    pub ex_date: NaiveDate,
    /// Date a settled position must be held to receive the entitlement.
    #[serde(default)]
    pub record_date: Option<NaiveDate>,
    /// When the action takes effect on balances.
    #[serde(default)]
    pub payable_date: Option<NaiveDate>,
    /// When any due bill is redeemed.
    #[serde(default)]
    pub due_bill_redemption_date: Option<NaiveDate>,
}

/// An acquisition settled entirely in cash.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct CashMerger {
    /// Alpaca's identifier for the action.
    #[serde(default)]
    pub id: Option<String>,
    /// Symbol of the acquiring company.
    #[serde(default)]
    pub acquirer_symbol: Option<String>,
    /// CUSIP of the acquiring company.
    #[serde(default)]
    pub acquirer_cusip: Option<String>,
    /// Symbol of the acquired company.
    pub acquiree_symbol: String,
    /// CUSIP of the acquired company.
    pub acquiree_cusip: String,
    /// Cash paid per share held.
    pub rate: f64,
    /// When the action was processed.
    pub process_date: NaiveDate,
    /// When the action takes effect.
    pub effective_date: NaiveDate,
    /// When the action takes effect on balances.
    #[serde(default)]
    pub payable_date: Option<NaiveDate>,
}

/// An acquisition settled entirely in shares.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct StockMerger {
    /// Alpaca's identifier for the action.
    #[serde(default)]
    pub id: Option<String>,
    /// Symbol of the acquiring company.
    pub acquirer_symbol: String,
    /// CUSIP of the acquiring company.
    pub acquirer_cusip: String,
    /// Shares of the acquirer paid per share held.
    pub acquirer_rate: f64,
    /// Symbol of the acquired company.
    pub acquiree_symbol: String,
    /// CUSIP of the acquired company.
    pub acquiree_cusip: String,
    /// Rate of the acquired security.
    pub acquiree_rate: f64,
    /// When the action was processed.
    pub process_date: NaiveDate,
    /// When the action takes effect.
    pub effective_date: NaiveDate,
    /// When the action takes effect on balances.
    #[serde(default)]
    pub payable_date: Option<NaiveDate>,
}

/// An acquisition settled in a mix of shares and cash.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct StockAndCashMerger {
    /// Alpaca's identifier for the action.
    #[serde(default)]
    pub id: Option<String>,
    /// Symbol of the acquiring company.
    pub acquirer_symbol: String,
    /// CUSIP of the acquiring company.
    pub acquirer_cusip: String,
    /// Shares of the acquirer paid per share held.
    pub acquirer_rate: f64,
    /// Symbol of the acquired company.
    pub acquiree_symbol: String,
    /// CUSIP of the acquired company.
    pub acquiree_cusip: String,
    /// Rate of the acquired security.
    pub acquiree_rate: f64,
    /// Cash paid per share held.
    pub cash_rate: f64,
    /// When the action was processed.
    pub process_date: NaiveDate,
    /// When the action takes effect.
    pub effective_date: NaiveDate,
    /// When the action takes effect on balances.
    #[serde(default)]
    pub payable_date: Option<NaiveDate>,
}

/// A security redeemed for cash.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Redemption {
    /// Alpaca's identifier for the action.
    #[serde(default)]
    pub id: Option<String>,
    /// The redeemed symbol.
    pub symbol: String,
    /// The security's CUSIP.
    pub cusip: String,
    /// Cash paid per share held.
    pub rate: f64,
    /// When the action was processed.
    pub process_date: NaiveDate,
    /// When the action takes effect on balances.
    #[serde(default)]
    pub payable_date: Option<NaiveDate>,
}

/// A ticker or CUSIP change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct NameChange {
    /// Alpaca's identifier for the action.
    #[serde(default)]
    pub id: Option<String>,
    /// The previous symbol.
    pub old_symbol: String,
    /// The previous CUSIP.
    pub old_cusip: String,
    /// The new symbol.
    pub new_symbol: String,
    /// The new CUSIP.
    pub new_cusip: String,
    /// When the action was processed.
    pub process_date: NaiveDate,
}

/// A security removed from accounts as worthless.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct WorthlessRemoval {
    /// Alpaca's identifier for the action.
    #[serde(default)]
    pub id: Option<String>,
    /// The removed symbol.
    pub symbol: String,
    /// The security's CUSIP.
    pub cusip: String,
    /// When the action was processed.
    pub process_date: NaiveDate,
}

/// Subscription rights distributed to shareholders.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct RightsDistribution {
    /// Alpaca's identifier for the action.
    #[serde(default)]
    pub id: Option<String>,
    /// Symbol the rights derive from.
    pub source_symbol: String,
    /// CUSIP the rights derive from.
    pub source_cusip: String,
    /// Symbol of the distributed rights.
    pub new_symbol: String,
    /// CUSIP of the distributed rights.
    pub new_cusip: String,
    /// Rights distributed per share held.
    pub rate: f64,
    /// When the action was processed.
    pub process_date: NaiveDate,
    /// First date on which buying does not confer entitlement.
    pub ex_date: NaiveDate,
    /// When the action takes effect on balances.
    #[serde(default)]
    pub payable_date: Option<NaiveDate>,
    /// Date a settled position must be held to receive the entitlement.
    #[serde(default)]
    pub record_date: Option<NaiveDate>,
    /// When the rights expire.
    #[serde(default)]
    pub expiration_date: Option<NaiveDate>,
}

/// Corporate actions grouped by kind, as the endpoint returns them.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct CorporateActions {
    /// Splits that increase the share count.
    #[serde(default)]
    pub forward_splits: Vec<ForwardSplit>,
    /// Splits that reduce the share count.
    #[serde(default)]
    pub reverse_splits: Vec<ReverseSplit>,
    /// Units separating into their components.
    #[serde(default)]
    pub unit_splits: Vec<UnitSplit>,
    /// Dividends paid in shares.
    #[serde(default)]
    pub stock_dividends: Vec<StockDividend>,
    /// Dividends paid in cash.
    #[serde(default)]
    pub cash_dividends: Vec<CashDividend>,
    /// Subsidiaries distributed as separate securities.
    #[serde(default)]
    pub spin_offs: Vec<SpinOff>,
    /// Acquisitions settled in cash.
    #[serde(default)]
    pub cash_mergers: Vec<CashMerger>,
    /// Acquisitions settled in shares.
    #[serde(default)]
    pub stock_mergers: Vec<StockMerger>,
    /// Acquisitions settled in shares and cash.
    #[serde(default)]
    pub stock_and_cash_mergers: Vec<StockAndCashMerger>,
    /// Securities redeemed for cash.
    #[serde(default)]
    pub redemptions: Vec<Redemption>,
    /// Ticker and CUSIP changes.
    #[serde(default)]
    pub name_changes: Vec<NameChange>,
    /// Securities removed as worthless.
    #[serde(default)]
    pub worthless_removals: Vec<WorthlessRemoval>,
    /// Subscription rights distributions.
    #[serde(default)]
    pub rights_distributions: Vec<RightsDistribution>,
}

impl CorporateActions {
    /// Total number of actions across every kind.
    #[must_use]
    pub fn len(&self) -> usize {
        self.forward_splits.len()
            + self.reverse_splits.len()
            + self.unit_splits.len()
            + self.stock_dividends.len()
            + self.cash_dividends.len()
            + self.spin_offs.len()
            + self.cash_mergers.len()
            + self.stock_mergers.len()
            + self.stock_and_cash_mergers.len()
            + self.redemptions.len()
            + self.name_changes.len()
            + self.worthless_removals.len()
            + self.rights_distributions.len()
    }

    /// Whether no actions were returned.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
