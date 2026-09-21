use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

/// Whether Enter inserts a newline or remains available to the parent dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextInputMode {
    SingleLine,
    MultiLine,
}

/// Editable UTF-8 text with character-based cursor and reusable rendering.
#[derive(Debug, Clone)]
pub struct TextInput {
    lines: Vec<String>,
    row: usize,
    col: usize,
}

impl Default for TextInput {
    fn default() -> Self {
        Self {
            lines: vec![String::new()],
            row: 0,
            col: 0,
        }
    }
}

impl TextInput {
    pub fn from_text(text: &str) -> Self {
        let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
        if lines.is_empty() {
            lines.push(String::new());
        }
        let row = lines.len() - 1;
        let col = lines.get(row).map_or(0, |line| line.chars().count());
        Self { lines, row, col }
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    pub fn cursor(&self) -> (usize, usize) {
        (self.row, self.col)
    }

    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    pub fn is_blank(&self) -> bool {
        self.lines.iter().all(|line| line.trim().is_empty())
    }

    /// Handle editing keys; return `true` when this input consumed the key.
    /// Submission, cancellation and app-specific shortcuts stay with parent.
    pub fn handle_key_event(&mut self, key: KeyEvent, mode: TextInputMode) -> bool {
        match key.code {
            KeyCode::Enter if mode == TextInputMode::MultiLine => self.insert_newline(),
            KeyCode::Backspace => self.backspace(),
            KeyCode::Left => self.move_left(),
            KeyCode::Right => self.move_right(),
            KeyCode::Up if mode == TextInputMode::MultiLine => self.move_up(),
            KeyCode::Down if mode == TextInputMode::MultiLine => self.move_down(),
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.insert_char(c);
            },
            _ => return false,
        }
        true
    }

    /// Render text with reverse-video cursor, matching Ferrit's existing
    /// commit and branch editors.
    pub fn render(&self, frame: &mut Frame<'_>, area: Rect) {
        let lines: Vec<Line<'static>> = self
            .lines
            .iter()
            .enumerate()
            .map(|(row, text)| {
                if row == self.row {
                    line_with_cursor(text, self.col)
                } else {
                    Line::raw(text.clone())
                }
            })
            .collect();
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
    }

    /// Render without cursor emphasis when this input is not focused.
    pub fn render_inactive(&self, frame: &mut Frame<'_>, area: Rect) {
        let lines: Vec<Line<'static>> = self.lines.iter().cloned().map(Line::raw).collect();
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
    }

    fn insert_char(&mut self, c: char) {
        let byte = self.byte_col();
        if let Some(line) = self.lines.get_mut(self.row) {
            line.insert(byte, c);
            self.col += 1;
        }
    }

    fn insert_newline(&mut self) {
        let byte = self.byte_col();
        if let Some(line) = self.lines.get_mut(self.row) {
            let rest = line.split_off(byte);
            self.lines.insert(self.row + 1, rest);
        }
        self.row += 1;
        self.col = 0;
    }

    fn backspace(&mut self) {
        if self.col > 0 {
            let end = self.byte_col();
            let Some(line) = self.lines.get_mut(self.row) else {
                return;
            };
            let start = line
                .char_indices()
                .nth(self.col - 1)
                .map_or(0, |(byte, _)| byte);
            line.replace_range(start..end, "");
            self.col -= 1;
        } else if self.row > 0 {
            let current = self.lines.remove(self.row);
            self.row -= 1;
            let prev_len = self
                .lines
                .get(self.row)
                .map_or(0, |line| line.chars().count());
            if let Some(line) = self.lines.get_mut(self.row) {
                line.push_str(&current);
            }
            self.col = prev_len;
        }
    }

    fn move_left(&mut self) {
        if self.col > 0 {
            self.col -= 1;
        } else if self.row > 0 {
            self.row -= 1;
            self.col = self.current_line().chars().count();
        }
    }

    fn move_right(&mut self) {
        let len = self.current_line().chars().count();
        if self.col < len {
            self.col += 1;
        } else if self.row + 1 < self.lines.len() {
            self.row += 1;
            self.col = 0;
        }
    }

    fn move_up(&mut self) {
        if self.row > 0 {
            self.row -= 1;
            self.col = self.col.min(self.current_line().chars().count());
        }
    }

    fn move_down(&mut self) {
        if self.row + 1 < self.lines.len() {
            self.row += 1;
            self.col = self.col.min(self.current_line().chars().count());
        }
    }

    fn current_line(&self) -> &str {
        self.lines.get(self.row).map_or("", String::as_str)
    }

    fn byte_col(&self) -> usize {
        self.current_line()
            .char_indices()
            .nth(self.col)
            .map_or_else(|| self.current_line().len(), |(byte, _)| byte)
    }
}

fn line_with_cursor(text: &str, col: usize) -> Line<'static> {
    let mut chars: Vec<char> = text.chars().collect();
    if col >= chars.len() {
        chars.push(' ');
    }
    let before: String = chars.get(..col).unwrap_or_default().iter().collect();
    let cursor = chars.get(col).copied().unwrap_or(' ');
    let after: String = chars
        .get(col.saturating_add(1)..)
        .unwrap_or_default()
        .iter()
        .collect();
    Line::from(vec![
        Span::raw(before),
        Span::styled(
            cursor.to_string(),
            ratatui::style::Style::new().add_modifier(Modifier::REVERSED),
        ),
        Span::raw(after),
    ])
}
