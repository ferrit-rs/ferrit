//! Terminal event loop and background-event routing.

use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use color_eyre::Result;
use ratatui::crossterm::event::{Event, KeyEventKind};

use crate::ui::App;
use crate::ui::components::remote as askpass;
use crate::ui::draw as ui;
use crate::ui::error::AppError;
use crate::ui::events::{AppEvent, Events};
use crate::ui::terminal::Tui;

/// How often the run loop wakes while an error toast is up, to count its timeout.
const TOAST_TICK_MS: u64 = 250;

impl App {
    /// The events a background worker or the watcher sends, as opposed to
    /// terminal input. `run()` and `deliver_event` share this.
    pub(crate) fn on_background_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::Refresh => self.request_refresh(),
            AppEvent::RefreshDone(completion) => self.on_refresh_done(*completion),
            AppEvent::DiffDone(completion) => self.on_diff_done(completion),
            AppEvent::ImageDone(completion) => self.on_image_done(completion),
            AppEvent::RemoteDone { op, message } => self.on_remote_done(op, message),
            AppEvent::RemoteCreated(result) => self.on_remote_created(result),
            AppEvent::GhChecked { generation, status } => self.on_gh_checked(generation, status),
            AppEvent::Askpass { prompt, reply } => {
                let events = askpass::ask(prompt, reply, self.modal.popup().is_some());
                self.apply(events);
            },
            AppEvent::StatsDone(completion) => self.on_stats_done(completion),
            AppEvent::Input(_) => {},
        }
    }

    /// Draw, then block for the next event batch, until `should_quit`. Events
    /// come from terminal input, a recursive worktree watch, and a poll (`[ui]
    /// poll_secs`, 10s by default).
    /// Bounded batches avoid repainting for every auto-repeat key while still
    /// guaranteeing regular redraws during sustained input.
    pub fn run(&mut self, terminal: &mut Tui) -> Result<()> {
        let mut events = Events::new(self.watch_root().as_deref(), self.prefs.poll_interval())?;
        self.watch_error = watcher_error(&events);
        self.last_error = self.watch_error.clone();
        // A background fetch/pull/push (`start_remote_op`) needs its own
        // way back onto this channel; only `run()` has an `Events` to ask
        // for one, so it hands `on_key` this clone rather than `on_key`
        // taking `&Events` directly (it is also called from `feed_key`,
        // which has none).
        self.workers.sender = Some(events.sender());
        // Answers ssh/git credential prompts in a popup (`app::askpass`);
        // without it a passphrase question would hang on the raw terminal.
        let askpass_sender = events.sender();
        let _askpass = ferrit_git::askpass::serve(move |prompt| {
            let (reply, answer) = mpsc::channel();
            askpass_sender
                .send(AppEvent::Askpass { prompt, reply })
                .ok()?;
            answer.recv().ok().flatten()
        });
        let mut prev_was_image = false;
        let mut overlay_tick = Instant::now();
        while !self.should_quit {
            let is_image = self.preview_is_image();
            if prev_was_image && !is_image {
                // Graphics pixels from the last image frame sit outside the
                // cell buffer; a full clear is the only way to wipe them.
                terminal.clear()?;
            }
            prev_was_image = is_image;
            terminal.draw(|frame| ui::draw_painted(frame, self))?;

            let animating = self.render.animating();
            // Frames while something animates; a slower tick while a toast is up,
            // so it can time out without waiting for a key.
            let timeout = (animating.any() || self.workers.remote_busy.is_some())
                .then_some(Duration::from_millis(16))
                .or_else(|| {
                    self.render
                        .toast
                        .is_some()
                        .then_some(Duration::from_millis(TOAST_TICK_MS))
                });
            let batch = if let Some(timeout) = timeout {
                match events.next_batch_timeout(timeout) {
                    Err(error) => return Err(error),
                    Ok(Some(batch)) => batch,
                    Ok(None) => {
                        self.render.tick(overlay_tick.elapsed());
                        overlay_tick = Instant::now();
                        continue;
                    },
                }
            } else {
                events.next_batch()?
            };

            for event in batch {
                match event {
                    AppEvent::Input(Event::Key(key)) if key.kind == KeyEventKind::Press => {
                        self.on_key(key);
                    },
                    AppEvent::Input(Event::Mouse(m)) => {
                        if self.render.toast_mouse(m) {
                            self.mouse_pointer.request(false);
                        } else {
                            self.on_mouse(m);
                        }
                        self.mouse_pointer.sync()?;
                    },
                    AppEvent::Input(_) => {},
                    background => self.on_background_event(background),
                }
                if self.should_quit {
                    break;
                }
            }
            // A setting changed that only this loop can carry out.
            if let Some(crate::ui::components::settings::state::TerminalRequest::Mouse(on)) =
                self.terminal_request.take()
                && let Err(error) = crate::ui::terminal::set_mouse(on)
            {
                self.report_notice(format!("cannot switch the mouse: {error}"));
            }
            // The app was rebuilt on a new repository: watch its worktree.
            if let Some(root) = self.take_watch_request() {
                events.watch(&root);
                self.watch_error = watcher_error(&events);
                self.last_error = self.watch_error.clone();
            }
            self.render
                .tick_after_batch(overlay_tick.elapsed(), animating);
            overlay_tick = Instant::now();
        }
        self.workers.stop_remote();
        self.sheets.dashboard.stop_and_join();
        Ok(())
    }

    /// Close the error toast now (`Esc`), unless something else owns the key:
    /// a popup, a question or the help. `true` when there was one to close.
    pub(crate) fn dismiss_toast(&mut self) -> bool {
        if self.modal.is_some() || self.help_is_open() {
            return false;
        }
        self.render.dismiss_toast()
    }
}

/// The Status line for a filesystem watcher that could not start, if any.
fn watcher_error(events: &Events) -> Option<Arc<AppError>> {
    events.watch_error().map(|detail| {
        Arc::new(AppError::WatcherUnavailable {
            detail: detail.to_owned(),
        })
    })
}
