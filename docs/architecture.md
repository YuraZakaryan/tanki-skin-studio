# Architecture

The workspace is split into layers, the way PdfCraft does it: a crate may depend only on crates
of a **lower** layer, and `cargo xtask layers` (part of `cargo xtask ci`) enforces it.

| Layer | Crate | Responsibility |
|-------|-------|----------------|
| 0 | `tanki-ui` | Reusable egui building blocks drawn to `design.pen`: dark/light tokens, bundled fonts and icons, widgets, rectangle layout, interface zoom. Knows nothing about caches or packs. |
| 0 | `tanki-i18n` | UI localisation (English keys in code, `ru.tsv` catalog, plural rules, current language). Knows nothing about the app. |
| 1 | `tanki-cache` | Find and scan a game cache; recognise which resource groups are which item (`targets`: built-in table, hand labels, consecutive-id modifications). |
| 1 | `tanki-pack` | The `.tankipack` format: validation, checksums, safe import from zip or folder. |
| 2 | `tanki-apply` | Transactional replacement of cache files with backup, verification and rollback; backup history; "is the game running" check. |
| 2 | `tanki-state` | SQLite state (packs, active pack, recents, hand labels, language, theme, interface size, the saved session) and the app data folder. |
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

`apps/tanki-studio/src/views/` holds one file per screen (overview, garage, resources, packs, info, empty
state, splash) and per overlay (dialogs, command palette, toasts); `shell.rs` is the sidebar and page frame.

- State lives in `app.rs`; everything the user can do is a method in `actions.rs`, called by buttons, the
  command palette (`commands.rs`, one registry with labels, icons and shortcuts, as in PdfCraft), the
  keyboard and the test hooks (`dev.rs`).
- The view model (`model.rs`: resource rows, garage items, sizes, times) is plain data with unit tests.
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
(`apps/tanki-studio/tests/it`), as PdfCraft does; widgets carry accessible names (`widget_info`) both for
screen readers and for those tests. See the README: `cargo xtask ci` runs everything that needs no window,
`cargo xtask e2e-ui` runs the real window on a fixture cache and checks pixels and the disk.
