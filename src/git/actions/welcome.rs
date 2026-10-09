//! The welcome screen: ferrit started in a folder with no repository
//! (`docs/PLAN_16_START_WITHOUT_REPO.md`). It says so and offers `git init`,
//! after a question that names the folder. Nothing is created without the yes.

use std::path::{Path, PathBuf};

use crate::app::App;
use crate::git;
use crate::interface::popups::confirm::{ConfirmAction, ConfirmPrompt};
use ratatui::crossterm::event::KeyCode;
use ratatui::crossterm::event::KeyEvent;

/// The rows of the screen, in order: `git init`, then quit.
const WELCOME_ROWS: usize = 2;

impl App {
    /// The highlighted row: 0 is `git init`, 1 is quit.
    pub fn welcome_selected(&self) -> usize {
        self.full_screens.welcome_selected
    }

    /// The folder the welcome screen is about, while it is up.
    pub fn welcome_dir(&self) -> Option<&Path> {
        self.full_screens.welcome_dir.as_deref()
    }

    /// Every key on the welcome screen (after a pending question, which owns
    /// input before it): the arrows or `j` / `k` move the highlight, `Enter`
    /// runs the highlighted row, and its own letters run a row from anywhere,
    /// `i` to ask about `git init` and `q` or `Esc` to leave. Nothing else does
    /// anything, so no pane action can run without a repository.
    pub(crate) fn welcome_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Down | KeyCode::Char('j') | KeyCode::End => {
                self.full_screens.welcome_selected =
                    (self.full_screens.welcome_selected + 1).min(WELCOME_ROWS - 1);
            },
            KeyCode::Up | KeyCode::Char('k') | KeyCode::Home => {
                self.full_screens.welcome_selected =
                    self.full_screens.welcome_selected.saturating_sub(1);
            },
            KeyCode::Enter if self.full_screens.welcome_selected == 0 => self.ask_init(),
            KeyCode::Enter | KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Char('i') => self.ask_init(),
            _ => {},
        }
    }

    /// The question: it names the absolute folder, and says so when that folder
    /// is the user's home directory, where a `git init` is the mistake this
    /// question exists to catch.
    fn ask_init(&mut self) {
        let Some(dir) = self.full_screens.welcome_dir.clone() else {
            return;
        };
        let home = std::env::var_os("HOME").is_some_and(|home| Path::new(&home) == dir);
        let message = if home {
            format!(
                "run git init in {}? This is your home folder.",
                dir.display()
            )
        } else {
            format!("run git init in {}?", dir.display())
        };
        self.modal.ask(ConfirmPrompt {
            message,
            action: ConfirmAction::InitRepo(dir),
        });
    }

    /// The yes: `git init`, then become an app on the new repository. A refusal
    /// (a read-only folder) is git's message and the welcome screen stays.
    pub(crate) fn init_here(&mut self, dir: &Path) {
        let made: Result<PathBuf, git::error::GitError> =
            git::repo::Repo::init(dir).map(|_| dir.to_path_buf());
        match made.and_then(|dir| self.attach_repository(&dir)) {
            Ok(()) => {},
            Err(error) => self.report_error(error),
        }
    }
}
