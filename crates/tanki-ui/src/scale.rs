//! Interface size: how large the whole interface is drawn. It is egui's zoom factor on top of the
//! display's own scale, so "100%" is the size the design was made for on every display, and the
//! text stays sharp (egui rasterises at `pixels_per_point`).
//!
//! The factor comes from settings, so it is untrusted: [`valid`] accepts finite numbers inside
//! [`MIN`]..=[`MAX`] only. Keyboard zoom of egui itself stays off; the app's own commands step
//! through [`PRESETS`] and the choice is stored.

use egui::Context;

/// The smallest and largest interface sizes, as factors.
pub const MIN: f32 = 0.5;
pub const MAX: f32 = 3.0;
/// The sizes the zoom commands step through, in percent.
pub const PRESETS: [u16; 11] = [50, 67, 75, 80, 90, 100, 110, 125, 150, 175, 200];

/// A factor from settings: finite and within [`MIN`]..=[`MAX`], or `None`.
pub fn valid(factor: f64) -> Option<f32> {
    let factor = factor as f32;
    (factor.is_finite() && (MIN..=MAX).contains(&factor)).then_some(factor)
}

/// Parse a stored value: a percentage such as `125` or `125%`.
pub fn parse(value: &str) -> Option<f32> {
    let percent: f64 = value.trim().trim_end_matches('%').trim().parse().ok()?;
    valid(percent / 100.0)
}

/// The percentage a factor shows as.
pub fn percent(factor: f32) -> u32 {
    (factor * 100.0).round().clamp(0.0, 1000.0) as u32
}

/// The next preset above (`direction > 0`) or below (`< 0`) the current size; the current size
/// when there is none. A size between presets steps to the nearest preset in that direction.
pub fn step(current: f32, direction: i32) -> f32 {
    let now = percent(current);
    let next = if direction > 0 {
        PRESETS.iter().copied().find(|p| u32::from(*p) > now)
    } else {
        PRESETS.iter().rev().copied().find(|p| u32::from(*p) < now)
    };
    next.map_or(current, |p| f32::from(p) / 100.0)
}

/// Draw at `factor` from the next frame on, when it differs from the factor last applied.
/// Also turns egui's own keyboard zoom off, so the stored size is the only one.
pub fn sync(ctx: &Context, factor: f32, applied: &mut Option<f32>) {
    if applied.is_none() {
        ctx.options_mut(|o| o.zoom_with_keyboard = false);
    }
    if *applied != Some(factor) {
        ctx.set_zoom_factor(factor);
        *applied = Some(factor);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_sensible_factors_are_accepted() {
        assert_eq!(valid(1.0), Some(1.0));
        for bad in [f64::NAN, f64::INFINITY, 0.0, 0.1, 40.0, -1.0] {
            assert_eq!(valid(bad), None, "{bad}");
        }
        assert_eq!(parse("125"), Some(1.25));
        assert_eq!(parse(" 150% "), Some(1.5));
        for bad in ["", "big", "10", "1000", "NaN"] {
            assert_eq!(parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn stepping_walks_the_presets_and_stops_at_the_ends() {
        assert_eq!(step(1.0, 1), 1.1);
        assert_eq!(step(1.0, -1), 0.9);
        assert_eq!(step(2.0, 1), 2.0, "no preset above 200%");
        assert_eq!(step(0.5, -1), 0.5, "no preset below 50%");
        // Between presets: the nearest one in the direction asked.
        assert_eq!(step(1.05, 1), 1.1);
        assert_eq!(step(1.05, -1), 1.0);
        let mut size = 1.0;
        for _ in 0..40 {
            size = step(size, 1);
        }
        assert_eq!(size, 2.0);
    }

    #[test]
    fn sync_sets_the_zoom_once_and_turns_off_keyboard_zoom() {
        let ctx = Context::default();
        let mut applied = None;
        sync(&ctx, 1.25, &mut applied);
        assert_eq!(applied, Some(1.25));
        // The zoom takes effect when the next frame starts.
        ctx.run_ui(egui::RawInput::default(), |_| {})
            .textures_delta
            .clear();
        assert!(!ctx.options(|o| o.zoom_with_keyboard));
        assert_eq!(ctx.zoom_factor(), 1.25);
    }
}
