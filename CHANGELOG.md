# Changelog

All notable changes are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- **Apply to all items**: one look on every hull and turret that has it. Skins of the same name in
  your packs (the item's own name in front left out, the pack's edition added: XT, Prime…) are one
  look, gathered from every installed pack, the active one first. Pick a look, then choose for each
  modification M0–M3 whether it changes and which skin it gets (its own, or another
  modification's), and even per item; a table shows every item and modification with what happens
  and why one is skipped (no such skin, no such modification in the game, already on, incomplete).
  The last step lists every change and what is skipped before anything is written. The run goes
  item by item off the window's thread, with a progress list and **Stop after this item**; every
  write is a transaction with a backup, and an item that fails is put back as it was while the
  others go on. **Undo all** puts the run back, and in **Packs → Backups** the run is one entry
  that undoes it. Open it from the Garage's **Apply to all items** button or the command palette.

## [0.4.0] - 2026-11-01

### Added

- When the computer has several game clients, each with its own cache, the app asks which cache to
  work with: on the first start that finds more than one, and again whenever a new one appears
  (at start or on a rescan), the new one marked **New**. Each cache is named by its application id
  from the disk, with its path and when it last changed. The choice is kept for the next starts;
  closing the question keeps the cache in use. Before, the app opened one of them by itself.
- The update dialog's "What's new" follows the interface language: release notes now carry a
  Russian section from `CHANGELOG.ru.md` after the English one, and the app shows the one for its
  language, English when there is no translation. Copies up to 0.3.0 read the English section
  only, as before.

### Changed

- The cache card in the sidebar looks like what it is, a picker: a chevron, a highlight on hover, a
  hand cursor and the tooltip "Switch game cache". It opens the same list of caches as the question
  at start, now to switch on purpose: the cache in use marked, **Choose folder…** for a folder that
  is not listed, and Cancel; the arrow keys and Enter work too. Before, it opened a plain menu.
- Paths in the home folder are written `~/…`, shortened in the middle when they do not fit, with
  the whole path in a tooltip.
- The "What's new" box is as high as the notes, and longer notes scroll with the bar in sight.
  Before, the box cut the notes off, so a heading such as "Fixed" could show no items.

### Fixed

- Russian counts (`1 835`) no longer show a wide hole between the digit groups in the monospace
  counters of the sidebar: the separator is a narrow no-break space, as Russian writes it, and a
  count never wraps across lines.
- A resource with a very long name no longer runs out of the inspector on the Resources page: the
  name is shortened with an ellipsis and shown whole in a tooltip, and so are long details.
- In Russian, dialogs no longer break: a text that takes more lines than in English makes the
  dialog taller instead of running into its buttons, a long title wraps under the close button,
  warnings wrap inside their box, and a dialog grows to fit its buttons (in Russian, the update
  offer's Skip this version button stuck out of it).

## [0.3.0] - 2026-10-31

### Added

- The app updates itself from this repository's GitHub releases. At start, at most every twelve
  hours, it asks for the newest release and offers it with what is new in it: **Update and
  restart** downloads the file for this copy, checks it against the release's `SHA256SUMS.txt` and
  GitHub's own digest, installs it (on macOS the new bundle from the `.dmg`, on Windows the `.msi`
  once the app has closed) and opens the new version, which then says it was updated. **Later**
  asks again next time, **Skip this version** waits for a newer one, and a portable copy is offered
  the download. **Check for updates** in the command palette asks at once, and the check at start
  can be turned off there or in the dialog. A failed check at start says nothing.
- A release's notes begin with its CHANGELOG section under "What's new" (`packaging/release-notes.sh`),
  which the update dialog shows.

### Changed

- The app is no longer offline-only: it reaches GitHub for its update check and, when asked, the
  download of an update. Nothing else goes online. README and SECURITY.md say so.

## [0.2.0] - 2026-10-30

### Added

- A pack is imported on its own thread with a progress card in the corner: the stage (reading,
  checking files, copying into the library, checking the copy), a bar with the percent, the time
  left once it can be told, and Cancel, which leaves nothing in the library. Before, a pack of
  hundreds of megabytes froze the window until it was in, and the system called the app not
  responding. A second import waits until the first is done.
- The Windows installer puts a shortcut on the desktop: on for a first install,
  a checkbox on the installer's first page, `INSTALLDESKTOPSHORTCUT=0` for an unattended install.
  An update keeps it only while it is still on the desktop. Before, the app was only in the Start
  menu.

### Changed

- When the cache has no garage picture of the selected modification, the Garage says so under
  "No preview available": the game downloads it only when its own garage shows that
  modification, so open it there and rescan the cache.
- Skin variants are listed M0, M1, M2, M3 first, then a skin that names no modification, then the
  special editions (XT, ALT, Prime, others), each by its modification; names in one modification
  sort naturally ("Camo 2" before "Camo 10"). Before, the selected modification's skins came
  first, so an M3 skin could sit above M0.
- Hulls are shown with a tank icon instead of a truck: the project's own drawing in Lucide's
  style, as Lucide has none. The palette and the Info page show a turret or paint by its own icon
  too, instead of the truck for every item.

### Fixed

- A picked skin belongs to the pack it was picked in. Packs share skin ids ("old", "default"),
  so after switching packs the other pack's skin of the same id looked picked and Apply was on
  for a skin never clicked there. Each pack now keeps its own picks, back when it is active
  again. A skin in the game from another pack is named with its pack ("Old · Pack name").
- Windows no longer opens a terminal next to the app: release builds are a window program.
- A first start fits the screen it opens on: the design's 1440 x 900 frame on a large screen, else
  90% of the width and 85% of the height, centred above a taskbar. A size the player chose is
  restored as before.
- The Garage and the Info page say which skin is in the game: the variant in the cache wears an
  "Applied" badge and the preview reads "In the game now". Nothing is picked until a skin is
  clicked, a pick that is not applied reads "Skin preview · not applied", and Restore resets the
  picks. Before, the first skin looked picked and its preview stayed after Restore, as if it were
  still in the game.
- Restore puts the game cache back as it was before every replacement, in one go, after a
  confirmation that lists the items that get their original files back. Before, every restore put
  back the newest backup again, so after two replacements the original files could not be restored
  from the app. A restored backup is marked and never restored twice, and a damaged backup stops
  the restore before anything is written.
- The confirmation applies exactly the pack it names. A pack dropped on the window while it was
  open became active and its skin was written instead, possibly into another item's files; drops
  now wait until the dialog is closed.
- The game is checked again when the replacement is confirmed, and restoring is refused while the
  game runs.
- Deleting a pack asks first. A double click on the bin deleted the next pack too, which had moved
  under the pointer.
- A double click on Apply no longer opens and at once closes the confirmation: the second click of a
  double click never dismisses a dialog.

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
- Code signing: Developer ID signing and notarization for macOS,
  Authenticode with a certificate or Azure Trusted Signing for Windows, from the secrets of the
  `release` environment; a release from the `release` branch fails without them.
- Quality gate: provenance check, formatting, translations, Clippy, crate layering, unit, UI and
  real-window tests, dependency policy (`cargo-deny`), minimum Rust version check.

[Unreleased]: https://github.com/YuraZakaryan/tanki-skin-studio/compare/v0.4.0...HEAD
[0.4.0]: https://github.com/YuraZakaryan/tanki-skin-studio/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/YuraZakaryan/tanki-skin-studio/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/YuraZakaryan/tanki-skin-studio/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/YuraZakaryan/tanki-skin-studio/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/YuraZakaryan/tanki-skin-studio/releases/tag/v0.1.0
