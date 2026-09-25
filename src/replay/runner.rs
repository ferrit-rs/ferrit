//! Steps a script through an `App` and a `TestBackend`: one input, one update,
//! one draw, next. Everything is synchronous (no event source, no clock); the
//! one place work happens on a thread, `async-key`, delivers the events itself
//! until the app reports it is idle, so a run never depends on timing.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use unicode_width::UnicodeWidthStr;

use super::fixture::{self, Fixture};
use super::script::{Directive, Expect, Script, Step};
use crate::app::config::{Config, ConfigLoad};
use crate::app::keymap::KeyBinding;
use crate::app::{App, screens};

/// How long `async-key` waits for background work before giving up. A safety
/// net for a hung script, never part of a passing run's timing.
const ASYNC_LIMIT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
pub struct Options {
    /// The terminal size before any `size` directive.
    pub size: (u16, u16),
    /// Use this fixture instead of the script's `fixture` directive.
    pub fixture: Option<String>,
    /// Build the fixture here and keep it, instead of a temporary directory.
    pub keep_fixture_in: Option<PathBuf>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            size: (120, 40),
            fixture: None,
            keep_fixture_in: None,
        }
    }
}

/// A frame kept by a `snapshot` directive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// 1-based, in script order.
    pub index: usize,
    pub label: String,
    /// The character grid, one line per row, trailing spaces trimmed.
    pub text: String,
}

#[derive(Debug, Clone, Default)]
pub struct Outcome {
    pub frames: Vec<Frame>,
}

/// Why a run stopped: the script line, what was wrong, and the frame on screen.
#[derive(Debug, Clone)]
pub struct Failure {
    /// 1-based script line; `0` for a problem before the first step.
    pub line: usize,
    pub message: String,
    pub frame: String,
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

/// The frame as plain text. A double-width symbol occupies two cells, the
/// second of which is blank in the buffer, so it is skipped to keep the columns
/// as the terminal shows them.
pub fn frame_text(buffer: &Buffer) -> String {
    let area = buffer.area;
    let mut rows = Vec::with_capacity(usize::from(area.height));
    for y in 0..area.height {
        let mut row = String::new();
        let mut skip = 0;
        for x in 0..area.width {
            if skip > 0 {
                skip -= 1;
                continue;
            }
            let symbol = buffer[(x, y)].symbol();
            skip = UnicodeWidthStr::width(symbol).saturating_sub(1);
            row.push_str(symbol);
        }
        rows.push(row.trim_end().to_owned());
    }
    rows.join("\n")
}

struct Session {
    fixture: Fixture,
    app: App,
    terminal: Terminal<TestBackend>,
    frame: String,
    frames: Vec<Frame>,
}

fn key_event(binding: KeyBinding) -> KeyEvent {
    let mut modifiers = KeyModifiers::NONE;
    if binding.ctrl {
        modifiers |= KeyModifiers::CONTROL;
    }
    if binding.alt {
        modifiers |= KeyModifiers::ALT;
    }
    KeyEvent::new(binding.code, modifiers)
}

impl Session {
    fn draw(&mut self) -> Result<(), String> {
        let app = &mut self.app;
        self.terminal
            .draw(|f| screens::draw(f, app))
            .map_err(|e| format!("draw failed: {e}"))?;
        self.frame = frame_text(self.terminal.backend().buffer());
        Ok(())
    }

    fn resize(&mut self, width: u16, height: u16) -> Result<(), String> {
        self.terminal.backend_mut().resize(width, height);
        self.terminal
            .resize(Rect::new(0, 0, width, height))
            .map_err(|e| format!("resize failed: {e}"))?;
        self.draw()
    }

    fn feed(&mut self, event: KeyEvent) -> Result<(), String> {
        self.app.feed_key(event);
        self.draw()
    }

    /// Feed `key`, then deliver background events until nothing is running.
    fn async_key(&mut self, binding: KeyBinding) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.app.set_event_sender(tx);
        self.app.feed_key(key_event(binding));
        let deadline = Instant::now() + ASYNC_LIMIT;
        let mut outcome = Ok(());
        while !self.app.is_idle() {
            let left = deadline.saturating_duration_since(Instant::now());
            if let Ok(event) = rx.recv_timeout(left) {
                self.app.deliver_event(event);
            } else {
                outcome = Err(format!(
                    "background work did not finish within {}s",
                    ASYNC_LIMIT.as_secs()
                ));
                break;
            }
        }
        while let Ok(event) = rx.try_recv() {
            self.app.deliver_event(event);
        }
        self.app.clear_event_sender();
        outcome?;
        self.draw()
    }

    fn reopen(&mut self, load: ConfigLoad) -> Result<(), String> {
        self.app = App::open_with(&self.fixture.dir, load).map_err(|e| e.to_string())?;
        self.draw()
    }

    fn step(&mut self, step: &Step) -> Result<(), String> {
        match &step.directive {
            Directive::Fixture(_) => Err("`fixture` must be the first directive".to_owned()),
            Directive::Size(w, h) | Directive::Resize(w, h) => self.resize(*w, *h),
            Directive::Key(keys) => {
                for &binding in keys {
                    self.feed(key_event(binding))?;
                }
                Ok(())
            },
            Directive::AsyncKey(binding) => self.async_key(*binding),
            Directive::Type(text) => {
                for c in text.chars() {
                    let code = if c == '\n' {
                        KeyCode::Enter
                    } else {
                        KeyCode::Char(c)
                    };
                    self.feed(KeyEvent::from(code))?;
                }
                Ok(())
            },
            Directive::Refresh => {
                self.app.refresh();
                self.draw()
            },
            Directive::Exec(args) => {
                let expanded: Vec<String> = args.iter().map(|a| self.fixture.expand(a)).collect();
                let refs: Vec<&str> = expanded.iter().map(String::as_str).collect();
                let (output, ok) = fixture::git_output(&self.fixture.dir, &refs)?;
                if ok {
                    Ok(())
                } else {
                    Err(format!("`git {}` failed: {output}", expanded.join(" ")))
                }
            },
            Directive::Write { path, content } => {
                let target = PathBuf::from(self.fixture.expand(path));
                let target = if target.is_absolute() {
                    target
                } else {
                    self.fixture.dir.join(target)
                };
                std::fs::write(&target, self.fixture.expand(content))
                    .map_err(|e| format!("{}: {e}", target.display()))
            },
            Directive::Config(text) => {
                let (config, issues) = Config::parse(text);
                self.reopen(ConfigLoad {
                    config,
                    file: None,
                    issues,
                })
            },
            Directive::Snapshot(label) => {
                let index = self.frames.len() + 1;
                self.frames.push(Frame {
                    index,
                    label: label.clone(),
                    text: self.frame.clone(),
                });
                Ok(())
            },
            Directive::ExpectText(text) => {
                if self.frame.contains(text.as_str()) {
                    Ok(())
                } else {
                    Err(format!("expected the screen to contain {text:?}"))
                }
            },
            Directive::ExpectNoText(text) => {
                if self.frame.contains(text.as_str()) {
                    Err(format!("expected the screen not to contain {text:?}"))
                } else {
                    Ok(())
                }
            },
            Directive::Git { args, expect } => {
                let expanded: Vec<String> = args.iter().map(|a| self.fixture.expand(a)).collect();
                let refs: Vec<&str> = expanded.iter().map(String::as_str).collect();
                let (output, _) = fixture::git_output(&self.fixture.dir, &refs)?;
                let command = format!("git {}", expanded.join(" "));
                match expect {
                    Expect::Contains(text) => {
                        let text = self.fixture.expand(text);
                        if output.contains(&text) {
                            Ok(())
                        } else {
                            Err(format!(
                                "`{command}` printed {output:?}, expected it to contain {text:?}"
                            ))
                        }
                    },
                    Expect::Exact(text) => {
                        let text = self.fixture.expand(text);
                        if output == text.trim_end() {
                            Ok(())
                        } else {
                            Err(format!(
                                "`{command}` printed {output:?}, expected exactly {text:?}"
                            ))
                        }
                    },
                }
            },
        }
    }
}

fn fail(line: usize, message: String, frame: &str) -> Failure {
    Failure {
        line,
        message,
        frame: frame.to_owned(),
    }
}

/// Run `script`. Stops at the first failing step.
pub fn run(script: &Script, options: &Options) -> Result<Outcome, Failure> {
    let declared = script.steps.iter().find_map(|s| match &s.directive {
        Directive::Fixture(name) => Some((s.line, name.clone())),
        _ => None,
    });
    let name = options
        .fixture
        .clone()
        .or_else(|| declared.as_ref().map(|(_, name)| name.clone()))
        .ok_or_else(|| {
            fail(
                0,
                "no fixture: add `fixture NAME` or pass --fixture".to_owned(),
                "",
            )
        })?;
    if let Some((line, _)) = &declared
        && script.steps.first().is_some_and(|s| s.line != *line)
    {
        return Err(fail(
            *line,
            "`fixture` must be the first directive".to_owned(),
            "",
        ));
    }
    let fixture = Fixture::build(&name, options.keep_fixture_in.as_deref())
        .map_err(|message| fail(0, message, ""))?;
    let app = App::open(&fixture.dir).map_err(|e| fail(0, e.to_string(), ""))?;
    let (width, height) = options.size;
    let terminal = Terminal::new(TestBackend::new(width, height))
        .map_err(|e| fail(0, format!("cannot make a terminal: {e}"), ""))?;
    let mut session = Session {
        fixture,
        app,
        terminal,
        frame: String::new(),
        frames: Vec::new(),
    };
    session.draw().map_err(|message| fail(0, message, ""))?;

    for step in &script.steps {
        if matches!(step.directive, Directive::Fixture(_)) {
            continue;
        }
        let result =
            catch_unwind(AssertUnwindSafe(|| session.step(step))).unwrap_or_else(|panic| {
                let message = panic
                    .downcast_ref::<&str>()
                    .map(|s| (*s).to_owned())
                    .or_else(|| panic.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "unknown panic".to_owned());
                Err(format!("the app panicked: {message}"))
            });
        if let Err(message) = result {
            return Err(fail(step.line, message, &session.frame));
        }
    }
    Ok(Outcome {
        frames: session.frames,
    })
}
