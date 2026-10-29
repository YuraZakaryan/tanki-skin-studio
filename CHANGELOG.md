# Changelog

All notable changes are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Changed

- The licence is now GPL-3.0-or-later (it was MIT): a version anyone distributes must stay open,
  with its source. The app's About command, the splash screen and the CLI help show the licence,
  that there is no warranty and where the source is.
- Every file states its licence and copyright (REUSE 3.3: SPDX headers, `REUSE.toml`, `LICENSES/`),
  checked in CI, and every download carries `THIRD-PARTY-LICENSES.html` with the notices of the
  Rust crates built in.

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
- Release workflow: a push to the `release` branch builds a universal macOS `.dmg` and, for Windows x64
  and ARM, a per-user `.msi` installer and a portable `.zip` from source, with checksums and a build-provenance attestation, and drafts a GitHub
  Release.
- Quality gate: provenance check, formatting, translations, Clippy, crate layering, unit, UI and
  real-window tests, dependency policy (`cargo-deny`), minimum Rust version check.
