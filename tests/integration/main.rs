//! Every integration test, in one binary.
//!
//! These were 32 separate files directly under `tests/`, which cargo turns into
//! 32 separate test *binaries*. On macOS the first execution of a
//! freshly-linked binary costs ~20s before a single line of test code runs —
//! a fixed charge per new binary, not proportional to its size. Any edit to
//! `src/` relinks all of them, so `just test` paid that charge 34 times over:
//! measured at ~830s of the ~880s step, against ~50s of actual compilation and
//! ~30s of actual test execution.
//!
//! Collapsing them into one target pays it once. Nothing else changed: each
//! file kept its own `#![cfg(feature = ...)]`, which gates a module exactly as
//! it gated a crate root, so the feature matrix still sees an ungated test.
//!
//! `live_capture.rs` stays a separate target because `just capture` selects it
//! with `--test live_capture`.

mod blocking;
mod broker_accounts;
mod broker_activities;
mod broker_documents;
mod broker_events;
mod broker_extended;
mod broker_funding;
mod broker_journals;
mod broker_rebalancing;
mod broker_route_smoke;
mod broker_trading;
mod common;
mod data_frames;
mod data_historical;
mod data_meta;
mod data_route_smoke;
mod data_stream;
mod enum_parity;
mod error_surface;
mod fixture_sweep;
mod harvested_go;
mod live_fixtures;
mod live_smoke;
mod order_builders;
mod request_builders;
mod request_construction;
mod rest_transport;
mod stream_subscriptions;
mod trading_extended;
mod trading_models;
mod trading_route_smoke;
mod trading_routes;
mod trading_stream;
mod wire_codecs;
