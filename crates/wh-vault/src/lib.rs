//! Encrypted local vault for Wormhole.

pub mod admin;
pub mod crypto;
pub mod records;
pub mod vault;

pub use records::Record;
pub use vault::{Result, Vault, VaultError};
