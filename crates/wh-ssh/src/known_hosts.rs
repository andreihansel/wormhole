//! OpenSSH-compatible known_hosts parsing and host key checking.

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use hmac::{Hmac, Mac};
use russh::keys::PublicKey;
use sha1::Sha1;

/// One `patterns key` pair. A line listing several comma-separated hosts
/// becomes several entries. `pattern` is a hostname, `[host]:port`, or a
/// hashed `|1|salt|hash` token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownHostEntry {
    pub pattern: String,
    pub key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostKeyStatus {
    Known,
    Unknown,
    /// An entry for this host with the same algorithm holds a different key.
    Changed,
}

pub fn parse(text: &str) -> Vec<KnownHostEntry> {
    let mut out = Vec::new();
    for line in text.lines().map(str::trim) {
        // Markers (@cert-authority, @revoked) are not supported yet.
        if line.is_empty() || line.starts_with('#') || line.starts_with('@') {
            continue;
        }
        let mut fields = line.split_whitespace();
        let (Some(hosts), Some(algo), Some(blob)) = (fields.next(), fields.next(), fields.next()) else {
            continue;
        };
        for pattern in hosts.split(',').filter(|p| !p.is_empty()) {
            out.push(KnownHostEntry { pattern: pattern.to_string(), key: format!("{algo} {blob}") });
        }
    }
    out
}

/// The pattern OpenSSH writes for a host: `host` on port 22, else `[host]:port`.
pub fn host_pattern(host: &str, port: u16) -> String {
    if port == 22 {
        host.to_string()
    } else {
        format!("[{host}]:{port}")
    }
}

pub fn check(entries: &[KnownHostEntry], host: &str, port: u16, key: &PublicKey) -> HostKeyStatus {
    let target = host_pattern(host, port);
    let mut changed = false;
    for entry in entries.iter().filter(|e| pattern_matches(&e.pattern, &target)) {
        let Ok(known) = PublicKey::from_openssh(&entry.key) else { continue };
        if known.key_data() == key.key_data() {
            return HostKeyStatus::Known;
        }
        changed |= known.algorithm() == key.algorithm();
    }
    if changed {
        HostKeyStatus::Changed
    } else {
        HostKeyStatus::Unknown
    }
}

fn pattern_matches(pattern: &str, target: &str) -> bool {
    match pattern.strip_prefix("|1|") {
        Some(hashed) => hashed_matches(hashed, target),
        None => pattern.eq_ignore_ascii_case(target),
    }
}

/// `|1|base64(salt)|base64(HMAC-SHA1(salt, host))`
fn hashed_matches(hashed: &str, target: &str) -> bool {
    let Some((salt, hash)) = hashed.split_once('|') else { return false };
    let (Ok(salt), Ok(hash)) = (B64.decode(salt), B64.decode(hash)) else { return false };
    let Ok(mut mac) = Hmac::<Sha1>::new_from_slice(&salt) else { return false };
    mac.update(target.as_bytes());
    mac.verify_slice(&hash).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(path: &str) -> String {
        std::fs::read_to_string(format!("{}/../../tests/fixtures/{path}", env!("CARGO_MANIFEST_DIR"))).unwrap()
    }

    fn key(name: &str) -> PublicKey {
        PublicKey::from_openssh(fixture(&format!("keys/{name}.pub")).trim()).unwrap()
    }

    #[test]
    fn host_pattern_brackets_nonstandard_ports() {
        assert_eq!(host_pattern("example.com", 22), "example.com");
        assert_eq!(host_pattern("example.com", 2222), "[example.com]:2222");
    }

    #[test]
    fn parse_skips_comments_and_splits_host_lists() {
        let entries = parse(&fixture("known_hosts/plain"));
        let patterns: Vec<_> = entries.iter().map(|e| e.pattern.as_str()).collect();
        assert_eq!(patterns, ["example.com", "203.0.113.9", "[example.com]:2222", "rsa-only.example"]);
        assert!(entries[0].key.starts_with("ssh-ed25519 AAAA"));
    }

    #[test]
    fn parse_skips_markers_and_malformed_lines() {
        let text = "@cert-authority *.example.com ssh-ed25519 AAAA\nonlyonefield\n";
        assert!(parse(text).is_empty());
    }

    #[test]
    fn known_host_plain_and_hashed() {
        for file in ["known_hosts/plain", "known_hosts/hashed"] {
            let entries = parse(&fixture(file));
            let k = key("id_ed25519");
            assert_eq!(check(&entries, "example.com", 22, &k), HostKeyStatus::Known, "{file}");
            assert_eq!(check(&entries, "203.0.113.9", 22, &k), HostKeyStatus::Known, "{file}");
            assert_eq!(check(&entries, "example.com", 2222, &k), HostKeyStatus::Known, "{file}");
        }
    }

    #[test]
    fn unknown_host_or_port() {
        let entries = parse(&fixture("known_hosts/hashed"));
        let k = key("id_ed25519");
        assert_eq!(check(&entries, "other.example", 22, &k), HostKeyStatus::Unknown);
        assert_eq!(check(&entries, "example.com", 2200, &k), HostKeyStatus::Unknown);
    }

    #[test]
    fn changed_host_key_is_detected() {
        for file in ["known_hosts/plain", "known_hosts/hashed"] {
            let entries = parse(&fixture(file));
            // Same algorithm (ed25519), different key.
            assert_eq!(check(&entries, "example.com", 22, &key("id_ed25519_enc")), HostKeyStatus::Changed, "{file}");
        }
    }

    #[test]
    fn different_algorithm_is_unknown_not_changed() {
        let entries = parse(&fixture("known_hosts/plain"));
        // Only an RSA key is recorded; an ed25519 key is new, not a conflict.
        assert_eq!(check(&entries, "rsa-only.example", 22, &key("id_ed25519")), HostKeyStatus::Unknown);
    }
}
