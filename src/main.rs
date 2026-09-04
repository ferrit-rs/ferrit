use std::path::PathBuf;

use clap::Parser;
use color_eyre::Result;

use ferrit::app::App;
use ferrit::tui;

/// A lazygit-style terminal UI for git, written in Rust.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Path to the git repository to open.
    #[arg(short, long, default_value = ".")]
    path: PathBuf,
}

fn main() -> Result<()> {
    color_eyre::install()?;
    let cli = Cli::parse();

    // Open the repo before touching the terminal, so a non-repo path is a
    // plain one-line message and a non-zero exit, no alt-screen garbage.
    let mut app = match App::open(&cli.path) {
        Ok(app) => app,
        Err(e) => {
            eprintln!("ferrit: {e}");
            std::process::exit(1);
        }
    };

    // Ask the terminal whether it speaks a graphics protocol, before the
    // alternate screen is up. Falls back to half-blocks on its own.
    app.detect_graphics();

    let mut terminal = tui::init()?;
    let result = app.run(&mut terminal);
    tui::restore()?;
    result
}
