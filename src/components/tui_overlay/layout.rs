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

    let x = (ax as i32 + offset.0 as i32)
        .clamp(parent.x as i32, (parent.x + parent.width - w) as i32) as u16;

    let y = (ay as i32 + offset.1 as i32)
        .clamp(parent.y as i32, (parent.y + parent.height - h) as i32) as u16;

    Rect::new(x, y, w, h)
}

fn resolve_constraint(constraint: Constraint, available: u16) -> u16 {
    match constraint {
        Constraint::Percentage(p) => (available as u32 * p as u32 / 100) as u16,
        Constraint::Length(l) => l,
        Constraint::Min(m) => available.max(m),
        Constraint::Max(m) => available.min(m),
        Constraint::Ratio(n, d) => {
            if d == 0 {
                0
            } else {
                (available as u32 * n / d) as u16
            }
        },
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
