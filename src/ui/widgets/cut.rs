//! Cutting text to a width in terminal cells, with an `…` where it was cut.

use unicode_width::UnicodeWidthStr;

/// `text` cut to `width` cells with a trailing `…`.
pub(crate) fn cut_end(text: &str, width: usize) -> String {
    if UnicodeWidthStr::width(text) <= width {
        return text.to_owned();
    }
    let keep: String = take_width(text.chars(), width.saturating_sub(1));
    if width == 0 {
        keep
    } else {
        format!("{keep}…")
    }
}

/// `text` cut in the middle (`src/app/…/mod.rs`): the end of a path is the part
/// that tells files apart, so it keeps the larger share.
pub(crate) fn cut_middle(text: &str, width: usize) -> String {
    if UnicodeWidthStr::width(text) <= width {
        return text.to_owned();
    }
    if width <= 1 {
        return cut_end(text, width);
    }
    let room = width - 1;
    let head = take_width(text.chars(), room / 3);
    let mut tail: Vec<char> = Vec::new();
    let mut used = 0;
    for c in text.chars().rev() {
        let w = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        if used + w > room - room / 3 {
            break;
        }
        used += w;
        tail.push(c);
    }
    tail.reverse();
    format!("{head}…{}", tail.into_iter().collect::<String>())
}

fn take_width(chars: impl Iterator<Item = char>, width: usize) -> String {
    let mut used = 0;
    let mut out = String::new();
    for c in chars {
        let w = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        if used + w > width {
            break;
        }
        used += w;
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cuts() {
        assert_eq!(cut_end("hello", 5), "hello");
        assert_eq!(cut_end("hello world", 6), "hello…");
        assert_eq!(cut_end("hello", 0), "");
        assert_eq!(cut_middle("src/app.rs", 20), "src/app.rs");
        let cut = cut_middle("src/app/screens/dashboard/mod.rs", 20);
        assert_eq!(UnicodeWidthStr::width(cut.as_str()), 20);
        assert!(cut.starts_with("src/") && cut.ends_with("/mod.rs") && cut.contains('…'));
        assert_eq!(cut_middle("abcdef", 1), "…");
        assert_eq!(cut_middle("abcdef", 2), "…f");
    }
}
