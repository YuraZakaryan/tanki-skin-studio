//! Rectangle splitting for pixel-exact screens. A screen is built the way the design file is: a
//! rectangle is cut into rows and columns of fixed or filling parts, and each part is drawn into
//! its own rectangle.

use egui::{Rect, Ui, UiBuilder, Vec2};

/// How one part of a split takes its share of the space.
#[derive(Clone, Copy, Debug)]
pub enum Size {
    /// Exactly this many points.
    Fixed(f32),
    /// The rest of the space, divided between filling parts in proportion to the weights.
    Fill(f32),
}

fn split(total: f32, gap: f32, parts: &[Size]) -> Vec<(f32, f32)> {
    let gaps = gap * parts.len().saturating_sub(1) as f32;
    let fixed: f32 = parts
        .iter()
        .map(|p| if let Size::Fixed(v) = p { *v } else { 0.0 })
        .sum();
    let weights: f32 = parts
        .iter()
        .map(|p| if let Size::Fill(w) = p { *w } else { 0.0 })
        .sum();
    let free = (total - gaps - fixed).max(0.0);
    let mut offset = 0.0;
    parts
        .iter()
        .map(|part| {
            let len = match part {
                Size::Fixed(v) => *v,
                Size::Fill(w) => free * w / weights.max(f32::EPSILON),
            };
            let start = offset;
            offset += len + gap;
            (start, len)
        })
        .collect()
}

/// Cut `rect` into columns left to right.
pub fn columns(rect: Rect, gap: f32, parts: &[Size]) -> Vec<Rect> {
    split(rect.width(), gap, parts)
        .into_iter()
        .map(|(x, w)| {
            Rect::from_min_size(
                egui::pos2(rect.min.x + x, rect.min.y),
                Vec2::new(w, rect.height()),
            )
        })
        .collect()
}

/// Cut `rect` into rows top to bottom.
pub fn rows(rect: Rect, gap: f32, parts: &[Size]) -> Vec<Rect> {
    split(rect.height(), gap, parts)
        .into_iter()
        .map(|(y, h)| {
            Rect::from_min_size(
                egui::pos2(rect.min.x, rect.min.y + y),
                Vec2::new(rect.width(), h),
            )
        })
        .collect()
}

/// Run `add_contents` in a child `Ui` confined to `rect` (for text edits, scroll areas and buttons
/// inside a painted box). The child is *not* added to the parent's layout: a scope would be, and
/// the parent's cursor would jump back to the end of that rectangle, pulling the next row of a
/// list up over the one before it.
pub fn in_rect<R>(ui: &mut Ui, rect: Rect, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
    let mut child = ui.new_child(UiBuilder::new().max_rect(rect));
    add_contents(&mut child)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(w: f32, h: f32) -> Rect {
        Rect::from_min_size(egui::pos2(10.0, 20.0), Vec2::new(w, h))
    }

    #[test]
    fn fixed_and_filling_columns_share_the_width() {
        let parts = columns(
            rect(1136.0, 100.0),
            24.0,
            &[Size::Fill(1.0), Size::Fixed(400.0)],
        );
        assert_eq!(parts[0].width(), 712.0);
        assert_eq!(parts[1].width(), 400.0);
        assert_eq!(parts[1].min.x, 10.0 + 712.0 + 24.0);
    }

    #[test]
    fn equal_weights_divide_evenly_after_the_gaps() {
        let parts = columns(rect(1136.0, 10.0), 16.0, &[Size::Fill(1.0); 4]);
        assert!(parts.iter().all(|p| (p.width() - 272.0).abs() < 1e-3));
    }

    #[test]
    fn rows_stack_with_gaps_and_never_go_negative() {
        let parts = rows(
            rect(100.0, 10.0),
            24.0,
            &[Size::Fixed(214.0), Size::Fill(1.0)],
        );
        assert_eq!(parts[1].height(), 0.0);
        assert_eq!(parts[1].min.y, 20.0 + 214.0 + 24.0);
    }
}
