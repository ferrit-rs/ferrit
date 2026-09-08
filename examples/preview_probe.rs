#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::exit,
    reason = "throwaway example: prints to stdout/stderr, exits non-zero on failed setup"
)]
//! Headless probe: open the repo in the current directory, walk the Files
//! pane, and print what the right-pane preview resolves to for each entry.
//! Handy for checking the image path without a real terminal.
//!
//! `cargo run --example preview_probe`

use std::path::Path;

use ferrit::app::{App, Pane};
use ferrit::image::preview::Preview;

fn main() {
    let mut app = match App::open(Path::new(".")) {
        Ok(app) => app,
        Err(e) => {
            eprintln!("open failed: {e}");
            std::process::exit(1);
        },
    };

    app.select(Pane::Files, 0);
    let rows = app.row_count(Pane::Files);
    if rows == 0 {
        println!("Files pane is empty (working tree clean).");
        return;
    }

    for i in 0..rows {
        app.select(Pane::Files, i);
        let kind: String = match app.preview() {
            Preview::None => "none (mock diff shown)".to_owned(),
            Preview::Note(msg) => format!("note: {msg}"),
            Preview::Image(_) => "IMAGE (decoded, right pane renders it)".to_owned(),
        };
        println!("[{i}] {}  ->  {kind}", app.file_display(i));
    }
}
