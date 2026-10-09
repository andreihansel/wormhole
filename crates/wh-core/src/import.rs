//! Turns a parsed `~/.ssh/config` into hosts, merging with what already exists.

use crate::model::{Host, Id};
use crate::ssh_config::ImportedHost;

#[derive(Debug, Default)]
pub struct ImportPlan {
    /// Hosts to upsert: new ones plus existing ones with updated settings.
    pub hosts: Vec<Host>,
    pub created: usize,
    pub updated: usize,
    /// `IdentityFile` paths per host, for the caller to import as keys.
    pub identity_files: Vec<(Id, Vec<String>)>,
}

/// Existing hosts are matched by label (the config alias), so re-importing
/// updates them in place instead of duplicating.
pub fn plan_import(existing: &[Host], parsed: &[ImportedHost]) -> ImportPlan {
    let mut plan = ImportPlan::default();
    let mut ids = Vec::with_capacity(parsed.len());
    for imp in parsed {
        let Some(id) = plan.upsert(existing, &imp.alias, &imp.hostname) else { continue };
        let h = plan.host_mut(id);
        h.settings.port = imp.port;
        h.settings.username = imp.user.clone();
        h.settings.agent_forwarding = imp.forward_agent;
        if !imp.identity_files.is_empty() {
            plan.identity_files.push((id, imp.identity_files.clone()));
        }
        ids.push((id, imp));
    }
    // Second pass so ProxyJump can refer to aliases defined later in the file.
    for (id, imp) in ids {
        let chain: Vec<Id> = imp.proxy_jump.iter().filter_map(|hop| plan.resolve_hop(existing, hop)).collect();
        plan.host_mut(id).settings.jump_chain = (!chain.is_empty()).then_some(chain);
    }
    plan
}

impl ImportPlan {
    /// Finds `label` in this plan or `existing`, or creates it. Sets its address.
    fn upsert(&mut self, existing: &[Host], label: &str, address: &str) -> Option<Id> {
        if let Some(h) = self.hosts.iter_mut().find(|h| h.label == label) {
            h.address = address.to_string();
            return Some(h.id);
        }
        let host = match existing.iter().find(|h| h.label == label) {
            Some(h) => {
                self.updated += 1;
                Host { address: address.to_string(), ..h.clone() }
            }
            None => {
                self.created += 1;
                Host::new(label, address).ok()?
            }
        };
        let id = host.id;
        self.hosts.push(host);
        Some(id)
    }

    fn host_mut(&mut self, id: Id) -> &mut Host {
        self.hosts.iter_mut().find(|h| h.id == id).expect("id came from this plan")
    }

    /// An alias already in the plan, or a literal `[user@]host[:port]` jump host.
    fn resolve_hop(&mut self, existing: &[Host], hop: &str) -> Option<Id> {
        if let Some(h) = self.hosts.iter().find(|h| h.label == hop) {
            return Some(h.id);
        }
        let (user, rest) = match hop.rsplit_once('@') {
            Some((u, r)) => (Some(u.to_string()), r),
            None => (None, hop),
        };
        let (addr, port) = match rest.rsplit_once(':') {
            Some((a, p)) if p.parse::<u16>().is_ok() => (a, p.parse().ok()),
            _ => (rest, None),
        };
        let id = self.upsert(existing, hop, addr)?;
        let h = self.host_mut(id);
        h.settings.username = user;
        h.settings.port = port;
        Some(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ssh_config;

    fn parsed(text: &str) -> Vec<ImportedHost> {
        ssh_config::parse(text, &|_| vec![]).hosts
    }

    fn by_label<'a>(plan: &'a ImportPlan, label: &str) -> &'a Host {
        plan.hosts.iter().find(|h| h.label == label).unwrap_or_else(|| panic!("no {label}"))
    }

    #[test]
    fn import_creates_hosts_with_settings() {
        let plan = plan_import(&[], &parsed("Host web\n  HostName 10.0.0.1\n  User deploy\n  Port 2222\n  ForwardAgent yes\n  IdentityFile ~/.ssh/k\n"));
        assert_eq!((plan.created, plan.updated), (1, 0));
        let h = by_label(&plan, "web");
        assert_eq!(h.address, "10.0.0.1");
        assert_eq!(h.settings.username.as_deref(), Some("deploy"));
        assert_eq!(h.settings.port, Some(2222));
        assert_eq!(h.settings.agent_forwarding, Some(true));
        assert_eq!(plan.identity_files, vec![(h.id, vec!["~/.ssh/k".to_string()])]);
    }

    #[test]
    fn proxyjump_alias_resolves_to_host_id() {
        let plan = plan_import(&[], &parsed("Host bastion\n  HostName 1.2.3.4\nHost app\n  ProxyJump bastion\n"));
        let bastion = by_label(&plan, "bastion").id;
        assert_eq!(by_label(&plan, "app").settings.jump_chain, Some(vec![bastion]));
    }

    #[test]
    fn proxyjump_literal_creates_jump_host() {
        let plan = plan_import(&[], &parsed("Host app\n  ProxyJump ops@10.9.9.9:2200\n"));
        let jump = by_label(&plan, "ops@10.9.9.9:2200");
        assert_eq!(jump.address, "10.9.9.9");
        assert_eq!(jump.settings.port, Some(2200));
        assert_eq!(jump.settings.username.as_deref(), Some("ops"));
        assert_eq!(by_label(&plan, "app").settings.jump_chain, Some(vec![jump.id]));
        assert_eq!(plan.created, 2);
    }

    #[test]
    fn import_is_idempotent() {
        let text = "Host bastion\nHost app\n  HostName 10.0.0.2\n  ProxyJump bastion,root@9.9.9.9\n";
        let first = plan_import(&[], &parsed(text));
        let second = plan_import(&first.hosts, &parsed(text));
        assert_eq!((second.created, second.updated), (0, 3));
        let ids = |p: &ImportPlan| {
            let mut v: Vec<_> = p.hosts.iter().map(|h| (h.label.clone(), h.id)).collect();
            v.sort();
            v
        };
        assert_eq!(ids(&first), ids(&second));
    }

    #[test]
    fn existing_group_and_tags_preserved_on_update() {
        let mut existing = Host::new("web", "old.example").unwrap();
        existing.group = Some(uuid::Uuid::new_v4());
        existing.tags = vec!["prod".into()];
        let plan = plan_import(std::slice::from_ref(&existing), &parsed("Host web\n  HostName new.example\n"));
        let h = by_label(&plan, "web");
        assert_eq!((h.id, h.group, h.tags.clone()), (existing.id, existing.group, existing.tags.clone()));
        assert_eq!(h.address, "new.example");
    }
}
