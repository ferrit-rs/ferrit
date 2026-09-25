//! The hidden command-line side of the harness: `--replay`, `--fixture`,
//! `--into`, `--dump-frames`, `--size` and `--tape`. Test-only: hidden from
//! `--help`, and refused in a release build unless `FERRIT_TEST` is set, so
//! they never matter in normal use.

use std::path::{Path, PathBuf};

use super::fixture::Fixture;
use super::runner::{self, Options};
use super::{script, tape};

/// The flags, as `main` parsed them.
#[derive(Debug, Clone, Default)]
pub struct Args {
    pub replay: Option<PathBuf>,
    pub fixture: Option<String>,
    pub into: Option<PathBuf>,
    pub dump_frames: Option<PathBuf>,
    pub size: Option<String>,
    pub tape: Option<PathBuf>,
}

impl Args {
    /// Was any harness flag given?
    pub fn any(&self) -> bool {
        self.replay.is_some() || self.fixture.is_some() || self.tape.is_some()
    }
}

/// Debug builds, or `FERRIT_TEST` in the environment.
fn enabled() -> bool {
    cfg!(debug_assertions) || std::env::var_os("FERRIT_TEST").is_some()
}

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .map_or_else(|| "script".to_owned(), |s| s.to_string_lossy().into_owned())
}

/// Run what the flags ask for. The text to print on success, or the reason.
pub fn run(args: &Args) -> Result<String, String> {
    if !enabled() {
        return Err(
            "--replay, --fixture and --tape are test-only: build with debug \
                    assertions or set FERRIT_TEST"
                .to_owned(),
        );
    }
    if let Some(path) = &args.tape {
        let script = script::parse(&read(path)?).map_err(|e| format!("{}: {e}", path.display()))?;
        return tape::tape(&stem(path), &script);
    }
    if let Some(path) = &args.replay {
        return replay(path, args);
    }
    let name = args
        .fixture
        .as_deref()
        .ok_or("no --replay, --fixture or --tape")?;
    let root = match &args.into {
        Some(dir) => dir.clone(),
        None => std::env::temp_dir().join(format!("ferrit-fixture-{}", std::process::id())),
    };
    let fixture = Fixture::build(name, Some(&root))?;
    Ok(fixture.dir.display().to_string())
}

fn parse_size(text: &str) -> Result<(u16, u16), String> {
    let parsed = text.split_once('x').and_then(|(w, h)| {
        Some((
            w.parse::<u16>().ok().filter(|&n| n > 0)?,
            h.parse::<u16>().ok().filter(|&n| n > 0)?,
        ))
    });
    parsed.ok_or_else(|| format!("`{text}` is not WxH"))
}

fn replay(path: &Path, args: &Args) -> Result<String, String> {
    let script = script::parse(&read(path)?).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut options = Options {
        fixture: args.fixture.clone(),
        keep_fixture_in: args.into.clone(),
        ..Options::default()
    };
    if let Some(size) = &args.size {
        options.size = parse_size(size)?;
    }
    let outcome = runner::run(&script, &options)
        .map_err(|failure| format!("{}: {failure}\n\n{}", path.display(), failure.frame))?;
    if let Some(dir) = &args.dump_frames {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        for frame in &outcome.frames {
            let file = dir.join(format!("{:03}-{}.txt", frame.index, frame.label));
            std::fs::write(&file, &frame.text).map_err(|e| format!("{}: {e}", file.display()))?;
        }
    }
    Ok(format!(
        "{}: ok, {} frame(s)",
        path.display(),
        outcome.frames.len()
    ))
}
