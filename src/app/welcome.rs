//! The welcome screen: ferrit started in a folder with no repository
//! (`docs/PLAN_16_START_WITHOUT_REPO.md`). It says so and offers `git init`,
//! after a question that names the folder. Nothing is created without the yes.

use std::path::{Path, PathBuf};

use super::{App, ConfirmAction, ConfirmPrompt, KeyCode, KeyEvent, git};

impl App {
    /// The folder the welcome screen is about, while it is up.
    pub fn welcome_dir(&self) -> Option<&Path> {
        self.welcome_dir.as_deref()
    }

    /// Every key on the welcome screen (after a pending question, which owns
    /// input before it): `i` asks about `git init`, `q` and `Esc` leave, and
    /// nothing else does anything, so no pane action can run without a
    /// repository.
    pub(super) fn welcome_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Char('i') => self.ask_init(),
            _ => {},
        }
    }

    /// The question: it names the absolute folder, and says so when that folder
    /// is the user's home directory, where a `git init` is the mistake this
    /// question exists to catch.
    fn ask_init(&mut self) {
        let Some(dir) = self.welcome_dir.clone() else {
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
        self.pending_confirm = Some(ConfirmPrompt {
            message,
            action: ConfirmAction::InitRepo(dir),
        });
    }

    /// The yes: `git init`, then become an app on the new repository. A refusal
    /// (a read-only folder) is git's message and the welcome screen stays.
    pub(super) fn init_here(&mut self, dir: &Path) {
        let made: Result<PathBuf, git::error::GitError> =
            git::Repo::init(dir).map(|_| dir.to_path_buf());
        match made.and_then(|dir| self.attach_repository(&dir)) {
            Ok(()) => {},
            Err(error) => self.report_error(error),
        }
    }
}
