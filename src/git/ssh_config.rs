//! The SSH host aliases a user has made for GitHub (`docs/PLAN_15_CREATE_REMOTE.md`):
//! `Host github.com-personal` with `HostName github.com` in `~/.ssh/config`,
//! the way several accounts share one machine. A remote written with the plain
//! host (`git@github.com:…`) never offers the alias's key; one written with the
//! alias (`git@github.com-personal:…`) does, and ferrit's passphrase popup then
//! answers for it. This reads the file; it never writes it.

use std::path::Path;

/// The aliases of `config` (the text of an ssh config) that point at
/// `github.com`, in the order they appear, without duplicates. Patterns with a
/// wildcard or a negation are not aliases, and neither is `github.com` itself.
/// `Include` and `Match` are not followed: a `Match` ends the block before it.
#[must_use]
pub fn github_aliases(config: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let mut patterns: Vec<String> = Vec::new();
    let mut hostname: Option<String> = None;

    let mut flush = |patterns: &mut Vec<String>, hostname: &mut Option<String>| {
        if hostname.as_deref() == Some("github.com") {
            for pattern in patterns.iter() {
                let plain = !pattern.contains(['*', '?', '!']);
                if plain && pattern != "github.com" && !found.contains(pattern) {
                    found.push(pattern.clone());
                }
            }
        }
        patterns.clear();
        *hostname = None;
    };

    for line in config.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // `Keyword value` or `Keyword=value`.
        let split = line.find(|c: char| c.is_whitespace() || c == '=');
        let (keyword, rest) = split.map_or((line, ""), |at| {
            (
                line.get(..at).unwrap_or_default(),
                line.get(at..)
                    .unwrap_or_default()
                    .trim_start_matches(|c: char| c.is_whitespace() || c == '='),
            )
        });
        match keyword.to_ascii_lowercase().as_str() {
            "host" => {
                flush(&mut patterns, &mut hostname);
                patterns = rest.split_whitespace().map(unquote).collect();
            },
            "match" => flush(&mut patterns, &mut hostname),
            "hostname" => hostname = Some(unquote(rest.trim()).to_ascii_lowercase()),
            _ => {},
        }
    }
    flush(&mut patterns, &mut hostname);
    found
}

fn unquote(text: &str) -> String {
    text.trim_matches('"').to_owned()
}

/// `github_aliases` of the file at `path`; none when it is absent or unreadable.
#[must_use]
pub fn read_github_aliases(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .map(|text| github_aliases(&text))
        .unwrap_or_default()
}
