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
| …plus the game starting and stopping, switching between two packs whose skins share ids, good/broken/other-game/archive/not-a-pack drops, the cache folder vanishing and returning, rescans, restarts on the same data | it `chaos` |
| Invariants after every step: real page, ≤ 3 toasts, no message twice, active pack in the library, no button or label cut off at the window's sides | it `chaos` |
| Every replacement made at random undoes to the original cache, byte for byte | it `monkey`, it `chaos`, ui `gui-monkey` |
| The same in the real window: 5 seeds × 30 random actions incl. out-of-range items, unknown filters, both languages, themes, interface sizes, the palette | ui `gui-monkey` |
| The monkey really goes around the app (≥ 4 pages, at least one replacement), so a monkey that only misses fails | it `monkey`, it `chaos` |

## Updates

| Case | Test |
|---|---|
| Tests and scripted runs never ask GitHub; the desktop app checks at start | it `nothing_asks_github_until_the_desktop_app_turns_the_check_on` |
| A newer release is offered at start with its notes; Later closes it | it `a_newer_release_is_offered_at_start…`, ui `update` |
| The check at start asks at most every 12 hours, never when turned off, and the choice survives a restart | it `the_check_at_start_asks_at_most_every_twelve_hours…` |
| A failed check at start says nothing; Check for updates says why, and offers Try again | it `a_failed_check_at_start_says_nothing…` |
| Check for updates on the newest version says so; its switch turns the check at start off | it `check_for_updates_says_when_this_is_the_newest_version` |
| Skip this version: not offered at start again, a newer one is, a requested check shows it | it `a_skipped_version_is_not_offered…` |
| Update and restart installs the release, records it and closes the app | it `update_and_restart_installs…` |
| A failed update is offered again with the reason (Try again, Download); a cancelled download returns quietly | it `a_failed_update_is_offered_again…` |
| A portable copy is offered the download; nothing is installed | it `a_copy_that_cannot_update_itself…` |
| An update waits until a pack import is done; a pack dropped while the update dialog is open is not imported | it `an_update_waits_until_a_pack_is_imported`, it `a_pack_dropped_during_an_update_is_not_imported` |
| A start into another version does not claim the update; a check time in the future does not hold the check off | it `a_start_into_another_version…`, it `a_check_time_in_the_future…` |
| After the restart: "updated to" once, or the helper's failure; leftovers removed | it `after_the_restart_the_new_version_or_the_failure_is_reported_once` |
| The download progress: bar, bytes, Cancel | ui `updating` |
| Only a newer version is offered; a release beats its release candidate; drafts, pre-releases and odd tags are not offered | unit `version`, unit `release` |
| Links outside this repository's releases are refused; GitHub's digest is read | unit `release` |
| What's new is read from the notes as plain text | unit `release` |
| A download must be listed in SHA256SUMS.txt and agree with GitHub's digest; a mismatch installs nothing | unit `verify`, unit `install` |
| The release file is named by version and platform; a development build and a portable copy do not replace themselves | unit `install` |
| macOS: only this app's bundle is replaced, whole, from a real disk image; another version in the image replaces nothing | unit `install::macos` |
| The helper waits until the app lets go of its lock; the restart waits for the old process; a failure is reported once | unit `install::helper` |
| An installer changed after its check is not run, and the app starts again | unit `an_installer_changed_after_its_check_is_not_run…` |
| A cancel is told apart from a failure: a failure after Cancel is still reported | unit `progress`, it `a_failed_update_is_offered_again…` |
| The real GitHub answers (by hand: `cargo test -p tanki-update -- --ignored`) | unit `the_published_release_is_offered_to_an_older_version` |

## Cache

| Case | Test |
|---|---|
| Found automatically; files indexed; base64 names decoded | unit `cache`, e2e `import_apply_restore_journey` |
| Not found: empty state with the Choose button | ui `empty` |
| A folder that is no cache is refused, the real cache stays | it `archive` |
| A model archive chosen as the cache becomes a pack | it `archive` |
| The cache folder disappears and comes back while the app runs | it `chaos` |
| Rescan | it `chaos`, ui `gui-monkey` |
| Several caches: asked on the first start and when a new one appears (also by a rescan), the new one marked; the choice and a closed question are kept | unit `caches_to_ask`, it `caches` |
| The cache card is a picker (chevron, hover, tooltip) and opens the switcher: the same list, Choose folder…, Cancel and Esc keep the cache, arrows and Enter switch, one cache listed too | it `caches` |
| Russian counts: narrow no-break space between digit groups, narrow in mono, never wrapped | unit `counts_are_grouped_the_way_each_language_writes_them`, unit `a_russian_count_is_narrow_in_mono_and_never_wraps` |
| Paths as `~/…`, shortened in the middle, whole in a tooltip | unit `a_path_in_the_home_folder_starts_with_a_tilde`, unit `fonts` |

## Packs

| Case | Test |
|---|---|
| Import a `.tankipack` zip and a folder; persisted and active | e2e `import_apply_restore_journey`, ui `gui-apply` |
| Broken zip, hostile archive (traversal, size), tampered checksum: refused with every issue listed | e2e, unit `pack`, it `dialogs`, ui `gui-bad-import` |
| A pack for another game | e2e `apply_refuses_wrong_game…`, it `chaos` |
| The same pack twice: refused | unit `pack` |
| Import on its own thread: the window goes on at once, the progress counts every byte to the end, the pack lands in the library | unit `pack`, it `import` |
| The progress card (stage, percent, time left, Cancel); Cancel stops at the next file or megabyte and leaves nothing in the library; a second import or a drop waits | unit `pack`, it `import`, ui `importing` |
| Undo an import, delete a pack, the next one becomes active | it `dialogs`, e2e `deleting_a_pack…` |
| Deleting a pack asks first; Cancel keeps it; a double click on the bin deletes no other pack | it `dialogs` |
| The second click of a double click never dismisses the dialog its first click opened | it `dialogs` |
| Folders without a manifest, any layout (Flash/2.0/html5, no Old/New, typos, editions XT/Prime/ALT, grouped folders, skins split in folders) | unit `archive::tests` |
| Files told by content (3DS signature, preview size), import report (unused, unsupported formats, unrecognised, skipped, guesses) | unit `archive::tests` |
| Same archive → same version; changed archive → new version | unit `archive::tests` |

## Garage and applying

| Case | Test |
|---|---|
| Skin variants are listed M0..M3, then no modification, then XT, ALT, Prime and other editions, each by modification; names sort naturally (numbers by value, no case) | unit `model`, it `garage` |
| Any modification's skin goes to the selected modification of the same item | it `dialogs`, ui `gui-apply` |
| A skin of another item goes to its own item, never to the selected one | it `garage` |
| One Apply button only (yellow, full width) | it `dialogs`, ui `garage` |
| A skin missing a file of the item (no model, no alpha): marked Incomplete, Apply off, reason shown, palette refused, nothing written | it `garage`, e2e `a_skin_missing_a_file…`, ui `incomplete` |
| Confirm lists the files; Cancel changes nothing | it `dialogs`, ui `confirm` |
| Game running: blocked until closed, Check again | it `dialogs`, ui `game-running`, ui `gui-game-running` |
| The game started while the confirmation is open: Apply is blocked again; Restore is refused while it runs | it `dialogs` |
| The confirmation applies the pack it names: a pack dropped meanwhile is not imported, another active pack changes nothing, a removed pack replaces nothing and says so | it `dialogs` |
| Apply writes every revision and a backup; Restore puts it back; tampered backup refused | e2e, cli, ui `gui-apply`, ui `gui-restore` |
| Which skin is in the game: the newest replacement not restored whose files are untouched; "Applied" badge, honest preview caption, Apply off for the skin already in the game; known after a restart; a file changed since drops it; Restore resets the picks and nothing is picked until a click | unit `apply`, it `garage` |
| A pick belongs to its pack: switching packs shows no pick of the other pack's skin of the same id, and switching back brings it again; a skin in the game from another pack is named with its pack | it `garage` |
| Restore asks first (the items listed, Cancel changes nothing), then undoes every replacement not restored yet, newest first, back to the original cache; a restored backup is marked and skipped; a damaged backup writes nothing | unit `apply`, it `dialogs`, ui `restore` |
| No backup to restore | it `garage` (one toast, not two) |
| Preview: the skin's own, none in the pack (said so), the cache picture with its alpha | it, ui `garage` |
| The chosen variant is scrolled into view | visual check (RU screenshots) |

## Apply to all items

| Case | Test |
|---|---|
| Looks: the same name without the item's own in front, plus the pack's edition (`xt-m0` is XT, `prime` is Prime); case, spaces, `-` and `_` do not count; an item's plain skin is no look unless the pack names it; only hulls and turrets in the cache; the first pack (the active one) wins for the same item and source | unit `bulk`, it `bulk` |
| Opens from the Garage's button and the palette; no look: the empty state, Add a pack opens the import | it `bulk`, ui `bulk-garage`, `bulk-empty` |
| Defaults: every modification on, its own skin when an item has one (or the edition's), else the highest modification's | unit `bulk` |
| Every cell says what happens: own skin, from another modification, the item's own pick, no such skin, no such modification in the game, already on (the recorded pack item's grade), incomplete, not touched | unit `bulk`, it `bulk`, ui `bulk-where` |
| A modification off, another source, an item's own pick and back to the common choice; the Hulls / Turrets boxes leave a kind out of the run; the filter only shows | unit `bulk`, it `bulk` |
| The last step lists every change and every skip; the backup is always on; Esc there asks first; Enter is the primary button | it `bulk`, ui `bulk-check` |
| The game running: the same warning, nothing written until it is closed | it `bulk` |
| One transaction per item and modification, all of one batch; the Garage shows what the items wear after | it `bulk`, ui `gui-bulk` |
| A failing item is put back as it was (its earlier modifications of the run too), the others are applied, the reason is said per item | it `bulk` |
| Stop after this item finishes it and writes no other; while it runs Esc does nothing and imports and cache switches wait | it `bulk`, ui `bulk-running` |
| Undo all and the batch's one entry in the backups put every file back (a later replacement on top of it too); Restore original files still undoes everything | unit `apply`, unit `model`, it `bulk`, ui `gui-bulk` |
| Two replacements in the same clock tick never share a backup folder | unit `apply` |
| Russian fits every step, down to the smallest window; toasts stay above the wizard's buttons | it `bulk`, ui `bulk-*` |
| Every screen on the design's own sample at 1440 × 900, English and Russian, dark and light, compared 1:1 with `design.pen`'s frames; the sample gives the frames' numbers | ui `design-*` (visual check against `target/design-preview/full/`), it `bulk` |
| Items in the game's order (resource id), "Already on" is no reason to list, an item is skipped when nothing of it changes | unit `bulk`, it `bulk` |

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
| A first start fits its screen (1440 x 900 when it fits, else 90% x 85%, centred above a taskbar) | unit `session` |
| The release app opens no terminal on Windows (`windows_subsystem`) | it `preferences` |
| The Windows installer adds a desktop shortcut (first install on, checkbox, `INSTALLDESKTOPSHORTCUT=0`, kept on update only while on the desktop) and removes it on uninstall | release build (`wix build`), a Windows install by hand |
| Language and theme survive a restart; a stale session is repaired | it `preferences`, ui `gui-persist`, `gui-language` |
| A second window on the same data folder is refused (OS file lock, freed on any exit, never stale); other data folders run side by side | unit `instance` |
| Log file `<data>/logs/tanki-studio.log`: rotated on start (2 old kept), capped at 16 MiB, early records kept, `RUST_LOG` filters, a broken folder or poisoned lock never stops the app; only the window owning the data folder rotates | unit `logging`, real binary (3 starts + a second window) |
| Every failure shown in a toast is logged in the core's English words; every panic is logged | unit `logging`, `errors::user_message` |
| Every string translated, no stale rows | `cargo xtask i18n` |
| The tank icon is the project's own drawing, never a glyph typed from the icon font | unit `fonts` |
| Every file states its licence and copyright (SPDX headers, `REUSE.toml`, `LICENSES/`) | CI `Licences (REUSE)` (`reuse lint`) |
| The app's About notice names version, licence, no warranty and source; the licence matches the crates' | unit `about` |
| Every download carries `THIRD-PARTY-LICENSES.html` (the crate notices) | `cargo about generate --fail` in both packaging scripts |
| Nothing git would commit is game content (file types, cache file names), a stray binary, a dump over 4 MiB or a name of the official client | unit `xtask::assets`, `cargo xtask assets` (in `ci`) |
| Counts grouped as each language writes them (3,660 / 3 660) | unit `tanki-i18n`, unit `model` |
| A missing author or license leaves no stray separator | unit `model` |
| Russian labels fit: Resources filters wrap to more rows, labelling chips go to a second row | it `chaos` (cut-off check), RU screenshots |
| A very long resource name stays inside the inspector, shortened, whole in a tooltip | it `resources` |
| Release notes in the interface language (Russian section), English when there is no translation; a copy up to 0.3.0 sees the English section alone; the notes box shows every line | unit `release::tests`, RU/EN screenshots |
| Dialogs fit Russian: texts sized to their lines, titles and warnings wrap, buttons inside the dialog | it `the_offer_fits_its_longer_russian_buttons`, RU screenshots (`cargo xtask shot … --lang ru`) |
| Core failures said in the interface language | unit `errors` |
