use serde::Serialize;

/// Errors as the UI sees them: `{ "kind": "...", "message": "..." }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ApiError {
    pub kind: &'static str,
    pub message: String,
}

impl ApiError {
    pub fn new(kind: &'static str, message: impl Into<String>) -> Self {
        Self { kind, message: message.into() }
    }
}

impl From<wh_vault::VaultError> for ApiError {
    fn from(e: wh_vault::VaultError) -> Self {
        use wh_vault::VaultError as V;
        let kind = match &e {
            V::Locked => "locked",
            V::BadPassword | V::ExportDecrypt => "bad_password",
            V::AlreadyInitialized => "already_initialized",
            V::NotInitialized => "not_initialized",
            V::GroupNotEmpty => "group_not_empty",
            V::NotAnExport => "invalid_file",
            V::Io(_) => "io",
            V::Corrupt | V::Crypto(_) | V::Db(_) => "internal",
        };
        Self::new(kind, e.to_string())
    }
}

impl From<wh_ssh::client::ConnectError> for ApiError {
    fn from(e: wh_ssh::client::ConnectError) -> Self {
        use wh_ssh::client::ConnectError as C;
        let kind = match &e {
            C::Timeout(_) => "timeout",
            C::Network { .. } => "network",
            C::HostKeyRejected(_) => "host_key_rejected",
            C::AuthFailed(_) => "auth_failed",
            C::Ssh(_) => "ssh",
        };
        Self::new(kind, e.to_string())
    }
}

impl From<wh_session::manager::SessionError> for ApiError {
    fn from(e: wh_session::manager::SessionError) -> Self {
        Self::new("not_found", e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wh_vault::VaultError;

    #[test]
    fn errors_serialize_as_kind_and_message() {
        let json = serde_json::to_value(ApiError::from(VaultError::BadPassword)).unwrap();
        assert_eq!(json, serde_json::json!({"kind": "bad_password", "message": "incorrect password"}));
    }

    #[test]
    fn vault_errors_map_to_stable_kinds() {
        assert_eq!(ApiError::from(VaultError::Locked).kind, "locked");
        assert_eq!(ApiError::from(VaultError::GroupNotEmpty).kind, "group_not_empty");
        assert_eq!(ApiError::from(VaultError::ExportDecrypt).kind, "bad_password");
        assert_eq!(ApiError::from(VaultError::NotAnExport).kind, "invalid_file");
        assert_eq!(ApiError::from(VaultError::Corrupt).kind, "internal");
    }
}
