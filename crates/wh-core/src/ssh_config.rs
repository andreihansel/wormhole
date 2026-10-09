//! `~/.ssh/config` parsing for import, following OpenSSH semantics:
//! for each concrete alias, the first value obtained for a keyword wins,
//! scanning every matching `Host` block (including wildcard ones) in order.

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportedHost {
    pub alias: String,
    pub hostname: String,
    pub user: Option<String>,
    pub port: Option<u16>,
    pub identity_files: Vec<String>,
    /// Each hop as written: an alias or `[user@]host[:port]`.
    pub proxy_jump: Vec<String>,
    pub forward_agent: Option<bool>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ParsedConfig {
    pub hosts: Vec<ImportedHost>,
    pub warnings: Vec<String>,
}

const MAX_INCLUDE_DEPTH: usize = 16;

/// A `Host`/`Match` section, or the global section before the first one.
struct Block {
    kind: BlockKind,
    directives: Vec<(String, Vec<String>)>,
}

enum BlockKind {
    Global,
    Host(Vec<String>),
    Match,
}

/// `include` resolves an `Include` argument to the contents of each file it names.
pub fn parse(text: &str, include: &dyn Fn(&str) -> Vec<String>) -> ParsedConfig {
    let mut warnings = Vec::new();
    let mut blocks = vec![Block { kind: BlockKind::Global, directives: Vec::new() }];
    collect(text, include, 0, &mut blocks, &mut warnings);

    let mut aliases: Vec<String> = Vec::new();
    for b in &blocks {
        if let BlockKind::Host(patterns) = &b.kind {
            for p in patterns.iter().filter(|p| !p.contains(['*', '?', '!'])) {
                if !aliases.contains(p) {
                    aliases.push(p.clone());
                }
            }
        }
    }
    let hosts = aliases.into_iter().map(|a| resolve(&a, &blocks)).collect();
    ParsedConfig { hosts, warnings }
}

fn collect(
    text: &str,
    include: &dyn Fn(&str) -> Vec<String>,
    depth: usize,
    blocks: &mut Vec<Block>,
    warnings: &mut Vec<String>,
) {
    for line in text.lines() {
        let Some((keyword, args)) = tokenize(line) else { continue };
        let current = blocks.last_mut().expect("global block always present");
        match keyword.as_str() {
            "host" => blocks.push(Block { kind: BlockKind::Host(args), directives: Vec::new() }),
            "match" => {
                warnings.push(format!("Match blocks are not supported and were skipped: Match {}", args.join(" ")));
                blocks.push(Block { kind: BlockKind::Match, directives: Vec::new() });
            }
            "include" if depth >= MAX_INCLUDE_DEPTH => {
                warnings.push(format!("Include nested too deeply, stopped at: {}", args.join(" ")));
            }
            "include" => {
                for arg in &args {
                    for content in include(arg) {
                        collect(&content, include, depth + 1, blocks, warnings);
                    }
                }
            }
            "port" if args.first().and_then(|p| p.parse::<u16>().ok()).filter(|&p| p != 0).is_none() => {
                warnings.push(format!("Invalid Port {:?} ignored", args.join(" ")));
            }
            "proxycommand" => {
                let name = match &current.kind {
                    BlockKind::Host(p) => p.join(" "),
                    _ => "*".into(),
                };
                warnings.push(format!("ProxyCommand is not supported and was ignored for Host {name}"));
            }
            _ => current.directives.push((keyword, args)),
        }
    }
}

/// Splits `Keyword args...` / `Keyword=args`, honouring double quotes.
fn tokenize(line: &str) -> Option<(String, Vec<String>)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let split = line.find(|c: char| c.is_whitespace() || c == '=')?;
    let keyword = line[..split].to_ascii_lowercase();
    let rest = line[split..].trim_start().strip_prefix('=').unwrap_or(&line[split..]).trim();
    let mut args = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for c in rest.chars() {
        match c {
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !cur.is_empty() {
                    args.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        args.push(cur);
    }
    Some((keyword, args))
}

fn applies(kind: &BlockKind, alias: &str) -> bool {
    match kind {
        BlockKind::Global => true,
        BlockKind::Match => false,
        BlockKind::Host(patterns) => {
            let mut positive = false;
            for p in patterns {
                match p.strip_prefix('!') {
                    Some(neg) if glob(neg, alias) => return false,
                    Some(_) => {}
                    None => positive |= glob(p, alias),
                }
            }
            positive
        }
    }
}

fn resolve(alias: &str, blocks: &[Block]) -> ImportedHost {
    let mut h = ImportedHost { alias: alias.to_string(), ..Default::default() };
    let (mut hostname, mut jump) = (None, None);
    for b in blocks.iter().filter(|b| applies(&b.kind, alias)) {
        for (kw, args) in &b.directives {
            let Some(first) = args.first() else { continue };
            match kw.as_str() {
                "hostname" => {
                    hostname.get_or_insert_with(|| first.clone());
                }
                "user" => {
                    h.user.get_or_insert_with(|| first.clone());
                }
                "port" => {
                    if h.port.is_none() {
                        h.port = first.parse().ok();
                    }
                }
                // OpenSSH accumulates identity files rather than taking the first.
                "identityfile" => h.identity_files.push(first.clone()),
                "proxyjump" => {
                    jump.get_or_insert_with(|| {
                        if first.eq_ignore_ascii_case("none") {
                            Vec::new()
                        } else {
                            first.split(',').map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect()
                        }
                    });
                }
                "forwardagent" => {
                    h.forward_agent.get_or_insert(first.eq_ignore_ascii_case("yes"));
                }
                _ => {}
            }
        }
    }
    h.hostname = hostname.unwrap_or_else(|| alias.to_string());
    h.proxy_jump = jump.unwrap_or_default();
    h
}

/// Case-insensitive `*` / `?` wildcard match. Backtracks only to the most
/// recent `*`, so it runs in O(pattern * text) rather than exponential time.
fn glob(pattern: &str, text: &str) -> bool {
    let (p, t) = (pattern.as_bytes(), text.as_bytes());
    let (mut pi, mut ti) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while ti < t.len() {
        match p.get(pi) {
            Some(b'*') => {
                star = Some((pi, ti));
                pi += 1;
            }
            Some(&c) if c == b'?' || c.eq_ignore_ascii_case(&t[ti]) => {
                pi += 1;
                ti += 1;
            }
            _ => match star {
                // Let the last `*` swallow one more character and retry.
                Some((sp, st)) => {
                    star = Some((sp, st + 1));
                    pi = sp + 1;
                    ti = st + 1;
                }
                None => return false,
            },
        }
    }
    p[pi..].iter().all(|&c| c == b'*')
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn no_include(_: &str) -> Vec<String> {
        vec![]
    }

    fn host<'a>(cfg: &'a ParsedConfig, alias: &str) -> &'a ImportedHost {
        cfg.hosts.iter().find(|h| h.alias == alias).unwrap_or_else(|| panic!("no host {alias}: {cfg:?}"))
    }

    #[test]
    fn parses_host_hostname_user_port_identityfile() {
        let cfg = parse(
            "Host web\n  HostName 203.0.113.10\n  User deploy\n  Port 2222\n  IdentityFile ~/.ssh/web\n  IdentityFile ~/.ssh/backup\n",
            &no_include,
        );
        assert_eq!(
            cfg.hosts,
            vec![ImportedHost {
                alias: "web".into(),
                hostname: "203.0.113.10".into(),
                user: Some("deploy".into()),
                port: Some(2222),
                identity_files: vec!["~/.ssh/web".into(), "~/.ssh/backup".into()],
                ..Default::default()
            }]
        );
    }

    #[test]
    fn hostname_defaults_to_alias_and_keywords_are_case_insensitive() {
        let cfg = parse("host db\nUSER=root\n", &no_include);
        let h = host(&cfg, "db");
        assert_eq!((h.hostname.as_str(), h.user.as_deref()), ("db", Some("root")));
    }

    #[test]
    fn wildcard_host_blocks_are_skipped_but_apply_defaults() {
        let cfg = parse("Host *.internal\n  User ops\nHost a b\n  Port 22\nHost *\n  User fallback\n  Port 9\n", &no_include);
        let aliases: Vec<_> = cfg.hosts.iter().map(|h| h.alias.as_str()).collect();
        assert_eq!(aliases, ["a", "b"]);
        assert_eq!(host(&cfg, "a").user.as_deref(), Some("fallback"));
        assert_eq!(host(&cfg, "a").port, Some(22), "first value wins");
    }

    #[test]
    fn negated_patterns_exclude() {
        let cfg = parse("Host a b\nHost * !b\n  User x\n", &no_include);
        assert_eq!(host(&cfg, "a").user.as_deref(), Some("x"));
        assert_eq!(host(&cfg, "b").user, None);
    }

    #[test]
    fn include_directive_followed() {
        let include = |path: &str| -> Vec<String> {
            assert_eq!(path, "~/.ssh/config.d/*");
            vec!["Host inc\n  HostName 10.1.1.1\n".into()]
        };
        let cfg = parse("Include ~/.ssh/config.d/*\nHost top\n", &include);
        assert_eq!(host(&cfg, "inc").hostname, "10.1.1.1");
        assert!(cfg.hosts.iter().any(|h| h.alias == "top"));
    }

    #[test]
    fn recursive_include_is_bounded() {
        let include = |_: &str| -> Vec<String> { vec!["Include loop\nHost x\n".into()] };
        let cfg = parse("Include loop\n", &include);
        assert!(cfg.warnings.iter().any(|w| w.contains("Include")));
    }

    #[test]
    fn proxyjump_becomes_chain() {
        let cfg = parse("Host t\n  ProxyJump bastion,ops@10.0.0.1:2222\nHost n\n  ProxyJump none\n", &no_include);
        assert_eq!(host(&cfg, "t").proxy_jump, ["bastion", "ops@10.0.0.1:2222"]);
        assert!(host(&cfg, "n").proxy_jump.is_empty());
    }

    #[test]
    fn proxycommand_ignored_with_warning() {
        let cfg = parse("Host t\n  ProxyCommand ssh -W %h:%p jump\n", &no_include);
        assert!(cfg.warnings.iter().any(|w| w.contains("ProxyCommand") && w.contains('t')));
    }

    #[test]
    fn match_blocks_skipped_with_warning() {
        let cfg = parse("Match host foo\n  User nope\nHost a\n", &no_include);
        assert_eq!(host(&cfg, "a").user, None);
        assert!(cfg.warnings.iter().any(|w| w.contains("Match")));
    }

    #[test]
    fn comments_quotes_and_forward_agent() {
        let cfg = parse("# hi\nHost \"q\"\n  IdentityFile \"/path with space/k\"\n  ForwardAgent yes\n", &no_include);
        let h = host(&cfg, "q");
        assert_eq!(h.identity_files, ["/path with space/k"]);
        assert_eq!(h.forward_agent, Some(true));
    }

    #[test]
    fn invalid_port_warns() {
        let cfg = parse("Host a\n  Port 99999\n", &no_include);
        assert_eq!(host(&cfg, "a").port, None);
        assert_eq!(cfg.warnings.len(), 1);
    }

    #[test]
    fn glob_is_not_exponential() {
        let start = std::time::Instant::now();
        // No 'b' in the text forces a naive backtracker through every split.
        assert!(!glob(&format!("{}*b", "*a".repeat(12)), &"a".repeat(40)));
        assert!(glob("web-*.prod.?", "web-01.prod.x"));
        assert!(start.elapsed() < std::time::Duration::from_millis(200), "took {:?}", start.elapsed());
    }

    proptest! {
        #[test]
        fn proptest_parser_never_panics(text in "\\PC{0,400}") {
            let _ = parse(&text, &no_include);
        }

        #[test]
        fn proptest_structured_input_never_panics(lines in prop::collection::vec("(Host|HostName|Port|User|ProxyJump|Include|Match)[ =][ -~]{0,30}", 0..20)) {
            let _ = parse(&lines.join("\n"), &|_| vec!["Host x\n".to_string()]);
        }
    }
}
