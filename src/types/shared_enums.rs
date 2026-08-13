//! Enums shared by more than one API surface.
//!
//! They live here so a module can use one without depending on the feature that
//! would otherwise own it: `data` needs `ContractType` and must build with
//! `trading` off. Each is re-exported from the surface it belongs to, so the
//! public path is the one a caller would guess.

use crate::types::wire::wire_enum;
wire_enum! {
    /// Represents the contract type of options
    pub enum ContractType {
        /// `call`
        Call => "call",
        /// `put`
        Put => "put",
    }
}
