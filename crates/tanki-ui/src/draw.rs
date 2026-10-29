// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Painting primitives the widgets share: bordered boxes with an inside stroke (how the design
//! draws borders), one-pixel rules, radial gradients and drop shadows.

use egui::{
    Color32, CornerRadius, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, Vec2,
    epaint::{Mesh, Shadow, Vertex},
};

/// A rounded box with an optional border drawn *inside* the rectangle, so the box keeps its size.
pub fn rounded(
    painter: &Painter,
    rect: Rect,
    radius: impl Into<CornerRadius>,
    fill: Color32,
    border: Option<(f32, Color32)>,
) {
    let radius = radius.into();
    painter.rect_filled(rect, radius, fill);
    if let Some((width, color)) = border {
        painter.rect_stroke(rect, radius, Stroke::new(width, color), StrokeKind::Inside);
    }
}

/// A horizontal one-pixel rule across `x0..x1` whose top edge is at `y`.
pub fn hrule(painter: &Painter, x0: f32, x1: f32, y: f32, color: Color32) {
    painter.rect_filled(
        Rect::from_min_max(Pos2::new(x0, y), Pos2::new(x1, y + 1.0)),
        0.0,
        color,
    );
}

/// A vertical one-pixel rule down `y0..y1` whose left edge is at `x`.
pub fn vrule(painter: &Painter, x: f32, y0: f32, y1: f32, color: Color32) {
    painter.rect_filled(
        Rect::from_min_max(Pos2::new(x, y0), Pos2::new(x + 1.0, y1)),
        0.0,
        color,
    );
}

/// A filled circle of diameter `size` centred at `center`.
pub fn dot(painter: &Painter, center: Pos2, size: f32, color: Color32) {
    painter.circle_filled(center, size / 2.0, color);
}

/// Radial gradient from `inner` at the centre to `outer` on the ellipse inscribed in `rect`
/// (and beyond), clipped to the rounded rectangle. The corners outside the radius are repainted
/// with `behind`, the colour of whatever the box sits on, because meshes cannot be rounded.
pub fn radial(
    painter: &Painter,
    rect: Rect,
    radius: f32,
    inner: Color32,
    outer: Color32,
    behind: Color32,
) {
    const STEPS: usize = 28;
    let mut mesh = Mesh::default();
    let center = rect.center();
    let half = rect.size() / 2.0;
    for iy in 0..=STEPS {
        for ix in 0..=STEPS {
            let fx = ix as f32 / STEPS as f32;
            let fy = iy as f32 / STEPS as f32;
            let pos = Pos2::new(
                rect.min.x + fx * rect.width(),
                rect.min.y + fy * rect.height(),
            );
            let d = Vec2::new((pos.x - center.x) / half.x, (pos.y - center.y) / half.y).length();
            let t = d.clamp(0.0, 1.0);
            mesh.vertices.push(Vertex {
                pos,
                uv: egui::epaint::WHITE_UV,
                color: lerp(inner, outer, t),
            });
        }
    }
    let row = STEPS + 1;
    for iy in 0..STEPS {
        for ix in 0..STEPS {
            let a = (iy * row + ix) as u32;
            let b = a + 1;
            let c = a + row as u32;
            let d = c + 1;
            mesh.indices.extend_from_slice(&[a, b, c, b, d, c]);
        }
    }
    painter.add(Shape::mesh(mesh));
    if radius > 0.0 {
        cut_corners(painter, rect, radius, behind, false);
    }
}

/// Repaint the corner areas outside a rounded rectangle's arcs with `color` (only the two top
/// corners when `top_only`). For images and meshes that cannot be rounded themselves.
pub fn cut_corners(painter: &Painter, rect: Rect, radius: f32, color: Color32, top_only: bool) {
    const SEGMENTS: usize = 12;
    let corners = [
        (rect.left_top(), Vec2::new(1.0, 1.0)),
        (rect.right_top(), Vec2::new(-1.0, 1.0)),
        (rect.right_bottom(), Vec2::new(-1.0, -1.0)),
        (rect.left_bottom(), Vec2::new(1.0, -1.0)),
    ];
    let count = if top_only { 2 } else { 4 };
    for (corner, dir) in corners.into_iter().take(count) {
        let center = corner + dir * radius;
        let mut points = vec![corner];
        // Quarter arc from the point on one edge to the point on the other.
        for i in 0..=SEGMENTS {
            let angle = std::f32::consts::FRAC_PI_2 * i as f32 / SEGMENTS as f32;
            let (sin, cos) = angle.sin_cos();
            points.push(center + Vec2::new(-dir.x * cos, -dir.y * sin) * radius);
        }
        // The arc runs from the point on the vertical edge to the one on the horizontal edge.
        points.reverse();
        painter.add(Shape::convex_polygon(points, color, Stroke::NONE));
    }
}

fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let mix = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    Color32::from_rgba_unmultiplied_const(
        mix(a.r(), b.r()),
        mix(a.g(), b.g()),
        mix(a.b(), b.b()),
        mix(a.a(), b.a()),
    )
}

/// A soft drop shadow under a rounded rectangle.
pub fn shadow(painter: &Painter, rect: Rect, radius: f32, offset_y: i8, blur: u8, color: Color32) {
    let shadow = Shadow {
        offset: [0, offset_y],
        blur,
        spread: 0,
        color,
    };
    painter.add(shadow.as_shape(rect, radius));
}

/// `color` with its alpha multiplied by `factor` (for hover and disabled states).
pub fn fade(color: Color32, factor: f32) -> Color32 {
    let [r, g, b, a] = color.to_srgba_unmultiplied();
    Color32::from_rgba_unmultiplied(r, g, b, (f32::from(a) * factor).round() as u8)
}

/// `color` mixed with white by `amount` (0..1): the hover state of a filled button.
pub fn lighten(color: Color32, amount: f32) -> Color32 {
    lerp(color, Color32::WHITE, amount)
}
