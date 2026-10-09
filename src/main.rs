use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use color_eyre::Result;

use ferrit::config::Config;
use ferrit::tui::App;
use ferrit::tui::terminal as tui;

/// The everyday git manager for the terminal: a full TUI for your repository, and
/// an empty folder to GitHub without leaving it.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Path to the git repository to open. Without it ferrit opens the folder
    /// it is run in, and offers `git init` there when it is not a repository.
    #[arg(short, long)]
    path: Option<PathBuf>,

    /// Print where the configuration file is (or would be) and exit.
    #[arg(long)]
    config_path: bool,

    // Test-only harness flags (`ferrit::replay::cli`), hidden from `--help` and
    // only there when built with `--features test-util`.
    #[cfg(feature = "test-util")]
    #[arg(long, hide = true, value_name = "SCRIPT")]
    replay: Option<PathBuf>,
    #[cfg(feature = "test-util")]
    #[arg(long, hide = true, value_name = "NAME")]
    fixture: Option<String>,
    #[cfg(feature = "test-util")]
    #[arg(long, hide = true, value_name = "DIR")]
    into: Option<PathBuf>,
    #[cfg(feature = "test-util")]
    #[arg(long, hide = true, value_name = "DIR")]
    dump_frames: Option<PathBuf>,
    #[cfg(feature = "test-util")]
    #[arg(long, hide = true, value_name = "WxH")]
    size: Option<String>,
    #[cfg(feature = "test-util")]
    #[arg(long, hide = true, value_name = "SCRIPT")]
    tape: Option<PathBuf>,
}

fn main() -> Result<ExitCode> {
    // ssh or git running ferrit as its askpass helper: answer and leave,
    // before clap sees the prompt as an argument.
    if let Some(code) = ferrit::git::askpass::run_helper(std::env::args().skip(1)) {
        return Ok(code);
    }
    color_eyre::install()?;
    let cli = Cli::parse();

    #[cfg(feature = "test-util")]
    let harness = ferrit::replay::cli::Args {
        replay: cli.replay,
        fixture: cli.fixture,
        into: cli.into,
        dump_frames: cli.dump_frames,
        size: cli.size,
        tape: cli.tape,
    };
    #[cfg(feature = "test-util")]
    if harness.any() {
        let printed = ferrit::replay::cli::run(&harness)
            .map_err(|message| color_eyre::eyre::eyre!(message))?;
        writeln!(std::io::stdout(), "{}", printed.trim_end())?;
        return Ok(ExitCode::SUCCESS);
    }

    if cli.config_path {
        let location = Config::default_path().map_or_else(
            || "no config directory".to_owned(),
            |p| p.display().to_string(),
        );
        writeln!(std::io::stdout(), "{location}")?;
        return Ok(ExitCode::SUCCESS);
    }

    // Open the repo before touching the terminal, so a non-repo path named with
    // `--path` is a plain one-line message and a non-zero exit, no alt-screen
    // garbage. With no `--path`, a folder that is not a repository opens the
    // welcome screen instead (`docs/PLAN_16_START_WITHOUT_REPO.md`).
    let explicit = cli.path.is_some();
    let path = cli.path.unwrap_or_else(|| PathBuf::from("."));
    let mut app = match App::open_or_welcome(&path, explicit, Config::load()) {
        Ok(app) => app,
        Err(e) => {
            // The TUI has not taken the screen yet: stderr is the only channel,
            // and a non-zero exit is the contract for "could not open the repo".
            #[allow(
                clippy::print_stderr,
                clippy::exit,
                reason = "startup failure path, before tui::init: stderr + non-zero exit is the contract"
            )]
            {
                eprintln!("ferrit: {e}");
                std::process::exit(1);
            }
        },
    };

    // Ask the terminal whether it speaks a graphics protocol, before the
    // alternate screen is up. Falls back to half-blocks on its own.
    app.detect_graphics();
    // The painted theme is RGB: a terminal without 24-bit colour gets the nearest
    // of its 256 (`docs/PLAN_18_THEMES.md`).
    app.set_color_depth(ferrit::theme::scheme::ColorDepth::detect(
        std::env::var("COLORTERM").ok().as_deref(),
    ));

    let mut terminal = tui::init(app.mouse_enabled())?;
    let result = app.run(&mut terminal);
    tui::restore()?;
    result.map(|()| ExitCode::SUCCESS)
}
