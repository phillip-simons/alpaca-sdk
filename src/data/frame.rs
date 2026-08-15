//! `DataFrame` conversion for the market data collections.
//!
//! `BarSet` and its siblings are `pub type … = HashMap<String, Vec<Bar>>`, and a
//! type alias cannot take an inherent `impl`, so the conversion arrives as an
//! extension trait — [`ToFrame`], brought into scope with a `use`. That choice
//! is not only forced: an extension trait is additive, where the newtype an
//! inherent method would have needed is a breaking change.

use std::collections::HashMap;

use polars::prelude::*;

use crate::data::enums::Exchange;
use crate::data::models::{Bar, DailyAuctions, ForexRate, Quote, Trade};

/// Converts a market data collection into a [`DataFrame`].
///
/// Implemented for the keyed collections — [`BarSet`](crate::data::BarSet) and
/// its siblings — and for a plain slice or `Vec` of the records themselves.
///
/// Every frame leads with the key column, `symbol` or `currency_pair`, then the
/// record's own fields in declaration order. Timestamps are
/// `Datetime(Nanoseconds, "UTC")`, prices and sizes are `Float64`, and a field
/// this crate models as `Option` is a nullable column rather than a filled-in
/// default.
///
/// Rows come out grouped by key and sorted by it, so two calls on the same data
/// produce the same frame — a `HashMap` iterates in an arbitrary order, and a
/// frame that changed row order between runs would be useless for comparing
/// anything.
///
/// ```no_run
/// use alpaca_sdk::data::{StockHistoricalDataClient, StockBarsRequest, TimeFrame, ToFrame};
/// # async fn example(client: StockHistoricalDataClient) -> Result<(), Box<dyn std::error::Error>> {
/// let bars = client
///     .get_stock_bars(&StockBarsRequest::new(["AAPL", "MSFT"], TimeFrame::day()))
///     .await?;
/// let frame = bars.df()?;
/// println!("{frame}");
/// # Ok(())
/// # }
/// ```
///
/// # Errors
/// Returns the polars error if the columns cannot be assembled into a frame.
/// The failure is a polars one rather than an [`Error`](crate::Error) because
/// the operation is: a `#[cfg]`-gated variant would make the crate's error type
/// change shape with a feature flag.
pub trait ToFrame {
    /// Builds the frame.
    ///
    /// # Errors
    /// Returns the polars error if the frame cannot be assembled.
    fn df(&self) -> PolarsResult<DataFrame>;
}

/// How one record type lays itself out as frame rows.
///
/// Separate from [`ToFrame`] so the two collection shapes — a slice and a map of
/// slices — share one set of column builders.
trait FrameRows: Sized {
    /// The name of the leading key column.
    const KEY: &'static str;

    /// The record's own copy of the key, used when there is no map key to take
    /// it from.
    fn key(&self) -> &str;

    /// How many frame rows this record produces. One, except for the nested
    /// types: a day of auctions is as many rows as it has prints.
    fn rows(&self) -> usize {
        1
    }

    /// Every column but the key one.
    fn columns(records: &[&Self]) -> Vec<Column>;
}

fn name(column: &'static str) -> PlSmallStr {
    PlSmallStr::from_static(column)
}

fn floats(column: &'static str, values: impl Iterator<Item = f64>) -> Column {
    Column::new(name(column), values.collect::<Vec<_>>())
}

fn strings<'a>(column: &'static str, values: impl Iterator<Item = Option<&'a str>>) -> Column {
    Column::new(name(column), values.collect::<Vec<_>>())
}

/// A `List(String)` column for a field this crate models as
/// `Option<Vec<String>>`.
///
/// Built through the typed builder rather than from a `Vec<Option<Series>>`,
/// which infers its element type from the first non-null entry and produces a
/// `List(Null)` column when every row is null. The dtype should not depend on
/// whether the day happened to carry any condition codes.
fn string_lists<'a>(
    column: &'static str,
    values: impl Iterator<Item = Option<&'a Vec<String>>> + Clone,
) -> Column {
    let rows = values.clone().count();
    let mut builder = ListStringChunkedBuilder::new(name(column), rows, rows * 2);
    for value in values {
        match value {
            Some(codes) => builder.append_values_iter(codes.iter().map(String::as_str)),
            None => builder.append_null(),
        }
    }
    builder.finish().into_column()
}

/// The `Datetime(Nanoseconds, "UTC")` column for a set of instants.
///
/// Nanoseconds because that is what Alpaca's timestamps carry and what the
/// msgpack extension decodes to; anything coarser would round on the way in.
fn timestamps(
    column: &'static str,
    values: impl Iterator<Item = chrono::DateTime<chrono::Utc>>,
) -> Column {
    let nanos: Vec<Option<i64>> = values.map(|at| at.timestamp_nanos_opt()).collect();
    Int64Chunked::from_iter_options(name(column), nanos.into_iter())
        .into_datetime(TimeUnit::Nanoseconds, Some(TimeZone::UTC))
        .into_column()
}

fn frame<T: FrameRows>(records: &[&T], keys: Vec<&str>) -> PolarsResult<DataFrame> {
    // The key column has one entry per frame row, which is not the same as one
    // per record — a nested type expands. It is the row count either way.
    let height = keys.len();
    let mut columns = vec![Column::new(name(T::KEY), keys)];
    columns.extend(T::columns(records));
    DataFrame::new(height, columns)
}

impl<T: FrameRows> ToFrame for [T] {
    fn df(&self) -> PolarsResult<DataFrame> {
        let records: Vec<&T> = self.iter().collect();
        let keys: Vec<&str> = self
            .iter()
            .flat_map(|record| std::iter::repeat_n(record.key(), record.rows()))
            .collect();
        frame(&records, keys)
    }
}

impl<T: FrameRows> ToFrame for HashMap<String, Vec<T>> {
    fn df(&self) -> PolarsResult<DataFrame> {
        // The map key wins over the record's own copy of it. The collection
        // deserializers fill that field in, but a map built by hand need not
        // have, and the key is what the response actually said.
        let mut symbols: Vec<&String> = self.keys().collect();
        symbols.sort();

        let mut records = Vec::new();
        let mut keys = Vec::new();
        for symbol in symbols {
            for record in &self[symbol] {
                keys.extend(std::iter::repeat_n(symbol.as_str(), record.rows()));
                records.push(record);
            }
        }
        frame(&records, keys)
    }
}

impl FrameRows for Bar {
    const KEY: &'static str = "symbol";

    fn key(&self) -> &str {
        &self.symbol
    }

    fn columns(records: &[&Self]) -> Vec<Column> {
        vec![
            timestamps("timestamp", records.iter().map(|bar| bar.timestamp)),
            floats("open", records.iter().map(|bar| bar.open)),
            floats("high", records.iter().map(|bar| bar.high)),
            floats("low", records.iter().map(|bar| bar.low)),
            floats("close", records.iter().map(|bar| bar.close)),
            floats("volume", records.iter().map(|bar| bar.volume)),
            Column::new(
                name("trade_count"),
                records
                    .iter()
                    .map(|bar| bar.trade_count)
                    .collect::<Vec<_>>(),
            ),
            Column::new(
                name("vwap"),
                records.iter().map(|bar| bar.vwap).collect::<Vec<_>>(),
            ),
        ]
    }
}

impl FrameRows for Quote {
    const KEY: &'static str = "symbol";

    fn key(&self) -> &str {
        &self.symbol
    }

    fn columns(records: &[&Self]) -> Vec<Column> {
        vec![
            timestamps("timestamp", records.iter().map(|quote| quote.timestamp)),
            floats("bid_price", records.iter().map(|quote| quote.bid_price)),
            floats("bid_size", records.iter().map(|quote| quote.bid_size)),
            strings(
                "bid_exchange",
                records
                    .iter()
                    .map(|quote| quote.bid_exchange.as_ref().map(Exchange::as_str)),
            ),
            floats("ask_price", records.iter().map(|quote| quote.ask_price)),
            floats("ask_size", records.iter().map(|quote| quote.ask_size)),
            strings(
                "ask_exchange",
                records
                    .iter()
                    .map(|quote| quote.ask_exchange.as_ref().map(Exchange::as_str)),
            ),
            string_lists(
                "conditions",
                records.iter().map(|quote| quote.conditions.as_ref()),
            ),
            strings("tape", records.iter().map(|quote| quote.tape.as_deref())),
        ]
    }
}

impl FrameRows for Trade {
    const KEY: &'static str = "symbol";

    fn key(&self) -> &str {
        &self.symbol
    }

    fn columns(records: &[&Self]) -> Vec<Column> {
        vec![
            timestamps("timestamp", records.iter().map(|trade| trade.timestamp)),
            strings(
                "exchange",
                records
                    .iter()
                    .map(|trade| trade.exchange.as_ref().map(Exchange::as_str)),
            ),
            floats("price", records.iter().map(|trade| trade.price)),
            floats("size", records.iter().map(|trade| trade.size)),
            Column::new(
                name("id"),
                records.iter().map(|trade| trade.id).collect::<Vec<_>>(),
            ),
            string_lists(
                "conditions",
                records.iter().map(|trade| trade.conditions.as_ref()),
            ),
            strings("tape", records.iter().map(|trade| trade.tape.as_deref())),
            strings(
                "taker_side",
                records.iter().map(|trade| trade.taker_side.as_deref()),
            ),
        ]
    }
}

impl FrameRows for ForexRate {
    /// Not `symbol`: these are keyed by pair, and calling the column `symbol`
    /// would make a frame of rates look joinable with one of bars.
    const KEY: &'static str = "currency_pair";

    fn key(&self) -> &str {
        &self.currency_pair
    }

    fn columns(records: &[&Self]) -> Vec<Column> {
        vec![
            timestamps("timestamp", records.iter().map(|rate| rate.timestamp)),
            floats("bid_price", records.iter().map(|rate| rate.bid_price)),
            floats("mid_price", records.iter().map(|rate| rate.mid_price)),
            floats("ask_price", records.iter().map(|rate| rate.ask_price)),
        ]
    }
}

/// The one nested record in the set: a day carries two lists of prints rather
/// than being a row itself.
///
/// It flattens to one row per print, with the session — `opening` or `closing` —
/// as a column, which is the shape that lets a caller filter one out. The
/// alternative, a frame of two list columns, would have made every useful
/// question require an explode first.
impl FrameRows for DailyAuctions {
    const KEY: &'static str = "symbol";

    fn key(&self) -> &str {
        &self.symbol
    }

    fn rows(&self) -> usize {
        self.opening.len() + self.closing.len()
    }

    fn columns(records: &[&Self]) -> Vec<Column> {
        let prints = || {
            records.iter().flat_map(|day| {
                let opening = day.opening.iter().map(move |print| (day, "opening", print));
                let closing = day.closing.iter().map(move |print| (day, "closing", print));
                opening.chain(closing)
            })
        };

        let days: Vec<Option<i32>> = prints()
            .map(|(day, _, _)| {
                i32::try_from(
                    day.date
                        .signed_duration_since(chrono::NaiveDate::default())
                        .num_days(),
                )
                .ok()
            })
            .collect();

        vec![
            Int32Chunked::from_iter_options(name("date"), days.into_iter())
                .into_date()
                .into_column(),
            Column::new(
                name("session"),
                prints().map(|(_, session, _)| session).collect::<Vec<_>>(),
            ),
            timestamps("timestamp", prints().map(|(_, _, print)| print.timestamp)),
            Column::new(
                name("exchange"),
                prints()
                    .map(|(_, _, print)| print.exchange.as_str())
                    .collect::<Vec<_>>(),
            ),
            floats("price", prints().map(|(_, _, print)| print.price)),
            Column::new(
                name("size"),
                prints().map(|(_, _, print)| print.size).collect::<Vec<_>>(),
            ),
            Column::new(
                name("condition"),
                prints()
                    .map(|(_, _, print)| print.condition.as_str())
                    .collect::<Vec<_>>(),
            ),
        ]
    }
}
