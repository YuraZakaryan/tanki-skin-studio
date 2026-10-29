// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The command registry, as in PdfCraft: one table of what the app can do, with the label, icon
//! and shortcut of each, used by the command palette and by the keyboard handler alike.

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};
use tanki_i18n::Lang;
use tanki_ui::{icons, theme::ThemePreference};

use crate::app::{Page, Studio};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Import,
    ImportFolder,
    RestoreLast,
    Rescan,
    Recognise,
    ChooseCache,
    OpenPalette,
    Go(Page),
    SetTheme(ThemePreference),
    SetLanguage(&'static str),
    Zoom(i32),
    About,
}

pub struct CommandSpec {
    pub command: Command,
    /// English label (the translation key).
    pub label: &'static str,
    /// English detail shown next to the label when there is no shortcut.
    pub detail: &'static str,
    pub icon: char,
    pub shortcut: Option<KeyboardShortcut>,
}

const fn spec(
    command: Command,
    label: &'static str,
    detail: &'static str,
    icon: char,
    shortcut: Option<KeyboardShortcut>,
) -> CommandSpec {
    CommandSpec {
        command,
        label,
        detail,
        icon,
        shortcut,
    }
}

pub const COMMANDS: &[CommandSpec] = &[
    spec(
        Command::Import,
        "Import pack",
        "ZIP or folder",
        icons::UPLOAD,
        Some(KeyboardShortcut::new(Modifiers::COMMAND, Key::I)),
    ),
    spec(
        Command::ImportFolder,
        "Import pack from folder",
        "Folder with manifest.json",
        icons::FOLDER_OPEN,
        None,
    ),
    spec(
        Command::RestoreLast,
        "Restore last replacement",
        "Put the original files back",
        icons::ROTATE_CCW,
        Some(KeyboardShortcut::new(Modifiers::COMMAND, Key::Z)),
    ),
    spec(
        Command::Rescan,
        "Rescan cache",
        "Read the game cache again",
        icons::REFRESH_CW,
        None,
    ),
    spec(
        Command::Recognise,
        "Recognise items from packs",
        "Match the cache with your packs' files",
        icons::SCAN,
        None,
    ),
    spec(
        Command::ChooseCache,
        "Choose cache folder",
        "Pick the folder by hand",
        icons::FOLDER_SEARCH,
        None,
    ),
    spec(
        Command::OpenPalette,
        "Command palette",
        "Search items, resources and actions",
        icons::SEARCH,
        Some(KeyboardShortcut::new(Modifiers::COMMAND, Key::K)),
    ),
    spec(
        Command::Go(Page::Overview),
        "Go to Overview",
        "Page",
        icons::LAYOUT_DASHBOARD,
        None,
    ),
    spec(
        Command::Go(Page::Garage),
        "Go to Garage",
        "Page",
        icons::WAREHOUSE,
        None,
    ),
    spec(
        Command::Go(Page::Resources),
        "Go to Resources",
        "Page",
        icons::DATABASE,
        None,
    ),
    spec(
        Command::Go(Page::Packs),
        "Go to Packs",
        "Page",
        icons::PACKAGE,
        None,
    ),
    spec(
        Command::Go(Page::Info),
        "Go to Info",
        "Page",
        icons::INFO,
        None,
    ),
    spec(
        Command::Zoom(1),
        "Zoom in",
        "Make the interface larger",
        icons::ZOOM_IN,
        Some(KeyboardShortcut::new(Modifiers::COMMAND, Key::Equals)),
    ),
    spec(
        Command::Zoom(-1),
        "Zoom out",
        "Make the interface smaller",
        icons::ZOOM_OUT,
        Some(KeyboardShortcut::new(Modifiers::COMMAND, Key::Minus)),
    ),
    spec(
        Command::Zoom(0),
        "Actual size",
        "Back to 100%",
        icons::SCAN,
        Some(KeyboardShortcut::new(Modifiers::COMMAND, Key::Num0)),
    ),
    spec(
        Command::SetTheme(ThemePreference::System),
        "System theme",
        "Follow the system appearance",
        icons::MONITOR,
        None,
    ),
    spec(
        Command::SetTheme(ThemePreference::Dark),
        "Dark theme",
        "Appearance",
        icons::MOON,
        None,
    ),
    spec(
        Command::SetTheme(ThemePreference::Light),
        "Light theme",
        "Appearance",
        icons::SUN,
        None,
    ),
    spec(
        Command::SetLanguage("en"),
        "English",
        "Language",
        icons::GLOBE,
        None,
    ),
    spec(
        Command::SetLanguage("ru"),
        "Русский",
        "Language",
        icons::GLOBE,
        None,
    ),
    spec(
        Command::About,
        "About Tanki Skin Studio",
        "Version, licence and source",
        icons::INFO,
        None,
    ),
];

/// "⌘I" on macOS, "Ctrl+I" elsewhere.
pub fn shortcut_label(shortcut: &KeyboardShortcut) -> String {
    let key = shortcut.logical_key.symbol_or_name();
    if cfg!(target_os = "macos") {
        format!("⌘{key}")
    } else {
        format!("Ctrl+{key}")
    }
}

impl Studio {
    pub fn command_enabled(&self, command: Command) -> bool {
        match command {
            Command::RestoreLast => self.can_restore(),
            _ => true,
        }
    }

    pub fn run(&mut self, command: Command) {
        match command {
            Command::Import => self.import(false),
            Command::ImportFolder => self.import(true),
            Command::RestoreLast => self.restore_last(),
            Command::Rescan => self.rescan(),
            Command::Recognise => self.auto_recognise(true),
            Command::ChooseCache => self.choose_cache(),
            Command::OpenPalette => self.open_palette(),
            Command::Go(page) => self.go(page),
            Command::SetTheme(pref) => self.set_theme_preference(pref),
            Command::Zoom(direction) => self.zoom(direction),
            Command::About => self.toast(crate::about::notice(), None),
            Command::SetLanguage(code) => {
                if let Some(lang) = Lang::from_code(code) {
                    self.set_language(lang);
                }
            }
        }
    }

    /// Run the command whose shortcut was pressed this frame, if any. Shortcuts that would clash
    /// with text editing (undo) are left to the focused text field.
    pub fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if self.dialog.is_some() {
            return;
        }
        // ⌘+ is ⌘⇧= on most keyboards: accept the plus key as well as equals.
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Plus)) {
            self.zoom(1);
            return;
        }
        let editing = ctx.egui_wants_keyboard_input();
        for spec in COMMANDS {
            let Some(shortcut) = spec.shortcut else {
                continue;
            };
            if editing && spec.command == Command::RestoreLast {
                continue;
            }
            if ctx.input_mut(|i| i.consume_shortcut(&shortcut)) {
                if self.command_enabled(spec.command) {
                    self.run(spec.command);
                }
                return;
            }
        }
    }
}
