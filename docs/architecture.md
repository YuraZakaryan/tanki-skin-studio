# Architecture

The workspace is split into layers: a crate may depend only on crates
of a **lower** layer, and `cargo xtask layers` (part of `cargo xtask ci`) enforces it.

| Layer | Crate | Responsibility |
|-------|-------|----------------|
| 0 | `tanki-ui` | Reusable egui building blocks drawn to `design.pen`: dark/light tokens, bundled fonts and icons, widgets, rectangle layout, interface zoom. Knows nothing about caches or packs. |
| 0 | `tanki-i18n` | UI localisation (English keys in code, `ru.tsv` catalog, plural rules, current language). Knows nothing about the app. |
| 0 | `tanki-model` | 3DS models read into plain meshes (untrusted input: bounds-checked, capped, never a panic), their geometry fingerprint, and indexed geometry with smoothing-group normals. No cache, pack or GPU. |
| 1 | `tanki-viewer` | A 3D stage for egui: a scene drawn with wgpu into a texture of its own only when the view changes, an orbit camera with inertia, quality levels for weaker graphics, a before/after comparison. Knows nothing about caches or packs. |
| 1 | `tanki-cache` | Find and scan a game cache; recognise which resource groups are which item (`targets`: built-in table, hand labels, consecutive-id modifications). |
| 1 | `tanki-pack` | The `.tankipack` format: validation, checksums, safe import from zip or folder. |
| 1 | `tanki-update` | Updates from the project's GitHub releases: check, download checked against the release's `SHA256SUMS.txt`, install (the macOS bundle from the `.dmg`, the Windows `.msi` after exit), restart. |
| 2 | `tanki-apply` | Transactional replacement of cache files with backup, verification and rollback; an item's look (`look`: its model, textures or paint, from the cache or a pack, for the 3D preview); backup history; backup housekeeping (`backups`: which backups are still needed, deleting old ones, a size limit, free disk space and a plan to make room); "is the game running" check. |
| 2 | `tanki-state` | SQLite state (packs, active pack, recents, hand labels, language, theme, interface size, the 3D preview, its quality, the whole tank and its last parts, the saved session) and the app data folder. |
| 3 | `tanki-core` | Thin facade re-exporting the layers above, plus the end-to-end tests that cross all of them. |
| — | `tanki-studio`, `tanki-cli`, `xtask` | Binaries and tooling; they may use any layer. |
| — | `tanki-testkit` | Synthetic cache and packs for tests. A dev-dependency only, and depends on nothing of ours. |

Rules enforced by `layers::TABLE`:

- every crate is classified; a new crate must be added to the table;
- a layered crate depends only on strictly lower layers (no sideways or upward edges);
- the UI kit (layer 0) never depends on the domain crates;
- `tanki-testkit` is a dev-dependency only;
- nothing depends on a binary or tool.

## Design

`design.pen` is the source of the look: colours are its variables (`tanki-ui::theme::Tokens`, dark and
light), sizes and positions are its frames. Widgets paint themselves at the design's exact sizes
(`tanki-ui::widgets`), and a screen is built the way the design file is: a rectangle is cut into rows and
columns with `tanki-ui::layout` and each part is drawn into its own rectangle. Chakra Petch, Inter and
JetBrains Mono (SIL OFL) and the Lucide icon font (ISC) are bundled in `crates/tanki-ui/assets`, with their
licences. `cargo xtask ui-diff <ours.png> <design-export.png>` compares a screenshot with an export of the
design pixel by pixel.

## Screens

`apps/tanki-studio/src/views/` holds one file per screen (overview, garage, resources, packs, info (the Item
page, the Garage's Details), settings, backups (a page of Settings), about, empty state, splash) and per overlay (dialogs, command palette, toasts, the "Apply to all items" wizard); `shell.rs`
is the sidebar and page frame.

- State lives in `app.rs`; everything the user can do is a method in `actions.rs`, called by buttons, the
  command palette (`commands.rs`, one registry with labels, icons and shortcuts), the
  keyboard and the test hooks (`dev.rs`).
- The view model (`model.rs`: resource rows, garage items, backup entries, sizes, times) is plain data with
  unit tests. "Apply to all items" has its own (`bulk.rs`): the looks shared by the packs' skins, the plan
  of what goes where, and the run, which writes item by item on its own thread through `tanki-apply`
  transactions tagged with one batch. Its cells for a modification not in the cache yet go on later
  (`later.rs`): the choice is queued in the state store (`tanki_state::later`), the cache is looked at
  again on its own (at launch, when the window comes back, every 5 minutes or hourly; a cheap stamp of
  the folder decides whether to read it), and the queued skins that are due go on as one batch.
- Backups as the app keeps them (`backups.rs`): the limit (a setting), the lines of the Backups page (a
  run is one line), deleting old ones, undoing one, and the low-space question. Every write checks first
  that its backup fits on the disk; a backup of a skin on now is only deleted after that question. The
  Garage's item states (skin on, original, waiting, lost to a game update) come from the backups and the
  queue of skins that go on later. The About page reads the changelogs built into the app
  (`changelog.rs`).
- Pages opened from another one (Item from the Garage, Backups from Settings) have a back arrow and keep
  their sidebar entry lit (`Page::section`, `Page::parent`).
- Responsive: the sidebar becomes an icon rail below 1100 points, two-column pages stack below 860 points
  of content width and scroll, long text is clipped to its box. The window cannot be smaller than 820 × 520.

## Language, theme, size and session

Strings in code are English and are the keys (`tl!("Overview")`); `crates/tanki-i18n/src/ru.tsv` translates
them, a missing translation shows the English text, and `cargo xtask i18n` (part of `ci`) fails when a
string in the code has no Russian row or a row is stale. The default is English; the choice is stored in
SQLite and restored at start. Theme and interface size (⌘+ / ⌘− / ⌘0) are stored the same way.

The session (page, window place, selected item, modification and skin variant, filters) is one JSON document
under the versioned key `ui.session.v1` (`tanki-state::session`): every field has a default, nothing in it is
trusted (a deleted pack, a folder or a monitor that is gone is repaired on load), selections are scoped to the
cache folder they were made in, it is written only when it changed, and language, theme and size have their
own keys so a damaged session can never lose them.

## Testing

`tanki-studio` is a library plus a thin binary, so its UI is tested in-process with `egui_kittest`
(`apps/tanki-studio/tests/it`); widgets carry accessible names (`widget_info`) both for
screen readers and for those tests. See the README: `cargo xtask ci` runs everything that needs no window,
`cargo xtask e2e-ui` runs the real window on a fixture cache and checks pixels and the disk.
