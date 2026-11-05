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
| The Telegram post of every version in CHANGELOG.ru.md has its title, headlines, #обновление tag and both buttons, and fits one message (4096 characters) | `cargo xtask telegram` |

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
| A skin's alpha goes only to an item that has one; an item with none takes the skin, alone and in Apply to all items, and undoes byte for byte | unit `apply`, it `bulk` |
| Paints: imported from a paints folder, found in the cache by their picture or their garage preview (a paint saved anew), one no pack has listed unnamed, effects left out; any paint in any paint's place, undone byte for byte; a paint is named without a modification and has no selector | e2e `paints_are_imported…`, ui `garage-paints` |
| Animated paints: the frame found in the picture (square cells, peak against half a cell, row ends and far frames), the most typical frame over a still paint, retimed into the replaced sheet over an animated one; marked in the list, and picking one says the game's server decides whether it moves | unit `sprite`, unit `paint`, ui `garage-paints`, ui `garage-paints-animated` |
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
| Every cell says what happens: own skin, from another modification, the item's own pick, no such skin, goes on later (no such modification in the cache yet), already on (the recorded pack item's grade), incomplete, not touched | unit `bulk`, it `bulk`, ui `bulk-where` |
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
| Items in the game's order (resource id), "Already on" is no reason to list, an item is skipped when nothing of it changes now or later | unit `bulk`, it `bulk` |
| Goes on later: queued on Apply (a newer choice for the same modification wins), kept across starts; the review lists it apart from the skipped ones; nothing goes on while the cache is unchanged, the game runs or a dialog or the wizard is open | unit `later`, unit `tanki-state`, it `bulk` |
| Once the game downloads the modification the skin goes on by itself in one batch; the Garage's bar says so and Undo puts the game's files back; the sidebar's Garage has a badge; a skin that can never go on leaves the queue with a toast | it `bulk`, ui `design-8-*` |
| Packs → Go on later lists what waits, Remove takes it out; the Item page marks a waiting modification | it `bulk`, ui `design-7-*`, `design-9-*` |
| Random use of the wizard and of the skins that go on later (every choice, runs with the game closed and running, stops, undos, the game downloading the missing modification, rescans, removals, stray clicks; seeded, `TANKI_BULK_MONKEYS=<n>` for more walks): the queue keeps one skin per modification, and every backup restores every cache file to the game's own | it `bulk` |
| Settings: rescan Automatic / Every hour / Manual is kept; Automatic looks at launch, on coming back to the window and every 5 minutes and reads the cache only when its stamp changed; Rescan now; theme, language; About with Check for updates and links; stacks and wraps in a narrow window | unit `later`, it `bulk`, it `navigation`, it `chaos`, ui `design-10-*` |

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

## 3D preview

| Case | Test |
|---|---|
| A 3DS model reads back with its objects, UVs and smoothing; damaged faces, UVs and non-finite vertices are dropped | unit `tanki-model` `three_ds` |
| Every truncation of a model and 2000 random corruptions read without a panic, faces always in range | unit `every_truncation_and_corruption_reads_without_panicking` |
| Normals follow smoothing groups: one group shares vertices, other groups and no group split them | unit `tanki-model` `geometry` |
| The geometry fingerprint survives another export of the same mesh (recognition) | unit `fingerprint_counts_survive_a_different_file`, `tanki-apply` recognise tests |
| The camera turns with a drag, keeps turning after a flick and stops; pitch and zoom stay in their limits; home takes the short way; turning by itself waits for the user to let go | unit `tanki-viewer` `camera` |
| Turning by itself is on by default: it waits five seconds, gathers speed and goes on very slowly; the way back home starts and ends at rest and can keep the zoom; a whole tank may fill more of the picture; a kept pose is restored within the limits; a still stage asks for a frame when it is to start, the wait counting the time between frames; turning it by hand pauses it, and it goes on from there five seconds later | unit `turning_by_itself_waits_five_seconds_…`, `turning_home_starts_and_ends_at_rest_…`, `a_larger_fill_comes_closer_…`, `a_still_stage_is_woken_…`, `turning_it_by_hand_pauses_…` |
| Another hull, turret or paint keeps the view: the same angle and zoom, still turning from where it was with no new wait; the zoom, angle and auto-turn come back after a restart; a kept view without an item or with a number that is none is not kept | it `switching_the_hull_turret_or_paint_keeps_the_tank_turning_…`, `the_zoom_turn_and_auto_turn_come_back_after_a_restart`, unit `a_3d_view_without_an_item_…` |
| The adapter picks the quality (OpenGL is always Economy); lighter levels never draw more | unit `tanki-viewer` `quality` |
| On a real adapter: the tank is opaque in the middle on a clear background, with and without anti-aliasing; details cover the paint; a paint picture replaces the paint colour | unit `tanki-viewer` `gpu` (skipped where there is no adapter) |
| A stage without a GPU is unavailable and harmless (the Garage shows the flat picture) | unit `a_stage_without_a_gpu_is_unavailable_and_harmless` |
| The cache look is the newest revision with every file; a replaced file changes its key; a variant without a model (or paint) has none; a painted hull is the hull's files with the paint's picture | unit `tanki-apply` `look` |
| A look becomes a textured part; the separate alpha picture becomes the details' alpha; a broken model or texture is an error and is not tried again | unit `tanki-studio` `preview3d` |
| The 3D preview is on until turned off; both choices survive a restart | e2e `the_3d_preview_is_on_until_turned_off_and_both_choices_survive_a_restart`, it `the_3d_preview_is_on_by_default_and_settings_turn_it_off_for_good` |
| Studio light: the top catches the key light and the sides are told apart; the contact shadow sits at the model and fades out; bright paint rolls off toward white keeping its hue; 4× anti-aliasing softens edges; the same view draws the same picture and home comes back to it | unit `tanki-viewer` `gpu` visual checks (`TANKI_VISUAL_DIR=<dir>` saves the pictures) |
| With a GPU, the Garage draws the item where the flat picture was; the toolbar is there and Details stays clear of it | it `the_garage_draws_the_item_in_3d_where_the_flat_picture_was` |
| A drag turns the model, the drag hint gives way to the caption, Home brings the first view back; the wheel zooms and never scrolls the page | it `dragging_turns_the_model…`, `the_wheel_zooms_the_stage…` |
| A picked skin compares with the game (two different halves), the divider follows a drag, auto-turn toggles, a PNG is saved with a clear background; with no skin picked, Compare is disabled | it `a_picked_skin_compares_with_the_game…`, `without_a_picked_skin_there_is_nothing_to_compare` |
| Quick settings turn 3D off (flat picture, only the settings button left) and on; they apply as they change, with no buttons but the ×; Escape and × close them; each quality still draws and survives a restart | it `quick_settings_…`, `escape_and_the_close_button_…`, `quality_choices_…` |
| Hulls and turrets wear the plain green paint of the cache until a skin or paint says otherwise; without it, the cache look as it is | it `hulls_and_turrets_wear_the_green_of_the_cache_until_a_paint_is_chosen` |
| A neutral lightmap leaves a colour as it is (the garage's calibrated exposure); a brighter one rolls off and keeps the hue | unit `a_neutral_lightmap_leaves_the_colour_as_it_is`, `bright_paint_rolls_off_toward_white_and_keeps_its_hue` |
| A model that cannot be read shows the flat picture and is not read again; a modification the cache lacks has no stage; hull and turret get their own scenes | it `a_model_that_cannot_be_read_…`, `every_modification_and_the_turret_…` |
| A 3DS object's pivot (local axes) is read; mounts, collision boxes and other helpers are told apart and never drawn | unit `helpers_are_told_apart_and_give_the_mount_and_the_pivot`, `a_tank_sets_its_turret_s_pivot_…` |
| A whole tank sets the turret's pivot on the hull's mount (the top's middle when a model has neither); its key changes with any part; the tank's parts start as the starter hull and turret at M3 in the cache's green, then follow what is chosen and survive a restart; a broken saved value gives the starter part | unit `a_tank_sets_its_turret_…`, `without_a_mount_or_pivot_…`, `an_assembled_look_…`, `the_tank_starts_as_the_starter_one_…` |
| A new user opens on the starter hull, shown whole in the cache's green; the switch shows the hull alone; a paint goes on both parts; the view stays while the parts change; the switch (Settings and the quick dialog) and the parts survive a restart; without the tank's other part in the cache the item is shown alone | it `a_new_user_sees_the_starter_tank_whole_…`, `the_tank_keeps_the_parts_chosen_last_…`, `without_the_tank_s_other_part_…`, `the_3d_preview_is_on_by_default_…` |
| A paint is shown on the hull last looked at; without any hull model it keeps its flat picture | it `a_paint_is_shown_on_the_hull_last_looked_at`, `a_paint_without_any_hull_model_…` |
| Let go while it turned by itself, it goes on the next frame at its usual speed, the way it was flicked (a flick slows down into it, never stopping or turning back); zooming does not stop it; grabbed while it only waited, it waits the usual time | unit `let_go_while_it_turned_by_itself_…` |
| While the next model loads, the one before stays on the stage, faded under a loading pill, never a blank frame; the next one fades in | it `while_the_next_model_loads_the_one_before_stays_…` (`TANKI_VISUAL_DIR` saves the frame) |
| Details of a whole tank: the same stage, tank and view as the Garage; the hull, turret and paint as cards, the picked one shown in the panels below; with the whole tank off, or its other part not in the cache, the item alone in 3D with its name and chips; with 3D off, its flat picture; model and cards never overlap from 820 to 1920 px | it `details_of_a_whole_tank_…`, `details_of_an_item_alone_…` (`TANKI_VISUAL_DIR` saves the pages) |
| Toolbar, Details and stage never overlap from 820 to 1920 px wide | it `the_toolbar_details_and_stage_never_overlap_at_any_window_size` |
| Monkey: 300 random clicks, drags, wheels, keys, item and size changes on the 3D preview; no stray dialog, the camera stays finite, every state renders | it `a_monkey_on_the_3d_preview_breaks_nothing` |

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
| A backup is needed while its skin is on (older ones of the same file too), old when taken off or overwritten by the game; only numeric backup folders inside the backups folder can be deleted | unit `tanki-apply::backups` |
| Delete old keeps every needed backup; the limit deletes the oldest old ones and never a needed one; undoing one backup undoes what sits on top of it first | unit `tanki-apply::backups`, it `backups` |
| A full disk: nothing written without its backup, a half-written backup removed, old backups offered first, needed ones only with consent naming the skins, Cancel deletes nothing | unit `tanki-apply::backups`, it `a_full_disk_writes_nothing_without_a_backup_and_frees_old_backups_first` |
| Every delete on the Backups page asks first (one, a day, all); Cancel deletes nothing; the question names the skins on now that lose their way back; the game files are never touched | it `every_delete_asks_first_and_names_the_skins_that_lose_their_way_back` |
| A skin a game update removed is Lost in the Garage, picked again for one-click Apply, and its backup is old | it `a_skin_a_game_update_removed_is_lost_in_the_garage_and_its_backup_is_old` |
| Details opens the Item page and Back returns; the Settings backups row opens the Backups page and Back returns; the sidebar has no Item, has About | it `backups`, it `navigation` |
| About: the running version's notes in both languages from the built-in changelogs, one release open at a time, community links, Check for updates says up to date | unit `changelog`, it `about_lists_the_releases_and_opens_one_at_a_time` |
| About's links show Telegram's and YouTube's official marks, unchanged, at their own proportions, with the trademark note; where each file came from is in `docs/brand-marks.md` | unit `brand`, it `backups` (About links), ui `design-11-*` |
| Settings, Backups, About, Item and Garage hold together from 420 to 1920 points wide | it `the_new_pages_hold_together_in_narrow_and_wide_windows` |
| Random use of pages, filters, the limit, applying, taking off, game updates, a full disk, deleting and undoing: no panic, the numbers shown are the files on disk, the limit holds, needed backups only go with consent (`TANKI_PAGES_MONKEYS` seeds) | it `random_use_of_backups_settings_about_and_the_garage_states_never_breaks` |
