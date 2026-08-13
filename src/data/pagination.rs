//! The market data pagination loop.
//!
//! Every historical endpoint answers with at most `page_limit` items and a
//! `next_page_token`. The tokens are walked and the pages merged into one map
//! before returning, so callers never see a page.
//!
//! Note this is *not* the broker API's pagination: that one exposes the page
//! boundary to the caller, because a broker route can page over more records
//! than fit in memory.

use std::collections::{BTreeMap, HashSet};

use serde::Serialize;
use serde_json::{Map, Value};

use crate::error::{Error, Result};
use crate::rest::RestClient;

/// The keys a market data response nests its payload under.
///
/// A response carries exactly one. Anything else means the shape changed and
/// guessing would silently return the wrong data.
const DATA_KEYS: &[&str] = &[
    "auctions",
    "bar",
    "bars",
    "corporate_actions",
    "news",
    "orderbook",
    "orderbooks",
    "quote",
    "quotes",
    "rates",
    "snapshot",
    "snapshots",
    "trade",
    "trades",
];

/// How a response's payload is located.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Unwrap {
    /// Take the single known data key. The normal case.
    DataKey,
    /// The whole response body is the payload.
    ///
    /// Only the stock snapshots endpoint, which returns symbols at the top
    /// level with no wrapper.
    WholeBody,
}

/// One paginated market data request.
#[derive(Debug, Clone)]
pub(crate) struct MarketDataRequest<'a> {
    /// The endpoint path.
    pub path: &'a str,
    /// The maximum number of items the endpoint returns per page.
    pub page_limit: u32,
    /// The page size to ask for.
    ///
    /// `None` means the endpoint takes no `limit` parameter at all, which is
    /// how the "latest" endpoints behave — sending one is an error there.
    pub page_size: Option<u32>,
    /// How to locate the payload in the response.
    pub unwrap: Unwrap,
}

impl<'a> MarketDataRequest<'a> {
    /// A paginated request asking for the maximum page the API serves, 10,000.
    pub fn paged(path: &'a str) -> Self {
        Self {
            path,
            page_limit: crate::config::DATA_MAX_LIMIT,
            page_size: Some(crate::config::DATA_MAX_LIMIT),
            unwrap: Unwrap::DataKey,
        }
    }

    /// A paginated request with a smaller page limit, for endpoints that cap
    /// lower: news at 50, option snapshots and corporate actions at 1,000.
    pub fn paged_with_limit(path: &'a str, page_limit: u32) -> Self {
        Self {
            path,
            page_limit,
            page_size: Some(page_limit),
            unwrap: Unwrap::DataKey,
        }
    }

    /// A single-shot request that sends no `limit`, for the latest endpoints.
    pub fn latest(path: &'a str) -> Self {
        Self {
            path,
            page_limit: crate::config::DATA_MAX_LIMIT,
            page_size: None,
            unwrap: Unwrap::DataKey,
        }
    }

    /// The response body is the payload, with no wrapping key.
    pub fn whole_body(mut self) -> Self {
        self.unwrap = Unwrap::WholeBody;
        self
    }
}

/// Fetches every page and merges them into one object.
///
/// The `limit` in `query`, if present, is the caller's cap on total items across
/// all pages, not per page.
pub(crate) async fn get_marketdata<Q: Serialize>(
    rest: &RestClient,
    request: &MarketDataRequest<'_>,
    query: &Q,
) -> Result<Map<String, Value>> {
    // Round-trip the caller's request through a map so the loop can override
    // `limit` and `page_token` per page.
    let mut params = to_param_map(query)?;

    let user_limit = params
        .get("limit")
        .and_then(parse_u32)
        .filter(|_| request.page_size.is_some());
    let mut page_token = params
        .get("page_token")
        .and_then(Value::as_str)
        .map(str::to_owned);

    // Every token followed so far, so a server that stops advancing cannot
    // spin this loop indefinitely.
    let mut seen_tokens: HashSet<String> = HashSet::new();
    if let Some(token) = &page_token {
        seen_tokens.insert(token.clone());
    }

    let mut merged: Map<String, Value> = Map::new();
    // Lists are accumulated per key; everything else overwrites.
    let mut lists: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    let mut total_items: u32 = 0;

    loop {
        let actual_limit = match (user_limit, request.page_size) {
            // A caller-supplied cap: ask only for what is still outstanding.
            (Some(limit), Some(_)) => {
                let remaining = limit.saturating_sub(total_items);
                if remaining < 1 {
                    break;
                }
                Some(remaining.min(request.page_limit))
            }
            (None, Some(page_size)) => Some(page_size.min(request.page_limit)),
            // The latest endpoints take no limit parameter.
            (_, None) => None,
        };

        match actual_limit {
            Some(limit) => {
                params.insert("limit".to_owned(), Value::from(limit));
            }
            None => {
                params.remove("limit");
            }
        }
        match &page_token {
            Some(token) => {
                params.insert("page_token".to_owned(), Value::from(token.clone()));
            }
            None => {
                params.remove("page_token");
            }
        }

        let query: Vec<(String, String)> = params
            .iter()
            .filter_map(|(key, value)| Some((key.clone(), stringify(value)?)))
            .collect();

        let response: Value = rest.get(request.path, &query).await?;

        for (key, value) in entries(&response, request.unwrap, request.path)? {
            match value {
                Value::Array(items) => lists.entry(key).or_default().extend(items),
                other => {
                    merged.insert(key, other);
                }
            }
        }

        if actual_limit.is_some() {
            // The cap counts every accumulated item, across all keys — one
            // limit on the response, not one per symbol.
            total_items = lists
                .values()
                .map(|items| u32::try_from(items.len()).unwrap_or(u32::MAX))
                .sum();
        }

        let next = response
            .get("next_page_token")
            .and_then(Value::as_str)
            .map(str::to_owned);

        let Some(next) = next else { break };

        // A token we have already followed means the server is not advancing.
        // Following it again accumulates pages until the process runs out of
        // memory; a correctly behaving endpoint never hits this path.
        if !seen_tokens.insert(next.clone()) {
            tracing::warn!(
                path = request.path,
                "server repeated a pagination token; stopping to avoid looping forever"
            );
            break;
        }

        page_token = Some(next);
    }

    for (key, items) in lists {
        merged.insert(key, Value::Array(items));
    }

    Ok(merged)
}

/// Locates the payload within a response.
fn entries(response: &Value, unwrap: Unwrap, path: &str) -> Result<Vec<(String, Value)>> {
    let object = response.as_object().ok_or_else(|| {
        Error::InvalidRequest(format!("{path}: expected a JSON object in the response"))
    })?;

    if unwrap == Unwrap::WholeBody {
        return Ok(object
            .iter()
            .filter(|(key, _)| key.as_str() != "next_page_token")
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect());
    }

    let selected: Vec<&str> = DATA_KEYS
        .iter()
        .copied()
        .filter(|key| object.contains_key(*key))
        .collect();

    match selected.as_slice() {
        [] => Err(Error::InvalidRequest(format!(
            "{path}: the response matched no known market data key"
        ))),
        [key] => {
            let value = object[*key].clone();
            // A payload that is already a list stays under its key: news, which
            // has no symbol to key it by, and the single-symbol market data
            // routes, which name the symbol in a sibling field instead.
            match value {
                Value::Object(inner) => Ok(inner.into_iter().collect()),
                other => Ok(vec![((*key).to_owned(), other)]),
            }
        }
        many => Err(Error::InvalidRequest(format!(
            "{path}: the response matched multiple known market data keys: {}",
            many.join(", ")
        ))),
    }
}

/// Serializes a request struct into a flat parameter map.
fn to_param_map<Q: Serialize>(query: &Q) -> Result<Map<String, Value>> {
    let value = serde_json::to_value(query).map_err(|source| Error::Decode {
        path: "<request>".to_owned(),
        body: String::new(),
        source,
    })?;

    match value {
        Value::Object(map) => Ok(map),
        Value::Null => Ok(Map::new()),
        other => Err(Error::InvalidRequest(format!(
            "request parameters must serialize to an object, got {other}"
        ))),
    }
}

/// Renders a parameter value for the query string, dropping nulls.
fn stringify(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(text) => Some(text.clone()),
        Value::Bool(flag) => Some(flag.to_string()),
        Value::Number(number) => Some(number.to_string()),
        // A list that reached here was not comma-joined by its field attribute.
        Value::Array(items) => Some(
            items
                .iter()
                .filter_map(stringify)
                .collect::<Vec<_>>()
                .join(","),
        ),
        Value::Object(_) => None,
    }
}

fn parse_u32(value: &Value) -> Option<u32> {
    match value {
        Value::Number(number) => number.as_u64().and_then(|n| u32::try_from(n).ok()),
        Value::String(text) => text.parse().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unwraps_the_single_data_key() {
        let response = json!({"bars": {"AAPL": [{"c": 1.0}]}, "next_page_token": null});
        let entries = entries(&response, Unwrap::DataKey, "/stocks/bars").unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, "AAPL");
    }

    #[test]
    fn news_stays_under_its_key() {
        // Every other payload is a map of symbol to data; news is a bare list,
        // so unwrapping it would lose the only key it has.
        let response = json!({"news": [{"id": 1}]});
        let entries = entries(&response, Unwrap::DataKey, "/news").unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, "news");
        assert!(entries[0].1.is_array());
    }

    #[test]
    fn whole_body_mode_keeps_top_level_symbols() {
        // The stock snapshots endpoint has no wrapper key.
        let response = json!({"AAPL": {"latestTrade": {}}, "next_page_token": null});
        let entries = entries(&response, Unwrap::WholeBody, "/stocks/snapshots").unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, "AAPL");
    }

    #[test]
    fn no_known_key_is_an_error() {
        let response = json!({"unexpected": {}});
        assert!(entries(&response, Unwrap::DataKey, "/stocks/bars").is_err());
    }

    #[test]
    fn multiple_known_keys_are_an_error() {
        // Ambiguous: picking one would silently discard the other.
        let response = json!({"bars": {}, "trades": {}});
        assert!(entries(&response, Unwrap::DataKey, "/stocks/bars").is_err());
    }

    #[test]
    fn stringify_drops_nulls_and_joins_lists() {
        assert_eq!(stringify(&json!(null)), None);
        assert_eq!(stringify(&json!("AAPL")), Some("AAPL".to_owned()));
        assert_eq!(stringify(&json!(50)), Some("50".to_owned()));
        assert_eq!(stringify(&json!(true)), Some("true".to_owned()));
        assert_eq!(stringify(&json!(["a", "b"])), Some("a,b".to_owned()));
    }
}
