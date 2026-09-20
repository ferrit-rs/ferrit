//! The commit-message popup's hand-rolled multi-line editor.

/// A hand-rolled multi-line text buffer for the commit-message popup.
/// Not `tui-textarea`: its only published version needs `ratatui = "0.29"`,
/// incompatible with the `0.30` in this tree (see the deviation note atop
/// `docs/PLAN_7_COMMIT.md`). Lines of text plus a `(row, char column)`
/// cursor — enough for a commit message: printable insert, backspace,
/// `Enter` for a new line, arrow movement. No wrapping, no selection, no
/// undo; the Goal section of that plan already scoped the message box to
/// exactly this.
#[derive(Debug, Clone)]
pub(super) struct TextBuffer {
    pub(super) lines: Vec<String>,
    pub(super) row: usize,
    /// Character index into `lines[row]`, not a byte offset — UTF-8 safe
    /// insert/delete always look this up via `char_indices`.
    pub(super) col: usize,
}

impl Default for TextBuffer {
    fn default() -> Self {
        Self {
            lines: vec![String::new()],
            row: 0,
            col: 0,
        }
    }
}

impl TextBuffer {
    /// Pre-fill from an existing message (Amend / Reword), cursor at the
    /// very end — the common place to keep typing from.
    pub(super) fn from_text(text: &str) -> Self {
        let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
        if lines.is_empty() {
            lines.push(String::new());
        }
        let row = lines.len() - 1;
        let col = lines.get(row).map_or(0, |l| l.chars().count());
        Self { lines, row, col }
    }

    pub(super) fn text(&self) -> String {
        self.lines.join("\n")
    }

    /// No subject line typed at all (only blank/whitespace lines) — `git
    /// commit` refuses this, and so does the popup (`do_commit`).
    pub(super) fn is_blank(&self) -> bool {
        self.lines.iter().all(|l| l.trim().is_empty())
    }

    fn current_line(&self) -> &str {
        self.lines.get(self.row).map_or("", String::as_str)
    }

    /// Byte offset of `self.col` (a char count) within the current line.
    fn byte_col(&self) -> usize {
        self.current_line()
            .char_indices()
            .nth(self.col)
            .map_or_else(|| self.current_line().len(), |(b, _)| b)
    }

    pub(super) fn insert_char(&mut self, c: char) {
        let byte = self.byte_col();
        if let Some(line) = self.lines.get_mut(self.row) {
            line.insert(byte, c);
            self.col += 1;
        }
    }

    pub(super) fn insert_newline(&mut self) {
        let byte = self.byte_col();
        if let Some(line) = self.lines.get_mut(self.row) {
            let rest = line.split_off(byte);
            self.lines.insert(self.row + 1, rest);
        }
        self.row += 1;
        self.col = 0;
    }

    /// Delete the char behind the cursor, or merge with the previous line
    /// at column 0. A no-op at the very start of the buffer.
    pub(super) fn backspace(&mut self) {
        if self.col > 0 {
            let end = self.byte_col();
            let Some(line) = self.lines.get_mut(self.row) else {
                return;
            };
            let start = line.char_indices().nth(self.col - 1).map_or(0, |(b, _)| b);
            line.replace_range(start..end, "");
            self.col -= 1;
        } else if self.row > 0 {
            let current = self.lines.remove(self.row);
            self.row -= 1;
            let prev_len = self.lines.get(self.row).map_or(0, |l| l.chars().count());
            if let Some(line) = self.lines.get_mut(self.row) {
                line.push_str(&current);
            }
            self.col = prev_len;
        }
    }

    pub(super) fn move_left(&mut self) {
        if self.col > 0 {
            self.col -= 1;
        } else if self.row > 0 {
            self.row -= 1;
            self.col = self.current_line().chars().count();
        }
    }

    pub(super) fn move_right(&mut self) {
        let len = self.current_line().chars().count();
        if self.col < len {
            self.col += 1;
        } else if self.row + 1 < self.lines.len() {
            self.row += 1;
            self.col = 0;
        }
    }

    pub(super) fn move_up(&mut self) {
        if self.row > 0 {
            self.row -= 1;
            self.col = self.col.min(self.current_line().chars().count());
        }
    }

    pub(super) fn move_down(&mut self) {
        if self.row + 1 < self.lines.len() {
            self.row += 1;
            self.col = self.col.min(self.current_line().chars().count());
        }
    }
}
