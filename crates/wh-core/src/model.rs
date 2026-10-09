use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub type Id = Uuid;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ValidationError {
    #[error("address is required")]
    MissingAddress,
    #[error("port must be between 1 and 65535")]
    InvalidPort,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Host {
    pub id: Id,
    pub label: String,
    pub address: String,
    pub group: Option<Id>,
    pub tags: Vec<String>,
    pub settings: ConnSettings,
}

/// Connection settings shared by hosts and groups. `None` means "inherit".
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ConnSettings {
    pub port: Option<u16>,
    pub username: Option<String>,
    pub identity: Option<Id>,
    pub jump_chain: Option<Vec<Id>>,
    pub env: BTreeMap<String, String>,
    pub startup_snippet: Option<Id>,
    pub agent_forwarding: Option<bool>,
}

/// Reusable credentials. The password itself is a vault secret, not stored here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Identity {
    pub id: Id,
    pub label: String,
    pub username: Option<String>,
    pub key: Option<Id>,
}

/// Reusable command or script. `{{name}}` placeholders are filled in at run time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snippet {
    pub id: Id,
    pub label: String,
    pub script: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ForwardKind {
    /// `-L bind:port:dest:port`
    Local,
    /// `-R bind:port:dest:port`
    Remote,
    /// `-D bind:port` (SOCKS5)
    Dynamic,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PortForward {
    pub id: Id,
    pub label: String,
    pub host: Id,
    pub kind: ForwardKind,
    pub bind_address: String,
    pub bind_port: u16,
    /// Unused for `Dynamic`.
    pub dest_host: String,
    pub dest_port: u16,
}

/// Public half of a keychain entry. The private key is a vault secret keyed by `id`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SshKey {
    pub id: Id,
    pub label: String,
    pub public_key: String,
    pub fingerprint: String,
    pub certificate: Option<String>,
}

/// A trusted host key. `pattern` uses known_hosts syntax (plain or hashed).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KnownHost {
    pub id: Id,
    pub pattern: String,
    /// OpenSSH public key: `algorithm base64`.
    pub key: String,
}

pub const DEFAULT_SSH_PORT: u16 = 22;

impl Host {
    pub fn new(label: &str, address: &str) -> Result<Self, ValidationError> {
        let address = address.trim();
        if address.is_empty() {
            return Err(ValidationError::MissingAddress);
        }
        Ok(Self {
            id: Uuid::new_v4(),
            label: label.trim().to_string(),
            address: address.to_string(),
            group: None,
            tags: Vec::new(),
            settings: ConnSettings::default(),
        })
    }

    pub fn port(&self) -> u16 {
        self.settings.port.unwrap_or(DEFAULT_SSH_PORT)
    }
}

pub fn validate_port(port: u32) -> Result<u16, ValidationError> {
    match u16::try_from(port) {
        Ok(p) if p != 0 => Ok(p),
        _ => Err(ValidationError::InvalidPort),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_requires_address() {
        assert_eq!(Host::new("web", "  "), Err(ValidationError::MissingAddress));
        assert_eq!(Host::new("web", "10.0.0.1").unwrap().address, "10.0.0.1");
    }

    #[test]
    fn port_defaults_to_22() {
        assert_eq!(Host::new("web", "10.0.0.1").unwrap().port(), 22);
    }

    #[test]
    fn rejects_port_0_and_65536() {
        assert_eq!(validate_port(0), Err(ValidationError::InvalidPort));
        assert_eq!(validate_port(65536), Err(ValidationError::InvalidPort));
        assert_eq!(validate_port(2222), Ok(2222));
    }
}
