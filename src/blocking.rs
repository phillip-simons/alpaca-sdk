//! A synchronous façade over the async clients.
//!
//! # Why this is one type and not a mirrored API
//!
//! The obvious shape for this feature is a `BlockingTradingClient` with a
//! synchronous copy of every method, which is what most SDKs do. This crate
//! covers 251 routes; mirroring them would double the surface that has to stay
//! correct, and the copy would drift from the original the first time a route
//! was added to one and not the other.
//!
//! [`Blocking`] wraps any of the clients instead and runs one call at a time.
//! Every route is reachable through it the day it is added to the async client,
//! and there is nothing to keep in sync:
//!
//! ```no_run
//! use alpaca_sdk::blocking::Blocking;
//! use alpaca_sdk::trading::TradingClient;
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let credentials = alpaca_sdk::Credentials::from_env()?;
//! let client = Blocking::new(TradingClient::new(&credentials, true)?)?;
//!
//! let account = client.call(|client| client.get_account())?;
//! println!("{}", account.id);
//! # Ok(())
//! # }
//! ```
//!
//! # Streams stay async
//!
//! This wraps request/response calls. The websocket and SSE streams are
//! [`Stream`](futures_util::Stream)s with no synchronous equivalent worth
//! pretending to — a blocking iterator over a live market data feed would
//! deadlock the moment the caller took longer than the socket's read buffer.

use std::future::Future;

use tokio::runtime::{Builder, Runtime};

use crate::error::{Error, Result};

/// Whether a caught panic is tokio's "runtime inside a runtime" complaint.
///
/// Matched on the message rather than a type, because that is all tokio gives:
/// it panics with a `&str`/`String` payload beginning
/// `Cannot start a runtime from within a runtime`. Matching on a substring is
/// fragile against a tokio reword, and the failure mode of a reword is that a
/// misuse resurfaces as the original panic — loud and accurate — rather than as
/// a wrong error. That is the right direction to fail in.
///
/// Takes `&(dyn Any + Send)` and must be called as `&*panic`, not `&panic`:
/// `Box<dyn Any + Send>` itself implements `Any`, so passing the box by
/// reference downcasts *the box* and never matches.
fn is_nested_runtime_panic(panic: &(dyn std::any::Any + Send)) -> bool {
    let message = panic
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| panic.downcast_ref::<String>().map(String::as_str));

    message.is_some_and(|message| message.contains("runtime from within a runtime"))
}

/// Runs an async client's calls to completion on a runtime it owns.
///
/// See the [module documentation](self) for why this is one wrapper rather than
/// a synchronous copy of every method.
///
/// # The async-context trap
///
/// Tokio panics in *two* places when a runtime meets an async context, and both
/// are reachable by a caller who tries this inside an `#[tokio::main]` fn:
///
/// - Blocking on a runtime from inside another runtime's thread.
///   [`call`](Self::call) catches that panic and returns
///   [`Error::InvalidRequest`] instead. In an async context, use the async
///   client directly — it is what this is wrapping.
///
///   The exception is [`tokio::task::spawn_blocking`], which is the supported
///   bridge from an async program into this façade: those threads are not
///   driving the reactor, so blocking on them is allowed and a call made there
///   succeeds. They do carry an ambient runtime handle, which is why the check
///   is tokio's rather than a test for one.
/// - *Dropping* a runtime from inside one. This is the easier of the two to hit
///   by accident, because it needs no call at all: constructing a `Blocking` in
///   an async fn and letting it fall out of scope is enough. The runtime is shut
///   down in the background on drop, which is allowed anywhere, so this one
///   cannot happen.
#[derive(Debug)]
pub struct Blocking<C> {
    client: C,
    /// `Some` for the whole life of the value; taken in [`Drop`], which is the
    /// only way to move a runtime out of a type that implements it.
    runtime: Option<Runtime>,
}

impl<C> Blocking<C> {
    /// Wraps `client` in a runtime of its own.
    ///
    /// The runtime is multi-threaded with a single worker. A current-thread
    /// runtime would save the thread, but it only makes progress while a call is
    /// blocked on it — so the connection pool's idle work would stop between
    /// calls, which is exactly the state a synchronous caller spends most of its
    /// time in.
    ///
    /// # Errors
    /// Returns [`Error::InvalidRequest`] if the runtime cannot be started, which
    /// in practice means the process cannot spawn a thread.
    pub fn new(client: C) -> Result<Self> {
        let runtime = Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .map_err(|e| Error::InvalidRequest(format!("could not start a runtime: {e}")))?;

        Ok(Self {
            client,
            runtime: Some(runtime),
        })
    }

    /// Runs one call on the wrapped client to completion.
    ///
    /// ```no_run
    /// # use alpaca_sdk::blocking::Blocking;
    /// # use alpaca_sdk::trading::{TradingClient, GetOrdersRequest};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let client: Blocking<TradingClient> = todo!();
    /// let request = GetOrdersRequest::default();
    /// let orders = client.call(|client| client.get_orders(Some(&request)))?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    /// Returns whatever the call returns, or [`Error::InvalidRequest`] if it was
    /// made from inside an async context — see [the trap](Self#the-async-context-trap).
    pub fn call<'a, F, Fut, T>(&'a self, call: F) -> Result<T>
    where
        F: FnOnce(&'a C) -> Fut,
        Fut: Future<Output = Result<T>> + 'a,
    {
        let Some(runtime) = self.runtime.as_ref() else {
            // Unreachable: the runtime is only taken in `Drop`, which consumes
            // the value. Reported rather than unwrapped so a future refactor
            // that breaks the invariant fails a call instead of the process.
            return Err(Error::InvalidRequest(
                "the blocking runtime has already shut down".to_owned(),
            ));
        };

        // Tokio's own check decides this, rather than a guess at it.
        //
        // The guess used to be `Handle::try_current().is_ok()`, which is true in
        // two different situations that behave differently. Inside an async fn,
        // `block_on` panics — the case worth refusing. Inside `spawn_blocking`,
        // a `Handle` is equally present and `block_on` works perfectly well,
        // because that thread is not driving the reactor. Refusing on the
        // handle alone therefore failed the one bridge an async caller has into
        // this façade, and failed it with an error on a call that would have
        // returned a result.
        //
        // `catch_unwind` asks the question that actually matters by letting
        // tokio answer it. Note that the panic hook still runs first, so an
        // async-context misuse prints tokio's message before this returns.
        //
        // Only *that* panic is converted. Any other one — a bug in this crate,
        // or in the caller's own future — is re-raised untouched, because
        // reporting it as "you called this from an async context" would be a
        // lie that hides a real defect. This also means the behaviour under
        // `panic = "abort"` is the same as without the wrapper: nothing is
        // caught, and the process aborts on a genuine panic either way.
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            runtime.block_on(call(&self.client))
        })) {
            Ok(result) => result,
            Err(panic) if is_nested_runtime_panic(&*panic) => Err(Error::InvalidRequest(
                "a blocking call cannot be made from inside an async runtime; \
                 use the async client directly here, or move the call onto \
                 `tokio::task::spawn_blocking`"
                    .to_owned(),
            )),
            Err(panic) => std::panic::resume_unwind(panic),
        }
    }

    /// The wrapped client, for anything this façade does not cover — including
    /// the streams, which stay async.
    pub fn inner(&self) -> &C {
        &self.client
    }
}

impl<C> Drop for Blocking<C> {
    /// Shuts the runtime down without waiting for it.
    ///
    /// `Runtime::drop` blocks until its threads have stopped, and blocking is
    /// not allowed inside an async context — so the ordinary drop panics if the
    /// value happens to go out of scope in an async fn. Shutting down in the
    /// background is permitted everywhere, and this wrapper spawns nothing whose
    /// completion a caller could be waiting on.
    fn drop(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_background();
        }
    }
}
