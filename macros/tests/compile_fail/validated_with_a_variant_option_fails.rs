//! And on an enum variant, which is where a document-shaped request would put
//! it. `UploadDocumentRequest` is an enum whose variants each carry a different
//! payload, so a per-variant rule is the shape someone would try next after the
//! per-field one.

use alpaca_sdk_macros::Validated;

trait Validated {
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Validated)]
pub enum Request {
    #[validated(nested)]
    Document(String),
    W8Ben(String),
}

fn main() {}
