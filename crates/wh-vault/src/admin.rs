//! Whole-vault operations: password change, group deletion, export/import.

use std::path::Path;

use serde::{Deserialize, Serialize};
use wh_core::inventory::Group;
use wh_core::model::{Host, Id};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::crypto::{self, KdfParams, SALT_LEN};
use crate::records::{KNOWN_KINDS, SECRET_KIND};
use crate::vault::{Result, Vault, VaultError};

const EXPORT_MAGIC: &[u8; 5] = b"WHEX\x01";
const EXPORT_AAD: &[u8] = b"wormhole/export/v1";

#[derive(Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
struct ExportEntry {
    kind: String,
    id: String,
    data: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupDelete {
    /// Refuse if the group still contains hosts or subgroups.
    RejectIfNotEmpty,
    /// Move contained hosts and subgroups up to the deleted group's parent.
    MoveContentsToParent,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ImportStats {
    pub records: usize,
    pub secrets: usize,
}

impl Vault {
    pub fn change_password(&mut self, old: &str, new: &str, params: KdfParams) -> Result<()> {
        let dek = self.unwrap_dek(old)?;
        self.wrap_dek(&dek, new, params)
    }

    pub fn delete_group(&self, id: Id, policy: GroupDelete) -> Result<()> {
        let Some(group) = self.get::<Group>(id)? else { return Ok(()) };
        let mut hosts: Vec<Host> = self.list::<Host>()?.into_iter().filter(|h| h.group == Some(id)).collect();
        let mut children: Vec<Group> = self.list::<Group>()?.into_iter().filter(|g| g.parent == Some(id)).collect();
        if policy == GroupDelete::RejectIfNotEmpty && !(hosts.is_empty() && children.is_empty()) {
            return Err(VaultError::GroupNotEmpty);
        }
        let tx = self.conn().unchecked_transaction()?;
        for h in &mut hosts {
            h.group = group.parent;
            self.put(h)?;
        }
        for g in &mut children {
            g.parent = group.parent;
            self.put(g)?;
        }
        self.delete::<Group>(id)?;
        tx.commit()?;
        Ok(())
    }

    /// Write every record and secret to a file sealed with `password`.
    pub fn export_to(&self, path: &Path, password: &str, params: KdfParams) -> Result<()> {
        let dek = self.dek()?;
        let mut stmt = self.conn().prepare("SELECT kind, id, data FROM records ORDER BY rowid")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Vec<u8>>(2)?)))?;
        let mut entries = Vec::new();
        for row in rows {
            let (kind, id, sealed) = row?;
            let aad = format!("{kind}/{id}");
            let data = crypto::open(dek, &sealed, aad.as_bytes()).map_err(|_| VaultError::Corrupt)?;
            entries.push(ExportEntry { kind, id, data: data.to_vec() });
        }
        let payload = Zeroizing::new(serde_json::to_vec(&entries).expect("export entries serialize"));

        let salt = crypto::random_bytes::<SALT_LEN>();
        let key = crypto::derive_key(password.as_bytes(), &salt, params)?;
        let kdf = serde_json::to_vec(&params).expect("KdfParams serializes");
        let mut out = Vec::new();
        out.extend_from_slice(EXPORT_MAGIC);
        out.extend_from_slice(&salt);
        out.extend_from_slice(&(kdf.len() as u32).to_be_bytes());
        out.extend_from_slice(&kdf);
        out.extend_from_slice(&crypto::seal(&key, &payload, EXPORT_AAD));
        std::fs::write(path, out)?;
        Ok(())
    }

    /// Merge an export into this vault. Records with the same id are overwritten.
    pub fn import_from(&self, path: &Path, password: &str) -> Result<ImportStats> {
        self.dek()?;
        let bytes = std::fs::read(path)?;
        let (salt, params, sealed) = parse_export(&bytes).ok_or(VaultError::NotAnExport)?;
        let key = crypto::derive_key(password.as_bytes(), &salt, params)?;
        let payload = crypto::open(&key, sealed, EXPORT_AAD).map_err(|_| VaultError::ExportDecrypt)?;
        let entries: Vec<ExportEntry> = serde_json::from_slice(&payload).map_err(|_| VaultError::NotAnExport)?;

        let mut stats = ImportStats::default();
        let tx = self.conn().unchecked_transaction()?;
        for e in &entries {
            if !KNOWN_KINDS.contains(&e.kind.as_str()) {
                return Err(VaultError::NotAnExport);
            }
            let id: Id = e.id.parse().map_err(|_| VaultError::NotAnExport)?;
            self.put_raw(&e.kind, id, &e.data)?;
            if e.kind == SECRET_KIND {
                stats.secrets += 1;
            } else {
                stats.records += 1;
            }
        }
        tx.commit()?;
        Ok(stats)
    }
}

fn parse_export(bytes: &[u8]) -> Option<([u8; SALT_LEN], KdfParams, &[u8])> {
    let rest = bytes.strip_prefix(EXPORT_MAGIC)?;
    let (salt, rest) = rest.split_at_checked(SALT_LEN)?;
    let (len, rest) = rest.split_at_checked(4)?;
    let len = u32::from_be_bytes(len.try_into().ok()?) as usize;
    let (kdf, sealed) = rest.split_at_checked(len)?;
    let params: KdfParams = serde_json::from_slice(kdf).ok()?;
    params.is_sane().then_some((salt.try_into().ok()?, params, sealed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::TEST_KDF;
    use crate::vault::tests::new_vault;
    use crate::VaultError;
    use uuid::Uuid;
    use wh_core::inventory::Group;
    use wh_core::model::Host;

    fn group(parent: Option<Id>) -> Group {
        Group { id: Uuid::new_v4(), name: "g".into(), parent, settings: Default::default() }
    }

    #[test]
    fn change_master_password_keeps_data_readable() {
        let dir = tempfile::tempdir().unwrap();
        let mut v = new_vault(&dir);
        let h = Host::new("web", "10.0.0.1").unwrap();
        v.put(&h).unwrap();
        v.put_secret(h.id, b"pw").unwrap();
        assert!(matches!(v.change_password("wrong", "new", TEST_KDF), Err(VaultError::BadPassword)));
        v.change_password("correct horse", "new pass", TEST_KDF).unwrap();
        v.lock();
        assert!(matches!(v.unlock("correct horse"), Err(VaultError::BadPassword)));
        v.unlock("new pass").unwrap();
        assert_eq!(v.get::<Host>(h.id).unwrap(), Some(h.clone()));
        assert_eq!(v.get_secret(h.id).unwrap().unwrap().as_slice(), b"pw");
    }

    #[test]
    fn delete_nonempty_group_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let v = new_vault(&dir);
        let g = group(None);
        let mut h = Host::new("web", "1").unwrap();
        h.group = Some(g.id);
        v.put(&g).unwrap();
        v.put(&h).unwrap();
        assert!(matches!(v.delete_group(g.id, GroupDelete::RejectIfNotEmpty), Err(VaultError::GroupNotEmpty)));
        assert!(v.get::<Group>(g.id).unwrap().is_some());
    }

    #[test]
    fn delete_group_moves_contents_to_parent() {
        let dir = tempfile::tempdir().unwrap();
        let v = new_vault(&dir);
        let top = group(None);
        let mid = group(Some(top.id));
        let leaf = group(Some(mid.id));
        let mut h = Host::new("web", "1").unwrap();
        h.group = Some(mid.id);
        for g in [&top, &mid, &leaf] {
            v.put(g).unwrap();
        }
        v.put(&h).unwrap();
        v.delete_group(mid.id, GroupDelete::MoveContentsToParent).unwrap();
        assert!(v.get::<Group>(mid.id).unwrap().is_none());
        assert_eq!(v.get::<Host>(h.id).unwrap().unwrap().group, Some(top.id));
        assert_eq!(v.get::<Group>(leaf.id).unwrap().unwrap().parent, Some(top.id));
    }

    #[test]
    fn delete_empty_group_succeeds() {
        let dir = tempfile::tempdir().unwrap();
        let v = new_vault(&dir);
        let g = group(None);
        v.put(&g).unwrap();
        v.delete_group(g.id, GroupDelete::RejectIfNotEmpty).unwrap();
        assert!(v.get::<Group>(g.id).unwrap().is_none());
    }

    #[test]
    fn export_roundtrip_with_export_password() {
        let src_dir = tempfile::tempdir().unwrap();
        let src = new_vault(&src_dir);
        let h = Host::new("web", "10.0.0.1").unwrap();
        src.put(&h).unwrap();
        src.put_secret(h.id, b"pw").unwrap();
        let file = src_dir.path().join("backup.whx");
        src.export_to(&file, "export pw", TEST_KDF).unwrap();
        assert!(!std::fs::read(&file).unwrap().windows(9).any(|w| w == b"10.0.0.1\""));

        let dst_dir = tempfile::tempdir().unwrap();
        let dst = new_vault(&dst_dir);
        let stats = dst.import_from(&file, "export pw").unwrap();
        assert_eq!(stats, ImportStats { records: 1, secrets: 1 });
        assert_eq!(dst.get::<Host>(h.id).unwrap(), Some(h));
        // Importing again is idempotent (upsert by id).
        dst.import_from(&file, "export pw").unwrap();
        assert_eq!(dst.list::<Host>().unwrap().len(), 1);
    }

    #[test]
    fn import_rejects_wrong_password_and_tampered_file() {
        let dir = tempfile::tempdir().unwrap();
        let v = new_vault(&dir);
        v.put(&Host::new("web", "1").unwrap()).unwrap();
        let file = dir.path().join("backup.whx");
        v.export_to(&file, "export pw", TEST_KDF).unwrap();
        assert!(matches!(v.import_from(&file, "nope"), Err(VaultError::ExportDecrypt)));
        let mut bytes = std::fs::read(&file).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        std::fs::write(&file, bytes).unwrap();
        assert!(matches!(v.import_from(&file, "export pw"), Err(VaultError::ExportDecrypt)));
        std::fs::write(&file, b"garbage").unwrap();
        assert!(matches!(v.import_from(&file, "export pw"), Err(VaultError::NotAnExport)));
    }

    #[test]
    fn import_rejects_absurd_kdf_params() {
        let dir = tempfile::tempdir().unwrap();
        let v = new_vault(&dir);
        let file = dir.path().join("evil.whx");
        let huge = KdfParams { mem_kib: u32::MAX, iterations: 1, parallelism: 1 };
        let kdf = serde_json::to_vec(&huge).unwrap();
        let mut bytes = EXPORT_MAGIC.to_vec();
        bytes.extend_from_slice(&[0; SALT_LEN]);
        bytes.extend_from_slice(&(kdf.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&kdf);
        bytes.extend_from_slice(&[0; 64]);
        std::fs::write(&file, bytes).unwrap();
        assert!(matches!(v.import_from(&file, "pw"), Err(VaultError::NotAnExport)));
    }
}
