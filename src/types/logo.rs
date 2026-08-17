//! The logo request, which two API surfaces share.
//!
//! `GET /v1beta1/logos/{symbol}` appears in both the market data spec and the
//! broker spec — the same route, reachable from either client — so its request
//! type lives here rather than in one of them. Putting it in `data` would leave
//! a `broker`-only build unable to call a route the broker API documents; the
//! same reasoning that puts [`ContractType`](crate::types::ContractType) here.
//!
//! Unverified: a data plan that reaches SIP still answers
//! `403 Subscription does not permit querying logos`, so logos are a separate
//! entitlement rather than part of a data plan.

use crate::types::setters::Setters;
use serde::{Deserialize, Serialize};

/// A request for a company logo.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Setters)]
#[non_exhaustive]
pub struct LogoRequest {
    /// Whether to answer with a generated placeholder when no logo exists.
    ///
    /// Alpaca defaults this to `true`, so an unset request never 404s — it
    /// returns an image either way. Set it to `false` to tell the two apart.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[setters(doc = "Whether a placeholder is acceptable.")]
    pub placeholder: Option<bool>,
}

impl LogoRequest {
    /// A request taking Alpaca's default: a placeholder when no logo exists.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_request_sends_nothing() {
        // Alpaca's own default is `true`, so sending it explicitly would be
        // noise — and sending `false` by accident would turn a placeholder
        // into a 404.
        let json = serde_json::to_value(LogoRequest::new()).unwrap();
        assert_eq!(json.as_object().unwrap().len(), 0);
    }
}
