//! Import/export, master password and settings.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use wh_core::import::plan_import;
use wh_core::model::{Host, Id, Identity, SshKey};
use wh_ssh::keys::{self, KeyError};

use crate::error::ApiError;
use crate::state::{ApiResult, App};

const SETTINGS_FILE: &str = "settings.json";

pub(crate) fn load_settings(data_dir: &Path) -> Settings {
    std::fs::read(data_dir.join(SETTINGS_FILE))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// `~/.ssh/x` -> `ssh_dir/x`, `~/x` -> `$HOME/x`, relative -> `ssh_dir/x`.
fn expand(path: &str, ssh_dir: &Path) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/.ssh/") {
        ssh_dir.join(rest)
    } else if let Some(rest) = path.strip_prefix("~/") {
        std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default().join(rest)
    } else {
        ssh_dir.join(path)
    }
}

fn file_name(path: &str) -> String {
    Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.to_string())
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ImportSummary {
    pub created: usize,
    pub updated: usize,
    pub keys_imported: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Record terminal output of new sessions under `<data_dir>/logs`.
    pub session_logs: bool,
    /// Lock the vault after this many idle minutes (0 = never). Enforced by the UI.
    pub auto_lock_minutes: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self { session_logs: false, auto_lock_minutes: 15 }
    }
}

impl App {
    /// Imports hosts from an OpenSSH config (default `~/.ssh/config`), following
    /// `Include`s relative to `ssh_dir`, and unencrypted `IdentityFile` keys.
    pub fn import_ssh_config(&self, config: &Path, ssh_dir: &Path) -> ApiResult<ImportSummary> {
        let text = std::fs::read_to_string(config).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => ApiError::new("not_found", format!("{} not found", config.display())),
            _ => ApiError::new("io", e.to_string()),
        })?;
        let include = |pattern: &str| -> Vec<String> {
            let pattern = expand(pattern, ssh_dir);
            let mut paths: Vec<PathBuf> =
                glob::glob(&pattern.to_string_lossy()).map(|g| g.filter_map(Result::ok).collect()).unwrap_or_default();
            paths.sort();
            paths.iter().filter_map(|p| std::fs::read_to_string(p).ok()).collect()
        };
        let parsed = wh_core::ssh_config::parse(&text, &include);
        let existing: Vec<Host> = self.vault().list()?;
        let mut plan = plan_import(&existing, &parsed.hosts);
        let mut summary = ImportSummary {
            created: plan.created,
            updated: plan.updated,
            keys_imported: 0,
            warnings: parsed.warnings,
        };
        for (host, files) in std::mem::take(&mut plan.identity_files) {
            if let Some(identity) = self.identity_for_files(&files, ssh_dir, &mut summary)? {
                if let Some(h) = plan.hosts.iter_mut().find(|h| h.id == host) {
                    h.settings.identity = Some(identity);
                }
            }
        }
        let v = self.vault();
        for h in &plan.hosts {
            v.put(h)?;
        }
        Ok(summary)
    }

    /// Imports the first usable key among `files` and returns an identity using it.
    fn identity_for_files(&self, files: &[String], ssh_dir: &Path, summary: &mut ImportSummary) -> ApiResult<Option<Id>> {
        for file in files {
            let Ok(text) = std::fs::read_to_string(expand(file, ssh_dir)) else {
                summary.warnings.push(format!("IdentityFile {file} could not be read"));
                continue;
            };
            let material = match keys::import_private(&text, None) {
                Ok(m) => m,
                Err(KeyError::NeedsPassphrase) => {
                    summary.warnings.push(format!(
                        "{file} is protected by a passphrase; import it from the Keychain to use it"
                    ));
                    continue;
                }
                Err(e) => {
                    summary.warnings.push(format!("{file}: {e}"));
                    continue;
                }
            };
            let existing_key = self.vault().list::<SshKey>()?.into_iter().find(|k| k.fingerprint == material.fingerprint);
            let key = match existing_key {
                Some(k) => k,
                None => {
                    summary.keys_imported += 1;
                    self.store_key(&file_name(file), material)?
                }
            };
            // Reuse a username-less identity for this key so hosts keep their own users.
            let existing_identity =
                self.vault().list::<Identity>()?.into_iter().find(|i| i.key == Some(key.id) && i.username.is_none());
            let identity = match existing_identity {
                Some(i) => i,
                None => self.save_identity(Identity { id: uuid::Uuid::new_v4(), label: key.label.clone(), username: None, key: Some(key.id) })?,
            };
            return Ok(Some(identity.id));
        }
        Ok(None)
    }

    pub fn vault_change_password(&self, old: &str, new: &str) -> ApiResult<()> {
        if new.is_empty() {
            return Err(ApiError::new("validation", "master password cannot be empty"));
        }
        Ok(self.vault().change_password(old, new, self.inner.kdf)?)
    }

    pub fn vault_export(&self, path: &str, password: &str) -> ApiResult<()> {
        if password.is_empty() {
            return Err(ApiError::new("validation", "export password cannot be empty"));
        }
        Ok(self.vault().export_to(Path::new(path), password, self.inner.kdf)?)
    }

    /// Returns the number of records imported.
    pub fn vault_import(&self, path: &str, password: &str) -> ApiResult<usize> {
        Ok(self.vault().import_from(Path::new(path), password)?.records)
    }

    pub fn settings(&self) -> Settings {
        self.inner.settings.lock().expect("settings lock").clone()
    }

    pub fn save_settings(&self, settings: Settings) -> ApiResult<()> {
        let json = serde_json::to_vec_pretty(&settings).expect("settings serialize");
        std::fs::write(self.inner.data_dir.join(SETTINGS_FILE), json).map_err(|e| ApiError::new("io", e.to_string()))?;
        *self.inner.settings.lock().expect("settings lock") = settings;
        Ok(())
    }

    /// Where a new session's log goes, or `None` when logging is off.
    pub(crate) fn session_log_path(&self, label: &str) -> Option<PathBuf> {
        if !self.settings().session_logs {
            return None;
        }
        let dir = self.inner.data_dir.join("logs");
        std::fs::create_dir_all(&dir).ok()?;
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let safe: String = label.chars().map(|c| if c.is_ascii_alphanumeric() || "-_.".contains(c) { c } else { '_' }).collect();
        Some(dir.join(format!("{stamp}-{safe}.log")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::tests::{app, unlocked};
    use wh_core::model::Host;

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!("{}/../../tests/fixtures/keys/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
    }

    /// A fake ~/.ssh with an Include, a plain key and a passphrase-protected key.
    fn ssh_dir() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir(d.path().join("config.d")).unwrap();
        std::fs::write(d.path().join("id_plain"), fixture("id_ed25519")).unwrap();
        std::fs::write(d.path().join("id_locked"), fixture("id_ed25519_enc")).unwrap();
        std::fs::write(
            d.path().join("config"),
            "Include config.d/*\nHost web\n  HostName 10.0.0.1\n  User deploy\n  IdentityFile ~/.ssh/id_plain\n\
             Host db\n  HostName 10.0.0.2\n  User deploy\n  IdentityFile ~/.ssh/id_plain\n\
             Host vault\n  IdentityFile ~/.ssh/id_locked\n  ProxyCommand nc %h %p\n",
        )
        .unwrap();
        std::fs::write(d.path().join("config.d/extra"), "Host extra\n  HostName 10.0.0.9\n").unwrap();
        d
    }

    #[test]
    fn import_ssh_config_creates_hosts_follows_includes_and_imports_keys() {
        let dir = tempfile::tempdir().unwrap();
        let a = unlocked(&dir);
        let ssh = ssh_dir();
        let summary = a.import_ssh_config(&ssh.path().join("config"), ssh.path()).unwrap();
        assert_eq!((summary.created, summary.updated, summary.keys_imported), (4, 0, 1));
        assert!(summary.warnings.iter().any(|w| w.contains("ProxyCommand")));
        assert!(summary.warnings.iter().any(|w| w.contains("id_locked") && w.contains("passphrase")));
        let inv = a.inventory().unwrap();
        let mut labels: Vec<_> = inv.hosts.iter().map(|h| h.label.as_str()).collect();
        labels.sort();
        assert_eq!(labels, ["db", "extra", "vault", "web"]);
        // Both hosts using id_plain share one key and one identity.
        assert_eq!(inv.keys.len(), 1);
        let ident = |label: &str| inv.hosts.iter().find(|h| h.label == label).unwrap().settings.identity;
        assert!(ident("web").is_some());
        assert_eq!(ident("web"), ident("db"));
        let web = inv.hosts.iter().find(|h| h.label == "web").unwrap().id;
        let r = a.resolve_host(web).unwrap();
        assert_eq!((r.username.as_deref(), r.key), (Some("deploy"), Some(inv.keys[0].id)));
    }

    #[test]
    fn reimporting_ssh_config_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let a = unlocked(&dir);
        let ssh = ssh_dir();
        a.import_ssh_config(&ssh.path().join("config"), ssh.path()).unwrap();
        let again = a.import_ssh_config(&ssh.path().join("config"), ssh.path()).unwrap();
        assert_eq!((again.created, again.updated, again.keys_imported), (0, 4, 0));
        let inv = a.inventory().unwrap();
        assert_eq!((inv.hosts.len(), inv.keys.len(), inv.identities.len()), (4, 1, 1));
    }

    #[test]
    fn missing_config_is_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let a = unlocked(&dir);
        let err = a.import_ssh_config(Path::new("/nope/config"), Path::new("/nope")).unwrap_err();
        assert_eq!(err.kind, "not_found");
    }

    #[test]
    fn export_import_and_change_password() {
        let dir = tempfile::tempdir().unwrap();
        let a = unlocked(&dir);
        a.save_host(Host::new("web", "1").unwrap()).unwrap();
        let file = dir.path().join("backup.whx");
        a.vault_export(file.to_str().unwrap(), "export-pw").unwrap();

        let other_dir = tempfile::tempdir().unwrap();
        let b = unlocked(&other_dir);
        assert_eq!(b.vault_import(file.to_str().unwrap(), "wrong").unwrap_err().kind, "bad_password");
        assert_eq!(b.vault_import(file.to_str().unwrap(), "export-pw").unwrap(), 1);
        assert_eq!(b.inventory().unwrap().hosts.len(), 1);

        assert_eq!(a.vault_change_password("wrong", "x").unwrap_err().kind, "bad_password");
        assert_eq!(a.vault_change_password("master", "").unwrap_err().kind, "validation");
        a.vault_change_password("master", "new-master").unwrap();
        a.vault_lock();
        a.vault_unlock("new-master").unwrap();
    }

    #[test]
    fn settings_persist_and_control_session_logs() {
        let dir = tempfile::tempdir().unwrap();
        let (a, _) = app(&dir);
        assert_eq!(a.settings(), Settings::default());
        assert_eq!(a.session_log_path("web"), None);
        a.save_settings(Settings { session_logs: true, auto_lock_minutes: 5 }).unwrap();
        let (reopened, _) = app(&dir);
        assert_eq!(reopened.settings(), Settings { session_logs: true, auto_lock_minutes: 5 });
        let path = reopened.session_log_path("web/prod 1").unwrap();
        assert!(path.starts_with(dir.path().join("logs")));
        assert!(path.file_name().unwrap().to_string_lossy().ends_with("-web_prod_1.log"));
    }
}
