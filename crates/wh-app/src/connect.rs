//! Connecting to hosts and running terminal sessions.

use std::sync::Arc;
use std::time::Duration;

use russh::keys::{Certificate, HashAlg, PublicKey};
use tokio::sync::oneshot;
use wh_core::inventory::Inventory;
use wh_core::model::{Id, KnownHost, Snippet, SshKey};
use wh_core::resolve::Resolved;
use wh_session::{backends, SessionId, SessionOptions};
use wh_ssh::client::{
    connect, BoxFuture, ConnectOptions, Connection, Credentials, Endpoint, HostKeyPolicy, ShellRequest,
};
use wh_ssh::keys;
use wh_ssh::known_hosts::{self, HostKeyStatus, KnownHostEntry};
use zeroize::Zeroizing;

use crate::error::ApiError;
use crate::state::{ApiResult, App, AppEvent};

/// How long an unanswered host key prompt waits before rejecting.
const PROMPT_TIMEOUT: Duration = Duration::from_secs(300);

struct VaultHostKeys(App);

impl HostKeyPolicy for VaultHostKeys {
    fn verify(&self, host: String, port: u16, key: PublicKey) -> BoxFuture<bool> {
        let app = self.0.clone();
        Box::pin(async move { app.check_host_key(host, port, key).await })
    }
}

/// `algorithm base64`, without the comment.
fn openssh_key(key: &PublicKey) -> String {
    key.to_openssh().unwrap_or_default().split_whitespace().take(2).collect::<Vec<_>>().join(" ")
}

impl App {
    async fn check_host_key(&self, host: String, port: u16, key: PublicKey) -> bool {
        let Ok(known) = self.vault().list::<KnownHost>() else { return false };
        let entries: Vec<KnownHostEntry> =
            known.into_iter().map(|k| KnownHostEntry { pattern: k.pattern, key: k.key }).collect();
        let fingerprint = key.fingerprint(HashAlg::Sha256).to_string();
        match known_hosts::check(&entries, &host, port, &key) {
            HostKeyStatus::Known => true,
            HostKeyStatus::Changed => {
                (self.inner.events)(AppEvent::HostKeyChanged { host, port, fingerprint });
                false
            }
            HostKeyStatus::Unknown => {
                let prompt = uuid::Uuid::new_v4();
                let (tx, rx) = oneshot::channel();
                self.inner.prompts.lock().expect("prompts lock").insert(prompt, tx);
                let algorithm = key.algorithm().to_string();
                (self.inner.events)(AppEvent::HostKeyPrompt { prompt, host: host.clone(), port, algorithm, fingerprint });
                let accepted = matches!(tokio::time::timeout(PROMPT_TIMEOUT, rx).await, Ok(Ok(true)));
                self.inner.prompts.lock().expect("prompts lock").remove(&prompt);
                if accepted {
                    let entry = (known_hosts::host_pattern(&host, port), openssh_key(&key));
                    return self.vault().add_known_hosts([entry]).is_ok();
                }
                false
            }
        }
    }

    /// Connection parameters for one host, with credentials from the vault.
    fn endpoint(&self, inv: &Inventory, host: Id) -> ApiResult<(Endpoint, Resolved)> {
        let r = wh_core::resolve::resolve(inv, host).ok_or_else(|| ApiError::new("not_found", "host not found"))?;
        let label = &inv.hosts[&host].label;
        let username = r
            .username
            .clone()
            .ok_or_else(|| ApiError::new("validation", format!("no username set for {label}")))?;
        let v = self.vault();
        // An identity's password takes precedence over one stored on the host.
        let mut password = None;
        for owner in r.identity.into_iter().chain([host]) {
            if let Some(secret) = v.get_secret(owner)? {
                password = Some(Zeroizing::new(String::from_utf8_lossy(&secret).into_owned()));
                break;
            }
        }
        let (mut key, mut certificate) = (None, None);
        if let Some(key_id) = r.key {
            let pem = v.get_secret(key_id)?.ok_or_else(|| ApiError::new("not_found", "private key missing"))?;
            key = Some(Arc::new(keys::load_stored(&String::from_utf8_lossy(&pem))?));
            certificate = v
                .get::<SshKey>(key_id)?
                .and_then(|k| k.certificate)
                .and_then(|c| Certificate::from_openssh(c.trim()).ok());
        }
        let credentials = Credentials { username, password, key, certificate };
        Ok((Endpoint { host: r.address.clone(), port: r.port, credentials }, r))
    }

    /// Connects to `host` through its (inherited) jump chain.
    pub(crate) async fn connect_host(&self, host: Id) -> ApiResult<(Connection, Resolved)> {
        let inv = self.load_inventory()?;
        let (target, resolved) = self.endpoint(&inv, host)?;
        let jumps = resolved
            .jump_chain
            .iter()
            .map(|&j| self.endpoint(&inv, j).map(|(ep, _)| ep))
            .collect::<ApiResult<Vec<_>>>()?;
        let policy = Arc::new(VaultHostKeys(self.clone()));
        let conn = connect(&jumps, &target, policy, &ConnectOptions::default()).await?;
        Ok((conn, resolved))
    }

    /// Connects (through any jump hosts) and opens a PTY shell.
    pub async fn open_ssh_session(&self, host: Id, cols: u32, rows: u32) -> ApiResult<SessionId> {
        let (conn, resolved) = self.connect_host(host).await?;
        let label = self.vault().get::<wh_core::model::Host>(host)?.map(|h| h.label).unwrap_or_default();
        let log_path = self.session_log_path(&label);
        let startup = match resolved.startup_snippet {
            Some(id) => self.vault().get::<Snippet>(id)?.map(|s| s.script),
            None => None,
        };
        let env = resolved.env.into_iter().collect();
        let shell = conn.open_shell(ShellRequest { cols, rows, env }).await?;
        let id = self
            .inner
            .sessions
            .spawn(backends::ssh(shell), SessionOptions { startup_command: startup, log_path });
        let mut conns = self.inner.session_conns.lock().expect("conns lock");
        conns.insert(id, Arc::new(conn));
        // The session may already have ended before we registered its connection.
        if !self.inner.sessions.ids().contains(&id) {
            conns.remove(&id);
        }
        Ok(id)
    }

    pub fn open_local_session(&self, cols: u16, rows: u16) -> ApiResult<SessionId> {
        let io = backends::local(None, cols, rows).map_err(|e| ApiError::new("io", e.to_string()))?;
        let opts = SessionOptions { log_path: self.session_log_path("local"), ..Default::default() };
        Ok(self.inner.sessions.spawn(io, opts))
    }

    pub fn known_hosts(&self) -> ApiResult<Vec<KnownHost>> {
        Ok(self.vault().list()?)
    }

    pub fn delete_known_host(&self, id: Id) -> ApiResult<()> {
        if self.vault().delete::<KnownHost>(id)? {
            Ok(())
        } else {
            Err(ApiError::new("not_found", "known host not found"))
        }
    }

    /// Trust the entries of an OpenSSH known_hosts file. Returns how many were new.
    pub fn import_known_hosts(&self, text: &str) -> ApiResult<usize> {
        let entries = known_hosts::parse(text).into_iter().map(|e| (e.pattern, e.key));
        Ok(self.vault().add_known_hosts(entries)?)
    }

    /// Answer a `HostKeyPrompt` event.
    pub fn host_key_respond(&self, prompt: uuid::Uuid, accept: bool) -> ApiResult<()> {
        let tx = self
            .inner
            .prompts
            .lock()
            .expect("prompts lock")
            .remove(&prompt)
            .ok_or_else(|| ApiError::new("not_found", "no such prompt"))?;
        let _ = tx.send(accept);
        Ok(())
    }

    pub fn session_write(&self, id: SessionId, data: &[u8]) -> ApiResult<()> {
        Ok(self.inner.sessions.write(id, data)?)
    }

    pub fn session_resize(&self, id: SessionId, cols: u32, rows: u32) -> ApiResult<()> {
        Ok(self.inner.sessions.resize(id, cols, rows)?)
    }

    pub fn session_close(&self, id: SessionId) -> ApiResult<()> {
        Ok(self.inner.sessions.close(id)?)
    }

    /// Returns how many sessions received the input.
    pub fn session_broadcast(&self, ids: &[SessionId], data: &[u8]) -> usize {
        self.inner.sessions.broadcast(ids, data)
    }
}
