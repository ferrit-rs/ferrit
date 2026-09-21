#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::print_stdout,
    reason = "throwaway example: prints to stdout, panics on failed setup"
)]
//! Throwaway: render one frame to stdout so a layout change is eyeballable.
use ferrit::components::screens as ui;
use ferrit::domain::app::{App, Pane};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

fn main() {
    for focus in [
        Pane::Status,
        Pane::Files,
        Pane::Branches,
        Pane::Commits,
        Pane::Stash,
    ] {
        let mut app = App::mock();
        app.focus = focus;
        let mut t = Terminal::new(TestBackend::new(90, 30)).unwrap();
        t.draw(|f| ui::draw(f, &mut app)).unwrap();
        println!("== {focus:?} ==\n{}", t.backend());
    }
}
