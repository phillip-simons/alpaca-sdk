//! The [broker API](https://docs.alpaca.markets/us/docs/about-broker-api) client.
//!
//! Ported from `alpaca/broker/client.py`.
//!
//! Two things differ from every other client in this crate. It authenticates
//! with HTTP basic auth rather than the `APCA-*` headers, and it acts *on behalf
//! of* an account, so most routes carry an account id in the path.

use eventsource_stream::Eventsource as _;
use futures_util::{Stream, StreamExt as _};
use reqwest::Method;
use uuid::Uuid;

use crate::auth::Credentials;
use crate::broker::enums::ACHRelationshipStatus;
use crate::broker::events::{BrokerEvent, EventVersion, GetEventsRequest};
use crate::broker::models::{
    ACHRelationship, Account, AllAccountsPositions, Bank, BatchJournalResponse, CIPInfo, Journal,
    Order, Portfolio, RebalancingRun, RunsPage, Subscription, SubscriptionsPage, TradeAccount,
    TradeDocument, Transfer,
};
use crate::broker::requests::{
    CreateACHRelationshipRequest, CreateBankRequest, CreateBatchJournalRequest,
    CreateJournalRequest, CreateOptionExerciseRequest, CreatePortfolioRequest,
    CreateReverseBatchJournalRequest, CreateRunRequest, CreateSubscriptionRequest,
    CreateTransferRequest, GetAccountActivitiesRequest, GetJournalsRequest, GetPortfoliosRequest,
    GetRunsRequest, GetSubscriptionsRequest, GetTradeDocumentsRequest, GetTransfersRequest,
    OrderRequest, UpdatePortfolioRequest, UploadDocument,
};
use crate::config::BaseUrl;
use crate::error::Result;
use crate::rest::{Empty, RestClient, RestConfig};
use crate::trading::{Activity, Position, Watchlist};

/// The most documents Alpaca accepts in one upload.
const DOCUMENT_UPLOAD_LIMIT: usize = 10;

/// The id an activity pages from, whichever kind it is.
fn activity_id(activity: &Activity) -> &str {
    match activity {
        Activity::Trade(trade) => &trade.id,
        Activity::NonTrade(non_trade) => &non_trade.id,
    }
}

/// alpaca-py's default page size for the token-paginated broker routes.
const DEFAULT_PAGE_SIZE: u32 = 100;

/// The page size to ask for, given a cap on the total.
///
/// alpaca-py narrows the last request to exactly what is still wanted rather
/// than fetching a full page and discarding the tail. Returns `None` when there
/// is no cap, leaving the caller's own page size alone.
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
/// let accounts = client.get_all_accounts_positions().await?;
/// println!("{} accounts hold positions", accounts.positions.len());
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct BrokerClient {
    rest: RestClient,
    /// A second HTTP client, for the routes that do not fit [`RestClient`].
    ///
    /// Two need it. The document download answers `301` to a presigned storage
    /// URL, and `RestClient` refuses redirects deliberately; this client follows
    /// them and drops the credentials when one crosses to another host, which is
    /// what `requests` does for alpaca-py and what keeps broker keys off a
    /// storage provider. The event streams need a response body that is read
    /// incrementally rather than decoded whole.
    raw: reqwest::Client,
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
            raw: Self::raw_client(&credentials, &config)?,
            rest: RestClient::new(&credentials, config)?,
        })
    }

    /// Builds the client used by the document download and the event streams.
    fn raw_client(credentials: &Credentials, config: &RestConfig) -> Result<reqwest::Client> {
        let mut headers = reqwest::header::HeaderMap::new();
        credentials.apply(&mut headers)?;
        headers.insert(
            reqwest::header::USER_AGENT,
            reqwest::header::HeaderValue::from_static(crate::config::user_agent()),
        );

        let mut builder = reqwest::Client::builder()
            .default_headers(headers)
            .redirect(reqwest::redirect::Policy::limited(10));
        if let Some(timeout) = config.timeout {
            builder = builder.timeout(timeout);
        }
        Ok(builder.build()?)
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
    /// This is alpaca-py's `PaginationType.NONE`: exactly one request, honouring
    /// whatever `limit` and `offset` the filter carries. Use
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
    /// This is alpaca-py's default, `PaginationType.FULL`: request pages until
    /// one comes back empty, then return the lot. `max_items` caps the total,
    /// as alpaca-py's `max_items_limit` does.
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
            // alpaca-py sets the offset to the running count on every pass,
            // including the first, so a caller-supplied offset is overwritten
            // once the walk starts.
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
    /// Subscribes to `/v2/events/trades`. alpaca-py still calls the v1 route,
    /// which Alpaca documents as "fully deprecated and no longer available" —
    /// porting it faithfully would have shipped a route that does not answer.
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
    /// transfer stream alpaca-py calls. That route is deprecated and open only
    /// to broker partners who already had it; new partners cannot use it at all.
    ///
    /// The v2 stream is also broader than its name here suggests: it covers bank
    /// relationships, wire banks and funding wallets as well as transfers. The
    /// method keeps alpaca-py's name so the mapping stays findable.
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

    /// Opens one of the five event streams.
    ///
    /// The version comes from the stream rather than from the client's
    /// `api_version`: Alpaca versions these endpoints individually, so two of
    /// the five are v1 and three are v2.
    ///
    /// The subscription itself is awaited so a rejected one — bad credentials, a
    /// filter the server dislikes — surfaces as an error here rather than as a
    /// single item on an otherwise empty stream.
    async fn events(
        &self,
        version: EventVersion,
        path: &str,
        filter: Option<&GetEventsRequest>,
    ) -> Result<impl Stream<Item = Result<BrokerEvent>> + use<>> {
        let config = self.rest.config();
        let url = format!(
            "{}/{}{path}",
            config.base_url.trim_end_matches('/'),
            version.segment()
        );

        let mut request = self
            .raw
            .get(&url)
            // alpaca-py's _get_sse_headers, verbatim.
            .header(reqwest::header::CONNECTION, "keep-alive")
            .header(reqwest::header::CACHE_CONTROL, "no-cache")
            .header(reqwest::header::CONTENT_TYPE, "text/event-stream")
            .header(reqwest::header::ACCEPT, "text/event-stream");
        if let Some(filter) = filter {
            // Rendered per version: the cursor parameter is named differently
            // on each, and means something different under the v1 name.
            request = request.query(&version.query(filter));
        }

        let response = request.send().await.map_err(crate::Error::Transport)?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(crate::Error::Api(crate::error::ApiError::from_body(
                status.as_u16(),
                path,
                body,
            )));
        }

        // Nothing is retried past this point: a stream that dies mid-flight has
        // already delivered events, and replaying it would repeat them. alpaca-py
        // does not reconnect either.
        Ok(response
            .bytes_stream()
            .eventsource()
            .map(|event| match event {
                Ok(event) => Ok(BrokerEvent::from(event)),
                Err(error) => Err(crate::broker::events::stream_error(&error)),
            }))
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
    /// Returns [`crate::Error::InvalidRequest`] if the filter combines `date`
    /// with `after` or `until`.
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
    /// Returns [`crate::Error::InvalidRequest`] if the filter combines `date`
    /// with `after` or `until`.
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
    /// **Unverified.** alpaca-py leaves this route unimplemented — its comment
    /// says the sandbox answers 404 — so [`CIPInfo`] is derived from the models
    /// and the spec rather than from a captured response.
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
    /// alpaca-py streams this straight to a path; the bytes are returned here so
    /// the caller decides where they go.
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
                .raw
                .get(&url)
                .send()
                .await
                .map_err(crate::Error::Transport)?;
            let status = response.status().as_u16();

            if response.status().is_success() {
                return Ok(response
                    .bytes()
                    .await
                    .map_err(crate::Error::Transport)?
                    .to_vec());
            }

            let body = response.text().await.unwrap_or_default();
            let api_error = crate::error::ApiError::from_body(status, &path, body);

            if !retry.should_retry(status) {
                return Err(crate::Error::Api(api_error));
            }
            if attempt == total_attempts {
                return Err(crate::Error::RetriesExhausted {
                    attempts: total_attempts,
                    last: api_error,
                });
            }
            tokio::time::sleep(retry.wait).await;
        }

        unreachable!("retry loop exited without returning")
    }

    /// Uploads up to ten documents to an account.
    ///
    /// Contents are base64-encoded, and capped at 10MB each when Alpaca does the
    /// KYC. The route answers `204`, so a success returns nothing.
    ///
    /// # Errors
    /// Returns [`crate::Error::InvalidRequest`] if more than ten documents are
    /// passed, or if any of them fails [`UploadDocument::validate`].
    pub async fn upload_documents_to_account(
        &self,
        account_id: Uuid,
        documents: &[UploadDocument],
    ) -> Result<()> {
        if documents.len() > DOCUMENT_UPLOAD_LIMIT {
            return Err(crate::Error::InvalidRequest(format!(
                "at most {DOCUMENT_UPLOAD_LIMIT} documents may be uploaded at once, got {}",
                documents.len()
            )));
        }
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
