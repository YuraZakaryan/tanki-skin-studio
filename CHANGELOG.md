# Changelog

All notable changes are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.1.1] - 2026-10-29

### Added

- An app icon, the project's own mark drawn in `design.pen`: in the window, the Dock and the
  taskbar, on the macOS app and in Finder, on `tanki-studio.exe`, its Start menu shortcut and in
  Installed apps. The files live in `assets/app-icon/` and `packaging/icons.sh` renders them.

## [0.1.0] - 2026-10-29

### Added

- Desktop app (egui) and command-line tool for managing skins in a local game cache.
- Cache discovery in the standard Adobe AIR application storage, with the client recognised on the
  user's machine; no game is named in the code.
- Recognition of items by content: file hashes, 3DS geometry, texture look, Garage previews and
  modification block order. Hand labels for the rest.
- Import of `.tankipack` packs and of plain skin folders of any layout, with an import report.
- Transactional apply with verified backups, rollback and restore. Incomplete skins cannot be
  applied, and the game must be closed first.
- English and Russian, dark and light themes, interface sizes, command palette, restored session,
  one window per data folder, rotated log file.
- Licensed GPL-3.0-or-later: a version anyone distributes must stay open, with its source. The
  app's About command, the splash screen and the CLI help show the licence, that there is no
  warranty and where the source is.
- Every file states its licence and copyright (REUSE 3.3: SPDX headers, `REUSE.toml`, `LICENSES/`),
  checked in CI, and every download carries `THIRD-PARTY-LICENSES.html` with the notices of the
  Rust crates built in.
- Release workflow: a push to the `release` branch builds a universal macOS `.dmg` and, for Windows
  x64 and ARM, a per-user `.msi` installer and a portable `.zip` from source, with checksums and a
  build-provenance attestation, and drafts a GitHub Release.
- Code signing as in WordCraft and PdfCraft: Developer ID signing and notarization for macOS,
  Authenticode with a certificate or Azure Trusted Signing for Windows, from the secrets of the
  `release` environment; a release from the `release` branch fails without them.
- Quality gate: provenance check, formatting, translations, Clippy, crate layering, unit, UI and
  real-window tests, dependency policy (`cargo-deny`), minimum Rust version check.

[Unreleased]: https://github.com/YuraZakaryan/tanki-skin-studio/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/YuraZakaryan/tanki-skin-studio/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/YuraZakaryan/tanki-skin-studio/releases/tag/v0.1.0
