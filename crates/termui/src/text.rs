//! Unicode-aware text helpers for terminal cells.

use unicode_width::UnicodeWidthStr;

/// Cut `text` to `width` cells, adding a trailing `…` when needed.
pub fn cut_end(text: &str, width: usize) -> String {
    if UnicodeWidthStr::width(text) <= width {
        return text.to_owned();
    }
    let keep = take_width(text.chars(), width.saturating_sub(1));
    if width == 0 {
        keep
    } else {
        format!("{keep}…")
    }
}

/// Cut `text` in the middle, keeping more cells from the end.
pub fn cut_middle(text: &str, width: usize) -> String {
    if UnicodeWidthStr::width(text) <= width {
        return text.to_owned();
    }
    if width <= 1 {
        return cut_end(text, width);
    }

    let room = width - 1;
    let head = take_width(text.chars(), room / 3);
    let mut tail = Vec::new();
    let mut used = 0;
    for character in text.chars().rev() {
        let character_width = unicode_width::UnicodeWidthChar::width(character).unwrap_or(0);
        if used + character_width > room - room / 3 {
            break;
        }
        used += character_width;
        tail.push(character);
    }
    tail.reverse();
    format!("{head}…{}", tail.into_iter().collect::<String>())
}

fn take_width(characters: impl Iterator<Item = char>, width: usize) -> String {
    let mut used = 0;
    let mut output = String::new();
    for character in characters {
        let character_width = unicode_width::UnicodeWidthChar::width(character).unwrap_or(0);
        if used + character_width > width {
            break;
        }
        used += character_width;
        output.push(character);
    }
    output
}

#[cfg(test)]
mod tests {
    use unicode_width::UnicodeWidthStr;

    use crate::text::{cut_end, cut_middle};

    #[test]
    fn cuts_end() {
        assert_eq!(cut_end("hello", 5), "hello");
        assert_eq!(cut_end("hello world", 6), "hello…");
        assert_eq!(cut_end("hello", 0), "");
    }

    #[test]
    fn cuts_middle() {
        assert_eq!(cut_middle("src/app.rs", 20), "src/app.rs");
        let cut = cut_middle("src/app/screens/dashboard/mod.rs", 20);
        assert_eq!(UnicodeWidthStr::width(cut.as_str()), 20);
        assert!(cut.starts_with("src/") && cut.ends_with("/mod.rs") && cut.contains('…'));
        assert_eq!(cut_middle("abcdef", 1), "…");
        assert_eq!(cut_middle("abcdef", 2), "…f");
    }
}
