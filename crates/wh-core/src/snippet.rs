//! Snippet templating: `{{name}}` placeholders filled in at run time.

use std::collections::BTreeMap;

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SnippetError {
    #[error("missing values for: {}", .0.join(", "))]
    Missing(Vec<String>),
}

enum Token<'a> {
    Text(&'a str),
    Var(&'a str),
}

fn is_name(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn tokens(script: &str) -> Vec<Token<'_>> {
    let mut out = Vec::new();
    let mut rest = script;
    while let Some(open) = rest.find("{{") {
        let after = &rest[open + 2..];
        let Some(close) = after.find("}}") else { break };
        let name = after[..close].trim();
        if is_name(name) {
            out.push(Token::Text(&rest[..open]));
            out.push(Token::Var(name));
            rest = &after[close + 2..];
        } else {
            // Not a placeholder: keep "{{" literally and continue after it.
            out.push(Token::Text(&rest[..open + 2]));
            rest = after;
        }
    }
    out.push(Token::Text(rest));
    out
}

/// Placeholder names in order of first appearance, without duplicates.
pub fn variables(script: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for t in tokens(script) {
        if let Token::Var(n) = t {
            if !names.iter().any(|x| x == n) {
                names.push(n.to_string());
            }
        }
    }
    names
}

/// Substitutes every placeholder verbatim (no shell escaping).
pub fn render(script: &str, vars: &BTreeMap<String, String>) -> Result<String, SnippetError> {
    let missing: Vec<String> = variables(script).into_iter().filter(|n| !vars.contains_key(n)).collect();
    if !missing.is_empty() {
        return Err(SnippetError::Missing(missing));
    }
    Ok(tokens(script)
        .into_iter()
        .map(|t| match t {
            Token::Text(s) => s,
            Token::Var(n) => vars[n].as_str(),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn lists_variables_in_order_without_duplicates() {
        assert_eq!(variables("cp {{src}} {{ dst }} && ls {{src}}"), ["src", "dst"]);
        assert!(variables("echo plain").is_empty());
    }

    #[test]
    fn snippet_substitutes_named_variables() {
        let out = render("systemctl {{action}} {{ unit }}", &vars(&[("action", "restart"), ("unit", "nginx")]));
        assert_eq!(out.unwrap(), "systemctl restart nginx");
    }

    #[test]
    fn snippet_missing_variable_errors() {
        let err = render("{{a}} {{b}} {{c}}", &vars(&[("b", "x")])).unwrap_err();
        assert_eq!(err, SnippetError::Missing(vec!["a".into(), "c".into()]));
    }

    #[test]
    fn values_are_inserted_verbatim() {
        assert_eq!(render("echo {{v}}", &vars(&[("v", "$HOME; {{x}}")])).unwrap(), "echo $HOME; {{x}}");
    }

    #[test]
    fn non_placeholders_are_left_alone() {
        // Shell/awk braces, unclosed and invalid names are not placeholders.
        let script = "awk '{print $1}' {{ unclosed and {{bad name}} ${HOME}";
        assert!(variables(script).is_empty());
        assert_eq!(render(script, &BTreeMap::new()).unwrap(), script);
    }
}
