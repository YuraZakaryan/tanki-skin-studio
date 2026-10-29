# Roadmap

## Implemented

- Cargo workspace with the desktop app, the CLI and layered Rust crates (`cargo xtask layers`).
- Game cache discovery on macOS and Windows (standard Adobe AIR application storage), manual folder selection, an index of every file,
  decoded base64 URL groups, previews with their alpha, and a virtualized searchable catalog.
- Recognition by content: file hashes, 3DS geometry fingerprints, texture look, Garage previews and
  modification block order; ambiguous matches are dropped; hand labels for the rest.
- `.tankipack` ZIP/folder validation and import; plain skin folders of any layout become packs, with
  an import report.
- Any modification's skin applies to the selected modification of the same item; incomplete skins
  cannot be applied.
- Transactional apply and restore with verified backups, game-running detection and a confirmation
  that lists every file.
- English and Russian, dark and light themes, interface sizes, a command palette, a restored session,
  one window per data folder, a rotated log file.
- Tests on synthetic fixtures only: unit, in-process UI (`egui_kittest`, monkey and chaos runs) and the
  real window (`cargo xtask e2e-ui`).

## Next milestones

1. Hands-on QA of the Windows build on a real install.
2. Signed installers for macOS and Windows; a Linux build.
3. A pack authoring and export flow, and side-by-side before/after previews.
4. Parse and round-trip `.tara` only after the format is verified, then support its contained assets.

The open-source repository contains no proprietary game models or textures.
