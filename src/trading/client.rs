//! The trading API client, ported from `alpaca/trading/client.py`.

use reqwest::Method;
use uuid::Uuid;

use crate::auth::Credentials;
use crate::config::BaseUrl;
use crate::error::Result;
use crate::rest::{Empty, RestClient, RestConfig};
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
}
