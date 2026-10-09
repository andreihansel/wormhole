//! Typed, encrypted record storage.

use serde::de::DeserializeOwned;
use serde::Serialize;
use wh_core::inventory::Group;
use wh_core::model::{Host, Id, Identity, KnownHost, PortForward, Snippet, SshKey};
use zeroize::Zeroizing;

use crate::crypto;
use crate::vault::{Result, Vault, VaultError};

pub(crate) const SECRET_KIND: &str = "secret";
/// Every kind the vault stores; imports containing anything else are rejected.
pub(crate) const KNOWN_KINDS: &[&str] = &["host", "group", "identity", "snippet", "forward", "key", "known_host", SECRET_KIND];

fn aad(kind: &str, id: Id) -> Vec<u8> {
    format!("{kind}/{id}").into_bytes()
}

/// Anything storable in the vault. `KIND` namespaces ids and is bound into the AAD.
pub trait Record: Serialize + DeserializeOwned {
    const KIND: &'static str;
    fn id(&self) -> Id;
}

macro_rules! record {
    ($t:ty, $kind:literal) => {
        impl Record for $t {
            const KIND: &'static str = $kind;
            fn id(&self) -> Id {
                self.id
            }
        }
    };
}
record!(Host, "host");
record!(Group, "group");
record!(Identity, "identity");
record!(Snippet, "snippet");
record!(PortForward, "forward");
record!(SshKey, "key");
record!(KnownHost, "known_host");

impl Vault {
    pub fn put<T: Record>(&self, rec: &T) -> Result<()> {
        let json = Zeroizing::new(serde_json::to_vec(rec).expect("records serialize"));
        self.put_raw(T::KIND, rec.id(), &json)
    }

    pub fn get<T: Record>(&self, id: Id) -> Result<Option<T>> {
        self.get_raw(T::KIND, id)?.map(|json| decode(&json)).transpose()
    }

    pub fn list<T: Record>(&self) -> Result<Vec<T>> {
        let dek = self.dek()?;
        let mut stmt = self.conn().prepare("SELECT id, data FROM records WHERE kind = ?1 ORDER BY rowid")?;
        let rows = stmt.query_map([T::KIND], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?)))?;
        rows.map(|row| {
            let (id, sealed) = row?;
            let id: Id = id.parse().map_err(|_| VaultError::Corrupt)?;
            let json = crypto::open(dek, &sealed, &aad(T::KIND, id)).map_err(|_| VaultError::Corrupt)?;
            decode(&json)
        })
        .collect()
    }

    /// Returns whether a record was removed.
    pub fn delete<T: Record>(&self, id: Id) -> Result<bool> {
        self.delete_raw(T::KIND, id)
    }

    /// Adds `(pattern, key)` pairs not already trusted. Returns how many were new.
    pub fn add_known_hosts(&self, entries: impl IntoIterator<Item = (String, String)>) -> Result<usize> {
        let mut known: Vec<KnownHost> = self.list()?;
        let mut added = 0;
        for (pattern, key) in entries {
            if known.iter().any(|k| k.pattern == pattern && k.key == key) {
                continue;
            }
            let rec = KnownHost { id: uuid::Uuid::new_v4(), pattern, key };
            self.put(&rec)?;
            known.push(rec);
            added += 1;
        }
        Ok(added)
    }

    /// Secret material (password, private key, passphrase) owned by another record.
    pub fn put_secret(&self, owner: Id, secret: &[u8]) -> Result<()> {
        self.put_raw(SECRET_KIND, owner, secret)
    }

    pub fn get_secret(&self, owner: Id) -> Result<Option<Zeroizing<Vec<u8>>>> {
        self.get_raw(SECRET_KIND, owner)
    }

    pub fn delete_secret(&self, owner: Id) -> Result<bool> {
        self.delete_raw(SECRET_KIND, owner)
    }

    pub(crate) fn put_raw(&self, kind: &str, id: Id, plaintext: &[u8]) -> Result<()> {
        let sealed = crypto::seal(self.dek()?, plaintext, &aad(kind, id));
        self.conn().execute(
            "INSERT OR REPLACE INTO records (kind, id, data) VALUES (?1, ?2, ?3)",
            rusqlite::params![kind, id.to_string(), sealed],
        )?;
        Ok(())
    }

    fn get_raw(&self, kind: &str, id: Id) -> Result<Option<Zeroizing<Vec<u8>>>> {
        let dek = self.dek()?;
        let sealed: Option<Vec<u8>> = rusqlite::OptionalExtension::optional(self.conn().query_row(
            "SELECT data FROM records WHERE kind = ?1 AND id = ?2",
            [kind, &id.to_string()],
            |r| r.get(0),
        ))?;
        sealed
            .map(|s| crypto::open(dek, &s, &aad(kind, id)).map_err(|_| VaultError::Corrupt))
            .transpose()
    }

    fn delete_raw(&self, kind: &str, id: Id) -> Result<bool> {
        self.dek()?;
        let n = self
            .conn()
            .execute("DELETE FROM records WHERE kind = ?1 AND id = ?2", [kind, &id.to_string()])?;
        Ok(n > 0)
    }
}

fn decode<T: Record>(json: &[u8]) -> Result<T> {
    serde_json::from_slice(json).map_err(|_| VaultError::Corrupt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::tests::new_vault;
    use crate::VaultError;
    use std::fmt::Debug;
    use uuid::Uuid;
    use wh_core::model::ForwardKind;

    fn roundtrip<T: Record + PartialEq + Debug + Clone>(rec: T, mutate: impl Fn(&mut T)) {
        let dir = tempfile::tempdir().unwrap();
        let v = new_vault(&dir);
        v.put(&rec).unwrap();
        assert_eq!(v.get::<T>(rec.id()).unwrap(), Some(rec.clone()));
        let mut changed = rec.clone();
        mutate(&mut changed);
        v.put(&changed).unwrap();
        assert_eq!(v.list::<T>().unwrap(), vec![changed]);
        assert!(v.delete::<T>(rec.id()).unwrap());
        assert!(!v.delete::<T>(rec.id()).unwrap());
        assert_eq!(v.get::<T>(rec.id()).unwrap(), None);
    }

    #[test]
    fn crud_hosts() {
        roundtrip(Host::new("web", "10.0.0.1").unwrap(), |h| h.label = "web-2".into());
    }

    #[test]
    fn crud_groups() {
        let g = Group { id: Uuid::new_v4(), name: "prod".into(), parent: None, settings: Default::default() };
        roundtrip(g, |g| g.name = "production".into());
    }

    #[test]
    fn crud_identities() {
        let i = Identity { id: Uuid::new_v4(), label: "ops".into(), username: Some("ops".into()), key: None };
        roundtrip(i, |i| i.username = Some("root".into()));
    }

    #[test]
    fn crud_snippets() {
        let s = Snippet { id: Uuid::new_v4(), label: "uptime".into(), script: "uptime".into() };
        roundtrip(s, |s| s.script = "uptime -p".into());
    }

    #[test]
    fn crud_forwards() {
        let f = PortForward {
            id: Uuid::new_v4(),
            label: "db".into(),
            host: Uuid::new_v4(),
            kind: ForwardKind::Local,
            bind_address: "127.0.0.1".into(),
            bind_port: 15432,
            dest_host: "localhost".into(),
            dest_port: 5432,
        };
        roundtrip(f, |f| f.bind_port = 15433);
    }

    #[test]
    fn crud_keys() {
        let k = SshKey {
            id: Uuid::new_v4(),
            label: "laptop".into(),
            public_key: "ssh-ed25519 AAAA".into(),
            fingerprint: "SHA256:x".into(),
            certificate: None,
        };
        roundtrip(k, |k| k.label = "desktop".into());
    }

    #[test]
    fn add_known_hosts_skips_duplicates() {
        let dir = tempfile::tempdir().unwrap();
        let v = new_vault(&dir);
        let entries = || {
            vec![
                ("example.com".to_string(), "ssh-ed25519 AAAA1".to_string()),
                ("[example.com]:2222".to_string(), "ssh-ed25519 AAAA1".to_string()),
                ("example.com".to_string(), "ssh-ed25519 AAAA1".to_string()),
            ]
        };
        assert_eq!(v.add_known_hosts(entries()).unwrap(), 2);
        assert_eq!(v.add_known_hosts(entries()).unwrap(), 0, "re-import is idempotent");
        let mut got: Vec<_> = v.list::<KnownHost>().unwrap().into_iter().map(|k| k.pattern).collect();
        got.sort();
        assert_eq!(got, ["[example.com]:2222", "example.com"]);
    }

    #[test]
    fn kinds_are_namespaced() {
        let dir = tempfile::tempdir().unwrap();
        let v = new_vault(&dir);
        let h = Host::new("web", "10.0.0.1").unwrap();
        v.put(&h).unwrap();
        assert_eq!(v.get::<Snippet>(h.id).unwrap(), None);
    }

    #[test]
    fn secrets_roundtrip_and_delete() {
        let dir = tempfile::tempdir().unwrap();
        let v = new_vault(&dir);
        let owner = Uuid::new_v4();
        v.put_secret(owner, b"s3cret").unwrap();
        assert_eq!(v.get_secret(owner).unwrap().unwrap().as_slice(), b"s3cret");
        assert!(v.delete_secret(owner).unwrap());
        assert!(v.get_secret(owner).unwrap().is_none());
    }

    #[test]
    fn lock_clears_key_material() {
        let dir = tempfile::tempdir().unwrap();
        let mut v = new_vault(&dir);
        let h = Host::new("web", "10.0.0.1").unwrap();
        v.put(&h).unwrap();
        v.lock();
        assert!(matches!(v.get::<Host>(h.id), Err(VaultError::Locked)));
        assert!(matches!(v.list::<Host>(), Err(VaultError::Locked)));
        assert!(matches!(v.put(&h), Err(VaultError::Locked)));
        assert!(matches!(v.get_secret(h.id), Err(VaultError::Locked)));
    }

    #[test]
    fn nothing_plaintext_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vault.db");
        {
            let v = new_vault(&dir);
            let h = Host::new("needle-label", "needle-address.example").unwrap();
            v.put(&h).unwrap();
            v.put_secret(h.id, b"needle-password").unwrap();
        }
        let raw = std::fs::read(&path).unwrap();
        for needle in ["needle-label", "needle-address", "needle-password", "correct horse"] {
            assert!(
                !raw.windows(needle.len()).any(|w| w == needle.as_bytes()),
                "{needle} found in plaintext on disk"
            );
        }
    }

    #[test]
    fn swapped_ciphertext_is_rejected() {
        // Copying one record's ciphertext onto another id must fail (AAD binds kind/id).
        let dir = tempfile::tempdir().unwrap();
        let v = new_vault(&dir);
        let a = Host::new("a", "1").unwrap();
        let b = Host::new("b", "2").unwrap();
        v.put(&a).unwrap();
        v.put(&b).unwrap();
        v.conn()
            .execute(
                "UPDATE records SET data = (SELECT data FROM records WHERE id = ?1) WHERE id = ?2",
                [a.id.to_string(), b.id.to_string()],
            )
            .unwrap();
        assert!(matches!(v.get::<Host>(b.id), Err(VaultError::Corrupt)));
    }
}
