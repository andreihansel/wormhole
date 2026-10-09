use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use serde::Serialize;
use wh_vault::crypto::KdfParams;
use tokio::sync::oneshot;
use wh_session::{SessionEvent, SessionId, SessionManager};
use wh_ssh::client::Connection;
use wh_vault::Vault;

use crate::error::ApiError;

pub type ApiResult<T> = Result<T, ApiError>;

/// Pushed to the UI as Tauri events.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AppEvent {
    SessionOutput { session: uuid::Uuid, data: Vec<u8> },
    SessionClosed { session: uuid::Uuid, reason: wh_session::CloseReason },
    /// An unknown host key needs the user's decision (`host_key_respond`).
    HostKeyPrompt { prompt: uuid::Uuid, host: String, port: u16, algorithm: String, fingerprint: String },
    /// A known host presented a different key; the connection was refused.
    HostKeyChanged { host: String, port: u16, fingerprint: String },
    TransferProgress { transfer: uuid::Uuid, done: u64, total: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct VaultStatus {
    pub initialized: bool,
    pub unlocked: bool,
}

pub type EventSink = Arc<dyn Fn(AppEvent) + Send + Sync>;

/// Cheap to clone; all clones share state.
#[derive(Clone)]
pub struct App {
    pub(crate) inner: Arc<Inner>,
}

pub(crate) struct Inner {
    vault: Mutex<Vault>,
    pub(crate) kdf: KdfParams,
    pub(crate) data_dir: PathBuf,
    pub(crate) settings: Mutex<crate::admin::Settings>,
    pub(crate) events: EventSink,
    pub(crate) sessions: SessionManager,
    /// SSH connections kept alive by their sessions.
    pub(crate) session_conns: SessionConns,
    /// Pending host key decisions, answered by `host_key_respond`.
    pub(crate) prompts: Mutex<HashMap<uuid::Uuid, oneshot::Sender<bool>>>,
    /// Running forward rules, each holding its own connection.
    pub(crate) forwards: tokio::sync::Mutex<HashMap<wh_core::model::Id, (wh_ssh::forward::ForwardHandle, Connection)>>,
    pub(crate) sftps: tokio::sync::Mutex<HashMap<uuid::Uuid, Arc<(wh_ssh::sftp::Sftp, Connection)>>>,
}

pub(crate) type SessionConns = Arc<Mutex<HashMap<SessionId, Arc<Connection>>>>;

impl App {
    /// `data_dir` holds the vault database and session logs.
    pub fn new(data_dir: PathBuf, kdf: KdfParams, events: EventSink) -> ApiResult<Self> {
        std::fs::create_dir_all(&data_dir).map_err(|e| ApiError::new("io", e.to_string()))?;
        let vault = Vault::open(&data_dir.join("vault.db"))?;
        let settings = crate::admin::load_settings(&data_dir);
        let session_conns = SessionConns::default();
        let (sink, conns) = (events.clone(), session_conns.clone());
        let sessions = SessionManager::new(Arc::new(move |e| match e {
            SessionEvent::Output { id, data } => sink(AppEvent::SessionOutput { session: id, data }),
            SessionEvent::Closed { id, reason } => {
                conns.lock().expect("conns lock").remove(&id);
                sink(AppEvent::SessionClosed { session: id, reason });
            }
        }));
        Ok(Self {
            inner: Arc::new(Inner {
                vault: Mutex::new(vault),
                kdf,
                data_dir,
                settings: Mutex::new(settings),
                events,
                sessions,
                session_conns,
                prompts: Mutex::default(),
                forwards: Default::default(),
                sftps: Default::default(),
            }),
        })
    }

    #[doc(hidden)]
    pub fn vault(&self) -> MutexGuard<'_, Vault> {
        self.inner.vault.lock().expect("vault lock poisoned")
    }

    pub fn data_dir(&self) -> &std::path::Path {
        &self.inner.data_dir
    }

    pub fn vault_status(&self) -> VaultStatus {
        let v = self.vault();
        VaultStatus { initialized: v.is_initialized().unwrap_or(false), unlocked: v.is_unlocked() }
    }

    pub fn vault_create(&self, password: &str) -> ApiResult<()> {
        if password.is_empty() {
            return Err(ApiError::new("validation", "master password cannot be empty"));
        }
        Ok(self.vault().initialize(password, self.inner.kdf)?)
    }

    pub fn vault_unlock(&self, password: &str) -> ApiResult<()> {
        Ok(self.vault().unlock(password)?)
    }

    pub fn vault_lock(&self) {
        self.vault().lock();
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::Mutex;

    pub(crate) const FAST_KDF: KdfParams = KdfParams { mem_kib: 256, iterations: 1, parallelism: 1 };

    pub(crate) fn app(dir: &tempfile::TempDir) -> (App, Arc<Mutex<Vec<AppEvent>>>) {
        let events: Arc<Mutex<Vec<AppEvent>>> = Arc::default();
        let sink = events.clone();
        let app = App::new(dir.path().to_path_buf(), FAST_KDF, Arc::new(move |e| sink.lock().unwrap().push(e))).unwrap();
        (app, events)
    }

    pub(crate) fn unlocked(dir: &tempfile::TempDir) -> App {
        let (a, _) = app(dir);
        a.vault_create("master").unwrap();
        a
    }

    #[test]
    fn fresh_app_has_uninitialized_vault() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(app(&dir).0.vault_status(), VaultStatus { initialized: false, unlocked: false });
    }

    #[test]
    fn create_lock_unlock_cycle() {
        let dir = tempfile::tempdir().unwrap();
        let a = unlocked(&dir);
        assert_eq!(a.vault_status(), VaultStatus { initialized: true, unlocked: true });
        a.vault_lock();
        assert!(!a.vault_status().unlocked);
        assert_eq!(a.vault_unlock("nope").unwrap_err().kind, "bad_password");
        a.vault_unlock("master").unwrap();
        assert!(a.vault_status().unlocked);
    }

    #[test]
    fn vault_persists_across_app_restarts() {
        let dir = tempfile::tempdir().unwrap();
        drop(unlocked(&dir));
        let (a, _) = app(&dir);
        assert_eq!(a.vault_status(), VaultStatus { initialized: true, unlocked: false });
        a.vault_unlock("master").unwrap();
    }

    #[test]
    fn empty_master_password_rejected() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(app(&dir).0.vault_create("").unwrap_err().kind, "validation");
    }
}
