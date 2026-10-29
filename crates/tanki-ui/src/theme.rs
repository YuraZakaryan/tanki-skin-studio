// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Design tokens (colours) for the dark and light themes, and the one-time setup of fonts and
//! egui visuals. The values are the variables of `design.pen`; widgets read them through
//! [`Tokens::get`], so switching theme is one call to [`apply`].

use egui::{Color32, Context, Id};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ThemeKind {
    #[default]
    Dark,
    Light,
}

impl ThemeKind {
    /// The stored preference value.
    pub fn code(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }

    /// Parse a stored preference; anything unknown is the default (dark).
    pub fn from_pref(pref: Option<&str>) -> Self {
        match pref {
            Some("light") => Self::Light,
            _ => Self::Dark,
        }
    }
}

/// What the user chose: a fixed theme, or whatever the operating system is using.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ThemePreference {
    /// Follow the system's light or dark appearance (dark when the system does not say).
    #[default]
    System,
    Light,
    Dark,
}

impl ThemePreference {
    pub const ALL: [ThemePreference; 3] = [Self::System, Self::Light, Self::Dark];

    /// The stored value.
    pub fn code(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    /// Parse a stored value; nothing stored (or anything unknown) follows the system.
    pub fn from_pref(pref: Option<&str>) -> Self {
        match pref {
            Some("light") => Self::Light,
            Some("dark") => Self::Dark,
            _ => Self::System,
        }
    }

    /// The theme to draw with, given what the system reports (`None` when it does not).
    pub fn resolve(self, system: Option<egui::Theme>) -> ThemeKind {
        match self {
            Self::Light => ThemeKind::Light,
            Self::Dark => ThemeKind::Dark,
            Self::System => match system {
                Some(egui::Theme::Light) => ThemeKind::Light,
                _ => ThemeKind::Dark,
            },
        }
    }
}

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// Colour with alpha from a `#RRGGBBAA` literal's parts (premultiplied by egui on use).
fn rgba(hex: u32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied((hex >> 16) as u8, (hex >> 8) as u8, hex as u8, alpha)
}

#[derive(Clone, Copy, Debug)]
pub struct Tokens {
    pub kind: ThemeKind,
    pub bg: Color32,
    pub surface: Color32,
    pub surface_2: Color32,
    pub surface_3: Color32,
    pub border: Color32,
    pub border_strong: Color32,
    pub text: Color32,
    pub text_2: Color32,
    pub text_3: Color32,
    pub primary: Color32,
    pub primary_soft: Color32,
    pub on_primary: Color32,
    pub accent: Color32,
    pub accent_soft: Color32,
    pub on_accent: Color32,
    pub danger: Color32,
    pub danger_soft: Color32,
    pub info: Color32,
    pub info_soft: Color32,
    pub stage: Color32,
    /// Dimming layer behind modals and the palette (the same in both themes).
    pub scrim: Color32,
}

impl Tokens {
    pub fn for_kind(kind: ThemeKind) -> Self {
        match kind {
            ThemeKind::Dark => Self {
                kind,
                bg: rgb(0x0A0E0B),
                surface: rgb(0x101712),
                surface_2: rgb(0x151E17),
                surface_3: rgb(0x1D2A20),
                border: rgb(0x27372B),
                border_strong: rgb(0x3A5240),
                text: rgb(0xEAF3EA),
                text_2: rgb(0xA2B5A5),
                text_3: rgb(0x8AA08E),
                primary: rgb(0x8CFF4D),
                primary_soft: rgba(0x8CFF4D, 0x1F),
                on_primary: rgb(0x0B1A08),
                accent: rgb(0xFFB020),
                accent_soft: rgba(0xFFB020, 0x1F),
                on_accent: rgb(0x1A1100),
                danger: rgb(0xFF6060),
                danger_soft: rgba(0xFF6060, 0x1F),
                info: rgb(0x5CC8FF),
                info_soft: rgba(0x5CC8FF, 0x1F),
                stage: rgb(0x0D140F),
                scrim: rgba(0x000000, 0xB8),
            },
            ThemeKind::Light => Self {
                kind,
                bg: rgb(0xECEFE8),
                surface: rgb(0xF6F8F3),
                surface_2: rgb(0xFFFFFF),
                surface_3: rgb(0xE4EADD),
                border: rgb(0xD0D8C8),
                border_strong: rgb(0xB5C1AA),
                text: rgb(0x13201A),
                text_2: rgb(0x4F6057),
                text_3: rgb(0x5B6B62),
                primary: rgb(0x1E7510),
                primary_soft: rgba(0x1E7510, 0x1A),
                on_primary: rgb(0xFFFFFF),
                accent: rgb(0xA64B00),
                accent_soft: rgba(0xA64B00, 0x1A),
                on_accent: rgb(0xFFFFFF),
                danger: rgb(0xB71C1C),
                danger_soft: rgba(0xB71C1C, 0x1A),
                info: rgb(0x0B6FA8),
                info_soft: rgba(0x0B6FA8, 0x1A),
                stage: rgb(0xDDE4D4),
                scrim: rgba(0x000000, 0xB8),
            },
        }
    }

    /// The tokens of the theme [`apply`] last installed on this context.
    pub fn get(ctx: &Context) -> Self {
        ctx.data(|d| d.get_temp::<Tokens>(Id::unique("tanki-tokens")))
            .unwrap_or_else(|| Self::for_kind(ThemeKind::Dark))
    }

    pub fn is_dark(&self) -> bool {
        self.kind == ThemeKind::Dark
    }
}

/// Install `kind` on the context: tokens for the widgets and matching egui visuals for the few
/// stock widgets still in use (scroll bars, text edits). Cheap; call on every theme change.
pub fn apply(ctx: &Context, kind: ThemeKind) {
    let tokens = Tokens::for_kind(kind);
    ctx.data_mut(|d| d.insert_temp(Id::unique("tanki-tokens"), tokens));
    let mut visuals = if tokens.is_dark() {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    visuals.panel_fill = tokens.bg;
    visuals.window_fill = tokens.surface_2;
    visuals.extreme_bg_color = tokens.surface_2;
    visuals.faint_bg_color = tokens.surface;
    visuals.override_text_color = Some(tokens.text);
    visuals.selection.bg_fill = tokens.primary_soft;
    visuals.selection.stroke = egui::Stroke::new(1.0, tokens.primary);
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, tokens.border);
    ctx.set_visuals(visuals);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_system_preference_follows_the_system_and_defaults_to_dark() {
        use egui::Theme;
        assert_eq!(
            ThemePreference::System.resolve(Some(Theme::Light)),
            ThemeKind::Light
        );
        assert_eq!(
            ThemePreference::System.resolve(Some(Theme::Dark)),
            ThemeKind::Dark
        );
        assert_eq!(ThemePreference::System.resolve(None), ThemeKind::Dark);
        assert_eq!(
            ThemePreference::Light.resolve(Some(Theme::Dark)),
            ThemeKind::Light
        );
        assert_eq!(
            ThemePreference::Dark.resolve(Some(Theme::Light)),
            ThemeKind::Dark
        );
    }

    #[test]
    fn stored_values_parse_and_old_ones_keep_working() {
        assert_eq!(ThemePreference::from_pref(None), ThemePreference::System);
        assert_eq!(
            ThemePreference::from_pref(Some("garbage")),
            ThemePreference::System
        );
        // Versions before "system" stored only dark or light.
        assert_eq!(
            ThemePreference::from_pref(Some("dark")),
            ThemePreference::Dark
        );
        assert_eq!(
            ThemePreference::from_pref(Some("light")),
            ThemePreference::Light
        );
        for pref in ThemePreference::ALL {
            assert_eq!(ThemePreference::from_pref(Some(pref.code())), pref);
        }
    }
}
