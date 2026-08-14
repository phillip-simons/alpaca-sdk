//! The [broker API](https://docs.alpaca.markets/us/docs/about-broker-api) client.
//!
//! Two things differ from every other client in this crate. It authenticates
//! with HTTP basic auth rather than the `APCA-*` headers, and it acts *on behalf
//! of* an account, so most routes carry an account id in the path.

use futures_util::Stream;
use reqwest::Method;
use uuid::Uuid;

use crate::auth::Credentials;
use crate::broker::enums::ACHRelationshipStatus;
use crate::broker::events::{BrokerEvent, EventVersion, GetEventsRequest};
use crate::broker::fixed_income::{
    EntryRequirement, GetEntryRequirementsRequest, GetUsCorporatesRequest, GetUsTreasuriesRequest,
    UsCorporates, UsTreasuries,
};
use crate::broker::fpsl::{
    FpslAnalytics, FpslLoansPage, FpslTier, GetFpslAnalyticsRequest, GetFpslLoansRequest,
};
use crate::broker::funding_wallet::{
    BatchCreateFundingWalletsRequest, CreateRecipientBankRequest, CreateWithdrawalRequest,
    DemoFundingRequest, FundingWallet, FundingWalletTransfer, FundingWalletTransfers,
    FundingWallets, GetFundingDetailsRequest, RecipientBank,
};
use crate::broker::instant_funding::{
    AccountInstantFundingLimits, CreateInstantFundingRequest,
    CreateInstantFundingSettlementRequest, GetAccountLimitsRequest, GetInstantFundingReportRequest,
    GetInstantFundingRequest, InstantFunding, InstantFundingLimits, InstantFundingReport,
};
use crate::broker::ipos::{GetIpoOfferingsRequest, IpoOfferingResponse, IpoOfferingsPage};
use crate::broker::jit::{
    CreateJitSettlementRequest, GetJitBalancesRequest, GetJitReportRequest, JitLedger,
    JitLedgerBalances, JitReport, JitTradingLimits,
};
use crate::broker::models::{
    ACHRelationship, Account, AllAccountsPositions, Bank, BatchJournalResponse, CIPInfo, Journal,
    Order, Portfolio, RebalancingRun, RunsPage, Subscription, SubscriptionsPage, TradeAccount,
    TradeDocument, Transfer,
};
use crate::broker::oauth::{
    GetOAuthClientRequest, OAuthClient, OAuthCode, OAuthRequest, OAuthToken,
};
use crate::broker::onboarding::{
    CountryInfo, EstimateOrderRequest, GetOnfidoTokenRequest, GetOptionsApprovalsRequest,
    IraExcessContribution, OnfidoToken, OptionsApproval, OptionsApprovalsPage,
    RequestOptionsApprovalRequest, TradingLimits, UpdateOnfidoOutcomeRequest,
};
use crate::broker::reporting::{
    AggregatePosition, AprTiers, CashInterestReport, EodPositions, GetAggregatePositionsRequest,
    GetCashInterestRequest, GetEodPositionsRequest,
};
use crate::broker::requests::{
    CreateACHRelationshipRequest, CreateAccountRequest, CreateBankRequest,
    CreateBatchJournalRequest, CreateJournalRequest, CreateOptionExerciseRequest,
    CreatePortfolioRequest, CreateReverseBatchJournalRequest, CreateRunRequest,
    CreateSubscriptionRequest, CreateTransferRequest, GetAccountActivitiesRequest,
    GetJournalsRequest, GetPortfoliosRequest, GetRunsRequest, GetSubscriptionsRequest,
    GetTradeDocumentsRequest, GetTransfersRequest, ListAccountsRequest, OrderRequest,
    UpdateAccountRequest, UpdatePortfolioRequest, UploadDocument,
};
use crate::broker::settlements::{GetSettlementsRequest, Settlement, Settlements};
use crate::config::BaseUrl;
use crate::error::Result;
use crate::rest::{Empty, RestClient, RestConfig};
use crate::sse::EventStreamRequest;
use crate::trading::{Activity, Position, Watchlist};
use crate::types::path::segment;
use crate::types::serde_util::OneOrMany;

/// A conventional ceiling on documents per upload.
///
/// Not documented by Alpaca — neither the reference nor the spec mentions a
/// count limit, only a 10MB ceiling on each document's contents. It is exposed
/// so a caller who wants a bound can apply one, and deliberately **not**
/// enforced by [`BrokerClient::upload_documents_to_account`]: an undocumented
/// limit is a guess, and rejecting a request Alpaca would have accepted is the
/// worse of the two errors.
pub const DOCUMENT_UPLOAD_LIMIT: usize = 10;

/// The id an activity pages from, whichever kind it is.
fn activity_id(activity: &Activity) -> &str {
    match activity {
        Activity::Trade(trade) => &trade.id,
        Activity::NonTrade(non_trade) => &non_trade.id,
    }
}

/// The page size assumed for the token-paginated broker routes when the caller
/// sets none. Alpaca's own default for these is 100.
const DEFAULT_PAGE_SIZE: u32 = 100;

/// The page size to ask for, given a cap on the total.
///
/// The last request is narrowed to exactly what is still wanted rather than
/// fetching a full page and discarding the tail — one fewer record crossing the
/// wire, and the cap is honoured exactly. Returns `None` when there is no cap,
/// leaving the caller's own page size alone.
fn page_limit(configured: &Option<u32>, max_items: Option<usize>, collected: usize) -> Option<u32> {
    let max_items = max_items?;
    let page_size = configured.unwrap_or(DEFAULT_PAGE_SIZE);
    let remaining = u32::try_from(max_items.saturating_sub(collected)).unwrap_or(u32::MAX);
    Some(page_size.min(remaining))
}

/// A client for Alpaca's broker API.
///
/// ```no_run
/// # use alpaca_sdk::{Credentials, broker::BrokerClient};
/// # async fn example() -> alpaca_sdk::Result<()> {
/// // Broker credentials authenticate with basic auth; the client converts
/// // a key pair for you.
/// let client = BrokerClient::new(&Credentials::from_env()?, true)?;
///
/// let accounts = client.list_accounts(None).await?;
/// println!("{} accounts", accounts.len());
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct BrokerClient {
    rest: RestClient,
    /// The client the event streams are read through.
    ///
    /// They need a response body read incrementally rather than decoded whole,
    /// and they carry **no timeout**: a total deadline on a `text/event-stream`
    /// caps the life of the subscription rather than bounding a slow call.
    events: reqwest::Client,
    /// The client the two document downloads use.
    ///
    /// Separate from `events` because a download is an ordinary call with a body
    /// that ends, so a total deadline is the right shape for it — where an event
    /// stream's body never finishes and a deadline would cap the life of the
    /// subscription. One client cannot be both, which is why there are two.
    ///
    /// It follows redirects — the download answers `301` to a presigned storage
    /// URL — and what keeps broker credentials off that storage provider is not
    /// this client but the credential *form*: `with_config` converts to basic
    /// auth, and reqwest strips `Authorization` when a redirect crosses hosts.
    plain: reqwest::Client,
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
        // The broker API authenticates with HTTP basic auth over the key pair,
        // where the trading and data APIs take the pair as two headers.
        let credentials = credentials.clone().into_basic();
        Ok(Self {
            events: crate::sse::streaming_client(&credentials)?,
            plain: crate::sse::download_client(&credentials, config.timeout)?,
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

    /// Lists accounts, optionally filtered.
    ///
    /// This route returns a reduced view of each account by default; name
    /// [`entities`](ListAccountsRequest::entities) to fill the rest back in.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn list_accounts(
        &self,
        filter: Option<&ListAccountsRequest>,
    ) -> Result<Vec<Account>> {
        match filter {
            Some(filter) => self.rest.get("/accounts", filter).await,
            None => self.rest.get("/accounts", &Empty).await,
        }
    }

    /// Opens a new account.
    ///
    /// # Errors
    /// Returns [`crate::Error::InvalidRequest`] if a field Alpaca requires on a
    /// new account is missing; see [`CreateAccountRequest::validate`].
    pub async fn create_account(&self, account: &CreateAccountRequest) -> Result<Account> {
        account.validate()?;
        self.rest.post("/accounts", account).await
    }

    /// Updates an account.
    ///
    /// Unset fields are not sent, so an update touches only what it names.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn update_account(
        &self,
        account_id: Uuid,
        update: &UpdateAccountRequest,
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
    /// **Deprecated by Alpaca.** Use
    /// [`get_eod_positions`](Self::get_eod_positions), which calls the
    /// `/v1/reporting/eod/positions` route Alpaca names as the replacement.
    /// No sunset date is published, so this still answers.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    #[deprecated(
        since = "0.1.0",
        note = "Alpaca deprecated this route; use `BrokerClient::get_eod_positions`, which calls the \
                /v1/reporting/eod/positions route Alpaca names as its replacement"
    )]
    pub async fn get_all_accounts_positions(
        &self,
        page: Option<u32>,
    ) -> Result<AllAccountsPositions> {
        // The only parameter the route takes, and it is not cosmetic: without
        // it a caller with more accounts than fit on one page cannot reach the
        // rest of them.
        match page {
            Some(page) => {
                self.rest
                    .get("/accounts/positions", &[("page", page.to_string())])
                    .await
            }
            None => self.rest.get("/accounts/positions", &Empty).await,
        }
    }

    // ----------------------------------------------------------- funding

    /// Opens an ACH relationship between an account and a bank account.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn create_ach_relationship_for_account(
        &self,
        account_id: Uuid,
        relationship: &CreateACHRelationshipRequest,
    ) -> Result<ACHRelationship> {
        self.rest
            .post(
                &format!("/accounts/{account_id}/ach_relationships"),
                relationship,
            )
            .await
    }

    /// Lists an account's ACH relationships, optionally filtered by status.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_ach_relationships_for_account(
        &self,
        account_id: Uuid,
        statuses: &[ACHRelationshipStatus],
    ) -> Result<Vec<ACHRelationship>> {
        let path = format!("/accounts/{account_id}/ach_relationships");
        if statuses.is_empty() {
            return self.rest.get(&path, &Empty).await;
        }
        // One comma-separated parameter, not a repeated one.
        let statuses = statuses
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(",");
        self.rest.get(&path, &[("statuses", statuses)]).await
    }

    /// Deletes an ACH relationship.
    ///
    /// # Errors
    /// Propagates transport and API failures.
    pub async fn delete_ach_relationship_for_account(
        &self,
        account_id: Uuid,
        ach_relationship_id: Uuid,
    ) -> Result<()> {
        self.send_void(
            Method::DELETE,
            &format!("/accounts/{account_id}/ach_relationships/{ach_relationship_id}"),
            None::<&Empty>,
        )
        .await
    }

    /// Connects a recipient bank to an account, for wires.
    ///
    /// # Errors
    /// Returns [`crate::Error::InvalidRequest`] if the address fields do not
    /// match the bank code type; see [`CreateBankRequest::validate`].
    pub async fn create_bank_for_account(
        &self,
        account_id: Uuid,
        bank: &CreateBankRequest,
    ) -> Result<Bank> {
        bank.validate()?;
        self.rest
            .post(&format!("/accounts/{account_id}/recipient_banks"), bank)
            .await
    }

    /// Lists an account's connected banks.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_banks_for_account(&self, account_id: Uuid) -> Result<Vec<Bank>> {
        self.rest
            .get(&format!("/accounts/{account_id}/recipient_banks"), &Empty)
            .await
    }

    /// Deletes a bank connection.
    ///
    /// # Errors
    /// Propagates transport and API failures.
    pub async fn delete_bank_for_account(&self, account_id: Uuid, bank_id: Uuid) -> Result<()> {
        self.send_void(
            Method::DELETE,
            &format!("/accounts/{account_id}/recipient_banks/{bank_id}"),
            None::<&Empty>,
        )
        .await
    }

    /// Moves money into or out of an account.
    ///
    /// # Errors
    /// Returns [`crate::Error::InvalidRequest`] if the amount is not positive.
    pub async fn create_transfer_for_account(
        &self,
        account_id: Uuid,
        transfer: &CreateTransferRequest,
    ) -> Result<Transfer> {
        transfer.validate()?;
        self.rest
            .post(&format!("/accounts/{account_id}/transfers"), transfer)
            .await
    }

    /// Fetches one page of an account's transfers.
    ///
    /// Exactly one request, honouring whatever `limit` and `offset` the filter
    /// carries. Use
    /// [`get_all_transfers_for_account`](Self::get_all_transfers_for_account) to
    /// walk every page.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_transfers_for_account(
        &self,
        account_id: Uuid,
        filter: Option<&GetTransfersRequest>,
    ) -> Result<Vec<Transfer>> {
        let path = format!("/accounts/{account_id}/transfers");
        match filter {
            Some(filter) => self.rest.get(&path, filter).await,
            None => self.rest.get(&path, &Empty).await,
        }
    }

    /// Walks every page of an account's transfers.
    ///
    /// Requests pages until one comes back empty, then returns the lot.
    /// `max_items` caps the total across every page, not per page.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_all_transfers_for_account(
        &self,
        account_id: Uuid,
        filter: Option<&GetTransfersRequest>,
        max_items: Option<usize>,
    ) -> Result<Vec<Transfer>> {
        let mut filter = filter.cloned().unwrap_or_default();
        let mut collected: Vec<Transfer> = Vec::new();

        loop {
            // The offset is the running count on every pass, including the
            // first, so a caller-supplied offset is overwritten once the walk
            // starts. Honouring it would make the cap mean something different
            // on the first page than on the rest.
            filter.offset = Some(u32::try_from(collected.len()).map_err(|_| {
                crate::Error::InvalidRequest("too many transfers to page through".to_owned())
            })?);

            let page: Vec<Transfer> = self
                .rest
                .get(&format!("/accounts/{account_id}/transfers"), &filter)
                .await?;

            // An empty page is how this endpoint says it is done; it carries no
            // token or total to check instead.
            if page.is_empty() {
                break;
            }

            collected.extend(page);

            if let Some(max_items) = max_items
                && collected.len() >= max_items
            {
                collected.truncate(max_items);
                break;
            }
        }

        Ok(collected)
    }

    /// Cancels a transfer that has not yet settled.
    ///
    /// # Errors
    /// Propagates transport and API failures.
    pub async fn cancel_transfer_for_account(
        &self,
        account_id: Uuid,
        transfer_id: Uuid,
    ) -> Result<()> {
        self.send_void(
            Method::DELETE,
            &format!("/accounts/{account_id}/transfers/{transfer_id}"),
            None::<&Empty>,
        )
        .await
    }

    // ------------------------------------------------------------ events

    /// Streams account status changes as they happen.
    ///
    /// A v1 stream, and current: Alpaca publishes no v2 successor.
    ///
    /// # Errors
    /// Propagates transport failures and any non-success status the server
    /// answers the subscription with.
    pub async fn get_account_status_events(
        &self,
        filter: Option<&GetEventsRequest>,
    ) -> Result<impl Stream<Item = Result<BrokerEvent>> + use<>> {
        self.events(EventVersion::V1, "/events/accounts/status", filter)
            .await
    }

    /// Streams trade events as they happen.
    ///
    /// Subscribes to `/v2/events/trades`. The v1 route is documented as "fully
    /// deprecated and no longer available", and calling it ships a stream that
    /// does not answer.
    ///
    /// # Errors
    /// Propagates transport failures and any non-success status the server
    /// answers the subscription with.
    pub async fn get_trade_events(
        &self,
        filter: Option<&GetEventsRequest>,
    ) -> Result<impl Stream<Item = Result<BrokerEvent>> + use<>> {
        self.events(EventVersion::V2, "/events/trades", filter)
            .await
    }

    /// Streams journal status changes as they happen.
    ///
    /// Subscribes to `/v2/events/journals/status`. The v1 stream still exists
    /// but is legacy, and Alpaca warns the two are not interchangeable: "there
    /// is no compatibility between /v1/events/journals/status and
    /// /v2/events/journals/status, the ids (ulid) are always different". A
    /// cursor saved from the v1 stream is meaningless here.
    ///
    /// # Errors
    /// Propagates transport failures and any non-success status the server
    /// answers the subscription with.
    pub async fn get_journal_events(
        &self,
        filter: Option<&GetEventsRequest>,
    ) -> Result<impl Stream<Item = Result<BrokerEvent>> + use<>> {
        self.events(EventVersion::V2, "/events/journals/status", filter)
            .await
    }

    /// Streams funding status changes as they happen.
    ///
    /// Subscribes to `/v2/events/funding/status`, which supersedes the v1
    /// transfer stream. That route is deprecated and open only to broker
    /// partners who already had it; new partners cannot use it at all.
    ///
    /// The v2 stream is broader than this method's name suggests: it covers bank
    /// relationships, wire banks and funding wallets as well as transfers.
    ///
    /// # Errors
    /// Propagates transport failures and any non-success status the server
    /// answers the subscription with.
    pub async fn get_transfer_events(
        &self,
        filter: Option<&GetEventsRequest>,
    ) -> Result<impl Stream<Item = Result<BrokerEvent>> + use<>> {
        self.events(EventVersion::V2, "/events/funding/status", filter)
            .await
    }

    /// Streams non-trading activity events as they happen.
    ///
    /// A v1 stream, and current. Alpaca documents two filters this crate does
    /// not expose — `include_preprocessing` and `group_id` — which are unique to
    /// this stream.
    ///
    /// # Errors
    /// Propagates transport failures and any non-success status the server
    /// answers the subscription with.
    pub async fn get_non_trading_activity_events(
        &self,
        filter: Option<&GetEventsRequest>,
    ) -> Result<impl Stream<Item = Result<BrokerEvent>> + use<>> {
        self.events(EventVersion::V1, "/events/nta", filter).await
    }

    /// Streams account activity events as they happen.
    ///
    /// A `v2beta1` stream, and the account-activity counterpart to the polled
    /// [`get_account_activities`](Self::get_account_activities).
    ///
    /// Takes [`EventStreamRequest`] rather than [`GetEventsRequest`]: this
    /// stream bounds its window by timestamp where the older ones use a date.
    ///
    /// # Errors
    /// Propagates transport failures and any non-success status the server
    /// answers the subscription with.
    pub async fn get_activity_events(
        &self,
        filter: Option<&EventStreamRequest>,
    ) -> Result<impl Stream<Item = Result<BrokerEvent>> + use<>> {
        self.event_stream(EventVersion::V2Beta1, "/events/activities", filter)
            .await
    }

    /// Streams admin actions as they happen.
    ///
    /// # Errors
    /// Propagates transport failures and any non-success status the server
    /// answers the subscription with.
    pub async fn get_admin_action_events(
        &self,
        filter: Option<&EventStreamRequest>,
    ) -> Result<impl Stream<Item = Result<BrokerEvent>> + use<>> {
        self.event_stream(EventVersion::V2, "/events/admin-actions", filter)
            .await
    }

    /// Streams IPO events as they happen.
    ///
    /// # Errors
    /// Propagates transport failures and any non-success status the server
    /// answers the subscription with.
    pub async fn get_ipo_events(
        &self,
        filter: Option<&EventStreamRequest>,
    ) -> Result<impl Stream<Item = Result<BrokerEvent>> + use<>> {
        self.event_stream(EventVersion::V2, "/events/ipos", filter)
            .await
    }

    /// Streams system events as they happen.
    ///
    /// # Errors
    /// Propagates transport failures and any non-success status the server
    /// answers the subscription with.
    pub async fn get_system_events(
        &self,
        filter: Option<&EventStreamRequest>,
    ) -> Result<impl Stream<Item = Result<BrokerEvent>> + use<>> {
        self.event_stream(EventVersion::V2, "/events/system", filter)
            .await
    }

    /// Fetches one account activity event by its ULID.
    ///
    /// The way to re-read an event whose id came off
    /// [`get_activity_events`](Self::get_activity_events), rather than
    /// replaying the stream from a cursor.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_account_activity_event(
        &self,
        account_id: Uuid,
        event_id: &str,
    ) -> Result<serde_json::Value> {
        // Untyped for the same reason the streams are: no captured payload
        // exists for this route in any SDK, and inventing a struct from the
        // spec alone would claim more than is known.
        let path = format!(
            "/accounts/{account_id}/events/activities/{}",
            segment(event_id)?
        );
        // Its own version segment, like the stream it pairs with — which is what
        // `at_version` is for. It went through the raw client once, and that cost
        // it the retry loop, the caller's timeout and the redirect refusal that
        // every other route here gets.
        self.rest.at_version("v2beta1").get(&path, &Empty).await
    }

    /// Opens one of the broker's event streams.
    ///
    /// The version comes from the stream rather than from the client's
    /// `api_version`: Alpaca versions these endpoints individually, so of the
    /// nine, two are v1, six are v2 and one is `v2beta1`.
    async fn events(
        &self,
        version: EventVersion,
        path: &str,
        filter: Option<&GetEventsRequest>,
    ) -> Result<impl Stream<Item = Result<BrokerEvent>> + use<>> {
        let url = format!(
            "{}/{}{path}",
            self.rest.config().base_url.trim_end_matches('/'),
            version.segment()
        );
        // Rendered per version: the cursor parameter is named differently on
        // each, and means something different under the v1 name.
        let query = filter
            .map(|filter| version.query(filter))
            .unwrap_or_default();
        crate::sse::subscribe(&self.events, &url, path, &query).await
    }

    /// Opens one of the timestamp-bounded event streams.
    ///
    /// The four streams the reference sweep found take an RFC-3339 window where
    /// the five older ones take a date, so they take a different filter type
    /// rather than sharing one that would be wrong half the time.
    async fn event_stream(
        &self,
        version: EventVersion,
        path: &str,
        filter: Option<&EventStreamRequest>,
    ) -> Result<impl Stream<Item = Result<BrokerEvent>> + use<>> {
        let url = format!(
            "{}/{}{path}",
            self.rest.config().base_url.trim_end_matches('/'),
            version.segment()
        );
        let query = filter.map(EventStreamRequest::query).unwrap_or_default();
        crate::sse::subscribe(&self.events, &url, path, &query).await
    }

    // ------------------------------------------------- account activities

    /// Fetches one page of account activities.
    ///
    /// Unlike the trading API's equivalent, this route spans every account the
    /// correspondent serves; filter by `account_id` for one of them. The
    /// response mixes trade and non-trade activities, which is why it decodes to
    /// [`crate::trading::Activity`].
    ///
    /// # Errors
    /// Returns [`crate::Error::InvalidRequest`] if the filter combines `category`
    /// with `activity_types`.
    pub async fn get_account_activities(
        &self,
        filter: Option<&GetAccountActivitiesRequest>,
    ) -> Result<Vec<Activity>> {
        match filter {
            Some(filter) => {
                filter.validate()?;
                self.rest.get("/accounts/activities", filter).await
            }
            None => self.rest.get("/accounts/activities", &Empty).await,
        }
    }

    /// Walks every page of account activities.
    ///
    /// This route pages by *cursor*, not by offset or by a server-supplied
    /// token: the next page starts after the `id` of the last activity already
    /// seen. An empty array ends the walk.
    ///
    /// Setting `date` on the filter changes the endpoint's behaviour — Alpaca
    /// may then return everything in one response and ignore paging — so this
    /// walk can finish in a single request. `max_items` still holds.
    ///
    /// # Errors
    /// Returns [`crate::Error::InvalidRequest`] if the filter combines `category`
    /// with `activity_types`.
    pub async fn get_all_account_activities(
        &self,
        filter: Option<&GetAccountActivitiesRequest>,
        max_items: Option<usize>,
    ) -> Result<Vec<Activity>> {
        let mut filter = filter.cloned().unwrap_or_default();
        filter.validate()?;

        let mut collected: Vec<Activity> = Vec::new();

        loop {
            if let Some(page_size) = page_limit(&filter.page_size, max_items, collected.len()) {
                filter.page_size = Some(page_size);
            }

            let page: Vec<Activity> = self.rest.get("/accounts/activities", &filter).await?;
            let Some(last) = page.last() else {
                break;
            };

            // The cursor is the last activity's id, so it is taken before the
            // page is moved into the accumulator.
            let cursor = activity_id(last).to_owned();
            collected.extend(page);

            if let Some(max_items) = max_items
                && collected.len() >= max_items
            {
                collected.truncate(max_items);
                break;
            }

            filter.page_token = Some(cursor);
        }

        Ok(collected)
    }

    // ---------------------------------------------------------------- CIP

    /// Fetches an account's Customer Identification Program record.
    ///
    /// **Unverified.** No captured response exists for this route — the sandbox
    /// is reported to answer 404 for it — so [`CIPInfo`] is derived from the
    /// broker spec rather than from a payload. Treat a decode failure on the
    /// first real response as expected work, not a regression.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_cip_data_for_account_by_id(&self, account_id: Uuid) -> Result<CIPInfo> {
        self.rest
            .get(&format!("/accounts/{account_id}/cip"), &Empty)
            .await
    }

    /// Submits an account's Customer Identification Program record.
    ///
    /// **Unverified**, for the same reason as
    /// [`get_cip_data_for_account_by_id`](Self::get_cip_data_for_account_by_id).
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn upload_cip_data_for_account_by_id(
        &self,
        account_id: Uuid,
        cip: &CIPInfo,
    ) -> Result<CIPInfo> {
        self.rest
            .post(&format!("/accounts/{account_id}/cip"), cip)
            .await
    }

    // --------------------------------------------------------- documents

    /// Lists an account's trade documents: statements, confirmations, tax forms.
    ///
    /// # Errors
    /// Returns [`crate::Error::InvalidRequest`] if the filter's date window is
    /// the wrong way round.
    pub async fn get_trade_documents_for_account(
        &self,
        account_id: Uuid,
        filter: Option<&GetTradeDocumentsRequest>,
    ) -> Result<Vec<TradeDocument>> {
        let path = format!("/accounts/{account_id}/documents");
        match filter {
            Some(filter) => {
                filter.validate()?;
                self.rest.get(&path, filter).await
            }
            None => self.rest.get(&path, &Empty).await,
        }
    }

    /// Fetches one trade document's metadata.
    ///
    /// This is the record, not the file; see
    /// [`download_trade_document_for_account_by_id`](Self::download_trade_document_for_account_by_id)
    /// for the contents.
    ///
    /// **Undocumented, and possibly not real.** This route appears in neither
    /// the `OpenAPI` spec nor the published reference, which list only the
    /// collection, the upload and the download; no captured payload exists for
    /// it either. It is implemented because other clients call it, and that is
    /// the whole of the evidence. Prefer
    /// [`get_trade_documents_for_account`](Self::get_trade_documents_for_account)
    /// and filter, and treat a 404 here as the route not existing rather than
    /// the document not existing. See `COVERAGE.md`.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_trade_document_for_account_by_id(
        &self,
        account_id: Uuid,
        document_id: Uuid,
    ) -> Result<TradeDocument> {
        self.rest
            .get(
                &format!("/accounts/{account_id}/documents/{document_id}"),
                &Empty,
            )
            .await
    }

    /// Downloads a trade document's contents.
    ///
    /// This route answers `301` with a presigned storage URL rather than the
    /// file, so it cannot go through [`RestClient`], which refuses redirects on
    /// purpose. It uses a second client that follows them and sheds the broker
    /// credentials on the way — a presigned URL carries its own authorisation
    /// and has no business seeing an API key.
    ///
    /// The bytes are returned rather than written, so the caller decides where
    /// they go — a document small enough to hold in memory is the common case,
    /// and streaming to a path is a policy this crate has no business setting.
    ///
    /// # Errors
    /// Propagates transport and API failures. Retries follow the client's retry
    /// configuration, as every other route does.
    pub async fn download_trade_document_for_account_by_id(
        &self,
        account_id: Uuid,
        document_id: Uuid,
    ) -> Result<Vec<u8>> {
        let path = format!("/accounts/{account_id}/documents/{document_id}/download");
        self.download(&path).await
    }

    /// Fetches a route that answers `301` to a presigned storage URL.
    ///
    /// Two routes do: the trade document download and the W-8BEN download.
    /// Neither can go through [`RestClient`], which refuses redirects on
    /// purpose, so both use the `plain` client, which follows them. What keeps
    /// the credentials off the storage provider is not that client but their
    /// *form*: `with_config` converts them to basic auth, and reqwest strips
    /// `Authorization` on a cross-host hop.
    ///
    /// The retry policy is the client's own — same status set, same backoff
    /// curve, same `Retry-After` handling — so these behave like every other
    /// route under a 429 or a 5xx.
    async fn download(&self, path: &str) -> Result<Vec<u8>> {
        let config = self.rest.config();
        let url = format!(
            "{}/{}{path}",
            config.base_url.trim_end_matches('/'),
            config.api_version
        );

        let retry = &config.retry;
        let total_attempts = retry.attempts + 1;

        for attempt in 1..=total_attempts {
            let response = self
                .plain
                .get(&url)
                .send()
                .await
                .map_err(crate::Error::transport)?;
            let status = response.status().as_u16();

            if response.status().is_success() {
                return Ok(response
                    .bytes()
                    .await
                    .map_err(crate::Error::transport)?
                    .to_vec());
            }

            // Read before the body: `text()` consumes the response, headers
            // and all.
            let retry_after = crate::rest::retry_after(response.headers());

            let body = response.text().await.unwrap_or_default();
            let api_error = crate::error::ApiError::from_body(status, path, body);

            if !retry.should_retry(status) {
                return Err(crate::Error::Api(api_error));
            }
            if attempt == total_attempts {
                return Err(crate::Error::RetriesExhausted {
                    attempts: total_attempts,
                    last: api_error,
                });
            }

            // The same delay policy `RestClient` uses, rather than a flat wait
            // that ignores both the backoff curve and the server's own answer.
            // This loop is hand-rolled only because the response body is bytes;
            // there is no reason for it to retry differently.
            let delay = retry_after.map_or_else(
                || retry.delay(attempt),
                |after| after.min(retry.retry_after_cap()),
            );
            tokio::time::sleep(delay).await;
        }

        unreachable!("retry loop exited without returning")
    }

    /// Uploads documents to an account.
    ///
    /// Contents are base64-encoded, and capped at 10MB each when Alpaca does
    /// the KYC. The route answers `204`, so a success returns nothing.
    ///
    /// [`DOCUMENT_UPLOAD_LIMIT`] is a conventional ceiling on how many documents
    /// go in one call. It is not in the reference or the spec, and it is not
    /// enforced here: it may well be real, but rejecting a request Alpaca would
    /// have accepted is the worse of the two failures, and the server's answer
    /// says more than a guess of ours would.
    ///
    /// # Errors
    /// Returns [`crate::Error::InvalidRequest`] if any document fails
    /// [`UploadDocument::validate`].
    pub async fn upload_documents_to_account(
        &self,
        account_id: Uuid,
        documents: &[UploadDocument],
    ) -> Result<()> {
        for document in documents {
            document.validate()?;
        }

        self.send_void(
            Method::POST,
            &format!("/accounts/{account_id}/documents/upload"),
            Some(documents),
        )
        .await
    }

    // ------------------------------------------------------- rebalancing

    /// Creates a portfolio accounts can be rebalanced towards.
    ///
    /// # Errors
    /// Returns [`crate::Error::InvalidRequest`] if any weight is not positive
    /// or an asset weight names no symbol.
    pub async fn create_portfolio(&self, portfolio: &CreatePortfolioRequest) -> Result<Portfolio> {
        portfolio.validate()?;
        self.rest.post("/rebalancing/portfolios", portfolio).await
    }

    /// Lists portfolios, optionally filtered.
    ///
    /// This route answers with a bare array — unlike subscriptions and runs, it
    /// does not paginate.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_all_portfolios(
        &self,
        filter: Option<&GetPortfoliosRequest>,
    ) -> Result<Vec<Portfolio>> {
        match filter {
            Some(filter) => self.rest.get("/rebalancing/portfolios", filter).await,
            None => self.rest.get("/rebalancing/portfolios", &Empty).await,
        }
    }

    /// Fetches one portfolio.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_portfolio_by_id(&self, portfolio_id: Uuid) -> Result<Portfolio> {
        self.rest
            .get(&format!("/rebalancing/portfolios/{portfolio_id}"), &Empty)
            .await
    }

    /// Changes a portfolio.
    ///
    /// Changing the weights or the conditions re-evaluates every subscribed
    /// account at the next opportunity, subject to the cooldown.
    ///
    /// # Errors
    /// Returns [`crate::Error::InvalidRequest`] if any weight the update sets is
    /// not positive or an asset weight names no symbol.
    pub async fn update_portfolio_by_id(
        &self,
        portfolio_id: Uuid,
        update: &UpdatePortfolioRequest,
    ) -> Result<Portfolio> {
        update.validate()?;
        self.rest
            .patch(&format!("/rebalancing/portfolios/{portfolio_id}"), update)
            .await
    }

    /// Retires a portfolio.
    ///
    /// Permitted only when nothing subscribes to it and no active portfolio
    /// lists it as a weight. The record survives; it just stops being usable.
    ///
    /// # Errors
    /// Propagates transport and API failures.
    pub async fn inactivate_portfolio_by_id(&self, portfolio_id: Uuid) -> Result<()> {
        self.send_void(
            Method::DELETE,
            &format!("/rebalancing/portfolios/{portfolio_id}"),
            None::<&Empty>,
        )
        .await
    }

    /// Subscribes an account to a portfolio.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn create_subscription(
        &self,
        subscription: &CreateSubscriptionRequest,
    ) -> Result<Subscription> {
        self.rest
            .post("/rebalancing/subscriptions", subscription)
            .await
    }

    /// Fetches one page of subscriptions.
    ///
    /// The page carries the token for the next one; see
    /// [`get_all_subscriptions`](Self::get_all_subscriptions) to walk them all.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_subscriptions(
        &self,
        filter: Option<&GetSubscriptionsRequest>,
    ) -> Result<SubscriptionsPage> {
        match filter {
            Some(filter) => self.rest.get("/rebalancing/subscriptions", filter).await,
            None => self.rest.get("/rebalancing/subscriptions", &Empty).await,
        }
    }

    /// Walks every page of subscriptions.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_all_subscriptions(
        &self,
        filter: Option<&GetSubscriptionsRequest>,
        max_items: Option<usize>,
    ) -> Result<Vec<Subscription>> {
        let mut filter = filter.cloned().unwrap_or_default();
        let mut collected: Vec<Subscription> = Vec::new();

        loop {
            if let Some(limit) = page_limit(&filter.limit, max_items, collected.len()) {
                filter.limit = Some(limit);
            }

            let page: SubscriptionsPage =
                self.rest.get("/rebalancing/subscriptions", &filter).await?;

            if page.subscriptions.is_empty() {
                break;
            }
            collected.extend(page.subscriptions);

            if let Some(max_items) = max_items
                && collected.len() >= max_items
            {
                collected.truncate(max_items);
                break;
            }

            // No token means this was the last page.
            match page.next_page_token {
                Some(token) => filter.page_token = Some(token),
                None => break,
            }
        }

        Ok(collected)
    }

    /// Fetches one subscription.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_subscription_by_id(&self, subscription_id: Uuid) -> Result<Subscription> {
        self.rest
            .get(
                &format!("/rebalancing/subscriptions/{subscription_id}"),
                &Empty,
            )
            .await
    }

    /// Ends a subscription, stopping the account's rebalancing.
    ///
    /// # Errors
    /// Propagates transport and API failures.
    pub async fn unsubscribe_account(&self, subscription_id: Uuid) -> Result<()> {
        self.send_void(
            Method::DELETE,
            &format!("/rebalancing/subscriptions/{subscription_id}"),
            None::<&Empty>,
        )
        .await
    }

    /// Starts a rebalancing run by hand.
    ///
    /// # Errors
    /// Returns [`crate::Error::InvalidRequest`] if any weight is not positive
    /// or an asset weight names no symbol.
    pub async fn create_manual_run(&self, run: &CreateRunRequest) -> Result<RebalancingRun> {
        run.validate()?;
        self.rest.post("/rebalancing/runs", run).await
    }

    /// Fetches one page of rebalancing runs.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_runs(&self, filter: Option<&GetRunsRequest>) -> Result<RunsPage> {
        match filter {
            Some(filter) => self.rest.get("/rebalancing/runs", filter).await,
            None => self.rest.get("/rebalancing/runs", &Empty).await,
        }
    }

    /// Walks every page of rebalancing runs.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_all_runs(
        &self,
        filter: Option<&GetRunsRequest>,
        max_items: Option<usize>,
    ) -> Result<Vec<RebalancingRun>> {
        let mut filter = filter.cloned().unwrap_or_default();
        let mut collected: Vec<RebalancingRun> = Vec::new();

        loop {
            if let Some(limit) = page_limit(&filter.limit, max_items, collected.len()) {
                filter.limit = Some(limit);
            }

            let page: RunsPage = self.rest.get("/rebalancing/runs", &filter).await?;

            if page.runs.is_empty() {
                break;
            }
            collected.extend(page.runs);

            if let Some(max_items) = max_items
                && collected.len() >= max_items
            {
                collected.truncate(max_items);
                break;
            }

            match page.next_page_token {
                Some(token) => filter.page_token = Some(token),
                None => break,
            }
        }

        Ok(collected)
    }

    /// Fetches one rebalancing run.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_run_by_id(&self, run_id: Uuid) -> Result<RebalancingRun> {
        self.rest
            .get(&format!("/rebalancing/runs/{run_id}"), &Empty)
            .await
    }

    /// Cancels a rebalancing run.
    ///
    /// Only queued and in-progress runs can be cancelled, and any orders already
    /// submitted are cancelled on a best-effort basis.
    ///
    /// # Errors
    /// Propagates transport and API failures.
    pub async fn cancel_run_by_id(&self, run_id: Uuid) -> Result<()> {
        self.send_void(
            Method::DELETE,
            &format!("/rebalancing/runs/{run_id}"),
            None::<&Empty>,
        )
        .await
    }

    // ---------------------------------------------------------- journals

    /// Opens a journal between two accounts.
    ///
    /// Journals are not account-scoped: both accounts are named in the body, so
    /// the route sits at the top level.
    ///
    /// # Errors
    /// Returns [`crate::Error::InvalidRequest`] if the fields set do not match
    /// the entry type; see [`CreateJournalRequest::validate`].
    pub async fn create_journal(&self, journal: &CreateJournalRequest) -> Result<Journal> {
        journal.validate()?;
        self.rest.post("/journals", journal).await
    }

    /// Moves cash out of one account into many.
    ///
    /// Each entry succeeds or fails on its own: the response carries one record
    /// per entry, and a failed one explains itself in
    /// [`error_message`](BatchJournalResponse::error_message) rather than
    /// failing the request.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn create_batch_journal(
        &self,
        batch: &CreateBatchJournalRequest,
    ) -> Result<Vec<BatchJournalResponse>> {
        self.rest.post("/journals/batch", batch).await
    }

    /// Moves cash into one account out of many.
    ///
    /// Per-entry outcomes work as they do for
    /// [`create_batch_journal`](Self::create_batch_journal).
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn create_reverse_batch_journal(
        &self,
        batch: &CreateReverseBatchJournalRequest,
    ) -> Result<Vec<BatchJournalResponse>> {
        self.rest.post("/journals/reverse_batch", batch).await
    }

    /// Lists journals, optionally filtered.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_journals(&self, filter: Option<&GetJournalsRequest>) -> Result<Vec<Journal>> {
        match filter {
            Some(filter) => self.rest.get("/journals", filter).await,
            None => self.rest.get("/journals", &Empty).await,
        }
    }

    /// Fetches one journal.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_journal_by_id(&self, journal_id: Uuid) -> Result<Journal> {
        self.rest
            .get(&format!("/journals/{journal_id}"), &Empty)
            .await
    }

    /// Cancels a journal that has not yet executed.
    ///
    /// # Errors
    /// Propagates transport and API failures.
    pub async fn cancel_journal_by_id(&self, journal_id: Uuid) -> Result<()> {
        self.send_void(
            Method::DELETE,
            &format!("/journals/{journal_id}"),
            None::<&Empty>,
        )
        .await
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
    /// **Undocumented.** Alpaca documents the `PATCH` on this path but no `GET`,
    /// in neither the spec nor the reference. It is implemented anyway because a
    /// captured payload proves it answers — which is stronger evidence than the
    /// documentation's silence is against it. See `COVERAGE.md`.
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
                &format!(
                    "/trading/accounts/{account_id}/positions/{}",
                    asset.as_path_segment()?
                ),
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
            Some(cancel) => {
                self.rest
                    .delete_effectful(&path, &[("cancel_orders", cancel)])
                    .await
            }
            None => self.rest.delete_effectful(&path, &Empty).await,
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
        let path = format!(
            "/trading/accounts/{account_id}/positions/{}",
            asset.as_path_segment()?
        );
        match close {
            Some(close) => self.rest.delete_effectful(&path, &close.to_query()).await,
            None => self.rest.delete_effectful(&path, &Empty).await,
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
            &format!(
                "/trading/accounts/{account_id}/positions/{}/exercise",
                contract.as_path_segment()?
            ),
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
    /// **One page.** Alpaca returns 50 orders by default and at most 500, and
    /// this sends exactly what it is given — so an account with more history
    /// than that gets a silently truncated list rather than an error. Use
    /// [`get_all_orders_for_account`](Self::get_all_orders_for_account) to walk
    /// the whole history.
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

    /// Every order on an account matching `filter`, following the cursor across
    /// pages.
    ///
    /// The broker route takes the same request type and the same 500-order page
    /// cap as the trading one, so it needs the same walk — a correspondent
    /// reconciling an account's history hits the identical silent truncation
    /// otherwise. See
    /// [`TradingClient::get_all_orders`](crate::trading::TradingClient::get_all_orders)
    /// for how the cursor works and which two filter fields it overrides.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_all_orders_for_account(
        &self,
        account_id: Uuid,
        filter: Option<&crate::trading::GetOrdersRequest>,
        max_items: Option<usize>,
    ) -> Result<Vec<Order>> {
        let path = format!("/trading/accounts/{account_id}/orders");
        let mut request = filter.cloned().unwrap_or_default();
        request.limit = Some(crate::config::ORDERS_MAX_LIMIT);
        request.direction = Some(crate::types::Sort::Desc);

        crate::trading::client::walk_orders(
            &self.rest,
            &path,
            request,
            max_items,
            |order: &Order| order.order.id,
        )
        .await
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
                &format!(
                    "/trading/accounts/{account_id}/watchlists/{watchlist_id}/{}",
                    segment(symbol)?
                ),
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
        self.rest
            .get(&format!("/assets/{}", asset.as_path_segment()?), &Empty)
            .await
    }

    // ---------------------------------------------- corporate announcements

    /// Searches corporate action announcements.
    ///
    /// # Errors
    /// Returns [`crate::Error::InvalidRequest`] if the date window exceeds 90
    /// days, or an API error if Alpaca rejects the request.
    #[deprecated(
        since = "0.1.0",
        note = "Alpaca deprecated this route; use `CorporateActionsClient::get_corporate_actions` \
                (the /v1/corporate-actions market data route) instead. No sunset date is published"
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
        note = "Alpaca deprecated this route; use `CorporateActionsClient::get_corporate_actions` \
                (the /v1/corporate-actions market data route) instead. No sunset date is published"
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

    /// A named market's calendar.
    ///
    /// **A `v2` route**, where the trading API's equivalent is `v3`. The models
    /// are shared with [`crate::trading::markets`]; the version is not.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_market_calendar(
        &self,
        market: &crate::trading::Market,
        filter: Option<&crate::trading::GetMarketCalendarRequest>,
    ) -> Result<crate::trading::MarketCalendar> {
        let path = format!("/calendar/{}", segment(market)?);
        match filter {
            Some(filter) => self.rest.at_version("v2").get(&path, filter).await,
            None => self.rest.at_version("v2").get(&path, &Empty).await,
        }
    }

    /// A company logo, as PNG bytes.
    ///
    /// **A `v1beta1` route.** Unverified: a data plan that reaches SIP still
    /// answers `403 Subscription does not permit querying logos`.
    ///
    /// # Errors
    /// Propagates transport and API failures.
    pub async fn get_logo(
        &self,
        symbol: &str,
        request: &crate::types::LogoRequest,
    ) -> Result<Vec<u8>> {
        let path = format!("/logos/{}", segment(symbol)?);
        self.rest
            .at_version("v1beta1")
            .get_bytes(&path, request)
            .await
    }

    // ---------------------------------------------- activities by type

    /// Account activities of one type, across every account.
    ///
    /// The narrowed counterpart to
    /// [`get_account_activities`](Self::get_account_activities): the type moves
    /// from the query string into the path, so `activity_types` has nothing
    /// left to say here.
    ///
    /// # Errors
    /// Returns [`crate::Error::InvalidRequest`] if the filter combines
    /// `activity_types` with `category`. Otherwise propagates transport, API,
    /// and decoding failures.
    pub async fn get_account_activities_by_type(
        &self,
        activity_type: &crate::trading::ActivityType,
        filter: Option<&GetAccountActivitiesRequest>,
    ) -> Result<Vec<Activity>> {
        let path = format!("/accounts/activities/{}", segment(activity_type)?);
        match filter {
            Some(filter) => {
                filter.validate()?;
                self.rest.get(&path, filter).await
            }
            None => self.rest.get(&path, &Empty).await,
        }
    }

    // --------------------------------------------------- fixed income

    /// The US corporate bond master list.
    ///
    /// One of the few spec-derived broker routes with a real captured payload
    /// behind it, harvested from the Go SDK's tests: see `fixtures/go/`.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_us_corporates(
        &self,
        filter: Option<&GetUsCorporatesRequest>,
    ) -> Result<UsCorporates> {
        match filter {
            Some(filter) => {
                self.rest
                    .get("/assets/fixed_income/us_corporates", filter)
                    .await
            }
            None => {
                self.rest
                    .get("/assets/fixed_income/us_corporates", &Empty)
                    .await
            }
        }
    }

    /// The US treasury master list.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_us_treasuries(
        &self,
        filter: Option<&GetUsTreasuriesRequest>,
    ) -> Result<UsTreasuries> {
        match filter {
            Some(filter) => {
                self.rest
                    .get("/assets/fixed_income/us_treasuries", filter)
                    .await
            }
            None => {
                self.rest
                    .get("/assets/fixed_income/us_treasuries", &Empty)
                    .await
            }
        }
    }

    /// What Regulation T requires to hold the named symbols.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_entry_requirements(
        &self,
        request: &GetEntryRequirementsRequest,
    ) -> Result<Vec<EntryRequirement>> {
        self.rest.get("/assets/entry-requirements", request).await
    }

    // ------------------------------------------------- instant funding

    /// Lists instant funding advances.
    ///
    /// Pages by offset, unlike the token-paginated broker routes.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_instant_funding(
        &self,
        filter: Option<&GetInstantFundingRequest>,
    ) -> Result<Vec<InstantFunding>> {
        match filter {
            Some(filter) => self.rest.get("/instant_funding", filter).await,
            None => self.rest.get("/instant_funding", &Empty).await,
        }
    }

    /// Advances cash against a deposit that has not cleared.
    ///
    /// # Errors
    /// Returns [`crate::Error::InvalidRequest`] if the amount is not positive.
    pub async fn create_instant_funding(
        &self,
        request: &CreateInstantFundingRequest,
    ) -> Result<InstantFunding> {
        request.validate()?;
        self.rest.post("/instant_funding", request).await
    }

    /// Fetches one advance.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_instant_funding_by_id(&self, funding_id: &str) -> Result<InstantFunding> {
        self.rest
            .get(
                &format!("/instant_funding/{}", segment(funding_id)?),
                &Empty,
            )
            .await
    }

    /// Cancels an advance.
    ///
    /// # Errors
    /// Propagates transport and API failures.
    pub async fn cancel_instant_funding(&self, funding_id: &str) -> Result<()> {
        self.send_void(
            Method::DELETE,
            &format!("/instant_funding/{}", segment(funding_id)?),
            None::<&Empty>,
        )
        .await
    }

    /// How much instant funding the correspondent may have outstanding.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_instant_funding_limits(&self) -> Result<InstantFundingLimits> {
        self.rest.get("/instant_funding/limits", &Empty).await
    }

    /// The named accounts' shares of that limit.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_instant_funding_account_limits(
        &self,
        request: &GetAccountLimitsRequest,
    ) -> Result<Vec<AccountInstantFundingLimits>> {
        self.rest
            .get("/instant_funding/limits/accounts", request)
            .await
    }

    /// A day's instant funding position, by account.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_instant_funding_reports(
        &self,
        filter: Option<&GetInstantFundingReportRequest>,
    ) -> Result<Vec<InstantFundingReport>> {
        match filter {
            Some(filter) => self.rest.get("/instant_funding/reports", filter).await,
            None => self.rest.get("/instant_funding/reports", &Empty).await,
        }
    }

    /// Lists instant funding settlements.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_instant_funding_settlements(
        &self,
        filter: Option<&GetSettlementsRequest>,
    ) -> Result<Settlements> {
        match filter {
            Some(filter) => self.rest.get("/instant_funding/settlements", filter).await,
            None => self.rest.get("/instant_funding/settlements", &Empty).await,
        }
    }

    /// Settles one or more advances.
    ///
    /// # Errors
    /// Returns [`crate::Error::InvalidRequest`] if no transfers are named.
    pub async fn create_instant_funding_settlement(
        &self,
        request: &CreateInstantFundingSettlementRequest,
    ) -> Result<Settlement> {
        request.validate()?;
        self.rest
            .post("/instant_funding/settlements", request)
            .await
    }

    /// Fetches one instant funding settlement.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_instant_funding_settlement(&self, settlement_id: Uuid) -> Result<Settlement> {
        self.rest
            .get(
                &format!("/instant_funding/settlements/{settlement_id}"),
                &Empty,
            )
            .await
    }

    // --------------------------------------------------------- JIT

    /// The correspondent's JIT ledgers.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_jit_ledgers(&self) -> Result<Vec<JitLedger>> {
        self.rest.get("/transfers/jit/ledgers", &Empty).await
    }

    /// One ledger's balances over a window, and the movements behind them.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_jit_ledger_balances(
        &self,
        ledger_id: &str,
        filter: Option<&GetJitBalancesRequest>,
    ) -> Result<JitLedgerBalances> {
        let path = format!("/transfers/jit/{}/balances", segment(ledger_id)?);
        match filter {
            Some(filter) => self.rest.get(&path, filter).await,
            None => self.rest.get(&path, &Empty).await,
        }
    }

    /// The correspondent's trading limits for the day.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_jit_trading_limits(&self) -> Result<JitTradingLimits> {
        self.rest.get("/transfers/jit/limits", &Empty).await
    }

    /// A JIT report, inline or as a link.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_jit_report(&self, request: &GetJitReportRequest) -> Result<JitReport> {
        self.rest.get("/transfers/jit/reports", request).await
    }

    /// Lists JIT settlements.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_jit_settlements(
        &self,
        filter: Option<&GetSettlementsRequest>,
    ) -> Result<Settlements> {
        match filter {
            Some(filter) => self.rest.get("/jit/settlements", filter).await,
            None => self.rest.get("/jit/settlements", &Empty).await,
        }
    }

    /// Settles a day's JIT obligation.
    ///
    /// **Documented by the spec and not by the reference**, which is the same
    /// footing as the other routes this crate calls on the spec's word alone.
    /// It is implemented rather than left out — undocumented is not absent —
    /// but a live sandbox is what would confirm it.
    ///
    /// # Errors
    /// Returns [`crate::Error::InvalidRequest`] if the settlement names no
    /// accounts or a non-positive amount.
    pub async fn create_jit_settlement(
        &self,
        request: &CreateJitSettlementRequest,
    ) -> Result<Settlement> {
        request.validate()?;
        self.rest.post("/jit/settlements", request).await
    }

    /// Fetches one JIT settlement.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_jit_settlement(&self, settlement_id: Uuid) -> Result<Settlement> {
        self.rest
            .get(&format!("/jit/settlements/{settlement_id}"), &Empty)
            .await
    }

    // -------------------------------------------------------- FPSL

    /// Lists fully-paid securities lending loans.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_fpsl_loans(
        &self,
        filter: Option<&GetFpslLoansRequest>,
    ) -> Result<FpslLoansPage> {
        match filter {
            Some(filter) => self.rest.get("/fpsl/loans", filter).await,
            None => self.rest.get("/fpsl/loans", &Empty).await,
        }
    }

    /// The revenue-split tiers.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_fpsl_tiers(&self) -> Result<Vec<FpslTier>> {
        self.rest.get("/fpsl/tiers", &Empty).await
    }

    /// One account's lending activity over a window.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_fpsl_analytics(
        &self,
        account_id: Uuid,
        filter: Option<&GetFpslAnalyticsRequest>,
    ) -> Result<FpslAnalytics> {
        let path = format!("/fpsl/analytics/{account_id}/loans");
        match filter {
            Some(filter) => self.rest.get(&path, filter).await,
            None => self.rest.get(&path, &Empty).await,
        }
    }

    // -------------------------------------------------------- IPOs

    /// Lists IPO offerings.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_ipo_offerings(
        &self,
        filter: Option<&GetIpoOfferingsRequest>,
    ) -> Result<IpoOfferingsPage> {
        match filter {
            Some(filter) => self.rest.get("/ipos", filter).await,
            None => self.rest.get("/ipos", &Empty).await,
        }
    }

    /// Fetches one offering.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_ipo_offering(&self, offering_reference: &str) -> Result<IpoOfferingResponse> {
        self.rest
            .get(&format!("/ipos/{}", segment(offering_reference)?), &Empty)
            .await
    }

    // --------------------------------------------------- reporting

    /// Positions across accounts as of one close.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_eod_positions(
        &self,
        filter: Option<&GetEodPositionsRequest>,
    ) -> Result<EodPositions> {
        match filter {
            Some(filter) => self.rest.get("/reporting/eod/positions", filter).await,
            None => self.rest.get("/reporting/eod/positions", &Empty).await,
        }
    }

    /// The same positions summed by symbol.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_aggregate_positions(
        &self,
        request: &GetAggregatePositionsRequest,
    ) -> Result<Vec<AggregatePosition>> {
        self.rest
            .get("/reporting/eod/aggregate_positions", request)
            .await
    }

    /// What each account earned on its idle cash.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_eod_cash_interest(
        &self,
        filter: Option<&GetCashInterestRequest>,
    ) -> Result<CashInterestReport> {
        match filter {
            Some(filter) => self.rest.get("/reporting/eod/cash_interest", filter).await,
            None => self.rest.get("/reporting/eod/cash_interest", &Empty).await,
        }
    }

    /// The cash interest rate tiers.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_apr_tiers(&self) -> Result<AprTiers> {
        self.rest.get("/cash_interest/apr_tiers", &Empty).await
    }

    // ------------------------------------------------------- OAuth

    /// Looks up a registered third-party application.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_oauth_client(
        &self,
        client_id: Uuid,
        filter: Option<&GetOAuthClientRequest>,
    ) -> Result<OAuthClient> {
        let path = format!("/oauth/clients/{client_id}");
        match filter {
            Some(filter) => self.rest.get(&path, filter).await,
            None => self.rest.get(&path, &Empty).await,
        }
    }

    /// Authorizes an application against an account.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn authorize_oauth(&self, request: &OAuthRequest) -> Result<OAuthCode> {
        self.rest.post("/oauth/authorize", request).await
    }

    /// Issues a bearer token.
    ///
    /// Takes a JSON body, not the form encoding OAuth token endpoints
    /// conventionally use. See [`crate::broker::oauth`].
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn issue_oauth_token(&self, request: &OAuthRequest) -> Result<OAuthToken> {
        self.rest.post("/oauth/token", request).await
    }

    // ---------------------------------------------- funding wallets

    /// Opens funding wallets for several accounts at once.
    ///
    /// **A `v1beta` route**, like every other funding wallet route.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn batch_create_funding_wallets(
        &self,
        request: &BatchCreateFundingWalletsRequest,
    ) -> Result<FundingWallets> {
        self.rest
            .at_version("v1beta")
            .post("/accounts/funding_wallet", request)
            .await
    }

    /// An account's funding wallet.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_funding_wallet(&self, account_id: Uuid) -> Result<FundingWallet> {
        let path = format!("/accounts/{account_id}/funding_wallet");
        self.rest.at_version("v1beta").get(&path, &Empty).await
    }

    /// Opens an account's funding wallet.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn create_funding_wallet(&self, account_id: Uuid) -> Result<FundingWallet> {
        let path = format!("/accounts/{account_id}/funding_wallet");
        self.rest.at_version("v1beta").post(&path, &Empty).await
    }

    /// The banking details money should be sent to.
    ///
    /// Returned untyped: the reference documents no response schema for this
    /// route at all, and inventing one would claim more than is known.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_funding_details(
        &self,
        account_id: Uuid,
        filter: Option<&GetFundingDetailsRequest>,
    ) -> Result<serde_json::Value> {
        let path = format!("/accounts/{account_id}/funding_wallet/funding_details");
        match filter {
            Some(filter) => self.rest.at_version("v1beta").get(&path, filter).await,
            None => self.rest.at_version("v1beta").get(&path, &Empty).await,
        }
    }

    /// The bank withdrawals are sent to.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_recipient_bank(&self, account_id: Uuid) -> Result<RecipientBank> {
        let path = format!("/accounts/{account_id}/funding_wallet/recipient_bank");
        self.rest.at_version("v1beta").get(&path, &Empty).await
    }

    /// Registers the bank withdrawals are sent to.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn create_recipient_bank(
        &self,
        account_id: Uuid,
        request: &CreateRecipientBankRequest,
    ) -> Result<RecipientBank> {
        let path = format!("/accounts/{account_id}/funding_wallet/recipient_bank");
        self.rest.at_version("v1beta").post(&path, request).await
    }

    /// Removes the registered bank.
    ///
    /// # Errors
    /// Propagates transport and API failures.
    pub async fn delete_recipient_bank(&self, account_id: Uuid) -> Result<()> {
        let path = format!("/accounts/{account_id}/funding_wallet/recipient_bank");
        self.rest.at_version("v1beta").delete(&path, &Empty).await
    }

    /// Money in and out of an account's funding wallet.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_funding_wallet_transfers(
        &self,
        account_id: Uuid,
    ) -> Result<FundingWalletTransfers> {
        let path = format!("/accounts/{account_id}/funding_wallet/transfers");
        self.rest.at_version("v1beta").get(&path, &Empty).await
    }

    /// One funding wallet transfer.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_funding_wallet_transfer(
        &self,
        account_id: Uuid,
        transfer_id: Uuid,
    ) -> Result<FundingWalletTransfer> {
        let path = format!("/accounts/{account_id}/funding_wallet/transfers/{transfer_id}");
        self.rest.at_version("v1beta").get(&path, &Empty).await
    }

    /// Sends money out of an account's funding wallet.
    ///
    /// # Errors
    /// Returns [`crate::Error::InvalidRequest`] if the amount is not positive.
    pub async fn create_funding_wallet_withdrawal(
        &self,
        account_id: Uuid,
        request: &CreateWithdrawalRequest,
    ) -> Result<FundingWalletTransfer> {
        request.validate()?;
        let path = format!("/accounts/{account_id}/funding_wallet/withdrawal");
        self.rest.at_version("v1beta").post(&path, request).await
    }

    /// Simulates an incoming deposit. Sandbox only.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn create_demo_funding(
        &self,
        request: &DemoFundingRequest,
    ) -> Result<DemoFundingRequest> {
        self.rest
            .at_version("v1beta")
            .post("/demo/banking/funding", request)
            .await
    }

    // ------------------------------------------- account onboarding extras

    /// Requests options trading for an account. **BETA.**
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn request_options_approval(
        &self,
        account_id: Uuid,
        request: &RequestOptionsApprovalRequest,
    ) -> Result<OptionsApproval> {
        let path = format!("/accounts/{account_id}/options/approval");
        self.rest.post(&path, request).await
    }

    /// Lists options approval requests. **BETA.**
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_options_approvals(
        &self,
        filter: Option<&GetOptionsApprovalsRequest>,
    ) -> Result<OptionsApprovalsPage> {
        match filter {
            Some(filter) => self.rest.get("/accounts/options/approvals", filter).await,
            None => self.rest.get("/accounts/options/approvals", &Empty).await,
        }
    }

    /// A token for Onfido's client-side identity-verification SDK.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_onfido_token(
        &self,
        account_id: Uuid,
        filter: Option<&GetOnfidoTokenRequest>,
    ) -> Result<OnfidoToken> {
        let path = format!("/accounts/{account_id}/onfido/sdk/tokens");
        match filter {
            Some(filter) => self.rest.get(&path, filter).await,
            None => self.rest.get(&path, &Empty).await,
        }
    }

    /// Reports what Onfido's SDK concluded.
    ///
    /// # Errors
    /// Propagates transport and API failures.
    pub async fn update_onfido_outcome(
        &self,
        account_id: Uuid,
        request: &UpdateOnfidoOutcomeRequest,
    ) -> Result<()> {
        self.send_void(
            Method::PATCH,
            &format!("/accounts/{account_id}/onfido/sdk"),
            Some(request),
        )
        .await
    }

    /// What Alpaca will serve in each country, keyed by country code.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_country_info(&self) -> Result<std::collections::HashMap<String, CountryInfo>> {
        self.rest.get("/country-info", &Empty).await
    }

    /// IRA contributions that exceeded the annual limit.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_ira_excess_contributions(&self) -> Result<Vec<IraExcessContribution>> {
        self.rest
            .get("/accounts/ira_excess_contributions", &Empty)
            .await
    }

    /// Downloads an account's W-8BEN document.
    ///
    /// A different route from the general document download, and the reference
    /// documents no response schema for it, so this returns the bytes as they
    /// arrive. Like that download it may answer `301` to a presigned URL, so it
    /// goes through the redirect-following client rather than [`RestClient`].
    ///
    /// # Errors
    /// Propagates transport and API failures.
    pub async fn download_w8ben_document(
        &self,
        account_id: Uuid,
        document_id: Uuid,
    ) -> Result<Vec<u8>> {
        let path = format!("/accounts/{account_id}/documents/w8ben/{document_id}/download");
        self.download(&path).await
    }

    // ------------------------------------------------- trading extras

    /// What an account may still trade today.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_trading_limits(&self, account_id: Uuid) -> Result<TradingLimits> {
        let path = format!("/trading/accounts/{account_id}/limits");
        self.rest.get(&path, &Empty).await
    }

    /// Costs an order without placing it.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn estimate_order(
        &self,
        account_id: Uuid,
        request: &EstimateOrderRequest,
    ) -> Result<Order> {
        let path = format!("/trading/accounts/{account_id}/orders/estimation");
        self.rest.post(&path, request).await
    }

    /// Declines to exercise an in-the-money option position at expiry.
    ///
    /// # Errors
    /// Propagates transport and API failures.
    pub async fn do_not_exercise(
        &self,
        account_id: Uuid,
        contract: &crate::types::AssetIdent,
    ) -> Result<()> {
        self.send_void(
            Method::POST,
            &format!(
                "/trading/accounts/{account_id}/positions/{}/do-not-exercise",
                contract.as_path_segment()?
            ),
            None::<&Empty>,
        )
        .await
    }

    // -------------------------------------------------- tokenization

    /// Mints a tokenized asset from an account's position.
    ///
    /// The account-scoped counterpart to
    /// [`TradingClient::mint_token`](crate::trading::TradingClient::mint_token);
    /// the models are shared.
    ///
    /// # Errors
    /// Returns [`crate::Error::InvalidRequest`] if `qty` is not positive.
    pub async fn mint_token_for_account(
        &self,
        account_id: Uuid,
        request: &crate::trading::MintTokenRequest,
    ) -> Result<crate::trading::TokenizationRequest> {
        request.validate()?;
        let path = format!("/accounts/{account_id}/tokenization/mint");
        self.rest.post(&path, request).await
    }

    /// Lists an account's tokenization requests.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_tokenization_requests_for_account(
        &self,
        account_id: Uuid,
        filter: Option<&crate::trading::GetTokenizationRequestsRequest>,
    ) -> Result<Vec<crate::trading::TokenizationRequest>> {
        let path = format!("/accounts/{account_id}/tokenization/requests");
        match filter {
            Some(filter) => self.rest.get(&path, filter).await,
            None => self.rest.get(&path, &Empty).await,
        }
    }

    /// Fetches one of an account's tokenization requests.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_tokenization_request_for_account(
        &self,
        account_id: Uuid,
        request_id: Uuid,
    ) -> Result<crate::trading::TokenizationRequest> {
        let path = format!("/accounts/{account_id}/tokenization/requests/{request_id}");
        self.rest.get(&path, &Empty).await
    }

    /// Fetches one by the caller's own request id.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_tokenization_request_by_client_id_for_account(
        &self,
        account_id: Uuid,
        request: &crate::trading::ByClientRequestId,
    ) -> Result<crate::trading::TokenizationRequest> {
        let path = format!("/accounts/{account_id}/tokenization/requests:by_client_request_id");
        self.rest.get(&path, request).await
    }

    /// Fetches one by the issuer's request id.
    ///
    /// Has no counterpart on the trading API, which knows only the client id.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_tokenization_request_by_issuer_id_for_account(
        &self,
        account_id: Uuid,
        issuer_request_id: &str,
    ) -> Result<crate::trading::TokenizationRequest> {
        let path = format!("/accounts/{account_id}/tokenization/requests:by_issuer_request_id");
        self.rest
            .get(&path, &[("issuer_request_id", issuer_request_id)])
            .await
    }

    /// Acknowledges an issuer's mint callback.
    ///
    /// # Errors
    /// Propagates transport and API failures.
    pub async fn tokenization_mint_callback(
        &self,
        account_id: Uuid,
        body: &serde_json::Value,
    ) -> Result<()> {
        self.send_void(
            Method::POST,
            &format!("/accounts/{account_id}/tokenization/callback/mint"),
            Some(body),
        )
        .await
    }

    /// Acknowledges an issuer's redeem callback.
    ///
    /// # Errors
    /// Propagates transport and API failures.
    pub async fn tokenization_redeem_callback(
        &self,
        account_id: Uuid,
        body: &serde_json::Value,
    ) -> Result<()> {
        self.send_void(
            Method::POST,
            &format!("/accounts/{account_id}/tokenization/callback/redeem"),
            Some(body),
        )
        .await
    }

    // ------------------------------------------------- crypto wallets

    /// An account's crypto deposit wallets.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_crypto_wallets_for_account(
        &self,
        account_id: Uuid,
        filter: Option<&crate::trading::GetCryptoWalletsRequest>,
    ) -> Result<Vec<crate::trading::CryptoWallet>> {
        let path = format!("/accounts/{account_id}/wallets");
        let wallets: OneOrMany<crate::trading::CryptoWallet> = match filter {
            Some(filter) => self.rest.get(&path, filter).await?,
            None => self.rest.get(&path, &Empty).await?,
        };
        Ok(wallets.into_vec())
    }

    /// An account's on-chain transfers.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_crypto_transfers_for_account(
        &self,
        account_id: Uuid,
    ) -> Result<Vec<crate::trading::CryptoTransfer>> {
        let path = format!("/accounts/{account_id}/wallets/transfers");
        let transfers: OneOrMany<crate::trading::CryptoTransfer> =
            self.rest.get(&path, &Empty).await?;
        Ok(transfers.into_vec())
    }

    /// Fetches one of an account's on-chain transfers.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_crypto_transfer_for_account(
        &self,
        account_id: Uuid,
        transfer_id: &str,
    ) -> Result<crate::trading::CryptoTransfer> {
        let path = format!(
            "/accounts/{account_id}/wallets/transfers/{}",
            segment(transfer_id)?
        );
        self.rest.get(&path, &Empty).await
    }

    /// Withdraws crypto from an account.
    ///
    /// **This route is not deprecated, and its trading-API counterpart is.**
    /// `POST /v2/wallets/transfers` sunsets 2026-10-09 with the web app as its
    /// replacement; this one carries no such notice, which is why the crate has
    /// the broker withdrawal and not the trading one.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn create_crypto_transfer_for_account(
        &self,
        account_id: Uuid,
        request: &serde_json::Value,
    ) -> Result<crate::trading::CryptoTransfer> {
        let path = format!("/accounts/{account_id}/wallets/transfers");
        self.rest.post(&path, request).await
    }

    /// The addresses an account may withdraw to.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn get_whitelisted_addresses_for_account(
        &self,
        account_id: Uuid,
    ) -> Result<Vec<crate::trading::WhitelistedAddress>> {
        let path = format!("/accounts/{account_id}/wallets/whitelists");
        let addresses: OneOrMany<crate::trading::WhitelistedAddress> =
            self.rest.get(&path, &Empty).await?;
        Ok(addresses.into_vec())
    }

    /// Allowlists a withdrawal address for an account.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn create_whitelisted_address_for_account(
        &self,
        account_id: Uuid,
        request: &crate::trading::CreateWhitelistedAddressRequest,
    ) -> Result<crate::trading::WhitelistedAddress> {
        let path = format!("/accounts/{account_id}/wallets/whitelists");
        self.rest.post(&path, request).await
    }

    /// Removes an allowlisted address.
    ///
    /// # Errors
    /// Propagates transport and API failures.
    pub async fn delete_whitelisted_address_for_account(
        &self,
        account_id: Uuid,
        address_id: &str,
    ) -> Result<()> {
        self.send_void(
            Method::DELETE,
            &format!(
                "/accounts/{account_id}/wallets/whitelists/{}",
                segment(address_id)?
            ),
            None::<&Empty>,
        )
        .await
    }

    /// What a proposed crypto transfer would cost in gas.
    ///
    /// **A `v1` route here and `v2` on the trading API**, for the same
    /// operation and the same response.
    ///
    /// # Errors
    /// Propagates transport, API, and decoding failures.
    pub async fn estimate_crypto_transfer_fee(
        &self,
        request: &crate::trading::TransferFeeEstimateRequest,
    ) -> Result<crate::trading::TransferFeeEstimate> {
        self.rest.get("/wallets/fees/estimate", request).await
    }
}
