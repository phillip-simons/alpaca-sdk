//! The `wire_enum!` macro behind every string enum in this crate.

/// Defines a string-valued enum with a catch-all `Unknown` variant.
///
/// Alpaca introduces new enum values without a version bump, and an SDK that
/// models them as a closed set rejects the whole payload the first time it meets
/// one — a new order status breaking deserialization in production. That is what
/// alpaca-py's `class X(str, Enum)` does. The generated `Unknown(String)` variant
/// keeps the raw wire value instead, so an unrecognized status is inspectable
/// rather than fatal.
///
/// `Serialize`/`Deserialize` are hand-rolled rather than derived. Derive-based
/// catch-alls (`#[serde(other)]`, variant-level `#[serde(untagged)]`) rely on
/// content buffering that behaves differently across formats and, in the case of
/// `other`, discard the unknown string. A plain string visitor behaves identically
/// under `serde_json` and `rmp-serde`, and the live market data stream is msgpack.
///
/// ```ignore
/// wire_enum! {
///     /// Which side of the market an order is on.
///     pub enum OrderSide {
///         /// Buy.
///         Buy => "buy",
///         /// Sell.
///         Sell => "sell",
///     }
/// }
/// ```
macro_rules! wire_enum {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $(
                $(#[$variant_meta:meta])*
                $variant:ident => $wire:literal
            ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        #[non_exhaustive]
        $vis enum $name {
            $(
                $(#[$variant_meta])*
                $variant,
            )+
            /// A value this version of the SDK does not recognize.
            ///
            /// Holds the raw wire string so it can be logged or matched on
            /// without waiting for a crate release.
            Unknown(::std::string::String),
        }

        impl $name {
            /// Every wire value this type recognizes, excluding `Unknown`.
            pub const WIRE_VALUES: &'static [&'static str] = &[$($wire),+];

            /// The value as it appears on the wire.
            #[must_use]
            pub fn as_str(&self) -> &str {
                match self {
                    $(Self::$variant => $wire,)+
                    Self::Unknown(value) => value.as_str(),
                }
            }

            /// Whether this value is one the SDK does not recognize.
            #[must_use]
            pub fn is_unknown(&self) -> bool {
                ::std::matches!(self, Self::Unknown(_))
            }
        }

        impl ::std::convert::From<&str> for $name {
            fn from(value: &str) -> Self {
                match value {
                    $($wire => Self::$variant,)+
                    other => Self::Unknown(::std::borrow::ToOwned::to_owned(other)),
                }
            }
        }

        impl ::std::convert::From<::std::string::String> for $name {
            fn from(value: ::std::string::String) -> Self {
                match value.as_str() {
                    $($wire => Self::$variant,)+
                    // Reuse the allocation the caller already made.
                    _ => Self::Unknown(value),
                }
            }
        }

        impl ::std::str::FromStr for $name {
            type Err = ::std::convert::Infallible;

            fn from_str(value: &str) -> ::std::result::Result<Self, Self::Err> {
                ::std::result::Result::Ok(Self::from(value))
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl ::serde::Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> ::std::result::Result<S::Ok, S::Error>
            where
                S: ::serde::Serializer,
            {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> ::serde::Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
            where
                D: ::serde::Deserializer<'de>,
            {
                struct WireVisitor;

                impl ::serde::de::Visitor<'_> for WireVisitor {
                    type Value = $name;

                    fn expecting(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                        f.write_str(::std::concat!("a ", ::std::stringify!($name), " string"))
                    }

                    fn visit_str<E>(self, value: &str) -> ::std::result::Result<Self::Value, E>
                    where
                        E: ::serde::de::Error,
                    {
                        ::std::result::Result::Ok(<$name as ::std::convert::From<&str>>::from(value))
                    }

                    // Formats that hand over an owned string let `Unknown` take
                    // the allocation rather than copying it.
                    fn visit_string<E>(
                        self,
                        value: ::std::string::String,
                    ) -> ::std::result::Result<Self::Value, E>
                    where
                        E: ::serde::de::Error,
                    {
                        ::std::result::Result::Ok(
                            <$name as ::std::convert::From<::std::string::String>>::from(value),
                        )
                    }
                }

                deserializer.deserialize_str(WireVisitor)
            }
        }
    };
}

pub(crate) use wire_enum;
