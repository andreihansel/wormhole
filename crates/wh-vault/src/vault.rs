use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use thiserror::Error;
use zeroize::Zeroizing;

use crate::crypto::{self, CryptoError, KdfParams, Key, KEY_LEN, SALT_LEN};

/// Each entry upgrades the schema by one version (tracked in `PRAGMA user_version`).
const MIGRATIONS: &[&str] = &["
    CREATE TABLE meta (k TEXT PRIMARY KEY, v BLOB NOT NULL);
    CREATE TABLE records (
        kind TEXT NOT NULL,
        id   TEXT NOT NULL,
        data BLOB NOT NULL,   -- sealed JSON, AAD = kind/id
        PRIMARY KEY (kind, id)
    );
"];

const DEK_AAD: &[u8] = b"wormhole/dek/v1";

#[derive(Debug, Error)]
pub enum VaultError {
    #[error("vault is locked")]
    Locked,
    #[error("incorrect password")]
    BadPassword,
    #[error("vault is already initialized")]
    AlreadyInitialized,
    #[error("vault is not initialized")]
    NotInitialized,
    #[error("group still contains hosts or subgroups")]
    GroupNotEmpty,
    #[error("incorrect export password, or the file was modified")]
    ExportDecrypt,
    #[error("not a Wormhole export file")]
    NotAnExport,
    #[error("vault data is corrupt or was tampered with")]
    Corrupt,
    #[error(transparent)]
    Crypto(#[from] CryptoError),
    #[error(transparent)]
    Db(#[from] rusqlite::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, VaultError>;

pub struct Vault {
    conn: Connection,
    dek: Option<Key>,
}

impl Vault {
    /// Open (or create) the database file. The vault starts locked.
    pub fn open(path: &Path) -> Result<Self> {
        let mut conn = Connection::open(path)?;
        migrate(&mut conn)?;
        Ok(Self { conn, dek: None })
    }

    pub fn is_initialized(&self) -> Result<bool> {
        Ok(self.meta("wrapped_dek")?.is_some())
    }

    /// Set the master password on a fresh vault and leave it unlocked.
    pub fn initialize(&mut self, password: &str, params: KdfParams) -> Result<()> {
        if self.is_initialized()? {
            return Err(VaultError::AlreadyInitialized);
        }
        let dek: Key = Zeroizing::new(crypto::random_bytes::<KEY_LEN>());
        self.wrap_dek(&dek, password, params)?;
        self.dek = Some(dek);
        Ok(())
    }

    pub fn unlock(&mut self, password: &str) -> Result<()> {
        self.dek = Some(self.unwrap_dek(password)?);
        Ok(())
    }

    pub(crate) fn unwrap_dek(&self, password: &str) -> Result<Key> {
        let (Some(wrapped), Some(salt), Some(kdf)) =
            (self.meta("wrapped_dek")?, self.meta("salt")?, self.meta("kdf")?)
        else {
            return Err(VaultError::NotInitialized);
        };
        let salt: [u8; SALT_LEN] = salt.try_into().map_err(|_| VaultError::Corrupt)?;
        let params: KdfParams = serde_json::from_slice(&kdf).map_err(|_| VaultError::Corrupt)?;
        if !params.is_sane() {
            return Err(VaultError::Corrupt);
        }
        let kek = crypto::derive_key(password.as_bytes(), &salt, params)?;
        let raw = crypto::open(&kek, &wrapped, DEK_AAD).map_err(|_| VaultError::BadPassword)?;
        let dek: [u8; KEY_LEN] = raw.as_slice().try_into().map_err(|_| VaultError::Corrupt)?;
        Ok(Zeroizing::new(dek))
    }

    /// Drops the data key; `Zeroizing` wipes it from memory.
    pub fn lock(&mut self) {
        self.dek = None;
    }

    pub fn is_unlocked(&self) -> bool {
        self.dek.is_some()
    }

    pub fn schema_version(&self) -> Result<u32> {
        Ok(self.conn.query_row("PRAGMA user_version", [], |r| r.get(0))?)
    }

    pub(crate) fn wrap_dek(&self, dek: &Key, password: &str, params: KdfParams) -> Result<()> {
        let salt = crypto::random_bytes::<SALT_LEN>();
        let kek = crypto::derive_key(password.as_bytes(), &salt, params)?;
        let wrapped = crypto::seal(&kek, dek.as_ref(), DEK_AAD);
        let kdf = serde_json::to_vec(&params).expect("KdfParams serializes");
        let tx = self.conn.unchecked_transaction()?;
        for (k, v) in [("salt", &salt[..]), ("kdf", &kdf[..]), ("wrapped_dek", &wrapped[..])] {
            tx.execute("INSERT OR REPLACE INTO meta (k, v) VALUES (?1, ?2)", params![k, v])?;
        }
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }

    pub(crate) fn dek(&self) -> Result<&Key> {
        self.dek.as_ref().ok_or(VaultError::Locked)
    }

    fn meta(&self, key: &str) -> Result<Option<Vec<u8>>> {
        Ok(self
            .conn
            .query_row("SELECT v FROM meta WHERE k = ?1", [key], |r| r.get(0))
            .optional()?)
    }
}

fn migrate(conn: &mut Connection) -> Result<()> {
    let current: usize = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(current) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", i + 1)?;
        tx.commit()?;
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::crypto::TEST_KDF;

    pub(crate) fn new_vault(dir: &tempfile::TempDir) -> Vault {
        let mut v = Vault::open(&dir.path().join("vault.db")).unwrap();
        v.initialize("correct horse", TEST_KDF).unwrap();
        v
    }

    #[test]
    fn create_vault_then_unlock_with_same_password() {
        let dir = tempfile::tempdir().unwrap();
        drop(new_vault(&dir));
        let mut v = Vault::open(&dir.path().join("vault.db")).unwrap();
        assert!(v.is_initialized().unwrap());
        assert!(!v.is_unlocked());
        v.unlock("correct horse").unwrap();
        assert!(v.is_unlocked());
    }

    #[test]
    fn unlock_with_wrong_password_errors() {
        let dir = tempfile::tempdir().unwrap();
        let mut v = new_vault(&dir);
        v.lock();
        assert!(matches!(v.unlock("wrong"), Err(VaultError::BadPassword)));
        assert!(!v.is_unlocked());
    }

    #[test]
    fn initialize_twice_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let mut v = new_vault(&dir);
        assert!(matches!(v.initialize("x", TEST_KDF), Err(VaultError::AlreadyInitialized)));
    }

    #[test]
    fn unlock_uninitialized_vault_errors() {
        let dir = tempfile::tempdir().unwrap();
        let mut v = Vault::open(&dir.path().join("vault.db")).unwrap();
        assert!(!v.is_initialized().unwrap());
        assert!(matches!(v.unlock("x"), Err(VaultError::NotInitialized)));
    }

    #[test]
    fn schema_migrations_apply_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vault.db");
        let latest = Vault::open(&path).unwrap().schema_version().unwrap();
        assert!(latest >= 1);
        // Re-opening is idempotent.
        assert_eq!(Vault::open(&path).unwrap().schema_version().unwrap(), latest);
    }
}
