//! Termius-style inheritance: a host's effective settings come from the host
//! itself, then its group, then each ancestor group up to the root.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::inventory::Inventory;
use crate::model::{ConnSettings, Id, DEFAULT_SSH_PORT};

/// Fully resolved connection parameters for one host.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Resolved {
    pub host: Id,
    pub address: String,
    pub port: u16,
    pub username: Option<String>,
    pub identity: Option<Id>,
    pub key: Option<Id>,
    pub jump_chain: Vec<Id>,
    pub env: BTreeMap<String, String>,
    pub startup_snippet: Option<Id>,
    pub agent_forwarding: bool,
}

pub fn resolve(inv: &Inventory, host: Id) -> Option<Resolved> {
    let h = inv.hosts.get(&host)?;
    // Most specific first: host, then its group, then ancestors.
    let groups = h.group.map(|g| inv.group_path(g)).unwrap_or_default();
    let levels: Vec<&ConnSettings> = std::iter::once(&h.settings)
        .chain(groups.iter().rev().filter_map(|g| inv.groups.get(g)).map(|g| &g.settings))
        .collect();

    let identity = levels.iter().find_map(|l| l.identity);
    // At each level an explicit username beats that level's identity.
    let username = levels.iter().find_map(|l| {
        l.username.clone().or_else(|| {
            l.identity.and_then(|i| inv.identities.get(&i)).and_then(|i| i.username.clone())
        })
    });
    let mut env = BTreeMap::new();
    for l in levels.iter().rev() {
        env.extend(l.env.iter().map(|(k, v)| (k.clone(), v.clone())));
    }

    Some(Resolved {
        host,
        address: h.address.clone(),
        port: levels.iter().find_map(|l| l.port).unwrap_or(DEFAULT_SSH_PORT),
        username,
        identity,
        key: identity.and_then(|i| inv.identities.get(&i)).and_then(|i| i.key),
        jump_chain: levels.iter().find_map(|l| l.jump_chain.clone()).unwrap_or_default(),
        env,
        startup_snippet: levels.iter().find_map(|l| l.startup_snippet),
        agent_forwarding: levels.iter().find_map(|l| l.agent_forwarding).unwrap_or(false),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Host, Identity};
    use uuid::Uuid;

    /// root -> child, with one host in `child`.
    fn fixture() -> (Inventory, Id, Id, Id) {
        let mut inv = Inventory::default();
        let root = inv.add_group("root", None).unwrap();
        let child = inv.add_group("child", Some(root)).unwrap();
        let mut h = Host::new("web", "10.0.0.5").unwrap();
        h.group = Some(child);
        let hid = h.id;
        inv.hosts.insert(hid, h);
        (inv, root, child, hid)
    }

    fn identity(inv: &mut Inventory, user: &str) -> (Id, Id) {
        let key = Uuid::new_v4();
        let id = Identity { id: Uuid::new_v4(), label: user.into(), username: Some(user.into()), key: Some(key) };
        let iid = id.id;
        inv.identities.insert(iid, id);
        (iid, key)
    }

    #[test]
    fn unknown_host_resolves_to_none() {
        assert_eq!(resolve(&Inventory::default(), Uuid::new_v4()), None);
    }

    #[test]
    fn ungrouped_host_uses_defaults() {
        let mut inv = Inventory::default();
        let h = Host::new("solo", "example.com").unwrap();
        let hid = h.id;
        inv.hosts.insert(hid, h);
        let r = resolve(&inv, hid).unwrap();
        assert_eq!((r.address.as_str(), r.port, r.username, r.agent_forwarding), ("example.com", 22, None, false));
        assert!(r.jump_chain.is_empty());
    }

    #[test]
    fn inherit_username_and_port_from_ancestor_group() {
        let (mut inv, root, _, hid) = fixture();
        let g = inv.groups.get_mut(&root).unwrap();
        g.settings.username = Some("deploy".into());
        g.settings.port = Some(2222);
        let r = resolve(&inv, hid).unwrap();
        assert_eq!(r.username.as_deref(), Some("deploy"));
        assert_eq!(r.port, 2222);
    }

    #[test]
    fn host_value_overrides_group() {
        let (mut inv, root, _, hid) = fixture();
        inv.groups.get_mut(&root).unwrap().settings.username = Some("deploy".into());
        inv.hosts.get_mut(&hid).unwrap().settings.username = Some("admin".into());
        assert_eq!(resolve(&inv, hid).unwrap().username.as_deref(), Some("admin"));
    }

    #[test]
    fn nearest_group_wins() {
        let (mut inv, root, child, hid) = fixture();
        inv.groups.get_mut(&root).unwrap().settings.port = Some(1111);
        inv.groups.get_mut(&child).unwrap().settings.port = Some(2222);
        assert_eq!(resolve(&inv, hid).unwrap().port, 2222);
    }

    #[test]
    fn env_vars_merge_with_host_precedence() {
        let (mut inv, root, child, hid) = fixture();
        let set = |m: &mut BTreeMap<String, String>, k: &str, v: &str| {
            m.insert(k.into(), v.into());
        };
        set(&mut inv.groups.get_mut(&root).unwrap().settings.env, "A", "root");
        set(&mut inv.groups.get_mut(&root).unwrap().settings.env, "B", "root");
        set(&mut inv.groups.get_mut(&child).unwrap().settings.env, "B", "child");
        set(&mut inv.hosts.get_mut(&hid).unwrap().settings.env, "C", "host");
        set(&mut inv.hosts.get_mut(&hid).unwrap().settings.env, "A", "host");
        let env = resolve(&inv, hid).unwrap().env;
        let got: Vec<_> = env.iter().map(|(k, v)| format!("{k}={v}")).collect();
        assert_eq!(got, ["A=host", "B=child", "C=host"]);
    }

    #[test]
    fn jump_chain_inherited_from_group() {
        let (mut inv, root, _, hid) = fixture();
        let bastion = Uuid::new_v4();
        inv.groups.get_mut(&root).unwrap().settings.jump_chain = Some(vec![bastion]);
        assert_eq!(resolve(&inv, hid).unwrap().jump_chain, vec![bastion]);
        // An explicit empty chain on the host disables the inherited one.
        inv.hosts.get_mut(&hid).unwrap().settings.jump_chain = Some(vec![]);
        assert!(resolve(&inv, hid).unwrap().jump_chain.is_empty());
    }

    #[test]
    fn identity_resolves_to_user_and_key() {
        let (mut inv, root, _, hid) = fixture();
        let (iid, key) = identity(&mut inv, "ops");
        inv.groups.get_mut(&root).unwrap().settings.identity = Some(iid);
        let r = resolve(&inv, hid).unwrap();
        assert_eq!((r.identity, r.key, r.username.as_deref()), (Some(iid), Some(key), Some("ops")));
    }

    #[test]
    fn host_identity_beats_group_username() {
        let (mut inv, root, _, hid) = fixture();
        let (iid, _) = identity(&mut inv, "ops");
        inv.groups.get_mut(&root).unwrap().settings.username = Some("deploy".into());
        inv.hosts.get_mut(&hid).unwrap().settings.identity = Some(iid);
        assert_eq!(resolve(&inv, hid).unwrap().username.as_deref(), Some("ops"));
    }

    #[test]
    fn explicit_username_beats_identity_at_same_level() {
        let (mut inv, _, _, hid) = fixture();
        let (iid, _) = identity(&mut inv, "ops");
        let h = inv.hosts.get_mut(&hid).unwrap();
        h.settings.identity = Some(iid);
        h.settings.username = Some("root".into());
        assert_eq!(resolve(&inv, hid).unwrap().username.as_deref(), Some("root"));
    }
}
