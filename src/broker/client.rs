//! The broker API client, ported from `alpaca/broker/client.py`.
//!
//! Two things differ from every other client in this crate. It authenticates
//! with HTTP basic auth rather than the `APCA-*` headers, and it acts *on behalf
//! of* an account, so most routes carry an account id in the path.

use reqwest::Method;
use uuid::Uuid;

use crate::auth::Credentials;
use crate::broker::models::{Account, AllAccountsPositions, Order, TradeAccount};
use crate::broker::requests::{CreateOptionExerciseRequest, OrderRequest};
use crate::config::BaseUrl;
use crate::error::Result;
use crate::rest::{Empty, RestClient, RestConfig};
use crate::trading::{Position, Watchlist};

/// A client for Alpaca's broker API.
///
/// ```no_run
/// # use alpaca_sdk::{Credentials, broker::BrokerClient};
/// # async fn example() -> alpaca_sdk::Result<()> {
/// // Broker credentials authenticate with basic auth; the client converts
/// // a key pair for you.
/// let client = BrokerClient::new(&Credentials::from_env()?, true)?;
///
/// let accounts = client.get_all_accounts_positions().await?;
/// println!("{} accounts hold positions", accounts.positions.len());
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct BrokerClient {
    rest: RestClient,
}

impl BrokerClient {
    /// A client targeting the broker sandbox or production environment.
    ///
    /// The credentials are converted to basic auth, which is what this API
    /// expects; passing an already-basic or OAuth credential leaves it alone.
    ///
    /// # Errors
    /// Returns an error if the credentials cannot be encoded as headers.
    pub fn new(credentials: &Credentials, sandbox: bool) -> Result<Self> {
        Self::with_config(
            credentials,
            RestConfig::from(BaseUrl::broker(sandbox)).api_version("v1"),
        )
    }

    /// A client with a custom endpoint, retry policy, or timeout.
    ///
    /// # Errors
    /// Returns an error if the credentials cannot be encoded as headers.
    pub fn with_config(credentials: &Credentials, config: RestConfig) -> Result<Self> {
        // alpaca-py sets use_basic_auth=True on BrokerClient and nothing else.
        let credentials = credentials.clone().into_basic();
        Ok(Self {
            rest: RestClient::new(&credentials, config)?,
        })
    }

    /// The underlying transport, for routes this client does not wrap.
    #[must_use]
    pub fn rest(&self) -> &RestClient {
        &self.rest
    }

    /// Issues a request whose response body is discarded.
    ///
    /// Several broker routes answer `204 No Content`, and the exercise route
    /// answers with a bare string that is not JSON.
    async fn send_void<B: serde::Serialize + ?Sized>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
    ) -> Result<()> {
        self.rest
            .request_raw(method, path, None::<&Empty>, body)
            .await?;
        Ok(())
    }

    // ---------------------------------------------------------- accounts

    /// Fetches one account.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_account_by_id(&self, account_id: Uuid) -> Result<Account> {
        self.rest
            .get(&format!("/accounts/{account_id}"), &Empty)
            .await
    }

    /// Lists accounts matching the query.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn list_accounts(&self, query: &[(&str, String)]) -> Result<Vec<Account>> {
        self.rest.get("/accounts", query).await
    }

    /// Opens a new account.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn create_account<B: serde::Serialize + ?Sized>(
        &self,
        account: &B,
    ) -> Result<Account> {
        self.rest.post("/accounts", account).await
    }

    /// Updates an account.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn update_account<B: serde::Serialize + ?Sized>(
        &self,
        account_id: Uuid,
        update: &B,
    ) -> Result<Account> {
        self.rest
            .patch(&format!("/accounts/{account_id}"), update)
            .await
    }

    /// Closes an active account.
    ///
    /// The account's records survive; only trading is stopped. Every position
    /// must be closed and every dollar withdrawn first — that is the caller's
    /// responsibility, and Alpaca rejects the request otherwise.
    ///
    /// alpaca-py also exposes this as a deprecated `delete_account`, which
    /// forwards here; there is one route, so there is one method.
    ///
    /// # Errors
    /// Propagates transport and API failures.
    pub async fn close_account(&self, account_id: Uuid) -> Result<()> {
        self.send_void(
            Method::POST,
            &format!("/accounts/{account_id}/actions/close"),
            None::<&Empty>,
        )
        .await
    }

    /// Positions held across every account, as of the last market close.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_all_accounts_positions(&self) -> Result<AllAccountsPositions> {
        self.rest.get("/accounts/positions", &Empty).await
    }

    // ------------------------------------------- trading on behalf of an account

    /// The trading view of an account: buying power, cash, equity.
    ///
    /// This is the same record [`crate::trading::TradingClient::get_account`]
    /// returns, for an account the broker acts on behalf of.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_trade_account_by_id(&self, account_id: Uuid) -> Result<TradeAccount> {
        self.rest
            .get(&format!("/trading/accounts/{account_id}/account"), &Empty)
            .await
    }

    /// An account's trading configuration.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_trade_configuration_for_account(
        &self,
        account_id: Uuid,
    ) -> Result<crate::trading::AccountConfiguration> {
        self.rest
            .get(
                &format!("/trading/accounts/{account_id}/account/configurations"),
                &Empty,
            )
            .await
    }

    /// Positions held by one account.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_all_positions_for_account(&self, account_id: Uuid) -> Result<Vec<Position>> {
        self.rest
            .get(&format!("/trading/accounts/{account_id}/positions"), &Empty)
            .await
    }

    /// One account's position in a single asset.
    ///
    /// # Errors
    /// Returns an API error if the account holds no position in the asset.
    pub async fn get_open_position_for_account(
        &self,
        account_id: Uuid,
        asset: &crate::types::AssetIdent,
    ) -> Result<Position> {
        self.rest
            .get(
                &format!("/trading/accounts/{account_id}/positions/{asset}"),
                &Empty,
            )
            .await
    }

    /// Liquidates every position held by one account.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn close_all_positions_for_account(
        &self,
        account_id: Uuid,
        cancel_orders: Option<bool>,
    ) -> Result<Vec<crate::trading::ClosePositionResponse>> {
        let path = format!("/trading/accounts/{account_id}/positions");
        match cancel_orders {
            Some(cancel) => self.rest.delete(&path, &[("cancel_orders", cancel)]).await,
            None => self.rest.delete(&path, &Empty).await,
        }
    }

    /// Liquidates one position held by an account.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn close_position_for_account(
        &self,
        account_id: Uuid,
        asset: &crate::types::AssetIdent,
        close: Option<crate::trading::ClosePositionRequest>,
    ) -> Result<Order> {
        let path = format!("/trading/accounts/{account_id}/positions/{asset}");
        match close {
            Some(close) => self.rest.delete(&path, &close.to_query()).await,
            None => self.rest.delete(&path, &Empty).await,
        }
    }

    /// Exercises an option contract held by an account.
    ///
    /// Every held share of the contract is exercised; Alpaca offers no partial
    /// exercise. Requests submitted outside market hours are rejected.
    ///
    /// # Errors
    /// Propagates transport and API failures.
    pub async fn exercise_options_position_for_account_by_id(
        &self,
        account_id: Uuid,
        contract: &crate::types::AssetIdent,
        exercise: &CreateOptionExerciseRequest,
    ) -> Result<()> {
        self.send_void(
            Method::POST,
            &format!("/trading/accounts/{account_id}/positions/{contract}/exercise"),
            Some(exercise),
        )
        .await
    }

    // ------------------------------------------------------------- orders

    /// Submits an order on behalf of an account.
    ///
    /// # Errors
    /// Returns [`crate::Error::InvalidRequest`] if the order fails
    /// [`crate::trading::OrderRequest::validate`], or an API error if Alpaca
    /// rejects it.
    pub async fn submit_order_for_account(
        &self,
        account_id: Uuid,
        order: &OrderRequest,
    ) -> Result<Order> {
        order.validate()?;
        self.rest
            .post(&format!("/trading/accounts/{account_id}/orders"), order)
            .await
    }

    /// Lists an account's orders, optionally filtered.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_orders_for_account(
        &self,
        account_id: Uuid,
        filter: Option<&crate::trading::GetOrdersRequest>,
    ) -> Result<Vec<Order>> {
        let path = format!("/trading/accounts/{account_id}/orders");
        match filter {
            Some(filter) => self.rest.get(&path, filter).await,
            None => self.rest.get(&path, &Empty).await,
        }
    }

    /// Fetches one of an account's orders by its Alpaca id.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_order_for_account_by_id(
        &self,
        account_id: Uuid,
        order_id: Uuid,
        filter: Option<&crate::trading::GetOrderByIdRequest>,
    ) -> Result<Order> {
        let path = format!("/trading/accounts/{account_id}/orders/{order_id}");
        match filter {
            Some(filter) => self.rest.get(&path, filter).await,
            None => self.rest.get(&path, &Empty).await,
        }
    }

    /// Fetches one of an account's orders by the client order id it was
    /// submitted with.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_order_for_account_by_client_id(
        &self,
        account_id: Uuid,
        client_order_id: &str,
    ) -> Result<Order> {
        self.rest
            .get(
                &format!("/trading/accounts/{account_id}/orders:by_client_order_id"),
                &[("client_order_id", client_order_id)],
            )
            .await
    }

    /// Replaces one of an account's open orders, returning the new order.
    ///
    /// # Errors
    /// Returns [`crate::Error::InvalidRequest`] if the replacement fails
    /// [`crate::trading::ReplaceOrderRequest::validate`], or an API error if
    /// Alpaca rejects it.
    pub async fn replace_order_for_account_by_id(
        &self,
        account_id: Uuid,
        order_id: Uuid,
        replacement: Option<&crate::trading::ReplaceOrderRequest>,
    ) -> Result<Order> {
        let path = format!("/trading/accounts/{account_id}/orders/{order_id}");
        match replacement {
            Some(replacement) => {
                replacement.validate()?;
                self.rest.patch(&path, replacement).await
            }
            None => self.rest.patch(&path, &Empty).await,
        }
    }

    /// Cancels every open order for an account.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn cancel_orders_for_account(
        &self,
        account_id: Uuid,
    ) -> Result<Vec<crate::trading::CancelOrderResponse>> {
        self.rest
            .delete(&format!("/trading/accounts/{account_id}/orders"), &Empty)
            .await
    }

    /// Cancels one of an account's open orders.
    ///
    /// # Errors
    /// Propagates transport and API failures.
    pub async fn cancel_order_for_account_by_id(
        &self,
        account_id: Uuid,
        order_id: Uuid,
    ) -> Result<()> {
        self.send_void(
            Method::DELETE,
            &format!("/trading/accounts/{account_id}/orders/{order_id}"),
            None::<&Empty>,
        )
        .await
    }

    /// The account's value over time.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_portfolio_history_for_account(
        &self,
        account_id: Uuid,
        filter: Option<&crate::trading::GetPortfolioHistoryRequest>,
    ) -> Result<crate::trading::PortfolioHistory> {
        let path = format!("/trading/accounts/{account_id}/account/portfolio/history");
        match filter {
            Some(filter) => self.rest.get(&path, filter).await,
            None => self.rest.get(&path, &Empty).await,
        }
    }

    /// Updates an account's trading configuration.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn update_trade_configuration_for_account(
        &self,
        account_id: Uuid,
        configuration: &crate::trading::AccountConfiguration,
    ) -> Result<crate::trading::AccountConfiguration> {
        self.rest
            .patch(
                &format!("/trading/accounts/{account_id}/account/configurations"),
                configuration,
            )
            .await
    }

    // -------------------------------------------------------- watchlists

    /// Lists an account's watchlists.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_watchlists_for_account(&self, account_id: Uuid) -> Result<Vec<Watchlist>> {
        self.rest
            .get(
                &format!("/trading/accounts/{account_id}/watchlists"),
                &Empty,
            )
            .await
    }

    /// Fetches one of an account's watchlists.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_watchlist_for_account_by_id(
        &self,
        account_id: Uuid,
        watchlist_id: Uuid,
    ) -> Result<Watchlist> {
        self.rest
            .get(
                &format!("/trading/accounts/{account_id}/watchlists/{watchlist_id}"),
                &Empty,
            )
            .await
    }

    /// Creates a watchlist for an account.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn create_watchlist_for_account(
        &self,
        account_id: Uuid,
        watchlist: &crate::trading::CreateWatchlistRequest,
    ) -> Result<Watchlist> {
        self.rest
            .post(
                &format!("/trading/accounts/{account_id}/watchlists"),
                watchlist,
            )
            .await
    }

    /// Replaces a watchlist's name and symbols.
    ///
    /// # Errors
    /// Returns [`crate::Error::InvalidRequest`] if neither field is set.
    pub async fn update_watchlist_for_account_by_id(
        &self,
        account_id: Uuid,
        watchlist_id: Uuid,
        update: &crate::trading::UpdateWatchlistRequest,
    ) -> Result<Watchlist> {
        update.validate()?;
        self.rest
            .put(
                &format!("/trading/accounts/{account_id}/watchlists/{watchlist_id}"),
                update,
            )
            .await
    }

    /// Adds one asset to an account's watchlist.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn add_asset_to_watchlist_for_account_by_id(
        &self,
        account_id: Uuid,
        watchlist_id: Uuid,
        symbol: &str,
    ) -> Result<Watchlist> {
        self.rest
            .post(
                &format!("/trading/accounts/{account_id}/watchlists/{watchlist_id}"),
                &serde_json::json!({ "symbol": symbol }),
            )
            .await
    }

    /// Removes one asset from an account's watchlist.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn remove_asset_from_watchlist_for_account_by_id(
        &self,
        account_id: Uuid,
        watchlist_id: Uuid,
        symbol: &str,
    ) -> Result<Watchlist> {
        self.rest
            .delete(
                &format!("/trading/accounts/{account_id}/watchlists/{watchlist_id}/{symbol}"),
                &Empty,
            )
            .await
    }

    /// Deletes one of an account's watchlists. This is permanent.
    ///
    /// # Errors
    /// Propagates transport and API failures.
    pub async fn delete_watchlist_from_account_by_id(
        &self,
        account_id: Uuid,
        watchlist_id: Uuid,
    ) -> Result<()> {
        self.send_void(
            Method::DELETE,
            &format!("/trading/accounts/{account_id}/watchlists/{watchlist_id}"),
            None::<&Empty>,
        )
        .await
    }

    // ------------------------------------------------------------- assets

    /// Lists tradable assets, optionally filtered.
    ///
    /// This route is not account-scoped: the asset master is the same for every
    /// account the broker serves.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_all_assets(
        &self,
        filter: Option<&crate::trading::GetAssetsRequest>,
    ) -> Result<Vec<crate::trading::Asset>> {
        match filter {
            Some(filter) => self.rest.get("/assets", filter).await,
            None => self.rest.get("/assets", &Empty).await,
        }
    }

    /// Fetches one asset by symbol or id.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_asset(
        &self,
        asset: &crate::types::AssetIdent,
    ) -> Result<crate::trading::Asset> {
        self.rest.get(&format!("/assets/{asset}"), &Empty).await
    }

    // ---------------------------------------------- corporate announcements

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
        filter: &crate::trading::GetCorporateAnnouncementsRequest,
    ) -> Result<Vec<crate::trading::CorporateActionAnnouncement>> {
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
    ) -> Result<crate::trading::CorporateActionAnnouncement> {
        self.rest
            .get(
                &format!("/corporate_actions/announcements/{announcement_id}"),
                &Empty,
            )
            .await
    }

    // ------------------------------------------------------ market status

    /// The broker API's market clock.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_clock(&self) -> Result<crate::trading::Clock> {
        self.rest.get("/clock", &Empty).await
    }

    /// The broker API's market calendar.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_calendar(
        &self,
        filter: Option<&crate::trading::GetCalendarRequest>,
    ) -> Result<Vec<crate::trading::Calendar>> {
        match filter {
            Some(filter) => self.rest.get("/calendar", filter).await,
            None => self.rest.get("/calendar", &Empty).await,
        }
    }
}
