mod app;
mod tui;

use std::path::PathBuf;

use clap::Parser;
use color_eyre::Result;

use crate::app::App;

/// A lazygit-style terminal UI for git, written in Rust.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Path to the git repository to open (wired up in phase 2).
    #[arg(short, long, default_value = ".")]
    path: PathBuf,
}

fn main() -> Result<()> {
    color_eyre::install()?;
    let cli = Cli::parse();
    let _ = cli.path;

    let mut terminal = tui::init()?;
    let result = App::new().run(&mut terminal);
    tui::restore()?;
    result
}
