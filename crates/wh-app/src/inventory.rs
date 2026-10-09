//! Inventory commands: hosts, groups, identities, keys, snippets, forwards.

use serde::{Deserialize, Serialize};
use wh_core::inventory::Group;
use wh_core::model::{Host, Id, Identity, PortForward, Snippet, SshKey};
use wh_core::resolve::Resolved;

use wh_core::inventory::Inventory;
use wh_core::model::validate_port;
use wh_ssh::keys::{self, KeyError, KeyMaterial, KeyType};
use wh_vault::admin::GroupDelete;

use crate::error::ApiError;
use crate::state::{ApiResult, App};

fn validation(msg: impl Into<String>) -> ApiError {
    ApiError::new("validation", msg)
}

fn not_found(what: &str) -> ApiError {
    ApiError::new("not_found", format!("{what} not found"))
}

impl From<KeyError> for ApiError {
    fn from(e: KeyError) -> Self {
        let kind = match e {
            KeyError::NeedsPassphrase => "needs_passphrase",
            KeyError::BadPassphrase => "bad_passphrase",
            KeyError::Unsupported => "unsupported_key",
            KeyError::CertificateMismatch => "validation",
        };
        ApiError::new(kind, e.to_string())
    }
}

/// Everything the UI lists. Contains no secrets.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct InventoryDto {
    pub groups: Vec<Group>,
    pub hosts: Vec<Host>,
    pub identities: Vec<Identity>,
    pub keys: Vec<SshKey>,
    pub snippets: Vec<Snippet>,
    pub forwards: Vec<PortForward>,
    /// Ids of hosts/identities that have a stored password.
    pub has_password: Vec<Id>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum NewKey {
    Ed25519,
    Rsa { bits: usize },
}

impl App {
    pub fn inventory(&self) -> ApiResult<InventoryDto> {
        let v = self.vault();
        let hosts: Vec<Host> = v.list()?;
        let identities: Vec<Identity> = v.list()?;
        let mut has_password = Vec::new();
        for id in hosts.iter().map(|h| h.id).chain(identities.iter().map(|i| i.id)) {
            if v.get_secret(id)?.is_some() {
                has_password.push(id);
            }
        }
        Ok(InventoryDto {
            groups: v.list()?,
            hosts,
            identities,
            keys: v.list()?,
            snippets: v.list()?,
            forwards: v.list()?,
            has_password,
        })
    }

    /// The vault's organizational data as a `wh_core` inventory.
    pub(crate) fn load_inventory(&self) -> ApiResult<Inventory> {
        let v = self.vault();
        Ok(Inventory {
            groups: v.list::<Group>()?.into_iter().map(|g| (g.id, g)).collect(),
            hosts: v.list::<Host>()?.into_iter().map(|h| (h.id, h)).collect(),
            identities: v.list::<Identity>()?.into_iter().map(|i| (i.id, i)).collect(),
        })
    }

    pub fn save_host(&self, mut host: Host) -> ApiResult<Host> {
        host.address = host.address.trim().to_string();
        if host.address.is_empty() {
            return Err(validation("address is required"));
        }
        if let Some(p) = host.settings.port {
            validate_port(p.into()).map_err(|e| validation(e.to_string()))?;
        }
        self.vault().put(&host)?;
        Ok(host)
    }

    /// Also removes the host's stored password.
    pub fn delete_host(&self, id: Id) -> ApiResult<()> {
        let v = self.vault();
        v.delete::<Host>(id)?;
        v.delete_secret(id)?;
        Ok(())
    }

    pub fn save_group(&self, group: Group) -> ApiResult<Group> {
        let mut inv = self.load_inventory()?;
        // Place it (detached) then move it, so the tree validates parent and cycles.
        inv.groups.insert(group.id, Group { parent: None, ..group.clone() });
        inv.move_group(group.id, group.parent).map_err(|e| validation(e.to_string()))?;
        self.vault().put(&group)?;
        Ok(group)
    }

    pub fn delete_group(&self, id: Id, move_contents_to_parent: bool) -> ApiResult<()> {
        let policy = if move_contents_to_parent { GroupDelete::MoveContentsToParent } else { GroupDelete::RejectIfNotEmpty };
        Ok(self.vault().delete_group(id, policy)?)
    }

    pub fn save_identity(&self, identity: Identity) -> ApiResult<Identity> {
        self.vault().put(&identity)?;
        Ok(identity)
    }

    pub fn delete_identity(&self, id: Id) -> ApiResult<()> {
        let v = self.vault();
        v.delete::<Identity>(id)?;
        v.delete_secret(id)?;
        Ok(())
    }

    /// Store (`Some`) or clear (`None`) the password of a host or identity.
    pub fn set_password(&self, owner: Id, password: Option<&str>) -> ApiResult<()> {
        let v = self.vault();
        match password {
            Some(p) => v.put_secret(owner, p.as_bytes())?,
            None => {
                v.delete_secret(owner)?;
            }
        }
        Ok(())
    }

    pub fn resolve_host(&self, id: Id) -> ApiResult<Resolved> {
        wh_core::resolve::resolve(&self.load_inventory()?, id).ok_or_else(|| not_found("host"))
    }

    pub fn key_generate(&self, label: &str, kind: NewKey) -> ApiResult<SshKey> {
        let kind = match kind {
            NewKey::Ed25519 => KeyType::Ed25519,
            NewKey::Rsa { bits } => KeyType::Rsa { bits },
        };
        self.store_key(label, keys::generate(kind, label)?)
    }

    pub fn key_import(&self, label: &str, private_key: &str, passphrase: Option<&str>) -> ApiResult<SshKey> {
        self.store_key(label, keys::import_private(private_key, passphrase)?)
    }

    pub(crate) fn store_key(&self, label: &str, material: KeyMaterial) -> ApiResult<SshKey> {
        let key = SshKey {
            id: uuid::Uuid::new_v4(),
            label: label.to_string(),
            public_key: material.public_openssh,
            fingerprint: material.fingerprint,
            certificate: None,
        };
        let v = self.vault();
        v.put_secret(key.id, material.private_openssh.as_bytes())?;
        v.put(&key)?;
        Ok(key)
    }

    /// Also removes the private key.
    pub fn delete_key(&self, id: Id) -> ApiResult<()> {
        let v = self.vault();
        v.delete::<SshKey>(id)?;
        v.delete_secret(id)?;
        Ok(())
    }

    pub fn save_snippet(&self, snippet: Snippet) -> ApiResult<Snippet> {
        self.vault().put(&snippet)?;
        Ok(snippet)
    }

    pub fn delete_snippet(&self, id: Id) -> ApiResult<()> {
        self.vault().delete::<Snippet>(id)?;
        Ok(())
    }

    pub fn save_forward(&self, forward: PortForward) -> ApiResult<PortForward> {
        self.vault().put(&forward)?;
        Ok(forward)
    }

    pub fn delete_forward(&self, id: Id) -> ApiResult<()> {
        self.vault().delete::<PortForward>(id)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::tests::{app, unlocked};
    use uuid::Uuid;

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!("{}/../../tests/fixtures/keys/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
    }

    fn group(name: &str, parent: Option<Id>) -> Group {
        Group { id: Uuid::new_v4(), name: name.into(), parent, settings: Default::default() }
    }

    #[test]
    fn inventory_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let a = unlocked(&dir);
        let g = a.save_group(group("prod", None)).unwrap();
        let mut h = Host::new("web", "10.0.0.1").unwrap();
        h.group = Some(g.id);
        let h = a.save_host(h).unwrap();
        let s = a.save_snippet(Snippet { id: Uuid::new_v4(), label: "up".into(), script: "uptime".into() }).unwrap();
        let inv = a.inventory().unwrap();
        assert_eq!((inv.groups, inv.hosts, inv.snippets), (vec![g], vec![h], vec![s]));
    }

    #[test]
    fn save_host_validates() {
        let dir = tempfile::tempdir().unwrap();
        let a = unlocked(&dir);
        let mut h = Host::new("web", "10.0.0.1").unwrap();
        h.address = "  ".into();
        assert_eq!(a.save_host(h.clone()).unwrap_err().kind, "validation");
        h.address = "ok".into();
        h.settings.port = Some(0);
        assert_eq!(a.save_host(h).unwrap_err().kind, "validation");
    }

    #[test]
    fn group_cannot_become_its_own_ancestor() {
        let dir = tempfile::tempdir().unwrap();
        let a = unlocked(&dir);
        let top = a.save_group(group("top", None)).unwrap();
        let child = a.save_group(group("child", Some(top.id))).unwrap();
        let looped = Group { parent: Some(child.id), ..top };
        assert_eq!(a.save_group(looped).unwrap_err().kind, "validation");
        assert_eq!(a.save_group(group("orphan", Some(Uuid::new_v4()))).unwrap_err().kind, "validation");
    }

    #[test]
    fn delete_group_respects_policy() {
        let dir = tempfile::tempdir().unwrap();
        let a = unlocked(&dir);
        let g = a.save_group(group("g", None)).unwrap();
        let mut h = Host::new("web", "1").unwrap();
        h.group = Some(g.id);
        a.save_host(h).unwrap();
        assert_eq!(a.delete_group(g.id, false).unwrap_err().kind, "group_not_empty");
        a.delete_group(g.id, true).unwrap();
        assert_eq!(a.inventory().unwrap().hosts[0].group, None);
    }

    #[test]
    fn passwords_are_flagged_but_never_listed() {
        let dir = tempfile::tempdir().unwrap();
        let a = unlocked(&dir);
        let h = a.save_host(Host::new("web", "1").unwrap()).unwrap();
        a.set_password(h.id, Some("s3cret")).unwrap();
        let inv = a.inventory().unwrap();
        assert_eq!(inv.has_password, vec![h.id]);
        assert!(!serde_json::to_string(&inv).unwrap().contains("s3cret"));
        a.set_password(h.id, None).unwrap();
        assert!(a.inventory().unwrap().has_password.is_empty());
    }

    #[test]
    fn delete_host_removes_its_password() {
        let dir = tempfile::tempdir().unwrap();
        let a = unlocked(&dir);
        let h = a.save_host(Host::new("web", "1").unwrap()).unwrap();
        a.set_password(h.id, Some("pw")).unwrap();
        a.delete_host(h.id).unwrap();
        assert!(a.vault().get_secret(h.id).unwrap().is_none());
    }

    #[test]
    fn resolve_host_uses_group_inheritance() {
        let dir = tempfile::tempdir().unwrap();
        let a = unlocked(&dir);
        let mut g = group("g", None);
        g.settings.username = Some("deploy".into());
        let g = a.save_group(g).unwrap();
        let mut h = Host::new("web", "1").unwrap();
        h.group = Some(g.id);
        let h = a.save_host(h).unwrap();
        assert_eq!(a.resolve_host(h.id).unwrap().username.as_deref(), Some("deploy"));
        assert_eq!(a.resolve_host(Uuid::new_v4()).unwrap_err().kind, "not_found");
    }

    #[test]
    fn key_generate_keeps_private_key_out_of_inventory() {
        let dir = tempfile::tempdir().unwrap();
        let a = unlocked(&dir);
        let k = a.key_generate("laptop", NewKey::Ed25519).unwrap();
        assert!(k.public_key.starts_with("ssh-ed25519 "));
        let json = serde_json::to_string(&a.inventory().unwrap()).unwrap();
        assert!(!json.contains("PRIVATE KEY"));
        let private = a.vault().get_secret(k.id).unwrap().unwrap();
        assert!(String::from_utf8_lossy(&private).contains("BEGIN OPENSSH PRIVATE KEY"));
        a.delete_key(k.id).unwrap();
        assert!(a.vault().get_secret(k.id).unwrap().is_none());
        assert!(a.inventory().unwrap().keys.is_empty());
    }

    #[test]
    fn key_import_reports_passphrase_problems() {
        let dir = tempfile::tempdir().unwrap();
        let a = unlocked(&dir);
        let enc = fixture("id_ed25519_enc");
        assert_eq!(a.key_import("k", &enc, None).unwrap_err().kind, "needs_passphrase");
        assert_eq!(a.key_import("k", &enc, Some("bad")).unwrap_err().kind, "bad_passphrase");
        let k = a.key_import("k", &enc, Some("fixture-pass")).unwrap();
        assert!(k.fingerprint.starts_with("SHA256:"));
    }

    #[test]
    fn commands_fail_while_locked() {
        let dir = tempfile::tempdir().unwrap();
        let (a, _) = app(&dir);
        assert_eq!(a.inventory().unwrap_err().kind, "locked");
    }
}
