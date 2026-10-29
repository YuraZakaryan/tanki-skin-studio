# Test checklist

Every behaviour of Tanki Skin Studio and the test that holds it. Good paths and bad paths. Run all of it
with `cargo xtask ci` (in-process tests) and `cargo xtask e2e-ui` (the real window). A deeper random
search: `TANKI_CHAOS_SEEDS=40 TANKI_CHAOS_STEPS=800 cargo test -p tanki-studio --test it chaos`.

Legend: **it** = `apps/tanki-studio/tests/it/*.rs` (the app in-process, egui_kittest), **e2e** =
`crates/tanki-core/tests/e2e.rs`, **cli** = `apps/tanki-cli/tests/cli.rs`, **ui** = `cargo xtask e2e-ui`
scene or journey (the real window), **unit** = `#[cfg(test)]` beside the code.

## Random use (monkey)

| Case | Test |
|---|---|
| Random clicks on every button, Escape, typed text; nothing panics, always a real page | it `monkey` (4 seeds × 250) |
| …plus window resizes (820–1920 px), ⌘K/⌘Z/⌘+/⌘−/⌘0, Enter/Tab/arrows, any text incl. emoji and Cyrillic | it `chaos` (6 seeds × 400) |
| …plus the game starting and stopping, good/broken/other-game/archive/not-a-pack drops, the cache folder vanishing and returning, rescans, restarts on the same data | it `chaos` |
| Invariants after every step: real page, ≤ 3 toasts, no message twice, active pack in the library, no button or label cut off at the window's sides | it `chaos` |
| Every replacement made at random undoes to the original cache, byte for byte | it `monkey`, it `chaos`, ui `gui-monkey` |
| The same in the real window: 5 seeds × 30 random actions incl. out-of-range items, unknown filters, both languages, themes, interface sizes, the palette | ui `gui-monkey` |
| The monkey really goes around the app (≥ 4 pages, at least one replacement), so a monkey that only misses fails | it `monkey`, it `chaos` |

## Cache

| Case | Test |
|---|---|
| Found automatically; files indexed; base64 names decoded | unit `cache`, e2e `import_apply_restore_journey` |
| Not found: empty state with the Choose button | ui `empty` |
| A folder that is no cache is refused, the real cache stays | it `archive` |
| A model archive chosen as the cache becomes a pack | it `archive` |
| The cache folder disappears and comes back while the app runs | it `chaos` |
| Rescan | it `chaos`, ui `gui-monkey` |

## Packs

| Case | Test |
|---|---|
| Import a `.tankipack` zip and a folder; persisted and active | e2e `import_apply_restore_journey`, ui `gui-apply` |
| Broken zip, hostile archive (traversal, size), tampered checksum: refused with every issue listed | e2e, unit `pack`, it `dialogs`, ui `gui-bad-import` |
| A pack for another game | e2e `apply_refuses_wrong_game…`, it `chaos` |
| The same pack twice: refused | unit `pack` |
| Undo an import, delete a pack, the next one becomes active | it `dialogs`, e2e `deleting_a_pack…` |
| Folders without a manifest, any layout (Flash/2.0/html5, no Old/New, typos, editions XT/Prime/ALT, grouped folders, skins split in folders) | unit `archive::tests` |
| Files told by content (3DS signature, preview size), import report (unused, unsupported formats, unrecognised, skipped, guesses) | unit `archive::tests` |
| Same archive → same version; changed archive → new version | unit `archive::tests` |

## Garage and applying

| Case | Test |
|---|---|
| Any modification's skin goes to the selected modification of the same item | it `dialogs`, ui `gui-apply` |
| A skin of another item goes to its own item, never to the selected one | it `garage` |
| One Apply button only (yellow, full width) | it `dialogs`, ui `garage` |
| A skin missing a file of the item (no model, no alpha): marked Incomplete, Apply off, reason shown, palette refused, nothing written | it `garage`, e2e `a_skin_missing_a_file…`, ui `incomplete` |
| Confirm lists the files; Cancel changes nothing | it `dialogs`, ui `confirm` |
| Game running: blocked until closed, Check again | it `dialogs`, ui `game-running`, ui `gui-game-running` |
| Apply writes every revision and a backup; Restore puts it back; tampered backup refused | e2e, cli, ui `gui-apply`, ui `gui-restore` |
| No backup to restore | it `garage` (one toast, not two) |
| Preview: the skin's own, none in the pack (said so), the cache picture with its alpha | it, ui `garage` |
| The chosen variant is scrolled into view | visual check (RU screenshots) |

## Recognition and labelling

| Case | Test |
|---|---|
| By model hash, 3DS geometry, lightmap, preview, texture look; block order anchored by known items | unit `recognize`, unit `geometry` |
| Never a guess (ambiguous runs dropped) | unit `recognize` |
| Hand labels persist and label the whole item | e2e `hand_labelled…`, `labelling_one_modification…`, ui `gui-label` |
| Discovery finds caches in any AIR application's storage (`<id>/Local Store/cache`) and keeps only real caches; the client id comes from the cache's path | unit `process`, real machine |
| The running client is the program whose AIR descriptor declares the cache's id (macOS bundle and Windows layouts); other programs never are | unit `process` |
| Cache names with huge numbers (outside data) are no markers and never overflow the id arithmetic | unit `recognize` |
| A background search answering after the cache was switched is dropped and asked again for the new cache | it `recognise` |

## Interface

| Case | Test |
|---|---|
| Every page, the palette, dialogs, light theme, narrow window | ui scenes |
| Language and theme survive a restart; a stale session is repaired | it `preferences`, ui `gui-persist`, `gui-language` |
| A second window on the same data folder is refused (OS file lock, freed on any exit, never stale); other data folders run side by side | unit `instance` |
| Log file `<data>/logs/tanki-studio.log`: rotated on start (2 old kept), capped at 16 MiB, early records kept, `RUST_LOG` filters, a broken folder or poisoned lock never stops the app; only the window owning the data folder rotates | unit `logging`, real binary (3 starts + a second window) |
| Every failure shown in a toast is logged in the core's English words; every panic is logged | unit `logging`, `errors::user_message` |
| Every string translated, no stale rows | `cargo xtask i18n` |
| Every file states its licence and copyright (SPDX headers, `REUSE.toml`, `LICENSES/`) | CI `Licences (REUSE)` (`reuse lint`) |
| The app's About notice names version, licence, no warranty and source; the licence matches the crates' | unit `about` |
| Every download carries `THIRD-PARTY-LICENSES.html` (the crate notices) | `cargo about generate --fail` in both packaging scripts |
| Nothing git would commit is game content (file types, cache file names), a stray binary, a dump over 4 MiB or a name of the official client | unit `xtask::assets`, `cargo xtask assets` (in `ci`) |
| Counts grouped as each language writes them (3,660 / 3 660) | unit `tanki-i18n`, unit `model` |
| A missing author or license leaves no stray separator | unit `model` |
| Russian labels fit: Resources filters wrap to more rows, labelling chips go to a second row | it `chaos` (cut-off check), RU screenshots |
| Core failures said in the interface language | unit `errors` |
