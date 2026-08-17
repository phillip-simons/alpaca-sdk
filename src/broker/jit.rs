//! [Just-in-time funding](https://docs.alpaca.markets/us/reference/get-v1-transfers-jit-ledgers):
//! ledgers, daily trading limits, reports, and settlements.
//!
//! JIT correspondents hold client cash themselves and settle with Alpaca on a
//! net basis at the end of the day, rather than pre-funding each account. The
//! ledger is the running record of that obligation.
//!
//! **Two path families, one feature.** Ledgers, limits, reports and balances
//! live under `/v1/transfers/jit/…`; settlements live under `/v1/jit/…`. That is
//! Alpaca's split, not a mistake here.
//!
//! Spec-derived, and unverified against a live response.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::broker::settlements::{SettlementAssetClass, TransmitterInfo};
use crate::types::SupportedCurrencies;
use crate::types::Validated;
use crate::types::setters::Setters;
use crate::types::wire::wire_enum;

wire_enum! {
    /// Which JIT report to run.
    pub enum JitReportType {
        /// Every transaction.
        Detail => "detail",
        /// Net position by account.
        NetSummary => "net_summary",
        /// The net payment due.
        NetPayment => "net_payment",
        /// The net payment, final.
        NetPaymentFinal => "net_payment_final",
        /// Gross position by account.
        GrossSummary => "gross_summary",
        /// The gross payment due.
        GrossPayment => "gross_payment",
        /// The gross payment, final.
        GrossPaymentFinal => "gross_payment_final",
        /// What is owed.
        Obligation => "obligation",
    }
}

wire_enum! {
    /// Whether a report comes back inline or as a link.
    pub enum JitResponseType {
        /// In the response body.
        Inline => "inline",
        /// As a presigned URL to fetch.
        DownloadUrl => "download_url",
    }
}

/// A JIT ledger.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct JitLedger {
    /// Alpaca's identifier for the ledger.
    #[serde(default)]
    pub id: Option<String>,
    /// Its name.
    #[serde(default)]
    pub ledger_name: Option<String>,
    /// Whether it is open.
    #[serde(default)]
    pub status: Option<String>,
    /// When it was opened.
    #[serde(default)]
    pub created_at: Option<String>,
}

/// One movement on a ledger.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct JitLedgerTransaction {
    /// The account it belongs to.
    #[serde(default)]
    pub account_id: Option<String>,
    /// That account's number.
    #[serde(default)]
    pub account_no: Option<String>,
    /// That account's name.
    #[serde(default)]
    pub account_name: Option<String>,
    /// The account on the other side.
    #[serde(default)]
    pub contra_account_name: Option<String>,
    /// What kind of entry it is.
    #[serde(default)]
    pub entry_type: Option<String>,
    /// What it is for.
    #[serde(default)]
    pub description: Option<String>,
    /// How much moved.
    #[serde(default, with = "crate::types::option_decimal")]
    pub amount: Option<Decimal>,
    /// The balance after it.
    #[serde(default, with = "crate::types::option_decimal")]
    pub balance: Option<Decimal>,
    /// The business day.
    #[serde(default)]
    pub system_date: Option<String>,
}

/// A ledger's balance over a window, and the movements behind it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct JitLedgerBalances {
    /// The ledger.
    #[serde(default)]
    pub id: Option<String>,
    /// Its name.
    #[serde(default)]
    pub ledger_name: Option<String>,
    /// Its number.
    #[serde(default)]
    pub ledger_no: Option<String>,
    /// The balance at the start of the window.
    #[serde(default, with = "crate::types::option_decimal")]
    pub starting_balance: Option<Decimal>,
    /// The balance at the end of it.
    #[serde(default, with = "crate::types::option_decimal")]
    pub ending_balance: Option<Decimal>,
    /// The net of everything in between.
    #[serde(default, with = "crate::types::option_decimal")]
    pub activity_amount: Option<Decimal>,
    /// The movements.
    #[serde(
        default,
        deserialize_with = "crate::types::serde_util::null_as_default"
    )]
    pub transactions: Vec<JitLedgerTransaction>,
}

/// A correspondent's trading limits for the day.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct JitTradingLimits {
    /// Which correspondent.
    #[serde(default)]
    pub correspondent: Option<String>,
    /// The net ceiling for the day.
    #[serde(default, with = "crate::types::option_decimal")]
    pub daily_net_limit: Option<Decimal>,
    /// How much of it is committed.
    #[serde(default, with = "crate::types::option_decimal")]
    pub in_use_limit: Option<Decimal>,
    /// Cash on hand.
    #[serde(default, with = "crate::types::option_decimal")]
    pub cash_held: Option<Decimal>,
    /// Buys already filled.
    #[serde(default, with = "crate::types::option_decimal")]
    pub executed_buys: Option<Decimal>,
    /// Sells already filled.
    #[serde(default, with = "crate::types::option_decimal")]
    pub executed_sells: Option<Decimal>,
    /// Buys still working.
    #[serde(default, with = "crate::types::option_decimal")]
    pub open_buys: Option<Decimal>,
    /// Sells still working.
    #[serde(default, with = "crate::types::option_decimal")]
    pub open_sells: Option<Decimal>,
}

/// A JIT report, however the caller asked for it.
///
/// The route answers with one of two shapes depending on `response_type`: a body
/// of report strings, or a link to download one.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum JitReport {
    /// A presigned URL to fetch the report from.
    Download(JitReportDownload),
    /// The report itself.
    Inline(Box<JitReportInline>),
}

impl Serialize for JitReport {
    /// Emits the inner value, not an externally-tagged wrapper.
    ///
    /// The derived form would write `{"Download": {…}}`, which the
    /// [`Deserialize`] below — and Alpaca — do not accept. Dropping
    /// `#[serde(untagged)]` for a hand-written deserializer means the serializer
    /// has to be hand-written too, or the type stops round-tripping through its
    /// own codec; that is the same defect the `Calendar` serializer exists to
    /// avoid.
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Download(download) => download.serialize(serializer),
            Self::Inline(inline) => inline.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for JitReport {
    /// Branches on the response shape rather than deriving `#[serde(untagged)]`.
    ///
    /// Untagged discards both arms' errors and reports only "data did not match
    /// any variant of untagged enum `JitReport`". That throws away the one
    /// useful thing here: [`JitReportInline`]'s deserializer names every report
    /// key it expected, and this route has never been seen against a real
    /// payload — so the first response that does not fit is the most valuable
    /// bug report the crate can receive.
    ///
    /// `url` is the discriminator: it is required on the download shape and is
    /// not one of the report keys.
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error as _;

        let value = serde_json::Value::deserialize(deserializer)?;
        if value.get("url").is_some() {
            return JitReportDownload::deserialize(value)
                .map(Self::Download)
                .map_err(D::Error::custom);
        }
        JitReportInline::deserialize(value)
            .map(|inline| Self::Inline(Box::new(inline)))
            .map_err(D::Error::custom)
    }
}

/// A report served in the response body.
///
/// One field per [`JitReportType`], and exactly one of them is populated: the
/// one matching the report that was asked for.
///
/// **Adding a field here means editing the hand-written `Deserialize` below.**
/// It mirrors the fields into a private `Raw`, so a new one is silently dropped
/// on the way in, and the round-trip test cannot see it.
///
/// `Deserialize` is hand-written rather than derived because every field is
/// optional. A derived one accepts *any* JSON object — including one carrying a
/// key none of these names — and produces an all-`None` value. Sitting inside an
/// untagged enum, that made [`JitReport`] unable to fail: an unrecognised body
/// became `Inline` with nothing in it, so a settlement report came back
/// silently empty instead of erroring.
///
/// `Default` is deliberately absent. The deserializer refuses a body with no
/// report key, so a derived `Default` would hand out a value that will not
/// round-trip through its own codec — the same asymmetry the `Calendar`
/// serializer in this crate exists to remove. `Serialize` stays because [`JitReport`]'s own
/// hand-written one delegates to it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct JitReportInline {
    /// The detail report.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// The net summary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub net_summary: Option<String>,
    /// The net payment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub net_payment: Option<String>,
    /// The final net payment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub net_payment_final: Option<String>,
    /// The gross summary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gross_summary: Option<String>,
    /// The gross payment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gross_payment: Option<String>,
    /// The final gross payment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gross_payment_final: Option<String>,
    /// The obligation report.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub obligation: Option<String>,
}

impl<'de> Deserialize<'de> for JitReportInline {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Raw {
            #[serde(default)]
            detail: Option<String>,
            #[serde(default)]
            net_summary: Option<String>,
            #[serde(default)]
            net_payment: Option<String>,
            #[serde(default)]
            net_payment_final: Option<String>,
            #[serde(default)]
            gross_summary: Option<String>,
            #[serde(default)]
            gross_payment: Option<String>,
            #[serde(default)]
            gross_payment_final: Option<String>,
            #[serde(default)]
            obligation: Option<String>,
        }

        let raw = Raw::deserialize(deserializer)?;
        let report = Self {
            detail: raw.detail,
            net_summary: raw.net_summary,
            net_payment: raw.net_payment,
            net_payment_final: raw.net_payment_final,
            gross_summary: raw.gross_summary,
            gross_payment: raw.gross_payment,
            gross_payment_final: raw.gross_payment_final,
            obligation: raw.obligation,
        };

        // The check that makes the untagged enum able to fail: a body with none
        // of the known report keys is not an empty report, it is a shape this
        // crate does not model.
        if report.report().is_none() {
            return Err(serde::de::Error::custom(
                "no known JIT report key in the response body; expected one of \
                 detail, net_summary, net_payment, net_payment_final, \
                 gross_summary, gross_payment, gross_payment_final, obligation",
            ));
        }
        Ok(report)
    }
}

impl JitReportInline {
    /// The populated report, whichever kind was asked for.
    #[must_use]
    pub fn report(&self) -> Option<&str> {
        self.detail
            .as_deref()
            .or(self.net_summary.as_deref())
            .or(self.net_payment.as_deref())
            .or(self.net_payment_final.as_deref())
            .or(self.gross_summary.as_deref())
            .or(self.gross_payment.as_deref())
            .or(self.gross_payment_final.as_deref())
            .or(self.obligation.as_deref())
    }
}

/// A report served as a link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct JitReportDownload {
    /// Where to fetch it.
    pub url: String,
    /// What it is called.
    pub filename: String,
    /// When the link stops working.
    pub expires_at: DateTime<Utc>,
}

/// Filters for a JIT report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Setters, Validated)]
#[non_exhaustive]
pub struct GetJitReportRequest {
    /// Which report to run.
    pub report_type: JitReportType,
    /// The business day to report on.
    pub system_date: NaiveDate,
    /// Which book to report on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(doc = "Restricts the report to one book.")]
    pub asset_class: Option<SettlementAssetClass>,
    /// Whether to serve the report or a link to it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(doc = "Asks for a link rather than the report itself.")]
    pub response_type: Option<JitResponseType>,
}

impl GetJitReportRequest {
    /// The `report_type` report for `system_date`.
    ///
    /// Both are required by the route, so both are arguments rather than
    /// builder steps.
    #[must_use]
    pub fn new(report_type: JitReportType, system_date: NaiveDate) -> Self {
        Self {
            report_type,
            system_date,
            asset_class: None,
            response_type: None,
        }
    }
}

/// A window over a ledger's balances.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Setters, Validated)]
#[non_exhaustive]
pub struct GetJitBalancesRequest {
    /// The first day.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_date: Option<NaiveDate>,
    /// The last day.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_date: Option<NaiveDate>,
}

impl GetJitBalancesRequest {
    /// A request with no window.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Restricts the window.
    ///
    /// # Errors
    /// Returns [`Error::InvalidRequest`](crate::Error::InvalidRequest) if `end`
    /// is before `start`.
    pub fn between(mut self, start: NaiveDate, end: NaiveDate) -> crate::Result<Self> {
        if end < start {
            return Err(crate::Error::InvalidRequest(format!(
                "end_date ({end}) is before start_date ({start})"
            )));
        }
        self.start_date = Some(start);
        self.end_date = Some(end);
        Ok(self)
    }
}

/// One account's share of a JIT settlement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Setters)]
#[non_exhaustive]
pub struct JitSettlementAccount {
    /// The account.
    pub account_number: String,
    /// How much it settles.
    #[serde(with = "crate::types::decimal")]
    pub amount: Decimal,
    /// Who sent the money, for travel-rule reporting.
    pub transmitter_info: TransmitterInfo,
}

impl JitSettlementAccount {
    /// Settles `amount` for `account_number`, sent by `transmitter_info`.
    #[must_use]
    pub fn new(account_number: String, amount: Decimal, transmitter_info: TransmitterInfo) -> Self {
        Self {
            account_number,
            amount,
            transmitter_info,
        }
    }
}

/// A request to settle a day's JIT obligation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Setters)]
#[non_exhaustive]
pub struct CreateJitSettlementRequest {
    /// The accounts to settle.
    pub accounts: Vec<JitSettlementAccount>,
    /// Which book.
    pub asset_class: SettlementAssetClass,
    /// The currency.
    pub currency: crate::types::SupportedCurrencies,
    /// Free-form notes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub additional_info: Option<String>,
}

impl CreateJitSettlementRequest {
    /// Settles `accounts` in `currency` on the `asset_class` book.
    #[must_use]
    pub fn new(
        accounts: Vec<JitSettlementAccount>,
        asset_class: SettlementAssetClass,
        currency: SupportedCurrencies,
    ) -> Self {
        Self {
            accounts,
            asset_class,
            currency,
            additional_info: None,
        }
    }
}

impl Validated for CreateJitSettlementRequest {
    /// The coherence checks a settlement cannot pass without contradicting
    /// itself.
    ///
    /// # Errors
    /// Returns [`Error::InvalidRequest`](crate::Error::InvalidRequest) if no
    /// accounts are named, or one of them settles a non-positive amount.
    fn validate(&self) -> crate::Result<()> {
        if self.accounts.is_empty() {
            return Err(crate::Error::InvalidRequest(
                "a settlement must name at least one account".to_owned(),
            ));
        }
        for account in &self.accounts {
            if account.amount <= Decimal::ZERO {
                return Err(crate::Error::InvalidRequest(format!(
                    "account {} settles {}, which is not a positive amount",
                    account.account_number, account.amount
                )));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_balances_window_rejects_an_end_before_its_start() {
        let start: NaiveDate = "2026-01-05".parse().unwrap();
        let end: NaiveDate = "2026-01-01".parse().unwrap();

        assert!(GetJitBalancesRequest::new().between(start, end).is_err());

        let request = GetJitBalancesRequest::new().between(end, start).unwrap();
        assert_eq!(request.start_date, Some(end));
        assert_eq!(request.end_date, Some(start));
    }

    #[test]
    fn a_report_decodes_as_either_shape() {
        let download: JitReport = serde_json::from_value(serde_json::json!({
            "url": "https://example.invalid/report.csv",
            "filename": "report.csv",
            "expires_at": "2026-01-02T15:04:05Z",
        }))
        .unwrap();
        assert!(matches!(download, JitReport::Download(_)));

        let inline: JitReport = serde_json::from_value(serde_json::json!({
            "net_summary": "account,amount\n123,1.00\n",
        }))
        .unwrap();
        assert!(matches!(inline, JitReport::Inline(_)));
    }

    #[test]
    fn a_settlement_that_settles_nothing_is_refused() {
        let request = CreateJitSettlementRequest::new(
            Vec::new(),
            SettlementAssetClass::UsEquity,
            SupportedCurrencies::Usd,
        );
        assert!(request.validate().is_err());
    }

    #[test]
    fn a_negative_settlement_amount_is_refused() {
        let request = CreateJitSettlementRequest::new(
            vec![JitSettlementAccount {
                account_number: "123".to_owned(),
                amount: Decimal::new(-1, 0),
                transmitter_info: TransmitterInfo::default(),
            }],
            SettlementAssetClass::Crypto,
            SupportedCurrencies::Usd,
        );
        assert!(request.validate().is_err());
    }

    /// Dropping `#[serde(untagged)]` for a hand-written `Deserialize` broke this
    /// once: the derived `Serialize` emitted `{"Download": {…}}`, which the
    /// deserializer then refused. Both shapes have to survive their own codec.
    #[test]
    fn a_jit_report_round_trips_in_both_shapes() {
        let download = serde_json::json!({
            "url": "https://example.test/report.csv",
            "filename": "report.csv",
            "expires_at": "2026-01-02T00:00:00Z"
        });
        let inline = serde_json::json!({"net_summary": "account,amount\n1,2\n"});

        for wire in [download, inline] {
            let report: JitReport = serde_json::from_value(wire.clone()).unwrap();
            let encoded = serde_json::to_value(&report).unwrap();
            assert_eq!(encoded, wire, "the encoded form must be what Alpaca sends");

            let decoded: JitReport = serde_json::from_value(encoded).unwrap();
            assert_eq!(decoded, report);
        }
    }

    /// And the check that made the decoder able to fail at all: a body carrying
    /// none of the known report keys is a shape this crate does not model, not
    /// an empty report.
    #[test]
    fn a_body_with_no_known_report_key_is_an_error() {
        let error = serde_json::from_value::<JitReport>(serde_json::json!({"surprise": "x"}))
            .unwrap_err()
            .to_string();
        assert!(error.contains("no known JIT report key"), "{error}");
    }
}
