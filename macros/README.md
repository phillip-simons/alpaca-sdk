# alpaca-sdk-macros

Derive macros for [`alpaca-sdk`](https://crates.io/crates/alpaca-sdk).

**This crate is not meant to be depended on directly.** It exists because a
procedural macro cannot live in the crate that uses it. `alpaca-sdk` pins it
with `=`, the two are published together, and nothing here is part of a stable
interface on its own.

If you are looking for the SDK, it is [`alpaca-sdk`](https://crates.io/crates/alpaca-sdk).

## What is in it

`#[derive(Setters)]`, which generates one consuming setter per `Option` field on
a request type:

```rust,ignore
#[derive(Debug, Default, Serialize, Setters)]
#[non_exhaustive]
pub struct GetOrdersRequest {
    /// Restricts the response to orders in this status.
    pub status: Option<QueryOrderStatus>,
    /// Restricts the response to orders carrying this subtag.
    #[setters(into)]
    pub subtag: Option<String>,
}

GetOrdersRequest::default()
    .status(QueryOrderStatus::Open)
    .subtag("desk-7");
```

Fields that are not `Option` are left alone — a required field is a constructor
argument. `#[setters(into)]` takes `impl Into<T>`, `#[setters(skip = "why")]`
generates nothing and records why, and `#[setters(doc = "…")]` overrides the
documentation the setter would otherwise inherit from its field.

See the [API documentation](https://docs.rs/alpaca-sdk-macros) for the full
rules. The *convention* governing where each attribute is used — which fields
take `into`, which take no setter and why — lives in `src/types/setters.rs` in
[the repository](https://github.com/phillip-simons/alpaca-sdk). It is a
`pub(crate)` module, so it is deliberately not on docs.rs: it is a rule for
contributors, not a surface for callers.

## Licence

Apache-2.0, the same as `alpaca-sdk`.
