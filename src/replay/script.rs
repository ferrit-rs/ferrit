//! The script format. One directive per line, `#` starts a comment:
//!
//! ```text
//! fixture canonical
//! size 120x40
//! key 2 j space              # focus Files, move, stage
//! expect-text "A  src/main.rs"
//! snapshot files-staged
//! git status --porcelain=v2 -> "1 M."
//! ```
//!
//! Strings are `"double quoted"` (with `\"`, `\\`, `\n`, `\t`) or
//! `'single quoted'` (literal). Inside `exec`, `write`, `git` and `config`,
//! `{dir}`, `{origin}` and `{other}` stand for the fixture's paths.

use crate::app::keymap::KeyBinding;

/// How a `git ... -> "..."` check compares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expect {
    /// `->`: the output contains the text.
    Contains(String),
    /// `=>`: the trimmed output is exactly the text.
    Exact(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Directive {
    /// `fixture NAME`: which repository to build (see `fixture::names`).
    Fixture(String),
    Size(u16, u16),
    Resize(u16, u16),
    /// `key A B C`: each key in turn, in `KeyBinding::parse` syntax.
    Key(Vec<KeyBinding>),
    /// `async-key K`: the key, then wait for background work to finish.
    AsyncKey(KeyBinding),
    /// `type "text"`: each character as a key (`\n` is Enter).
    Type(String),
    Refresh,
    /// `exec ARGS...`: run `git ARGS...` in the fixture, no assertion.
    Exec(Vec<String>),
    /// `write PATH "content"`: replace a file in the worktree.
    Write {
        path: String,
        content: String,
    },
    /// `config "toml"`: reopen the app with this `config.toml` text.
    Config(String),
    /// `snapshot LABEL`: keep the current frame, named.
    Snapshot(String),
    ExpectText(String),
    ExpectNoText(String),
    /// `git ARGS... -> "text"` or `=> "text"`.
    Git {
        args: Vec<String>,
        expect: Expect,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    /// 1-based line in the script.
    pub line: usize,
    pub directive: Directive,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Script {
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

/// A word or a quoted string from one line.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Token {
    text: String,
    quoted: bool,
}

fn tokenize(line: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut chars = line.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }
        if c == '#' {
            break;
        }
        // One token: bare characters and quoted parts, joined the way a shell
        // joins them (`--format="->"` is one word). It counts as quoted if any
        // part was, so a quoted `->` is never taken for the arrow.
        let mut text = String::new();
        let mut quoted = false;
        while let Some(&next) = chars.peek() {
            if next.is_whitespace() || next == '#' {
                break;
            }
            chars.next();
            if next == '"' || next == '\'' {
                quoted = true;
                let mut closed = false;
                while let Some(inner) = chars.next() {
                    if inner == next {
                        closed = true;
                        break;
                    }
                    if next == '"' && inner == '\\' {
                        match chars.next() {
                            Some('n') => text.push('\n'),
                            Some('t') => text.push('\t'),
                            Some('"') => text.push('"'),
                            Some('\\') => text.push('\\'),
                            Some(other) => return Err(format!("unknown escape `\\{other}`")),
                            None => return Err("a string ends in a lone backslash".to_owned()),
                        }
                    } else {
                        text.push(inner);
                    }
                }
                if !closed {
                    return Err("unterminated string".to_owned());
                }
            } else {
                text.push(next);
            }
        }
        tokens.push(Token { text, quoted });
    }
    Ok(tokens)
}

fn size(text: &str) -> Result<(u16, u16), String> {
    let (w, h) = text
        .split_once('x')
        .ok_or_else(|| format!("`{text}` is not WxH"))?;
    let parse = |part: &str| {
        part.parse::<u16>()
            .ok()
            .filter(|&n| n > 0)
            .ok_or_else(|| format!("`{text}` is not WxH"))
    };
    Ok((parse(w)?, parse(h)?))
}

fn key(text: &str) -> Result<KeyBinding, String> {
    KeyBinding::parse(text).ok_or_else(|| format!("`{text}` is not a key"))
}

fn exactly_one<'a>(name: &str, args: &'a [Token]) -> Result<&'a Token, String> {
    match args {
        [only] => Ok(only),
        _ => Err(format!("`{name}` takes exactly one argument")),
    }
}

fn directive(tokens: &[Token]) -> Result<Directive, String> {
    let Some((head, args)) = tokens.split_first() else {
        return Err("empty".to_owned());
    };
    match head.text.as_str() {
        "fixture" => Ok(Directive::Fixture(
            exactly_one("fixture", args)?.text.clone(),
        )),
        "size" => {
            let (w, h) = size(&exactly_one("size", args)?.text)?;
            Ok(Directive::Size(w, h))
        },
        "resize" => {
            let (w, h) = size(&exactly_one("resize", args)?.text)?;
            Ok(Directive::Resize(w, h))
        },
        "key" => {
            if args.is_empty() {
                return Err("`key` needs at least one key".to_owned());
            }
            args.iter()
                .map(|token| key(&token.text))
                .collect::<Result<Vec<_>, _>>()
                .map(Directive::Key)
        },
        "async-key" => Ok(Directive::AsyncKey(key(
            &exactly_one("async-key", args)?.text
        )?)),
        "type" => Ok(Directive::Type(exactly_one("type", args)?.text.clone())),
        "refresh" if args.is_empty() => Ok(Directive::Refresh),
        "refresh" => Err("`refresh` takes no argument".to_owned()),
        "exec" => {
            if args.is_empty() {
                return Err("`exec` needs git arguments".to_owned());
            }
            Ok(Directive::Exec(
                args.iter().map(|t| t.text.clone()).collect(),
            ))
        },
        "write" => match args {
            [path, content] => Ok(Directive::Write {
                path: path.text.clone(),
                content: content.text.clone(),
            }),
            _ => Err("`write` takes a path and a string".to_owned()),
        },
        "config" => Ok(Directive::Config(exactly_one("config", args)?.text.clone())),
        "snapshot" => {
            let label = &exactly_one("snapshot", args)?.text;
            if label.is_empty()
                || !label
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            {
                return Err(format!(
                    "`{label}` is not a label (letters, digits, - and _)"
                ));
            }
            Ok(Directive::Snapshot(label.clone()))
        },
        "expect-text" => Ok(Directive::ExpectText(
            exactly_one("expect-text", args)?.text.clone(),
        )),
        "expect-no-text" => Ok(Directive::ExpectNoText(
            exactly_one("expect-no-text", args)?.text.clone(),
        )),
        "git" => git_check(args),
        other => Err(format!("unknown directive `{other}`")),
    }
}

/// `git ARGS... (->|=>) "expected"`.
fn git_check(args: &[Token]) -> Result<Directive, String> {
    let arrow = args
        .iter()
        .rposition(|t| !t.quoted && (t.text == "->" || t.text == "=>"))
        .ok_or_else(|| "`git` needs `-> \"text\"` or `=> \"text\"`".to_owned())?;
    let (before, after) = args.split_at(arrow);
    let Some(arrow_token) = after.first() else {
        return Err("`git` needs an expectation".to_owned());
    };
    let expected = match after.get(1..) {
        Some([only]) => only.text.clone(),
        _ => return Err("the arrow is followed by exactly one string".to_owned()),
    };
    if before.is_empty() {
        return Err("`git` needs arguments before the arrow".to_owned());
    }
    let expect = if arrow_token.text == "->" {
        Expect::Contains(expected)
    } else {
        Expect::Exact(expected)
    };
    Ok(Directive::Git {
        args: before.iter().map(|t| t.text.clone()).collect(),
        expect,
    })
}

/// Parse a whole script. The first error stops it, with its line.
pub fn parse(text: &str) -> Result<Script, ParseError> {
    let mut steps = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let line = index + 1;
        let tokens = tokenize(raw).map_err(|message| ParseError { line, message })?;
        if tokens.is_empty() {
            continue;
        }
        let directive = directive(&tokens).map_err(|message| ParseError { line, message })?;
        steps.push(Step { line, directive });
    }
    Ok(Script { steps })
}
