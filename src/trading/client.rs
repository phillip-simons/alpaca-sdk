//! The [trading API](https://docs.alpaca.markets/us/docs/trading-api) client.
//!
//! Ported from `alpaca/trading/client.py`.

use reqwest::Method;
use uuid::Uuid;

use crate::auth::Credentials;
use crate::config::BaseUrl;
use crate::error::Result;
use crate::rest::{Empty, RestClient, RestConfig};
use crate::sse::EventStreamRequest;
use crate::trading::enums::ActivityType;
use crate::trading::locates::{
    CreateLocateRequest, GetLocateQuotesRequest, GetLocatesRequest, Locate, LocateQuotes,
    LocatesPage,
};
use crate::trading::markets::{GetMarketCalendarRequest, Market, MarketCalendar};
use crate::trading::models::{
    AccountConfiguration, Activity, Asset, Calendar, Clock, ClosePositionResponse,
    CorporateActionAnnouncement, OptionContract, OptionContractsResponse, Order, PortfolioHistory,
    Position, TradeAccount, Watchlist,
};
use crate::trading::requests::{
    CancelOrderResponse, ClosePositionRequest, CreateWatchlistRequest,
    GetCorporateAnnouncementsRequest, GetOptionContractsRequest, GetOrderByIdRequest,
    GetOrdersRequest, GetPortfolioHistoryRequest, OrderRequest, ReplaceOrderRequest,
    UpdateWatchlistRequest,
};
use crate::trading::tokenization::{
    ByClientRequestId, GetTokenizationRequestsRequest, MintTokenRequest, TokenizationRequest,
};
use crate::trading::wallets::{
    CreateWhitelistedAddressRequest, CryptoTransfer, CryptoWallet, GetCryptoWalletsRequest,
    TransferFeeEstimate, TransferFeeEstimateRequest, WhitelistedAddress,
};
use crate::types::AssetIdent;

/// A client for Alpaca's trading API.
///
/// ```no_run
/// # use alpaca_sdk::{Credentials, trading::TradingClient};
/// # async fn example() -> alpaca_sdk::Result<()> {
/// let credentials = Credentials::from_env()?;
/// let client = TradingClient::new(&credentials, true)?;
///
/// let account = client.get_account().await?;
/// println!("{:?}", account.buying_power);
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct TradingClient {
    rest: RestClient,
    /// A second HTTP client, for the activity event stream: its response body
    /// is read incrementally rather than decoded whole.
    raw: reqwest::Client,
}

impl TradingClient {
    /// A client targeting the paper or live trading environment.
    ///
    /// # Errors
    /// Returns an error if the credentials cannot be encoded as headers or the
    /// underlying HTTP client fails to build.
    pub fn new(credentials: &Credentials, paper: bool) -> Result<Self> {
        Self::with_config(credentials, RestConfig::from(BaseUrl::trading(paper)))
    }

    /// A client with a custom endpoint, retry policy, or timeout.
    ///
    /// # Errors
    /// Returns an error if the credentials cannot be encoded as headers or the
    /// underlying HTTP client fails to build.
    pub fn with_config(credentials: &Credentials, config: RestConfig) -> Result<Self> {
        Ok(Self {
            raw: crate::sse::streaming_client(credentials, &config)?,
            rest: RestClient::new(credentials, config)?,
        })
    }

    /// The underlying transport, for routes this client does not wrap.
    ///
    /// This is the typed replacement for alpaca-py's `raw_data=True`.
    #[must_use]
    pub fn rest(&self) -> &RestClient {
        &self.rest
    }

    /// Issues a request whose response body is discarded.
    ///
    /// Several routes answer `204 No Content`, and `exercise_options_position`
    /// answers with a bare string that is not JSON.
    async fn send_void(&self, method: Method, path: &str) -> Result<()> {
        self.rest
            .request_raw(method, path, None::<&Empty>, None::<&Empty>)
            .await?;
        Ok(())
    }

    // ------------------------------------------------------------- orders

    /// Submits an order.
    ///
    /// # Errors
    /// Returns [`crate::Error::InvalidRequest`] if the order fails
    /// [`OrderRequest::validate`], or an API error if Alpaca rejects it.
    pub async fn submit_order(&self, order: &OrderRequest) -> Result<Order> {
        order.validate()?;
        self.rest.post("/orders", order).await
    }

    /// Lists orders, optionally filtered.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_orders(&self, filter: Option<&GetOrdersRequest>) -> Result<Vec<Order>> {
        match filter {
            Some(filter) => self.rest.get("/orders", filter).await,
            None => self.rest.get("/orders", &Empty).await,
        }
    }

    /// Fetches one order by its Alpaca id.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_order_by_id(
        &self,
        order_id: Uuid,
        filter: Option<&GetOrderByIdRequest>,
    ) -> Result<Order> {
        let path = format!("/orders/{order_id}");
        match filter {
            Some(filter) => self.rest.get(&path, filter).await,
            None => self.rest.get(&path, &Empty).await,
        }
    }

    /// Fetches one order by the client order id supplied when it was submitted.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_order_by_client_id(&self, client_order_id: &str) -> Result<Order> {
        self.rest
            .get(
                "/orders:by_client_order_id",
                &[("client_order_id", client_order_id)],
            )
            .await
    }

    /// Replaces an open order, returning the new order.
    ///
    /// # Errors
    /// Returns [`crate::Error::InvalidRequest`] if the replacement fails
    /// [`ReplaceOrderRequest::validate`], or an API error if Alpaca rejects it.
    pub async fn replace_order_by_id(
        &self,
        order_id: Uuid,
        replacement: Option<&ReplaceOrderRequest>,
    ) -> Result<Order> {
        let path = format!("/orders/{order_id}");
        match replacement {
            Some(replacement) => {
                replacement.validate()?;
                self.rest.patch(&path, replacement).await
            }
            None => self.rest.patch(&path, &Empty).await,
        }
    }

    /// Cancels every open order, reporting the outcome for each.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn cancel_orders(&self) -> Result<Vec<CancelOrderResponse>> {
        self.rest.delete("/orders", &Empty).await
    }

    /// Cancels one open order.
    ///
    /// # Errors
    /// Propagates transport and API failures.
    pub async fn cancel_order_by_id(&self, order_id: Uuid) -> Result<()> {
        self.send_void(Method::DELETE, &format!("/orders/{order_id}"))
            .await
    }

    // ---------------------------------------------------------- positions

    /// Lists every open position.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_all_positions(&self) -> Result<Vec<Position>> {
        self.rest.get("/positions", &Empty).await
    }

    /// Fetches one open position by symbol or asset id.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_open_position(&self, asset: &AssetIdent) -> Result<Position> {
        self.rest.get(&format!("/positions/{asset}"), &Empty).await
    }

    /// Liquidates every open position, reporting the outcome for each.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn close_all_positions(
        &self,
        cancel_orders: Option<bool>,
    ) -> Result<Vec<ClosePositionResponse>> {
        match cancel_orders {
            Some(cancel) => {
                self.rest
                    .delete("/positions", &[("cancel_orders", cancel)])
                    .await
            }
            None => self.rest.delete("/positions", &Empty).await,
        }
    }

    /// Liquidates one position, in full or in part.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn close_position(
        &self,
        asset: &AssetIdent,
        close: Option<ClosePositionRequest>,
    ) -> Result<Order> {
        let path = format!("/positions/{asset}");
        match close {
            Some(close) => self.rest.delete(&path, &close.to_query()).await,
            None => self.rest.delete(&path, &Empty).await,
        }
    }

    /// Exercises a held option contract.
    ///
    /// # Errors
    /// Propagates transport and API failures.
    pub async fn exercise_options_position(&self, contract: &AssetIdent) -> Result<()> {
        self.send_void(Method::POST, &format!("/positions/{contract}/exercise"))
            .await
    }

    // ------------------------------------------------------------ account

    /// Fetches the trading account.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_account(&self) -> Result<TradeAccount> {
        self.rest.get("/account", &Empty).await
    }

    /// Fetches the account's configuration.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_account_configurations(&self) -> Result<AccountConfiguration> {
        self.rest.get("/account/configurations", &Empty).await
    }

    /// Updates the account's configuration.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn set_account_configurations(
        &self,
        configuration: &AccountConfiguration,
    ) -> Result<AccountConfiguration> {
        self.rest
            .patch("/account/configurations", configuration)
            .await
    }

    /// Fetches the account's value over time.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_portfolio_history(
        &self,
        filter: Option<&GetPortfolioHistoryRequest>,
    ) -> Result<PortfolioHistory> {
        let path = "/account/portfolio/history";
        match filter {
            Some(filter) => self.rest.get(path, filter).await,
            None => self.rest.get(path, &Empty).await,
        }
    }

    /// Lists account activities, optionally filtered by query parameters.
    ///
    /// The endpoint returns a heterogeneous array; see [`Activity`].
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_account_activities(&self, query: &[(&str, String)]) -> Result<Vec<Activity>> {
        self.rest.get("/account/activities", query).await
    }

    // ------------------------------------------------------------- assets

    /// Lists assets, optionally filtered.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_all_assets(
        &self,
        filter: Option<&crate::trading::requests::GetAssetsRequest>,
    ) -> Result<Vec<Asset>> {
        match filter {
            Some(filter) => self.rest.get("/assets", filter).await,
            None => self.rest.get("/assets", &Empty).await,
        }
    }

    /// Fetches one asset by symbol or id.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_asset(&self, asset: &AssetIdent) -> Result<Asset> {
        self.rest.get(&format!("/assets/{asset}"), &Empty).await
    }

    // ------------------------------------------------------ market status

    /// Fetches the current market clock.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_clock(&self) -> Result<Clock> {
        self.rest.get("/clock", &Empty).await
    }

    /// Fetches the market calendar.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_calendar(
        &self,
        filter: Option<&crate::trading::requests::GetCalendarRequest>,
    ) -> Result<Vec<Calendar>> {
        match filter {
            Some(filter) => self.rest.get("/calendar", filter).await,
            None => self.rest.get("/calendar", &Empty).await,
        }
    }

    // --------------------------------------------------------- watchlists

    /// Lists the account's watchlists.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_watchlists(&self) -> Result<Vec<Watchlist>> {
        self.rest.get("/watchlists", &Empty).await
    }

    /// Fetches one watchlist.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_watchlist_by_id(&self, watchlist_id: Uuid) -> Result<Watchlist> {
        self.rest
            .get(&format!("/watchlists/{watchlist_id}"), &Empty)
            .await
    }

    /// Creates a watchlist.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn create_watchlist(&self, watchlist: &CreateWatchlistRequest) -> Result<Watchlist> {
        self.rest.post("/watchlists", watchlist).await
    }

    /// Updates a watchlist's name, symbols, or both.
    ///
    /// # Errors
    /// Returns [`crate::Error::InvalidRequest`] if neither field is set.
    pub async fn update_watchlist_by_id(
        &self,
        watchlist_id: Uuid,
        update: &UpdateWatchlistRequest,
    ) -> Result<Watchlist> {
        update.validate()?;
        self.rest
            .put(&format!("/watchlists/{watchlist_id}"), update)
            .await
    }

    /// Adds one asset to a watchlist.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn add_asset_to_watchlist_by_id(
        &self,
        watchlist_id: Uuid,
        symbol: &str,
    ) -> Result<Watchlist> {
        self.rest
            .post(
                &format!("/watchlists/{watchlist_id}"),
                &serde_json::json!({ "symbol": symbol }),
            )
            .await
    }

    /// Removes one asset from a watchlist.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn remove_asset_from_watchlist_by_id(
        &self,
        watchlist_id: Uuid,
        symbol: &str,
    ) -> Result<Watchlist> {
        self.rest
            .delete(&format!("/watchlists/{watchlist_id}/{symbol}"), &Empty)
            .await
    }

    /// Deletes a watchlist.
    ///
    /// # Errors
    /// Propagates transport and API failures.
    pub async fn delete_watchlist_by_id(&self, watchlist_id: Uuid) -> Result<()> {
        self.send_void(Method::DELETE, &format!("/watchlists/{watchlist_id}"))
            .await
    }

    // -------------------------------------------------- corporate actions

    /// Searches corporate action announcements.
    ///
    /// # Errors
    /// Returns [`crate::Error::InvalidRequest`] if the date window exceeds 90
    /// days, or an API error if Alpaca rejects the request.
    #[deprecated(
        since = "0.1.0",
        note = "Alpaca deprecated this route; use the corporate actions market data endpoint instead"
    )]
    pub async fn get_corporate_announcements(
        &self,
        filter: &GetCorporateAnnouncementsRequest,
    ) -> Result<Vec<CorporateActionAnnouncement>> {
        filter.validate()?;
        self.rest
            .get("/corporate_actions/announcements", &filter.to_query())
            .await
    }

    /// Fetches one corporate action announcement.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    #[deprecated(
        since = "0.1.0",
        note = "Alpaca deprecated this route; use the corporate actions market data endpoint instead"
    )]
    pub async fn get_corporate_announcement_by_id(
        &self,
        announcement_id: Uuid,
    ) -> Result<CorporateActionAnnouncement> {
        self.rest
            .get(
                &format!("/corporate_actions/announcements/{announcement_id}"),
                &Empty,
            )
            .await
    }

    // ------------------------------------------------------------ options

    /// Lists option contracts matching the filter.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_option_contracts(
        &self,
        filter: &GetOptionContractsRequest,
    ) -> Result<OptionContractsResponse> {
        self.rest.get("/options/contracts", filter).await
    }

    /// Fetches one option contract by symbol or id.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_option_contract(&self, contract: &AssetIdent) -> Result<OptionContract> {
        self.rest
            .get(&format!("/options/contracts/{contract}"), &Empty)
            .await
    }

    /// Declines to exercise an in-the-money option position at expiry.
    ///
    /// Answers `204 No Content`, so there is nothing to return. Not in
    /// alpaca-py.
    ///
    /// # Errors
    /// Propagates transport and API failures.
    pub async fn exercise_do_not_exercise(&self, contract: &AssetIdent) -> Result<()> {
        let path = format!("/positions/{contract}/do-not-exercise");
        self.rest.post(&path, &Empty).await
    }

    // ------------------------------------------------ activities by type

    /// Account activities of one type.
    ///
    /// The narrowed counterpart to
    /// [`get_account_activities`](Self::get_account_activities): the type moves
    /// from the query string into the path, which is the only way to ask for
    /// exactly one kind.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_account_activities_by_type(
        &self,
        activity_type: &ActivityType,
        query: &[(&str, String)],
    ) -> Result<Vec<Activity>> {
        let path = format!("/account/activities/{activity_type}");
        self.rest.get(&path, query).await
    }

    // ---------------------------------------------------- watchlists by name

    /// Fetches one watchlist by name.
    ///
    /// The name goes in the query string, not the path: the route is literally
    /// `/v2/watchlists:by_name`, colon and all.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_watchlist_by_name(&self, name: &str) -> Result<Watchlist> {
        self.rest
            .get("/watchlists:by_name", &[("name", name)])
            .await
    }

    /// Updates a watchlist by name.
    ///
    /// # Errors
    /// Returns [`crate::Error::InvalidRequest`] if the update changes nothing.
    pub async fn update_watchlist_by_name(
        &self,
        name: &str,
        update: &UpdateWatchlistRequest,
    ) -> Result<Watchlist> {
        update.validate()?;
        self.rest
            .request(
                Method::PUT,
                "/watchlists:by_name",
                Some(&[("name", name)]),
                Some(update),
            )
            .await
    }

    /// Adds one asset to a watchlist by name.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn add_asset_to_watchlist_by_name(
        &self,
        name: &str,
        symbol: &str,
    ) -> Result<Watchlist> {
        self.rest
            .request(
                Method::POST,
                "/watchlists:by_name",
                Some(&[("name", name)]),
                Some(&serde_json::json!({ "symbol": symbol })),
            )
            .await
    }

    /// Deletes a watchlist by name.
    ///
    /// # Errors
    /// Propagates transport and API failures.
    pub async fn delete_watchlist_by_name(&self, name: &str) -> Result<()> {
        self.rest
            .delete("/watchlists:by_name", &[("name", name)])
            .await
    }

    // ---------------------------------------------------------- calendar

    /// A named market's calendar.
    ///
    /// **A `v3` route**, where [`get_calendar`](Self::get_calendar) is `v2` and
    /// the broker's equivalent is `v2` again. It answers with pre-market, core,
    /// lunch and post-market windows as absolute instants rather than with the
    /// naive eastern-time open and close [`Calendar`] carries.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_market_calendar(
        &self,
        market: &Market,
        filter: Option<&GetMarketCalendarRequest>,
    ) -> Result<MarketCalendar> {
        let path = format!("/calendar/{market}");
        match filter {
            Some(filter) => self.rest.at_version("v3").get(&path, filter).await,
            None => self.rest.at_version("v3").get(&path, &Empty).await,
        }
    }

    // ----------------------------------------------------------- locates

    /// Lists locates, optionally filtered.
    ///
    /// **A `v1` route**, unlike the rest of this client.
    ///
    /// Returns one page. The response carries a `next_page_token`; pass it back
    /// in [`GetLocatesRequest::page_token`] for the next.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_locates(&self, filter: Option<&GetLocatesRequest>) -> Result<LocatesPage> {
        match filter {
            Some(filter) => self.rest.at_version("v1").get("/locates", filter).await,
            None => self.rest.at_version("v1").get("/locates", &Empty).await,
        }
    }

    /// Fetches one locate.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_locate_by_id(&self, locate_id: Uuid) -> Result<Locate> {
        self.rest
            .at_version("v1")
            .get(&format!("/locates/{locate_id}"), &Empty)
            .await
    }

    /// What the named symbols currently cost to borrow.
    ///
    /// Partial success is normal: a symbol that is easy to borrow needs no
    /// locate and comes back under
    /// [`errors`](crate::trading::LocateQuotes::errors) rather than as a failed request.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_locate_quotes(
        &self,
        request: &GetLocateQuotesRequest,
    ) -> Result<LocateQuotes> {
        self.rest
            .at_version("v1")
            .get("/locates/quotes", request)
            .await
    }

    /// Requests a locate.
    ///
    /// # Errors
    /// Returns [`crate::Error::InvalidRequest`] if the request contradicts
    /// itself; see [`CreateLocateRequest::validate`].
    pub async fn create_locate(&self, request: &CreateLocateRequest) -> Result<Locate> {
        request.validate()?;
        self.rest.at_version("v1").post("/locates", request).await
    }

    // ------------------------------------------------------ tokenization

    /// Mints a tokenized asset from a position.
    ///
    /// # Errors
    /// Returns [`crate::Error::InvalidRequest`] if `qty` is not positive.
    pub async fn mint_token(&self, request: &MintTokenRequest) -> Result<TokenizationRequest> {
        request.validate()?;
        self.rest.post("/tokenization/mint", request).await
    }

    /// Lists tokenization requests.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_tokenization_requests(
        &self,
        filter: Option<&GetTokenizationRequestsRequest>,
    ) -> Result<Vec<TokenizationRequest>> {
        match filter {
            Some(filter) => self.rest.get("/tokenization/requests", filter).await,
            None => self.rest.get("/tokenization/requests", &Empty).await,
        }
    }

    /// Fetches one tokenization request.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_tokenization_request(&self, request_id: Uuid) -> Result<TokenizationRequest> {
        self.rest
            .get(&format!("/tokenization/requests/{request_id}"), &Empty)
            .await
    }

    /// Fetches one tokenization request by the caller's own id.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_tokenization_request_by_client_id(
        &self,
        request: &ByClientRequestId,
    ) -> Result<TokenizationRequest> {
        self.rest
            .get("/tokenization/requests:by_client_request_id", request)
            .await
    }

    // ----------------------------------------------------- crypto funding

    /// The account's crypto deposit wallets.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_crypto_wallets(
        &self,
        filter: Option<&GetCryptoWalletsRequest>,
    ) -> Result<CryptoWallet> {
        match filter {
            Some(filter) => self.rest.get("/wallets", filter).await,
            None => self.rest.get("/wallets", &Empty).await,
        }
    }

    /// The account's on-chain transfers.
    ///
    /// The withdrawal route that would create one is deliberately absent: it is
    /// deprecated with a sunset of 2026-10-09 and the reference's replacement
    /// is the Alpaca web application, not another endpoint. See
    /// [`crate::trading::wallets`].
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_crypto_transfers(&self) -> Result<CryptoTransfer> {
        self.rest.get("/wallets/transfers", &Empty).await
    }

    /// Fetches one on-chain transfer.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_crypto_transfer(&self, transfer_id: &str) -> Result<CryptoTransfer> {
        self.rest
            .get(&format!("/wallets/transfers/{transfer_id}"), &Empty)
            .await
    }

    /// The addresses withdrawals may be sent to.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_whitelisted_addresses(&self) -> Result<WhitelistedAddress> {
        self.rest.get("/wallets/whitelists", &Empty).await
    }

    /// Allowlists a withdrawal address.
    ///
    /// New entries land [`Pending`](crate::trading::WhitelistStatus::Pending) and become usable
    /// after Alpaca's cooling-off period.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn create_whitelisted_address(
        &self,
        request: &CreateWhitelistedAddressRequest,
    ) -> Result<WhitelistedAddress> {
        self.rest.post("/wallets/whitelists", request).await
    }

    /// Removes an allowlisted withdrawal address.
    ///
    /// # Errors
    /// Propagates transport and API failures.
    pub async fn delete_whitelisted_address(&self, address_id: &str) -> Result<()> {
        self.rest
            .delete(&format!("/wallets/whitelists/{address_id}"), &Empty)
            .await
    }

    /// What a proposed transfer would cost in gas.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn estimate_transfer_fee(
        &self,
        request: &TransferFeeEstimateRequest,
    ) -> Result<TransferFeeEstimate> {
        self.rest.get("/wallets/fees/estimate", request).await
    }

    // ------------------------------------------------------------- events

    /// Streams account activity events as they happen.
    ///
    /// A `v2beta1` route, and the push counterpart to
    /// [`get_account_activities`](Self::get_account_activities). The broker API
    /// carries the same stream; this is the one a trading-only build can reach.
    ///
    /// # Errors
    /// Propagates transport failures and any non-success status the server
    /// answers the subscription with.
    pub async fn get_activity_events(
        &self,
        filter: Option<&EventStreamRequest>,
    ) -> Result<impl futures_util::Stream<Item = Result<crate::sse::Event>> + use<>> {
        let path = "/events/activities";
        let url = format!(
            "{}/v2beta1{path}",
            self.rest.config().base_url.trim_end_matches('/')
        );
        let query = filter.map(EventStreamRequest::query).unwrap_or_default();
        crate::sse::subscribe(&self.raw, &url, path, &query).await
    }
}
