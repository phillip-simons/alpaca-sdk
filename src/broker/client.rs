//! The broker API client, ported from `alpaca/broker/client.py`.
//!
//! Two things differ from every other client in this crate. It authenticates
//! with HTTP basic auth rather than the `APCA-*` headers, and it acts *on behalf
//! of* an account, so most routes carry an account id in the path.

use uuid::Uuid;

use crate::auth::Credentials;
use crate::broker::models::{Account, AllAccountsPositions};
use crate::config::BaseUrl;
use crate::error::Result;
use crate::rest::{Empty, RestClient, RestConfig};
use crate::trading::{Order, Position, Watchlist};

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

    /// Positions held across every account, as of the last market close.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_all_accounts_positions(&self) -> Result<AllAccountsPositions> {
        self.rest.get("/accounts/positions", &Empty).await
    }

    // ------------------------------------------- trading on behalf of an account

    /// Positions held by one account.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_all_positions_for_account(&self, account_id: Uuid) -> Result<Vec<Position>> {
        self.rest
            .get(&format!("/trading/accounts/{account_id}/positions"), &Empty)
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
