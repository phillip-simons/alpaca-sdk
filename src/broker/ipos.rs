//! [IPO offerings](https://docs.alpaca.markets/us/reference/listipoofferings).
//!
//! What is coming to market, at what price band, and whether it is still taking
//! orders. Pairs with the IPO event stream on
//! [`BrokerClient::get_ipo_events`](crate::broker::BrokerClient::get_ipo_events).
//!
//! Spec-derived, and unverified against a live response.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::types::Validated;
use crate::types::setters::Setters;
use crate::types::wire::wire_enum;

/// Whether an offering is open to orders.
#[wire_enum]
pub enum IpoAvailability {
    /// Taking orders.
    #[wire = "available"]
    Available,
    /// Not taking orders, but still listed.
    #[wire = "not_available"]
    NotAvailable,
    /// Finished.
    #[wire = "closed"]
    Closed,
}

/// An initial public offering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct IpoOffering {
    /// Alpaca's reference for the offering, which is how it is fetched.
    pub ipo_reference: String,
    /// The issuer's name.
    pub name: String,
    /// What kind of offering it is.
    pub offering_type: String,
    /// Whether it is open to orders.
    pub availability: IpoAvailability,
    /// Whether it has stopped accepting new orders.
    ///
    /// Not the same thing as
    /// [`availability`](Self::availability): an offering can still be
    /// `Available` and refuse new orders while it prices.
    pub no_new_orders: bool,
    /// The bottom of the price band.
    #[serde(with = "crate::types::decimal")]
    pub min_price: Decimal,
    /// The top of it.
    #[serde(with = "crate::types::decimal")]
    pub max_price: Decimal,
    /// The ticker it will trade under.
    #[serde(default)]
    pub ticker_symbol: Option<String>,
    /// Its CUSIP.
    #[serde(default)]
    pub cusip_id: Option<String>,
    /// A description of the offering.
    #[serde(default)]
    pub description: Option<String>,
    /// How many shares are expected.
    #[serde(default)]
    pub anticipated_shares: Option<i64>,
    /// The smallest order accepted.
    #[serde(default)]
    pub min_ticket_size: Option<String>,
    /// The largest.
    #[serde(default)]
    pub max_ticket_size: Option<String>,
    /// The increment orders must be a multiple of.
    #[serde(default)]
    pub unit_step_size: Option<String>,
    /// When it starts trading.
    #[serde(default)]
    pub trade_date: Option<NaiveDate>,
    /// When it settles.
    #[serde(default)]
    pub settlement_date: Option<NaiveDate>,
    /// Where to read the prospectus.
    #[serde(default)]
    pub prospectus_url: Option<String>,
    /// A small logo.
    #[serde(default)]
    pub logo_small: Option<String>,
    /// Who is underwriting it.
    #[serde(
        default,
        deserialize_with = "crate::types::serde_util::null_as_default"
    )]
    pub underwriters: Vec<String>,
}

/// A page of offerings.
///
/// The list nests under `data`, and so does the single-offering response — an
/// envelope this API uses nowhere else.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct IpoOfferingsPage {
    /// The offerings.
    #[serde(
        rename = "data",
        default,
        deserialize_with = "crate::types::serde_util::null_as_default"
    )]
    pub offerings: Vec<IpoOffering>,
    /// The token for the next page, or `None` at the end.
    #[serde(default)]
    pub next_page_token: Option<String>,
}

/// One offering, in the same `data` envelope the list uses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct IpoOfferingResponse {
    /// The offering.
    #[serde(rename = "data")]
    pub offering: IpoOffering,
}

/// Filters for listing offerings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Setters, Validated)]
#[non_exhaustive]
pub struct GetIpoOfferingsRequest {
    /// Only offerings in this state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub availability: Option<IpoAvailability>,
    /// Only this ticker.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub ticker: Option<String>,
    /// How many to return.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// The token from a previous page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(into)]
    pub page_token: Option<String>,
}

impl GetIpoOfferingsRequest {
    /// A request with no filters.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_and_the_single_offering_share_one_envelope() {
        let page: IpoOfferingsPage = serde_json::from_value(serde_json::json!({
            "data": [{
                "ipo_reference": "IPO123",
                "name": "Example Corp",
                "offering_type": "ipo",
                "availability": "available",
                "no_new_orders": false,
                "min_price": "17.00",
                "max_price": "19.00",
            }],
            "next_page_token": null,
        }))
        .unwrap();

        assert_eq!(page.offerings.len(), 1);
        assert_eq!(page.offerings[0].min_price, Decimal::new(1700, 2));
        // Available and refusing orders are different states, and both are
        // reported.
        assert!(!page.offerings[0].no_new_orders);
        assert!(page.offerings[0].underwriters.is_empty());
    }
}
