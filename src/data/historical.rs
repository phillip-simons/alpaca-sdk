//! The [historical market data](https://docs.alpaca.markets/us/docs/about-market-data-api) clients.
//!
//! Eight clients rather than one, because each targets a different API version:
//! stocks on `v2`, crypto on `v1beta3`, options, forex, logos, news, and the
//! screener on `v1beta1`, and corporate actions on `v1`.

use std::collections::HashMap;

use serde::Deserialize as _;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::auth::Credentials;
use crate::config::BaseUrl;
use crate::data::corporate_actions::CorporateActions;
use crate::data::enums::CryptoFeed;
use crate::data::events::CorporateActionEventsRequest;
use crate::data::meta::{Codes, Tape, TapeQuery, TickType};
use crate::data::models::{
    AuctionSet, Bar, BarSet, DailyAuctions, ForexRate, ForexRateSet, MostActives, Movers, News,
    NewsSet, OptionsSnapshot, Orderbook, Quote, QuoteSet, Snapshot, Trade, TradeSet, WithSymbol,
};
use crate::data::pagination::Merged;
use crate::data::pagination::{MarketDataRequest, get_marketdata};
use crate::data::requests::{
    CorporateActionsRequest, CryptoBarsRequest, CryptoLatestRequest, CryptoSnapshotRequest,
    ForexLatestRatesRequest, ForexRatesRequest, MarketMoversRequest, MostActivesRequest,
    NewsRequest, OptionBarsRequest, OptionChainRequest, OptionLatestRequest, OptionSnapshotRequest,
    SingleSymbolRequest, StockAuctionsRequest, StockBarsRequest, StockLatestRequest,
    StockSnapshotRequest, StockTimeseriesRequest, TimeseriesRequest,
};
use crate::error::{Error, Result};
use crate::rest::{Empty, RestClient, RestConfig};
use crate::types::LogoRequest;
use crate::types::path::segment;

/// Whether a symbol's entry means "nothing for this one".
///
/// A multi-symbol request returns a key per symbol, and Alpaca answers `null`
/// for a symbol it has nothing for — a delisted or misspelled ticker. That is
/// not a payload that failed to parse, it is the absence of a payload, and
/// propagating a decode error for it threw away every *good* symbol in the same
/// response. A request takes up to 100 symbols, so one bad ticker made the whole
/// batch unusable.
///
/// `fixtures/go/marketdata__test_snapshots__01.json` — a captured payload that
/// ships in this crate — is exactly this shape: `"INVALID": null` beside a valid
/// AAPL and MSFT.
fn is_absent(value: &Value) -> bool {
    value.is_null()
}

/// Deserializes a merged payload into a map of symbol to a list of records,
/// filling in the symbol each list was keyed by.
fn into_sets<T>(merged: Merged) -> Result<HashMap<String, Vec<T>>>
where
    T: DeserializeOwned + WithSymbol,
{
    let mut sets = HashMap::with_capacity(merged.data.len());

    for (symbol, value) in merged.data {
        if is_absent(&value) {
            continue;
        }
        // `&Value` is itself a `Deserializer`, so the payload is borrowed
        // rather than cloned. Cloning it here — purely so a failure could report
        // a body — doubled peak allocation on every *successful* multi-symbol
        // response, which is the common path.
        let mut records: Vec<T> =
            Vec::<T>::deserialize(&value).map_err(|source| Error::Decode {
                path: merged.path.clone(),
                body: decode_body(&symbol, &value),
                source,
            })?;
        for record in &mut records {
            record.set_symbol(&symbol);
        }
        sets.insert(symbol, records);
    }

    Ok(sets)
}

/// The offending entry, for [`Error::Decode`]'s `body`.
///
/// That field is documented as carrying the payload "so the mismatch can be
/// diagnosed without re-issuing", and these helpers used to pass an empty
/// string — leaving a decode failure with nothing to diagnose from.
fn decode_body(key: &str, value: &Value) -> String {
    crate::rest::truncate(&format!("{key}: {value}"))
}

/// Deserializes one single-symbol payload: a bare list under `key`, with the
/// symbol beside it rather than above it.
///
/// The multi-symbol routes key their records by symbol and the record carries
/// none; these name the symbol in a sibling field. Both end up with the symbol
/// filled in, so the models are shared.
fn into_single<T>(mut merged: Merged, key: &str, symbol: &str) -> Result<Vec<T>>
where
    T: DeserializeOwned + WithSymbol,
{
    let records = merged.data.remove(key).unwrap_or(Value::Array(Vec::new()));
    // A `null` under the key means the endpoint had nothing for this symbol,
    // which is an empty result rather than a malformed one.
    if is_absent(&records) {
        return Ok(Vec::new());
    }
    let mut records: Vec<T> = Vec::<T>::deserialize(&records).map_err(|source| Error::Decode {
        path: merged.path.clone(),
        body: decode_body(key, &records),
        source,
    })?;
    for record in &mut records {
        record.set_symbol(symbol);
    }
    Ok(records)
}

/// Deserializes a merged payload into a map of symbol to a single record.
fn into_latest<T>(merged: Merged) -> Result<HashMap<String, T>>
where
    T: DeserializeOwned + WithSymbol,
{
    let mut latest = HashMap::with_capacity(merged.data.len());

    for (symbol, value) in merged.data {
        if is_absent(&value) {
            continue;
        }
        let mut record: T = T::deserialize(&value).map_err(|source| Error::Decode {
            path: merged.path.clone(),
            body: decode_body(&symbol, &value),
            source,
        })?;
        record.set_symbol(&symbol);
        latest.insert(symbol, record);
    }

    Ok(latest)
}

/// Historical market data for US equities.
#[derive(Debug, Clone)]
pub struct StockHistoricalDataClient {
    rest: RestClient,
}

impl StockHistoricalDataClient {
    /// A client for the stock market data API.
    ///
    /// # Errors
    /// Returns an error if the credentials cannot be encoded as headers.
    pub fn new(credentials: &Credentials) -> Result<Self> {
        Self::with_config(
            credentials,
            RestConfig::from(BaseUrl::Data).api_version("v2"),
        )
    }

    /// A client with a custom endpoint, retry policy, or timeout.
    ///
    /// # Errors
    /// Returns an error if the credentials cannot be encoded as headers.
    pub fn with_config(credentials: &Credentials, config: RestConfig) -> Result<Self> {
        Ok(Self {
            rest: RestClient::new(credentials, config)?,
        })
    }

    /// The underlying transport.
    #[must_use]
    pub fn rest(&self) -> &RestClient {
        &self.rest
    }

    /// Historical bars, keyed by symbol.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_stock_bars(&self, request: &StockBarsRequest) -> Result<BarSet> {
        let merged = get_marketdata(
            &self.rest,
            &MarketDataRequest::paged("/stocks/bars"),
            request,
        )
        .await?;
        into_sets(merged)
    }

    /// Historical quotes, keyed by symbol.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_stock_quotes(&self, request: &StockTimeseriesRequest) -> Result<QuoteSet> {
        let merged = get_marketdata(
            &self.rest,
            &MarketDataRequest::paged("/stocks/quotes"),
            request,
        )
        .await?;
        into_sets(merged)
    }

    /// Historical trades, keyed by symbol.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_stock_trades(&self, request: &StockTimeseriesRequest) -> Result<TradeSet> {
        let merged = get_marketdata(
            &self.rest,
            &MarketDataRequest::paged("/stocks/trades"),
            request,
        )
        .await?;
        into_sets(merged)
    }

    /// The latest trade for each symbol.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_stock_latest_trade(
        &self,
        request: &StockLatestRequest,
    ) -> Result<HashMap<String, Trade>> {
        let merged = get_marketdata(
            &self.rest,
            &MarketDataRequest::latest("/stocks/trades/latest"),
            request,
        )
        .await?;
        into_latest(merged)
    }

    /// The latest quote for each symbol.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_stock_latest_quote(
        &self,
        request: &StockLatestRequest,
    ) -> Result<HashMap<String, Quote>> {
        let merged = get_marketdata(
            &self.rest,
            &MarketDataRequest::latest("/stocks/quotes/latest"),
            request,
        )
        .await?;
        into_latest(merged)
    }

    /// The latest bar for each symbol.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_stock_latest_bar(
        &self,
        request: &StockLatestRequest,
    ) -> Result<HashMap<String, Bar>> {
        let merged = get_marketdata(
            &self.rest,
            &MarketDataRequest::latest("/stocks/bars/latest"),
            request,
        )
        .await?;
        into_latest(merged)
    }

    /// A snapshot of the latest trade, quote, and bars for each symbol.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_stock_snapshot(
        &self,
        request: &StockSnapshotRequest,
    ) -> Result<HashMap<String, Snapshot>> {
        // The only endpoint that returns symbols at the top level with no
        // wrapping key, so there is no `bars`/`quotes` layer to unwrap.
        let merged = get_marketdata(
            &self.rest,
            &MarketDataRequest::latest("/stocks/snapshots").whole_body(),
            request,
        )
        .await?;
        into_latest(merged)
    }

    /// Historical opening and closing auctions, keyed by symbol.
    ///
    /// Only the `sip` feed serves auctions.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_stock_auctions(&self, request: &StockAuctionsRequest) -> Result<AuctionSet> {
        let merged = get_marketdata(
            &self.rest,
            &MarketDataRequest::paged("/stocks/auctions"),
            request,
        )
        .await?;
        into_sets(merged)
    }

    /// Historical bars for one symbol.
    ///
    /// The single-symbol routes are not aliases of their multi-symbol siblings:
    /// they answer with a bare list and the symbol beside it rather than a map
    /// keyed by symbol. Prefer [`get_stock_bars`](Self::get_stock_bars) when
    /// asking about more than one symbol — it is one request rather than *n*.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_stock_bars_for_symbol(
        &self,
        symbol: &str,
        request: &SingleSymbolRequest,
    ) -> Result<Vec<Bar>> {
        let path = format!("/stocks/{}/bars", segment(symbol)?);
        let merged = get_marketdata(
            &self.rest,
            &MarketDataRequest::paged(&path).whole_body(),
            request,
        )
        .await?;
        into_single(merged, "bars", symbol)
    }

    /// Historical quotes for one symbol.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_stock_quotes_for_symbol(
        &self,
        symbol: &str,
        request: &SingleSymbolRequest,
    ) -> Result<Vec<Quote>> {
        let path = format!("/stocks/{}/quotes", segment(symbol)?);
        let merged = get_marketdata(
            &self.rest,
            &MarketDataRequest::paged(&path).whole_body(),
            request,
        )
        .await?;
        into_single(merged, "quotes", symbol)
    }

    /// Historical trades for one symbol.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_stock_trades_for_symbol(
        &self,
        symbol: &str,
        request: &SingleSymbolRequest,
    ) -> Result<Vec<Trade>> {
        let path = format!("/stocks/{}/trades", segment(symbol)?);
        let merged = get_marketdata(
            &self.rest,
            &MarketDataRequest::paged(&path).whole_body(),
            request,
        )
        .await?;
        into_single(merged, "trades", symbol)
    }

    /// Historical auctions for one symbol.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_stock_auctions_for_symbol(
        &self,
        symbol: &str,
        request: &SingleSymbolRequest,
    ) -> Result<Vec<DailyAuctions>> {
        let path = format!("/stocks/{}/auctions", segment(symbol)?);
        let merged = get_marketdata(
            &self.rest,
            &MarketDataRequest::paged(&path).whole_body(),
            request,
        )
        .await?;
        into_single(merged, "auctions", symbol)
    }

    /// The latest bar for one symbol.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_stock_latest_bar_for_symbol(
        &self,
        symbol: &str,
        request: &SingleSymbolRequest,
    ) -> Result<Bar> {
        let path = format!("/stocks/{}/bars/latest", segment(symbol)?);
        self.latest_for_symbol(&path, "bar", symbol, request).await
    }

    /// The latest quote for one symbol.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_stock_latest_quote_for_symbol(
        &self,
        symbol: &str,
        request: &SingleSymbolRequest,
    ) -> Result<Quote> {
        let path = format!("/stocks/{}/quotes/latest", segment(symbol)?);
        self.latest_for_symbol(&path, "quote", symbol, request)
            .await
    }

    /// The latest trade for one symbol.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_stock_latest_trade_for_symbol(
        &self,
        symbol: &str,
        request: &SingleSymbolRequest,
    ) -> Result<Trade> {
        let path = format!("/stocks/{}/trades/latest", segment(symbol)?);
        self.latest_for_symbol(&path, "trade", symbol, request)
            .await
    }

    /// A snapshot for one symbol.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_stock_snapshot_for_symbol(
        &self,
        symbol: &str,
        request: &SingleSymbolRequest,
    ) -> Result<Snapshot> {
        // No wrapping key at all on this one, unlike its three siblings: the
        // snapshot's fields are the response body.
        let path = format!("/stocks/{}/snapshot", segment(symbol)?);
        let merged = get_marketdata(
            &self.rest,
            &MarketDataRequest::latest(&path).whole_body(),
            request,
        )
        .await?;

        let body = Value::Object(merged.data);
        let mut snapshot: Snapshot =
            Snapshot::deserialize(&body).map_err(|source| Error::Decode {
                path: merged.path,
                body: crate::rest::truncate(&body.to_string()),
                source,
            })?;
        snapshot.set_symbol(symbol);
        Ok(snapshot)
    }

    /// The shared body of the three single-symbol "latest" routes: one record
    /// under a singular key, with the symbol beside it.
    async fn latest_for_symbol<T>(
        &self,
        path: &str,
        key: &str,
        symbol: &str,
        request: &SingleSymbolRequest,
    ) -> Result<T>
    where
        T: DeserializeOwned + WithSymbol,
    {
        let mut merged = get_marketdata(
            &self.rest,
            &MarketDataRequest::latest(path).whole_body(),
            request,
        )
        .await?;

        let Some(record) = merged.data.remove(key) else {
            // `remove` left the payload untouched when it answered `None`, so
            // the response can still be reported alongside the reason.
            let body = serde_json::to_string(&merged.data).unwrap_or_default();
            return Err(Error::decode_shape(
                path,
                &body,
                format_args!("the response carried no `{key}`"),
            ));
        };
        let mut record: T = T::deserialize(&record).map_err(|source| Error::Decode {
            path: path.to_owned(),
            body: decode_body(key, &record),
            source,
        })?;
        record.set_symbol(symbol);
        Ok(record)
    }

    /// The mapping from stock exchange codes to exchange names.
    ///
    /// The decoder for [`Trade::exchange`] and [`Quote::bid_exchange`].
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_stock_exchange_codes(&self) -> Result<Codes> {
        self.rest.get("/stocks/meta/exchanges", &Empty).await
    }

    /// The mapping from stock condition codes to condition names.
    ///
    /// The decoder for [`Trade::conditions`]. `tape` is required — the route
    /// answers `400` without it, which is a live-capture finding rather than
    /// something any other SDK records.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_stock_condition_codes(
        &self,
        tick_type: &TickType,
        tape: Tape,
    ) -> Result<Codes> {
        let path = format!("/stocks/meta/conditions/{}", segment(tick_type)?);
        self.rest.get(&path, &TapeQuery { tape }).await
    }
}

/// Historical market data for crypto.
///
/// These endpoints serve unauthenticated requests, so [`CryptoHistoricalDataClient::new`]
/// takes no credentials.
#[derive(Debug, Clone)]
pub struct CryptoHistoricalDataClient {
    rest: RestClient,
}

impl CryptoHistoricalDataClient {
    /// A client that sends no credentials.
    ///
    /// # Errors
    /// Returns an error if the underlying HTTP client fails to build.
    pub fn new() -> Result<Self> {
        Ok(Self {
            rest: RestClient::unauthenticated(
                RestConfig::from(BaseUrl::Data).api_version("v1beta3"),
            )?,
        })
    }

    /// A client that authenticates, which raises the rate limit.
    ///
    /// # Errors
    /// Returns an error if the credentials cannot be encoded as headers.
    pub fn with_credentials(credentials: &Credentials) -> Result<Self> {
        Self::with_config(
            Some(credentials),
            RestConfig::from(BaseUrl::Data).api_version("v1beta3"),
        )
    }

    /// A client with a custom endpoint, retry policy, or timeout.
    ///
    /// # Errors
    /// Returns an error if the credentials cannot be encoded as headers.
    pub fn with_config(credentials: Option<&Credentials>, config: RestConfig) -> Result<Self> {
        let rest = match credentials {
            Some(credentials) => RestClient::new(credentials, config)?,
            None => RestClient::unauthenticated(config)?,
        };
        Ok(Self { rest })
    }

    /// The underlying transport.
    #[must_use]
    pub fn rest(&self) -> &RestClient {
        &self.rest
    }

    /// Historical bars, keyed by symbol.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_crypto_bars(
        &self,
        request: &CryptoBarsRequest,
        feed: CryptoFeed,
    ) -> Result<BarSet> {
        let path = format!("/crypto/{}/bars", segment(feed)?);
        let merged = get_marketdata(&self.rest, &MarketDataRequest::paged(&path), request).await?;
        into_sets(merged)
    }

    /// Historical quotes, keyed by symbol.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_crypto_quotes(
        &self,
        request: &TimeseriesRequest,
        feed: CryptoFeed,
    ) -> Result<QuoteSet> {
        let path = format!("/crypto/{}/quotes", segment(feed)?);
        let merged = get_marketdata(&self.rest, &MarketDataRequest::paged(&path), request).await?;
        into_sets(merged)
    }

    /// Historical trades, keyed by symbol.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_crypto_trades(
        &self,
        request: &TimeseriesRequest,
        feed: CryptoFeed,
    ) -> Result<TradeSet> {
        let path = format!("/crypto/{}/trades", segment(feed)?);
        let merged = get_marketdata(&self.rest, &MarketDataRequest::paged(&path), request).await?;
        into_sets(merged)
    }

    /// The latest trade for each symbol.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_crypto_latest_trade(
        &self,
        request: &CryptoLatestRequest,
        feed: CryptoFeed,
    ) -> Result<HashMap<String, Trade>> {
        let path = format!("/crypto/{}/latest/trades", segment(feed)?);
        let merged = get_marketdata(&self.rest, &MarketDataRequest::latest(&path), request).await?;
        into_latest(merged)
    }

    /// The latest quote for each symbol.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_crypto_latest_quote(
        &self,
        request: &CryptoLatestRequest,
        feed: CryptoFeed,
    ) -> Result<HashMap<String, Quote>> {
        let path = format!("/crypto/{}/latest/quotes", segment(feed)?);
        let merged = get_marketdata(&self.rest, &MarketDataRequest::latest(&path), request).await?;
        into_latest(merged)
    }

    /// The latest bar for each symbol.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_crypto_latest_bar(
        &self,
        request: &CryptoLatestRequest,
        feed: CryptoFeed,
    ) -> Result<HashMap<String, Bar>> {
        let path = format!("/crypto/{}/latest/bars", segment(feed)?);
        let merged = get_marketdata(&self.rest, &MarketDataRequest::latest(&path), request).await?;
        into_latest(merged)
    }

    /// The latest orderbook for each symbol.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_crypto_latest_orderbook(
        &self,
        request: &CryptoLatestRequest,
        feed: CryptoFeed,
    ) -> Result<HashMap<String, Orderbook>> {
        let path = format!("/crypto/{}/latest/orderbooks", segment(feed)?);
        let merged = get_marketdata(&self.rest, &MarketDataRequest::latest(&path), request).await?;
        into_latest(merged)
    }

    /// A snapshot of the latest data for each symbol.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_crypto_snapshot(
        &self,
        request: &CryptoSnapshotRequest,
        feed: CryptoFeed,
    ) -> Result<HashMap<String, Snapshot>> {
        let path = format!("/crypto/{}/snapshots", segment(feed)?);
        let merged = get_marketdata(&self.rest, &MarketDataRequest::latest(&path), request).await?;
        into_latest(merged)
    }
}

/// Historical market data for options.
#[derive(Debug, Clone)]
pub struct OptionHistoricalDataClient {
    rest: RestClient,
}

impl OptionHistoricalDataClient {
    /// A client for the option market data API.
    ///
    /// # Errors
    /// Returns an error if the credentials cannot be encoded as headers.
    pub fn new(credentials: &Credentials) -> Result<Self> {
        Self::with_config(
            credentials,
            RestConfig::from(BaseUrl::Data).api_version("v1beta1"),
        )
    }

    /// A client with a custom endpoint, retry policy, or timeout.
    ///
    /// # Errors
    /// Returns an error if the credentials cannot be encoded as headers.
    pub fn with_config(credentials: &Credentials, config: RestConfig) -> Result<Self> {
        Ok(Self {
            rest: RestClient::new(credentials, config)?,
        })
    }

    /// The underlying transport.
    #[must_use]
    pub fn rest(&self) -> &RestClient {
        &self.rest
    }

    /// Historical bars, keyed by contract symbol.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_option_bars(&self, request: &OptionBarsRequest) -> Result<BarSet> {
        let merged = get_marketdata(
            &self.rest,
            &MarketDataRequest::paged("/options/bars"),
            request,
        )
        .await?;
        into_sets(merged)
    }

    /// Historical trades, keyed by contract symbol.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_option_trades(&self, request: &TimeseriesRequest) -> Result<TradeSet> {
        let merged = get_marketdata(
            &self.rest,
            &MarketDataRequest::paged("/options/trades"),
            request,
        )
        .await?;
        into_sets(merged)
    }

    /// The latest quote for each contract.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_option_latest_quote(
        &self,
        request: &OptionLatestRequest,
    ) -> Result<HashMap<String, Quote>> {
        let merged = get_marketdata(
            &self.rest,
            &MarketDataRequest::latest("/options/quotes/latest"),
            request,
        )
        .await?;
        into_latest(merged)
    }

    /// The latest trade for each contract.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_option_latest_trade(
        &self,
        request: &OptionLatestRequest,
    ) -> Result<HashMap<String, Trade>> {
        let merged = get_marketdata(
            &self.rest,
            &MarketDataRequest::latest("/options/trades/latest"),
            request,
        )
        .await?;
        into_latest(merged)
    }

    /// A snapshot of the latest data for each contract.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_option_snapshot(
        &self,
        request: &OptionSnapshotRequest,
    ) -> Result<HashMap<String, OptionsSnapshot>> {
        let merged = get_marketdata(
            &self.rest,
            &MarketDataRequest::paged_with_limit("/options/snapshots", 1000),
            request,
        )
        .await?;
        into_latest(merged)
    }

    /// Every contract in an underlying's option chain.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_option_chain(
        &self,
        request: &OptionChainRequest,
    ) -> Result<HashMap<String, OptionsSnapshot>> {
        let path = format!(
            "/options/snapshots/{}",
            segment(&request.underlying_symbol)?
        );
        let merged = get_marketdata(
            &self.rest,
            &MarketDataRequest::paged_with_limit(&path, 1000),
            request,
        )
        .await?;
        into_latest(merged)
    }

    /// The mapping from option exchange codes to exchange names.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_option_exchange_codes(&self) -> Result<HashMap<String, String>> {
        self.rest.get("/options/meta/exchanges", &Empty).await
    }

    /// The mapping from option condition codes to condition names.
    ///
    /// Unlike the stock equivalent this takes no `tape`, and rejects one.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_option_condition_codes(&self, tick_type: &TickType) -> Result<Codes> {
        let path = format!("/options/meta/conditions/{}", segment(tick_type)?);
        self.rest.get(&path, &Empty).await
    }
}

/// Foreign exchange rates.
///
/// Not verified against a live response: the routes answer
/// `403 forbidden: insufficient grants` on a plan that reaches SIP, so forex is
/// a per-product entitlement rather than part of a data plan. The models follow
/// the published reference; the first real payload decides whether they are
/// right.
///
/// See <https://docs.alpaca.markets/us/reference/rates-1>.
#[derive(Debug, Clone)]
pub struct ForexDataClient {
    rest: RestClient,
}

impl ForexDataClient {
    /// A client for the forex API.
    ///
    /// # Errors
    /// Returns an error if the credentials cannot be encoded as headers.
    pub fn new(credentials: &Credentials) -> Result<Self> {
        Self::with_config(
            credentials,
            RestConfig::from(BaseUrl::Data).api_version("v1beta1"),
        )
    }

    /// A client with a custom endpoint, retry policy, or timeout.
    ///
    /// # Errors
    /// Returns an error if the credentials cannot be encoded as headers.
    pub fn with_config(credentials: &Credentials, config: RestConfig) -> Result<Self> {
        Ok(Self {
            rest: RestClient::new(credentials, config)?,
        })
    }

    /// The underlying transport.
    #[must_use]
    pub fn rest(&self) -> &RestClient {
        &self.rest
    }

    /// Historical rates, keyed by currency pair.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_forex_rates(&self, request: &ForexRatesRequest) -> Result<ForexRateSet> {
        let merged = get_marketdata(
            &self.rest,
            &MarketDataRequest::paged_with_limit("/forex/rates", 1000),
            request,
        )
        .await?;
        into_sets(merged)
    }

    /// The latest rate for each currency pair.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_forex_latest_rates(
        &self,
        request: &ForexLatestRatesRequest,
    ) -> Result<HashMap<String, ForexRate>> {
        let merged = get_marketdata(
            &self.rest,
            &MarketDataRequest::latest("/forex/latest/rates"),
            request,
        )
        .await?;
        into_latest(merged)
    }
}

/// Company logos.
///
/// The one route in this crate that answers with an image rather than JSON, so
/// it returns the PNG bytes. Like forex, it is unverified: a plan that reaches
/// SIP still answers `403 Subscription does not permit querying logos`.
///
/// See <https://docs.alpaca.markets/us/reference/logos-5>.
#[derive(Debug, Clone)]
pub struct LogoClient {
    rest: RestClient,
}

impl LogoClient {
    /// A client for the logo API.
    ///
    /// # Errors
    /// Returns an error if the credentials cannot be encoded as headers.
    pub fn new(credentials: &Credentials) -> Result<Self> {
        Self::with_config(
            credentials,
            RestConfig::from(BaseUrl::Data).api_version("v1beta1"),
        )
    }

    /// A client with a custom endpoint, retry policy, or timeout.
    ///
    /// # Errors
    /// Returns an error if the credentials cannot be encoded as headers.
    pub fn with_config(credentials: &Credentials, config: RestConfig) -> Result<Self> {
        Ok(Self {
            rest: RestClient::new(credentials, config)?,
        })
    }

    /// The underlying transport.
    #[must_use]
    pub fn rest(&self) -> &RestClient {
        &self.rest
    }

    /// The company logo for `symbol`, as PNG bytes.
    ///
    /// Alpaca serves a generated placeholder when it has no logo, so an empty
    /// result means the request failed rather than that no logo exists. Set
    /// [`LogoRequest::placeholder`] to `false` to tell the two apart.
    ///
    /// # Errors
    /// Propagates transport and API failures.
    pub async fn get_logo(&self, symbol: &str, request: &LogoRequest) -> Result<Vec<u8>> {
        let path = format!("/logos/{}", segment(symbol)?);
        self.rest.get_bytes(&path, request).await
    }
}

/// News articles.
#[derive(Debug, Clone)]
pub struct NewsClient {
    rest: RestClient,
}

impl NewsClient {
    /// A client for the news API.
    ///
    /// # Errors
    /// Returns an error if the credentials cannot be encoded as headers.
    pub fn new(credentials: &Credentials) -> Result<Self> {
        Self::with_config(
            credentials,
            RestConfig::from(BaseUrl::Data).api_version("v1beta1"),
        )
    }

    /// A client with a custom endpoint, retry policy, or timeout.
    ///
    /// # Errors
    /// Returns an error if the credentials cannot be encoded as headers.
    pub fn with_config(credentials: &Credentials, config: RestConfig) -> Result<Self> {
        Ok(Self {
            rest: RestClient::new(credentials, config)?,
        })
    }

    /// The underlying transport.
    #[must_use]
    pub fn rest(&self) -> &RestClient {
        &self.rest
    }

    /// News articles matching the filter.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_news(&self, request: &NewsRequest) -> Result<NewsSet> {
        // News pages at 50, not the usual 10,000.
        let merged = get_marketdata(
            &self.rest,
            &MarketDataRequest::paged_with_limit("/news", 50),
            request,
        )
        .await?;

        let articles: Vec<News> = match merged.data.get("news") {
            Some(value) => Vec::<News>::deserialize(value).map_err(|source| Error::Decode {
                path: merged.path.clone(),
                body: decode_body("news", value),
                source,
            })?,
            None => Vec::new(),
        };

        Ok(NewsSet {
            news: articles,
            // The merge loop follows every page, so nothing is left to resume.
            // Always None once pagination has run to completion; the field
            // exists for the single-page shape.
            next_page_token: None,
        })
    }
}

/// Stock screener rankings.
#[derive(Debug, Clone)]
pub struct ScreenerClient {
    rest: RestClient,
}

impl ScreenerClient {
    /// A client for the screener API.
    ///
    /// # Errors
    /// Returns an error if the credentials cannot be encoded as headers.
    pub fn new(credentials: &Credentials) -> Result<Self> {
        Self::with_config(
            credentials,
            RestConfig::from(BaseUrl::Data).api_version("v1beta1"),
        )
    }

    /// A client with a custom endpoint, retry policy, or timeout.
    ///
    /// # Errors
    /// Returns an error if the credentials cannot be encoded as headers.
    pub fn with_config(credentials: &Credentials, config: RestConfig) -> Result<Self> {
        Ok(Self {
            rest: RestClient::new(credentials, config)?,
        })
    }

    /// The underlying transport.
    #[must_use]
    pub fn rest(&self) -> &RestClient {
        &self.rest
    }

    /// The most actively traded stocks.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_most_actives(&self, request: &MostActivesRequest) -> Result<MostActives> {
        self.rest
            .get("/screener/stocks/most-actives", request)
            .await
    }

    /// The day's biggest gainers and losers.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_market_movers(&self, request: &MarketMoversRequest) -> Result<Movers> {
        // The market type selects the path rather than filtering the query.
        let path = format!("/screener/{}/movers", segment(&request.market_type)?);
        self.rest.get(&path, request).await
    }
}

/// Corporate actions, polled or streamed.
#[derive(Debug, Clone)]
pub struct CorporateActionsClient {
    rest: RestClient,
    /// A second HTTP client, for the event stream: its body is read
    /// incrementally rather than decoded whole.
    raw: reqwest::Client,
    /// The endpoint, kept because the stream's version segment is `v1beta1`
    /// while the polled route's is `v1`.
    base_url: String,
}

impl CorporateActionsClient {
    /// A client for the corporate actions API.
    ///
    /// # Errors
    /// Returns an error if the credentials cannot be encoded as headers.
    pub fn new(credentials: &Credentials) -> Result<Self> {
        Self::with_config(
            credentials,
            RestConfig::from(BaseUrl::Data).api_version("v1"),
        )
    }

    /// A client with a custom endpoint, retry policy, or timeout.
    ///
    /// # Errors
    /// Returns an error if the credentials cannot be encoded as headers.
    pub fn with_config(credentials: &Credentials, config: RestConfig) -> Result<Self> {
        Ok(Self {
            raw: crate::sse::streaming_client(credentials, crate::sse::Redirects::Refuse)?,
            base_url: config.base_url.clone(),
            rest: RestClient::new(credentials, config)?,
        })
    }

    /// The underlying transport.
    #[must_use]
    pub fn rest(&self) -> &RestClient {
        &self.rest
    }

    /// Corporate actions matching the filter, grouped by kind.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_corporate_actions(
        &self,
        request: &CorporateActionsRequest,
    ) -> Result<CorporateActions> {
        let merged = get_marketdata(
            &self.rest,
            &MarketDataRequest::paged_with_limit("/corporate-actions", 1000),
            request,
        )
        .await?;

        let body = Value::Object(merged.data);
        CorporateActions::deserialize(&body).map_err(|source| Error::Decode {
            path: merged.path,
            body: crate::rest::truncate(&body.to_string()),
            source,
        })
    }

    /// Streams every corporate-action mutation as it happens.
    ///
    /// The push counterpart to
    /// [`get_corporate_actions`](Self::get_corporate_actions), covering
    /// `insert`, `update` and `delete` across all fifteen action types. Each
    /// event's payload is JSON for the caller to deserialize — the envelope's
    /// `event_type` selects which of fifteen shapes `ca` takes, and no captured
    /// payload exists to model any of them from.
    ///
    /// On its own version segment, `v1beta1`, rather than the `v1` the polled
    /// route uses.
    ///
    /// # Errors
    /// Propagates transport failures and any non-success status the server
    /// answers the subscription with.
    pub async fn get_corporate_action_events(
        &self,
        filter: Option<&CorporateActionEventsRequest>,
    ) -> Result<impl futures_util::Stream<Item = Result<crate::sse::Event>> + use<>> {
        let path = "/events/corporate-actions";
        let url = format!("{}/v1beta1{path}", self.base_url.trim_end_matches('/'));
        let query = filter
            .map(CorporateActionEventsRequest::query)
            .unwrap_or_default();
        crate::sse::subscribe(&self.raw, &url, path, &query).await
    }
}
