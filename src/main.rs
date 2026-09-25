use std::io::Write;
use std::path::PathBuf;

use clap::Parser;
use color_eyre::Result;

use ferrit::app::App;
use ferrit::app::config::Config;
use ferrit::app::terminal as tui;

/// A lazygit-style terminal UI for git, written in Rust.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Path to the git repository to open.
    #[arg(short, long, default_value = ".")]
    path: PathBuf,

    /// Print where the configuration file is (or would be) and exit.
    #[arg(long)]
    config_path: bool,

    // Test-only harness flags (`ferrit::replay::cli`), hidden from `--help`.
    #[arg(long, hide = true, value_name = "SCRIPT")]
    replay: Option<PathBuf>,
    #[arg(long, hide = true, value_name = "NAME")]
    fixture: Option<String>,
    #[arg(long, hide = true, value_name = "DIR")]
    into: Option<PathBuf>,
    #[arg(long, hide = true, value_name = "DIR")]
    dump_frames: Option<PathBuf>,
    #[arg(long, hide = true, value_name = "WxH")]
    size: Option<String>,
    #[arg(long, hide = true, value_name = "SCRIPT")]
    tape: Option<PathBuf>,
}

fn main() -> Result<()> {
    color_eyre::install()?;
    let cli = Cli::parse();

    let harness = ferrit::replay::cli::Args {
        replay: cli.replay,
        fixture: cli.fixture,
        into: cli.into,
        dump_frames: cli.dump_frames,
        size: cli.size,
        tape: cli.tape,
    };
    if harness.any() {
        let printed = ferrit::replay::cli::run(&harness)
            .map_err(|message| color_eyre::eyre::eyre!(message))?;
        writeln!(std::io::stdout(), "{}", printed.trim_end())?;
        return Ok(());
    }

    if cli.config_path {
        let location = Config::default_path().map_or_else(
            || "no config directory".to_owned(),
            |p| p.display().to_string(),
        );
        writeln!(std::io::stdout(), "{location}")?;
        return Ok(());
    }

    // Open the repo before touching the terminal, so a non-repo path is a
    // plain one-line message and a non-zero exit, no alt-screen garbage.
    let mut app = match App::open_with(&cli.path, Config::load()) {
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

    let mut terminal = tui::init(app.mouse_enabled())?;
    let result = app.run(&mut terminal);
    tui::restore()?;
    result
}
