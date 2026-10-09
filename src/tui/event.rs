//! What a component asks the app to do. A component decides from what it is
//! given and returns events; `App::apply` is the one place that changes the
//! state, so a component never reaches into another.

use crate::git::error::GitResult;
use crate::git::operation::OperationOutcome;
use crate::tui::App;
use crate::tui::components::panes::{Pane, SelectionKey};
use crate::tui::components::popups::Popup;
use crate::tui::error::AppError;

/// One change a component asks for.
pub(crate) enum Event {
    /// Put this popup over the panes.
    OpenPopup(Popup),
    /// Take the popup away.
    ClosePopup,
    /// Start (`true`) or end the commit editor's slide animation.
    CommitAnimation(bool),
    /// Read the repository again.
    Refresh,
    /// Tell the user what went wrong.
    Report(AppError),
    /// Select this row once a refresh lists it.
    SelectWhenListed(Pane, SelectionKey),
    /// Keep (or forget) the text of a cancelled commit editor.
    KeepCommitDraft(Option<String>),
    /// Pick the next commit identity.
    CycleAuthor,
    /// Show where a rebase or a rewrite stopped.
    FinishOperation(GitResult<OperationOutcome>),
}

impl App {
    /// Make the changes components asked for, in order.
    pub(crate) fn apply(&mut self, events: Vec<Event>) {
        for event in events {
            match event {
                Event::OpenPopup(popup) => self.modal.open_popup(popup),
                Event::ClosePopup => self.modal.close_popup(),
                Event::CommitAnimation(open) => {
                    if open {
                        self.render.commit.open();
                    } else {
                        self.render.commit.close();
                    }
                },
                Event::Refresh => self.request_refresh(),
                Event::Report(error) => self.report_error(error),
                Event::SelectWhenListed(pane, key) => {
                    self.nav.select_when_listed(pane, key);
                },
                Event::KeepCommitDraft(draft) => self.commit_draft = draft,
                Event::CycleAuthor => self.authorship.cycle(),
                Event::FinishOperation(result) => self.finish_operation(result),
            }
        }
    }
}
