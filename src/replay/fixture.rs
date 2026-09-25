//! Named throwaway repositories for replay scripts. Deterministic: fixed
//! author, fixed commit dates, local configuration for every knob a user's own
//! `git config` could change, so two builds have the same commit ids and a
//! frame that shows one is the same on every machine.
//!
//! Never the real working tree: every fixture is a fresh directory, removed on
//! drop unless it was built `--into` a directory the caller chose.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

/// The fixtures `Fixture::build` knows.
pub const NAMES: &[&str] = &["canonical", "history", "conflict", "detached", "remote"];

/// `2026-01-01 12:00:00 UTC`; each commit is one minute later.
const EPOCH: u64 = 1_767_268_800;

static COUNTER: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug)]
pub struct Fixture {
    /// The directory that holds the fixture (removed on drop unless kept).
    pub root: PathBuf,
    /// The repository a script works in.
    pub dir: PathBuf,
    /// `remote` only: the bare repository `dir` was cloned from.
    pub origin: Option<PathBuf>,
    /// `remote` only: a second clone, for changes made "from elsewhere".
    pub other: Option<PathBuf>,
    keep: bool,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if !self.keep {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
}

/// A repository being built: runs `git` there with the fixture environment and
/// hands out one commit time at a time.
struct Builder {
    dir: PathBuf,
    tick: u64,
}

impl Builder {
    fn git(&self, args: &[&str]) -> Result<String, String> {
        run_git(&self.dir, args, self.tick).and_then(|out| {
            if out.status.success() {
                Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
            } else {
                Err(format!(
                    "git {args:?} failed: {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                ))
            }
        })
    }

    /// Like `git`, but a non-zero exit is fine (a merge that conflicts).
    fn git_may_fail(&self, args: &[&str]) -> Result<(), String> {
        run_git(&self.dir, args, self.tick).map(|_| ())
    }

    fn write(&self, relative: &str, content: &str) -> Result<(), String> {
        let path = self.dir.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, content).map_err(|e| format!("{}: {e}", path.display()))
    }

    fn commit_all(&mut self, message: &str) -> Result<(), String> {
        self.tick += 60;
        self.git(&["add", "-A"])?;
        self.git(&["commit", "-q", "-m", message])?;
        Ok(())
    }

    /// Local settings for everything a user's global config could change.
    fn configure(&self) -> Result<(), String> {
        for (key, value) in [
            ("user.name", "Ferrit Fixture"),
            ("user.email", "fixture@ferrit.invalid"),
            ("commit.gpgsign", "false"),
            ("tag.gpgsign", "false"),
            ("core.autocrlf", "false"),
            ("core.editor", "true"),
            ("pull.rebase", "false"),
            ("push.default", "simple"),
            ("init.defaultBranch", "main"),
        ] {
            self.git(&["config", key, value])?;
        }
        Ok(())
    }
}

/// `git -C dir args` with the fixture environment.
fn run_git(dir: &Path, args: &[&str], tick: u64) -> Result<std::process::Output, String> {
    let when = format!("{} +0000", EPOCH + tick);
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "Ferrit Fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@ferrit.invalid")
        .env("GIT_AUTHOR_DATE", &when)
        .env("GIT_COMMITTER_NAME", "Ferrit Fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@ferrit.invalid")
        .env("GIT_COMMITTER_DATE", &when)
        .env("GIT_EDITOR", "true")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|e| format!("cannot run git: {e}"))
}

/// `git args` in `dir`, for a script's `git ... -> "..."` check and `exec`.
/// The output, and whether it succeeded.
pub fn git_output(dir: &Path, args: &[&str]) -> Result<(String, bool), String> {
    run_git(dir, args, 0).map(|out| {
        let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&out.stderr));
        // Only the end is trimmed: the first column of `--porcelain` is a space.
        (text.trim_end().to_owned(), out.status.success())
    })
}

impl Fixture {
    /// Build `name`. `into`: a directory to build in and keep; otherwise a
    /// fresh temporary one, removed when the `Fixture` is dropped.
    pub fn build(name: &str, into: Option<&Path>) -> Result<Self, String> {
        if !NAMES.contains(&name) {
            return Err(format!(
                "unknown fixture `{name}` (known: {})",
                NAMES.join(", ")
            ));
        }
        let (root, keep) = match into {
            Some(dir) => (dir.to_path_buf(), true),
            None => (
                std::env::temp_dir().join(format!(
                    "ferrit-replay-{}-{}",
                    std::process::id(),
                    COUNTER.fetch_add(1, Ordering::Relaxed)
                )),
                false,
            ),
        };
        std::fs::create_dir_all(&root).map_err(|e| format!("{}: {e}", root.display()))?;
        let mut fixture = Self {
            dir: root.join(name),
            root,
            origin: None,
            other: None,
            keep,
        };
        match name {
            "canonical" => canonical(&fixture)?,
            "history" => {
                history(&fixture.dir)?;
            },
            "conflict" => conflict(&fixture.dir)?,
            "detached" => {
                let builder = history(&fixture.dir)?;
                builder.git(&["checkout", "-q", "--detach"])?;
            },
            "remote" => remote(&mut fixture)?,
            _ => {},
        }
        Ok(fixture)
    }

    /// Replace `{dir}`, `{origin}` and `{other}` in `text` with this fixture's paths.
    pub fn expand(&self, text: &str) -> String {
        let mut out = text.replace(&placeholder("dir"), &self.dir.to_string_lossy());
        if let Some(origin) = &self.origin {
            out = out.replace(&placeholder("origin"), &origin.to_string_lossy());
        }
        if let Some(other) = &self.other {
            out = out.replace(&placeholder("other"), &other.to_string_lossy());
        }
        out
    }
}

/// `{name}`, the form scripts write.
fn placeholder(name: &str) -> String {
    format!("{{{name}}}")
}

fn init(dir: &Path) -> Result<Builder, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let builder = Builder {
        dir: dir.to_path_buf(),
        tick: 0,
    };
    builder.git(&["init", "-q", "-b", "main"])?;
    builder.configure()?;
    Ok(builder)
}

/// The screen of `docs/PLAN_1_LAYOUT.md`: four commits on `main`, two other
/// branches, one modified, one untracked and one staged file, an empty stash.
fn canonical(fixture: &Fixture) -> Result<(), String> {
    let mut b = init(&fixture.dir)?;
    let main_rs = |edited: bool| -> String {
        let greeting = if edited {
            "    println!(\"ferrit {}\", 1);"
        } else {
            "    println!(\"ferrit\");"
        };
        let mut lines = vec![
            "fn main() {".to_owned(),
            greeting.to_owned(),
            "}".to_owned(),
            String::new(),
        ];
        for n in 0..60 {
            lines.push(if edited && n == 50 {
                "fn step_50() { todo!() }".to_owned()
            } else {
                format!("fn step_{n:02}() {{}}")
            });
        }
        let mut text = lines.join("\n");
        text.push('\n');
        text
    };
    b.write("README.md", "# canonical\n")?;
    b.write("Cargo.toml", "[package]\nname = \"canonical\"\n")?;
    b.write("src/main.rs", &main_rs(false))?;
    b.write("docs/PLAN.md", "plan v1\n")?;
    b.commit_all("chore: initial commit")?;
    b.write("docs/PLAN.md", "plan v2\n")?;
    b.commit_all("docs: drop the arch stub")?;
    b.git(&["branch", "fix/parse-args"])?;
    b.write("docs/INSPIRATION.md", "notes\n")?;
    b.commit_all("docs: add inspiration notes")?;
    b.git(&["branch", "feat/tui-skeleton"])?;
    b.write("docs/PLAN.md", "plan v3\n")?;
    b.commit_all("docs: expand the layout plan")?;
    b.write("src/main.rs", &main_rs(true))?;
    b.write("docs/notes.md", "scratch\n")?;
    b.write("Cargo.lock", "# lock\n")?;
    b.git(&["add", "Cargo.lock"])?;
    Ok(())
}

/// `f` written `base`, `one`, `two`, `three`, one commit each.
fn history(dir: &Path) -> Result<Builder, String> {
    let mut b = init(dir)?;
    for content in ["base", "one", "two", "three"] {
        b.write("f", &format!("{content}\n"))?;
        b.commit_all(content)?;
    }
    Ok(b)
}

/// `history`, then a merge of a branch that rewrote `f`: `f` is `UU`.
fn conflict(dir: &Path) -> Result<(), String> {
    let mut b = history(dir)?;
    b.git(&["checkout", "-q", "-b", "side", "HEAD~2"])?;
    b.write("f", "side\n")?;
    b.commit_all("side")?;
    b.git(&["checkout", "-q", "main"])?;
    b.git_may_fail(&["merge", "side"])
}

/// A bare `origin`, a clone `remote` with three commits pushed and tracking it,
/// and a second clone `other`.
fn remote(fixture: &mut Fixture) -> Result<(), String> {
    let origin = fixture.root.join("origin.git");
    let other = fixture.root.join("other");
    std::fs::create_dir_all(&origin).map_err(|e| e.to_string())?;
    let bare = Builder {
        dir: origin.clone(),
        tick: 0,
    };
    bare.git(&["init", "-q", "--bare", "-b", "main"])?;

    let mut work = Builder {
        dir: fixture.dir.clone(),
        tick: 0,
    };
    std::fs::create_dir_all(&fixture.dir).map_err(|e| e.to_string())?;
    work.git(&["init", "-q", "-b", "main"])?;
    work.configure()?;
    work.git(&["remote", "add", "origin", &origin.to_string_lossy()])?;
    for content in ["base", "one", "two"] {
        work.write("f", &format!("{content}\n"))?;
        work.commit_all(content)?;
    }
    work.git(&["push", "-q", "-u", "origin", "main"])?;

    let clone = Builder {
        dir: other.clone(),
        tick: 600,
    };
    std::fs::create_dir_all(&other).map_err(|e| e.to_string())?;
    clone.git(&[
        "clone",
        "-q",
        &origin.to_string_lossy(),
        &other.to_string_lossy(),
    ])?;
    clone.configure()?;
    fixture.origin = Some(origin);
    fixture.other = Some(other);
    Ok(())
}
