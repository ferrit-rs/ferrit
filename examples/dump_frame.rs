//! Throwaway: render one frame to stdout so a layout change is eyeballable.
use ferrit::app::{App, Pane};
use ferrit::ui;
use ratatui::Terminal;
use ratatui::backend::TestBackend;

fn main() {
    for focus in [Pane::Status, Pane::Files, Pane::Branches, Pane::Commits, Pane::Stash] {
        let mut app = App::mock();
        app.focus = focus;
        let mut t = Terminal::new(TestBackend::new(90, 30)).unwrap();
        t.draw(|f| ui::draw(f, &mut app)).unwrap();
        println!("== {focus:?} ==\n{}", t.backend());
    }
}
