//! The attribute is refused on a *field*, not only on the item.
//!
//! This is the position someone would actually reach for. `#[validated(nested)]`
//! on a field holding a request of its own — walking `Contact` inside
//! `CreateAccountRequest` — is the obvious next feature to ask for, and it is a
//! real capability a derive could offer. Until it does, being ignored there
//! would be the worst outcome: the caller believes their nested types are
//! checked, and nothing says otherwise.

use alpaca_sdk_macros::Validated;

trait Validated {
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}

pub struct Contact;

#[derive(Validated)]
pub struct Request {
    #[validated(nested)]
    pub contact: Contact,
}

fn main() {}
