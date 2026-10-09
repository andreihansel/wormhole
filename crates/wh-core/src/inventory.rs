use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::model::{ConnSettings, Host, Id, Identity};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TreeError {
    #[error("group not found")]
    UnknownGroup,
    #[error("a group cannot be moved inside itself or its descendants")]
    Cycle,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Group {
    pub id: Id,
    pub name: String,
    pub parent: Option<Id>,
    pub settings: ConnSettings,
}

/// In-memory view of the vault's organizational data.
#[derive(Debug, Default, Clone)]
pub struct Inventory {
    pub groups: HashMap<Id, Group>,
    pub hosts: HashMap<Id, Host>,
    pub identities: HashMap<Id, Identity>,
}

impl Inventory {
    pub fn add_group(&mut self, name: &str, parent: Option<Id>) -> Result<Id, TreeError> {
        if let Some(p) = parent {
            if !self.groups.contains_key(&p) {
                return Err(TreeError::UnknownGroup);
            }
        }
        let id = Uuid::new_v4();
        self.groups.insert(id, Group { id, name: name.trim().to_string(), parent, settings: ConnSettings::default() });
        Ok(id)
    }

    pub fn move_group(&mut self, id: Id, new_parent: Option<Id>) -> Result<(), TreeError> {
        if !self.groups.contains_key(&id) {
            return Err(TreeError::UnknownGroup);
        }
        if let Some(p) = new_parent {
            if !self.groups.contains_key(&p) {
                return Err(TreeError::UnknownGroup);
            }
            if self.group_path(p).contains(&id) {
                return Err(TreeError::Cycle);
            }
        }
        self.groups.get_mut(&id).expect("checked above").parent = new_parent;
        Ok(())
    }

    /// Ancestors of `id` (inclusive), root first.
    pub fn group_path(&self, id: Id) -> Vec<Id> {
        let mut path = Vec::new();
        let mut cur = Some(id);
        while let Some(g) = cur.and_then(|c| self.groups.get(&c)) {
            path.push(g.id);
            cur = g.parent;
        }
        path.reverse();
        path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_group_rejects_unknown_parent() {
        let mut inv = Inventory::default();
        assert_eq!(inv.add_group("x", Some(Uuid::new_v4())), Err(TreeError::UnknownGroup));
    }

    #[test]
    fn group_tree_rejects_cycles() {
        let mut inv = Inventory::default();
        let a = inv.add_group("a", None).unwrap();
        let b = inv.add_group("b", Some(a)).unwrap();
        let c = inv.add_group("c", Some(b)).unwrap();
        assert_eq!(inv.move_group(a, Some(c)), Err(TreeError::Cycle));
        assert_eq!(inv.move_group(a, Some(a)), Err(TreeError::Cycle));
        assert_eq!(inv.groups[&a].parent, None, "failed move must not mutate");
        assert_eq!(inv.move_group(c, None), Ok(()));
    }

    #[test]
    fn group_path_returns_ancestors_root_first() {
        let mut inv = Inventory::default();
        let a = inv.add_group("a", None).unwrap();
        let b = inv.add_group("b", Some(a)).unwrap();
        let c = inv.add_group("c", Some(b)).unwrap();
        assert_eq!(inv.group_path(c), vec![a, b, c]);
        assert_eq!(inv.group_path(a), vec![a]);
    }
}
