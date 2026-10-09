use crate::model::Host;

/// Case-insensitive match against label, address and tags.
/// Every whitespace-separated term must match somewhere.
pub fn matches(host: &Host, query: &str) -> bool {
    let haystack: Vec<String> = [&host.label, &host.address]
        .into_iter()
        .chain(&host.tags)
        .map(|s| s.to_lowercase())
        .collect();
    query
        .to_lowercase()
        .split_whitespace()
        .all(|term| haystack.iter().any(|h| h.contains(term)))
}

/// Hosts carrying `tag` (case-insensitive).
pub fn filter_by_tag<'a>(hosts: impl IntoIterator<Item = &'a Host>, tag: &str) -> Vec<&'a Host> {
    hosts
        .into_iter()
        .filter(|h| h.tags.iter().any(|t| t.eq_ignore_ascii_case(tag)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(label: &str, addr: &str, tags: &[&str]) -> Host {
        let mut h = Host::new(label, addr).unwrap();
        h.tags = tags.iter().map(|t| t.to_string()).collect();
        h
    }

    #[test]
    fn search_matches_label_address_and_tags_case_insensitive() {
        let h = host("Web-Prod", "203.0.113.7", &["Nginx"]);
        assert!(matches(&h, "web"));
        assert!(matches(&h, "113.7"));
        assert!(matches(&h, "NGINX"));
        assert!(!matches(&h, "postgres"));
    }

    #[test]
    fn empty_query_matches_everything() {
        assert!(matches(&host("a", "b", &[]), "   "));
    }

    #[test]
    fn all_terms_must_match() {
        let h = host("web-prod", "10.0.0.1", &["eu"]);
        assert!(matches(&h, "web eu"));
        assert!(!matches(&h, "web us"));
    }

    #[test]
    fn filter_by_tag() {
        let a = host("a", "1", &["Prod"]);
        let b = host("b", "2", &["staging"]);
        let got: Vec<_> = super::filter_by_tag([&a, &b], "prod").iter().map(|h| h.label.clone()).collect();
        assert_eq!(got, ["a"]);
    }
}
