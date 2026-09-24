use ratatui_core::layout::{Constraint, Rect};

use super::anchor::Anchor;

/// Resolve overlay placement from parent area, constraints, anchor, and offset.
pub(super) fn resolve_rect(
    parent: Rect,
    width: Constraint,
    height: Constraint,
    anchor: Anchor,
    offset: (i16, i16),
) -> Rect {
    let w = resolve_constraint(width, parent.width).min(parent.width);
    let h = resolve_constraint(height, parent.height).min(parent.height);

    let (ax, ay) = anchor_origin(anchor, parent, w, h);

    let x = shift_clamped(ax, offset.0, parent.x, parent.x + parent.width - w);
    let y = shift_clamped(ay, offset.1, parent.y, parent.y + parent.height - h);

    Rect::new(x, y, w, h)
}

/// `origin + offset`, held inside `lo..=hi`. `lo <= hi` since the overlay is no
/// larger than its parent, and both bounds are `u16`, so the result is too.
fn shift_clamped(origin: u16, offset: i16, lo: u16, hi: u16) -> u16 {
    let shifted = i32::from(origin) + i32::from(offset);
    u16::try_from(shifted.clamp(i32::from(lo), i32::from(hi))).unwrap_or(hi)
}

/// `available * num / den`, saturating at `u16::MAX`; `0` when `den` is `0`.
/// Widened to `u64` so a large ratio cannot overflow the product.
fn scale(available: u16, num: u32, den: u32) -> u16 {
    let scaled = (u64::from(available) * u64::from(num))
        .checked_div(u64::from(den))
        .unwrap_or(0);
    u16::try_from(scaled).unwrap_or(u16::MAX)
}

fn resolve_constraint(constraint: Constraint, available: u16) -> u16 {
    match constraint {
        Constraint::Percentage(p) => scale(available, u32::from(p), 100),
        Constraint::Length(l) => l,
        Constraint::Min(m) => available.max(m),
        Constraint::Max(m) => available.min(m),
        Constraint::Ratio(n, d) => scale(available, n, d),
        Constraint::Fill(_) => available,
    }
}

fn anchor_origin(anchor: Anchor, parent: Rect, w: u16, h: u16) -> (u16, u16) {
    let x = match anchor {
        Anchor::TopLeft | Anchor::Left | Anchor::BottomLeft => parent.x,
        Anchor::Top | Anchor::Center | Anchor::Bottom => parent.x + (parent.width - w) / 2,
        Anchor::TopRight | Anchor::Right | Anchor::BottomRight => parent.x + parent.width - w,
    };

    let y = match anchor {
        Anchor::TopLeft | Anchor::Top | Anchor::TopRight => parent.y,
        Anchor::Left | Anchor::Center | Anchor::Right => parent.y + (parent.height - h) / 2,
        Anchor::BottomLeft | Anchor::Bottom | Anchor::BottomRight => parent.y + parent.height - h,
    };

    (x, y)
}

#[cfg(test)]
mod tests {
    use super::{scale, shift_clamped};

    #[test]
    fn scale_takes_a_fraction_of_the_available_cells() {
        assert_eq!(scale(80, 50, 100), 40);
        assert_eq!(scale(80, 1, 3), 26);
    }

    #[test]
    fn scale_saturates_and_survives_a_zero_denominator() {
        assert_eq!(scale(80, 1, 0), 0);
        assert_eq!(scale(u16::MAX, u32::MAX, 1), u16::MAX);
    }

    #[test]
    fn shift_clamped_moves_and_holds_inside_the_bounds() {
        assert_eq!(shift_clamped(10, 5, 0, 100), 15);
        assert_eq!(shift_clamped(10, -50, 3, 100), 3);
        assert_eq!(shift_clamped(10, 500, 3, 100), 100);
    }
}
