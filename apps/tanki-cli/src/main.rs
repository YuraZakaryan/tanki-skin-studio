// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use anyhow::{Context, Result};
use std::{
    env,
    path::{Path, PathBuf},
};
use tanki_core::{
    apply::{apply_variant, latest_transaction, restore_transaction},
    cache::{existing_cache_roots, scan_cache},
    pack::{import_pack, load_pack},
    paths::app_data_dir,
    state::StateStore,
};

fn chosen_cache(arg: Option<&String>) -> Result<PathBuf> {
    if let Some(x) = arg {
        return Ok(PathBuf::from(x));
    }
    existing_cache_roots()
        .into_iter()
        .next()
        .map(|(_, p)| p)
        .context("no cache found; pass a folder path")
}
/// One section of a report: a count, a heading and the first entries.
fn print_list(heading: &str, entries: &[String]) {
    if entries.is_empty() {
        return;
    }
    println!("{} {heading}:", entries.len());
    for entry in entries.iter().take(30) {
        println!("  {entry}");
    }
    if entries.len() > 30 {
        println!("  ... and {} more", entries.len() - 30);
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let data = app_data_dir();
    let store = StateStore::open(&data.join("state.sqlite"))?;
    match args.get(1).map(String::as_str) {
        Some("scan") => {
            let root = chosen_cache(args.get(2))?;
            let scan = scan_cache(&root, None)?;
            println!(
                "{}: {} resources in {}",
                scan.game,
                scan.resources.len(),
                scan.root.display()
            );
            for r in scan.resources.iter().take(30) {
                println!(
                    "{}  {}",
                    r.kind,
                    r.logical_url.as_deref().unwrap_or(&r.group)
                );
            }
        }
        Some("recognise") => {
            // recognise [cache-folder]: items found by content, from the imported packs (read only).
            let root = chosen_cache(args.get(2))?;
            let scan = scan_cache(&root, None)?;
            let packs = store.list_packs()?;
            let found = tanki_core::recognize::recognise_by_content(&scan, &packs);
            for r in &found {
                println!(
                    "{}  {} {} {}  ({})",
                    r.marker, r.kind, r.id, r.grade, r.evidence
                );
            }
            println!(
                "{} resources recognised by content in {} packs",
                found.len(),
                packs.len()
            );
        }
        Some("archive-pack") => {
            // archive-pack <archive-folder> <pack-folder>: build a pack from a model archive.
            let source = Path::new(args.get(2).context("archive folder required")?);
            let out = Path::new(args.get(3).context("output pack folder required")?);
            let summary = tanki_core::archive::build_model_pack(source, out)?;
            println!(
                "Built {} ({} items, {} variants); import it with `import {}`",
                out.display(),
                summary.items,
                summary.variants,
                out.display()
            );
            // The import report: what could not be used, so a differently laid out or named
            // archive is noticed instead of silently importing less.
            if summary.without_preview > 0 {
                println!(
                    "{} variants have no preview picture",
                    summary.without_preview
                );
            }
            if !summary.unsupported.is_empty() {
                let formats: Vec<String> = summary
                    .unsupported
                    .iter()
                    .map(|(ext, n)| format!("{n} .{ext}"))
                    .collect();
                println!("Formats the client cannot use: {}", formats.join(", "));
            }
            print_list(
                "files were not recognised and are not in the pack",
                &summary.unused,
            );
            print_list(
                "folders hold models or pictures but no known hull or turret",
                &summary.unrecognised,
            );
            print_list(
                "folders were left out: only a name says what they are",
                &summary.skipped,
            );
            print_list("guesses to check", &summary.notes);
            if summary.truncated {
                println!("The archive is too large: only part of it was read");
            }
        }
        Some("pack-validate") => {
            let path = Path::new(args.get(2).context("pack path required")?);
            let manifest = load_pack(path)?;
            println!(
                "Valid pack: {} {} ({} items)",
                manifest.name,
                manifest.version,
                manifest.items.len()
            );
        }
        Some("import") => {
            let path = Path::new(args.get(2).context("pack path required")?);
            let pack = import_pack(path, &data.join("packs"))?;
            store.save_pack(&pack)?;
            println!("Imported {} {}", pack.manifest.name, pack.manifest.version);
        }
        Some("packs") => {
            for p in store.list_packs()? {
                println!(
                    "{}@{}  {}",
                    p.manifest.id,
                    p.manifest.version,
                    p.path.display()
                );
            }
        }
        Some("apply") => {
            let id = args.get(2).context("pack ID required")?;
            let variant_id = args.get(3).context("variant ID required")?;
            let root = chosen_cache(args.get(4))?;
            let scan = scan_cache(&root, None)?;
            let pack = store
                .list_packs()?
                .into_iter()
                .find(|p| &p.manifest.id == id)
                .context("pack not imported")?;
            let (item, variant) = pack
                .manifest
                .items
                .iter()
                .find_map(|item| {
                    item.variants
                        .iter()
                        .find(|v| &v.id == variant_id)
                        .map(|v| (item, v))
                })
                .context("variant not found")?;
            let tx = apply_variant(&scan, &pack, &item.target, variant, &data.join("backups"))?;
            store.record_recent_asset(&format!(
                "{}/{}/{}",
                item.target.kind, item.target.id, item.target.grade
            ))?;
            println!(
                "Applied {} to {} files; transaction {}. Restart the game.",
                variant.name,
                tx.changes.len(),
                tx.id
            );
        }
        Some("restore") => {
            let (dir, tx) =
                latest_transaction(&data.join("backups"))?.context("no backup found")?;
            restore_transaction(&dir, &tx)?;
            println!("Restored {} files. Restart the game.", tx.changes.len());
        }
        _ => println!(
            "Tanki Skin Studio CLI {} ({}, no warranty; source: github.com/YuraZakaryan/tanki-skin-studio)\n  scan [cache-path]\n  recognise [cache-path]\n  archive-pack ARCHIVE-FOLDER PACK-FOLDER\n  pack-validate PACK\n  import PACK\n  packs\n  apply PACK-ID VARIANT-ID [cache-path]\n  restore",
            env!("CARGO_PKG_VERSION"),
            env!("CARGO_PKG_LICENSE")
        ),
    }
    Ok(())
}
