//! And on a field *inside* an enum variant, which is the last of the four
//! positions `validated_attribute` walks.
//!
//! Covered because the walk is four `find_map`s and a reader has no way to tell
//! which of them are exercised. Three of the four were not, and a refactor that
//! dropped one would have been silent — the derive would go on accepting an
//! attribute it documents as impossible, in the one position where being
//! ignored is worst.

use alpaca_sdk_macros::Validated;

trait Validated {
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}

pub struct Document;

#[derive(Validated)]
pub enum Request {
    Upload(#[validated(nested)] Document),
    W8Ben(String),
}

fn main() {}
