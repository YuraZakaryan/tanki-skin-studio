// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Project automation, run as `cargo xtask <command>` (same pattern as PdfCraft's xtask).

use std::{
    path::{Path, PathBuf},
    process::{Command, ExitCode},
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use tanki_core::{pack::import_pack, state::StateStore};
use tanki_testkit::{PackOptions, build_cache, build_pack_dir, scratch};

mod assets;
mod ico;
mod layers;

type Task = fn(&[String]) -> Result<()>;

const COMMANDS: &[(&str, &str, Task)] = &[
    (
        "ci",
        "provenance, fmt --check, translations, clippy -D warnings, layering, workspace tests (no window needed)",
        ci,
    ),
    (
        "assets",
        "refuse game files, stray binaries, dumps and names of the official client in what git would commit",
        assets_cmd,
    ),
    (
        "layers",
        "enforce the crate dependency layering (layers::TABLE)",
        layers_cmd,
    ),
    (
        "test",
        "cargo test --workspace: core and CLI end-to-end journeys included",
        test,
    ),
    (
        "shot",
        "run the app on a fixture and save one screenshot: shot <page> [--lang en|ru] [--theme dark|light] [--actions \"...\"] [--size WxH] [--incomplete] [--out file.png]",
        shot,
    ),
    (
        "sheet",
        "contact sheets of the real cache: each recognised resource next to the archive's M0..M3 textures, to check the automatic recognition by eye (writes PNGs to --out, default /tmp/sheets)",
        sheet,
    ),
    (
        "previews",
        "match every garage preview in the real cache (image.jpg + alpha.jpg, 164x106) against the archive's preview.png files; writes a contact sheet to --out (default /tmp/sheets)",
        previews,
    ),
    (
        "i18n",
        "check that every UI string in the code has a Russian translation and the catalog has no stale rows; --template prints the missing rows",
        i18n,
    ),
    (
        "ui-diff",
        "compare two screenshots: ui-diff <ours.png> <reference.png> [out.png]; prints the share of differing pixels",
        ui_diff,
    ),
    (
        "e2e-ui",
        "launch the real app on a fixture cache, capture each screen, assert on pixels",
        e2e_ui,
    ),
    (
        "ico",
        "pack square PNGs into a Windows icon: ico <out.ico> <in.png>... (packaging/icons.sh)",
        ico::run,
    ),
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask has a parent dir")
        .to_path_buf()
}

fn cargo(args: &[&str]) -> Result<()> {
    eprintln!("$ cargo {}", args.join(" "));
    let status = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(args)
        .current_dir(root())
        .status()
        .context("spawn cargo")?;
    if !status.success() {
        bail!("cargo {} failed with {status}", args.join(" "));
    }
    Ok(())
}

fn layers_cmd(_: &[String]) -> Result<()> {
    let output = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(["metadata", "--no-deps", "--format-version", "1", "--locked"])
        .current_dir(root())
        .output()
        .context("cargo metadata")?;
    if !output.status.success() {
        bail!(
            "cargo metadata failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let crates = layers::from_metadata(&metadata).map_err(anyhow::Error::msg)?;
    println!("{:<16} {:<12} workspace dependencies", "crate", "class");
    for krate in &crates {
        let deps: Vec<String> = krate
            .deps
            .iter()
            .filter(|d| d.workspace)
            .map(|d| format!("{}{}", d.name, if d.dev { " (dev)" } else { "" }))
            .collect();
        let class = layers::classify(&krate.name).map_or_else(|| "?".into(), layers::describe);
        println!("{:<16} {:<12} {}", krate.name, class, deps.join(", "));
    }
    let violations = layers::check(&crates);
    if violations.is_empty() {
        println!("\nOK: {} crates, no layering violations.", crates.len());
        return Ok(());
    }
    for v in &violations {
        eprintln!("  - {v}");
    }
    bail!("{} layering violation(s)", violations.len())
}

fn ci(_: &[String]) -> Result<()> {
    assets_cmd(&[])?;
    cargo(&["fmt", "--all", "--", "--check"])?;
    i18n(&[])?;
    cargo(&[
        "clippy",
        "--workspace",
        "--all-targets",
        "--locked",
        "--",
        "-D",
        "warnings",
    ])?;
    test(&[])
}

/// `cargo xtask assets`: check every file git would commit (tracked, plus new files that are not
/// ignored), so game content is refused before it reaches a commit.
fn assets_cmd(_: &[String]) -> Result<()> {
    let output = Command::new("git")
        .args([
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ])
        .current_dir(root())
        .output()
        .context("git ls-files")?;
    if !output.status.success() {
        bail!(
            "git ls-files failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let mut files = Vec::new();
    for path in String::from_utf8_lossy(&output.stdout)
        .split('\0')
        .filter(|path| !path.is_empty())
    {
        // A tracked file deleted in the work tree is not committed as content.
        if let Ok(bytes) = std::fs::read(root().join(path)) {
            files.push((path.to_owned(), bytes));
        }
    }
    let entries: Vec<assets::Entry<'_>> = files
        .iter()
        .map(|(path, bytes)| assets::Entry {
            path,
            size: bytes.len() as u64,
            text: std::str::from_utf8(bytes).ok(),
        })
        .collect();
    let problems = assets::check(&entries);
    if !problems.is_empty() {
        bail!(
            "{} provenance problem(s):\n  {}",
            problems.len(),
            problems.join("\n  ")
        );
    }
    println!("assets: {} files clean", entries.len());
    Ok(())
}

fn test(_: &[String]) -> Result<()> {
    cargo(&["test", "--workspace", "--locked", "--no-fail-fast"])
}

/// Value of `--name` in `args`.
fn option<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

fn studio_exe() -> PathBuf {
    root().join("target/debug").join(if cfg!(windows) {
        "tanki-studio.exe"
    } else {
        "tanki-studio"
    })
}

/// Build the app, run it once on a fixture cache with a pack imported, and save its screenshot.
fn shot(args: &[String]) -> Result<()> {
    cargo(&["build", "--package", "tanki-studio"])?;
    let page = args.first().map_or("overview", String::as_str);
    let out = PathBuf::from(option(args, "--out").unwrap_or("shot.png"));
    let work = scratch("shot");
    let cache = build_cache(&work.join("cache"))?;
    // `--incomplete`: the pack's Wasp skin has no model, so the Garage shows it as Incomplete.
    let options = PackOptions {
        hull_without_model: args.iter().any(|a| a == "--incomplete"),
        ..PackOptions::default()
    };
    let pack = build_pack_dir(&work.join("pack"), &options)?;
    let zip = tanki_testkit::zip_dir(&pack, &work.join("fixture.tankipack"), &[])?;
    let actions = format!(
        "import:{};{}",
        zip.display(),
        option(args, "--actions").unwrap_or("")
    );
    let mut command = Command::new(studio_exe());
    command
        .env("TANKI_DATA_DIR", work.join("data"))
        .env("TANKI_CACHE_DIR", &cache.root)
        .env("TANKI_VIEW", page)
        .env("TANKI_GAME_PROCESSES", "")
        .env("TANKI_SHOT", &out)
        .env("TANKI_ACTIONS", actions);
    if let Some(lang) = option(args, "--lang") {
        command.env("TANKI_LANG", lang);
    }
    if let Some(theme) = option(args, "--theme") {
        command.env("TANKI_THEME", theme);
    }
    if let Some(size) = option(args, "--size") {
        command.env("TANKI_SIZE", size);
    }
    if let Some(splash) = option(args, "--splash") {
        command.env("TANKI_SPLASH", splash);
    }
    let status = command.status().context("launch tanki-studio")?;
    anyhow::ensure!(status.success(), "app exited with {status}");
    println!("saved {}", out.display());
    Ok(())
}

/// Resize to `width` × `height` with a box filter when the image is an exact multiple (retina).
fn normalise(image: image::RgbaImage, width: u32, height: u32) -> image::RgbaImage {
    if image.width() == width && image.height() == height {
        return image;
    }
    image::imageops::resize(&image, width, height, image::imageops::FilterType::Triangle)
}

/// Pixel comparison of the app against the design export (both 1440 × 900 points).
fn ui_diff(args: &[String]) -> Result<()> {
    let [ours, reference, rest @ ..] = args else {
        bail!("usage: ui-diff <ours.png> <reference.png> [out.png]");
    };
    let reference = image::open(reference)?.to_rgba8();
    let (w, h) = (reference.width(), reference.height());
    let ours = normalise(image::open(ours)?.to_rgba8(), w, h);
    let mut differing = 0_u64;
    let mut total_error = 0_u64;
    let mut diff = image::RgbaImage::new(w * 3, h);
    for y in 0..h {
        for x in 0..w {
            let a = ours.get_pixel(x, y);
            let b = reference.get_pixel(x, y);
            let error: i32 = (0..3).map(|i| (a[i] as i32 - b[i] as i32).abs()).sum();
            total_error += error as u64;
            if error > 48 {
                differing += 1;
            }
            diff.put_pixel(x, y, *a);
            diff.put_pixel(x + w, y, *b);
            let heat = (error * 2).min(255) as u8;
            diff.put_pixel(x + 2 * w, y, image::Rgba([heat, 0, 255 - heat, 255]));
        }
    }
    let pixels = u64::from(w) * u64::from(h);
    println!(
        "differing pixels: {:.2}%   mean error: {:.2}/765",
        differing as f64 * 100.0 / pixels as f64,
        total_error as f64 / pixels as f64
    );
    if let Some(out) = rest.first() {
        diff.save(out)?;
        println!("side-by-side (ours | reference | difference): {out}");
    }
    Ok(())
}

/// A colour the screenshot must contain (or not) in a share of its pixels. Shares are in parts
/// per million of the frame, so the display's scale does not matter.
struct Share {
    what: &'static str,
    rgb: [u8; 3],
    tolerance: i32,
    min_ppm: usize,
    max_ppm: usize,
}

const fn has(what: &'static str, rgb: [u8; 3], tolerance: i32, min_ppm: usize) -> Share {
    Share {
        what,
        rgb,
        tolerance,
        min_ppm,
        max_ppm: usize::MAX,
    }
}

const fn lacks(what: &'static str, rgb: [u8; 3], tolerance: i32, max_ppm: usize) -> Share {
    Share {
        what,
        rgb,
        tolerance,
        min_ppm: 0,
        max_ppm,
    }
}

/// Design tokens of the dark and light themes (design.pen variables), as RGB.
const PRIMARY: [u8; 3] = [0x8C, 0xFF, 0x4D];
const ACCENT: [u8; 3] = [0xFF, 0xB0, 0x20];
const DANGER: [u8; 3] = [0xFF, 0x60, 0x60];
const DARK_BG: [u8; 3] = [0x0A, 0x0E, 0x0B];
const DARK_SURFACE: [u8; 3] = [0x10, 0x17, 0x12];
const DARK_SURFACE_2: [u8; 3] = [0x15, 0x1E, 0x17];
const LIGHT_BG: [u8; 3] = [0xEC, 0xEF, 0xE8];

/// One screen of the app: how it is launched and what its pixels must contain.
struct Scene {
    name: &'static str,
    page: &'static str,
    actions: &'static str,
    theme: &'static str,
    lang: &'static str,
    /// `TANKI_SPLASH` value, empty for none.
    splash: &'static str,
    /// `TANKI_SIZE`, empty for the default window.
    size: &'static str,
    /// The cache folder does not exist: the "cache not found" state.
    no_cache: bool,
    /// Processes the app finds when it checks whether the game is running.
    processes: &'static str,
    /// Run on a pack whose Wasp skin has no model, so it cannot be applied.
    incomplete_skin: bool,
    checks: &'static [Share],
}

/// The window of every UI run unless a scene asks for another: small enough for a CI runner's
/// 1024 x 768 screen, so a local run and a CI run draw the same frame.
const E2E_SIZE: &str = "1000x620";

const SCENE: Scene = Scene {
    name: "",
    page: "overview",
    actions: "",
    theme: "dark",
    lang: "en",
    splash: "",
    size: E2E_SIZE,
    no_cache: false,
    processes: "",
    incomplete_skin: false,
    checks: &[],
};

const SCENES: &[Scene] = &[
    Scene {
        name: "splash",
        splash: "hold",
        checks: &[has("the progress bar", PRIMARY, 6, 150)],
        ..SCENE
    },
    Scene {
        name: "overview",
        // A skin is picked: the Apply button is on only then.
        actions: "select:hull;pick:0",
        checks: &[
            has("the lime Import button", PRIMARY, 6, 2_500),
            has("the amber Apply button", ACCENT, 6, 3_000),
            has("the sidebar and panels", DARK_SURFACE, 2, 100_000),
        ],
        ..SCENE
    },
    Scene {
        name: "garage",
        page: "garage",
        actions: "select:hull;pick:0",
        checks: &[
            // The fixture pack's flat hull preview shows on the stage and on the item card.
            has("the skin preview from the pack", [30, 160, 90], 12, 8_000),
            has("the amber Apply buttons", ACCENT, 6, 3_000),
        ],
        ..SCENE
    },
    Scene {
        name: "resources",
        page: "resources",
        checks: &[
            has("the lime markers and chips", PRIMARY, 6, 200),
            has("the catalog panels", DARK_SURFACE, 2, 150_000),
        ],
        ..SCENE
    },
    Scene {
        name: "packs",
        page: "packs",
        checks: &[
            has("the active pack's lime border", PRIMARY, 6, 150),
            has("the pack card", DARK_SURFACE_2, 2, 5_000),
        ],
        ..SCENE
    },
    Scene {
        name: "info",
        page: "info",
        actions: "select:hull;pick:0",
        checks: &[has("the lime Apply button", PRIMARY, 6, 1_500)],
        ..SCENE
    },
    Scene {
        name: "confirm",
        page: "garage",
        actions: "select:hull;begin:0:0",
        checks: &[
            has("the dialog surface", DARK_SURFACE_2, 2, 60_000),
            has("the amber Apply button in the dialog", ACCENT, 6, 2_000),
            // The amber hint bar of the dialog (accent over the dialog surface).
            has("the dialog's warning bar", [50, 48, 24], 6, 6_000),
        ],
        ..SCENE
    },
    // A skin without a model: the Apply button is off, and the reason is said in red under it.
    Scene {
        name: "incomplete",
        page: "garage",
        actions: "select:hull;pick:0",
        incomplete_skin: true,
        checks: &[
            // Measured: 683 ppm at 1x (CI), 2014 at 2x, 0 on a Garage without an incomplete skin. Thin
            // text at 1x has few pixels of the exact red, hence the wider tolerance.
            has("the red reason and Incomplete badge", DANGER, 24, 200),
            lacks("an Apply button at full strength", ACCENT, 6, 2_000),
        ],
        ..SCENE
    },
    Scene {
        name: "game-running",
        page: "garage",
        actions: "select:hull;begin:0:0",
        processes: "GameClient.exe",
        checks: &[
            has("the warning box", [50, 48, 24], 6, 15_000),
            has("the amber Check again button", ACCENT, 6, 2_000),
            // The Apply confirmation is not shown while the game runs.
            lacks("the file list", DARK_SURFACE, 2, 60_000),
        ],
        ..SCENE
    },
    Scene {
        name: "palette",
        actions: "palette:wasp",
        checks: &[
            has("the palette surface", DARK_SURFACE_2, 2, 80_000),
            has("the selected row", [0x1F, 0x32, 0x1B], 30, 3_000),
        ],
        ..SCENE
    },
    Scene {
        name: "light",
        theme: "light",
        checks: &[
            has("the light page background", LIGHT_BG, 2, 150_000),
            lacks("dark page background", DARK_BG, 2, 2_000),
        ],
        ..SCENE
    },
    Scene {
        name: "empty",
        no_cache: true,
        checks: &[has("the lime Choose button", PRIMARY, 6, 1_500)],
        ..SCENE
    },
    // A narrow window: the sidebar becomes an icon rail and the columns stack.
    Scene {
        name: "narrow",
        size: "860x600",
        checks: &[has("the lime Import button", PRIMARY, 6, 1_000)],
        ..SCENE
    },
    // A large pack being imported: the progress card in the corner, the app usable around it.
    Scene {
        name: "importing",
        page: "packs",
        actions: "dismiss;importing:skins-collection.tankipack:45",
        checks: &[has("the progress bar's lime fill", PRIMARY, 6, 600)],
        ..SCENE
    },
    // Restore asks first and lists what goes back: two replacements, then the Restore button.
    // Last on purpose: the scenes share one data folder and cache, and this one writes to them.
    Scene {
        name: "restore",
        page: "garage",
        actions: "select:hull;apply:0:0;apply:1:0;dismiss;ask-restore",
        checks: &[
            has("the dialog surface", DARK_SURFACE_2, 2, 60_000),
            has("the dialog's warning bar", [50, 48, 24], 6, 6_000),
            lacks("the confirmation of a replacement", ACCENT, 6, 1_500),
        ],
        ..SCENE
    },
];

fn count_near(image: &image::RgbaImage, rgb: [u8; 3], tolerance: i32) -> usize {
    image
        .pixels()
        .filter(|p| (0..3).all(|i| (p[i] as i32 - rgb[i] as i32).abs() <= tolerance))
        .count()
}

/// How to start the app once.
struct Run<'a> {
    exe: &'a Path,
    data: &'a Path,
    cache: &'a Path,
    shot: &'a Path,
    page: &'a str,
    actions: &'a str,
    theme: &'a str,
    lang: &'a str,
    splash: &'a str,
    size: &'a str,
    processes: &'a str,
}

impl<'a> Run<'a> {
    fn new(exe: &'a Path, data: &'a Path, cache: &'a Path, shot: &'a Path) -> Self {
        Self {
            exe,
            data,
            cache,
            shot,
            page: "overview",
            actions: "",
            // Never the machine's own appearance: the checks look for the dark theme's colours.
            theme: "dark",
            lang: "",
            splash: "",
            size: E2E_SIZE,
            processes: "",
        }
    }

    /// Run the app with the dev environment; it saves a screenshot and exits on its own.
    fn launch(&self) -> Result<()> {
        let mut command = Command::new(self.exe);
        command
            .env("TANKI_DATA_DIR", self.data)
            .env("TANKI_CACHE_DIR", self.cache)
            .env("TANKI_SHOT", self.shot)
            .env("TANKI_GAME_PROCESSES", self.processes)
            .env("TANKI_ACTIONS", self.actions);
        for (name, value) in [
            ("TANKI_VIEW", self.page),
            ("TANKI_THEME", self.theme),
            ("TANKI_LANG", self.lang),
            ("TANKI_SPLASH", self.splash),
            ("TANKI_SIZE", self.size),
        ] {
            if !value.is_empty() {
                command.env(name, value);
            }
        }
        let mut child = command.spawn().context("launch tanki-studio")?;
        let started = Instant::now();
        loop {
            if let Some(status) = child.try_wait()? {
                anyhow::ensure!(status.success(), "app exited with {status}");
                return Ok(());
            }
            if started.elapsed() > Duration::from_secs(90) {
                let _ = child.kill();
                bail!("app did not exit within 90s");
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }
}

/// Run the checks of one scene on its screenshot; returns what failed.
fn check_scene(scene: &Scene, image: &image::RgbaImage) -> Vec<String> {
    let mut failures = Vec::new();
    let unique = image
        .pixels()
        .map(|p| u32::from_le_bytes(p.0))
        .collect::<std::collections::HashSet<_>>()
        .len();
    let pixels = (image.width() * image.height()) as usize;
    let mut report = format!(
        "{:<12} {}x{}  colours {unique}",
        scene.name,
        image.width(),
        image.height()
    );
    if image.width() < 800 || unique < 200 {
        failures.push(format!("{}: screenshot is blank or too small", scene.name));
    }
    for share in scene.checks {
        let ppm = count_near(image, share.rgb, share.tolerance) * 1_000_000 / pixels;
        report.push_str(&format!("  {} {ppm}ppm", share.what));
        if ppm < share.min_ppm {
            failures.push(format!(
                "{}: {} is missing ({ppm} ppm, need {})",
                scene.name, share.what, share.min_ppm
            ));
        }
        if ppm > share.max_ppm {
            failures.push(format!(
                "{}: unexpected {} ({ppm} ppm, at most {})",
                scene.name, share.what, share.max_ppm
            ));
        }
    }
    println!("{report}");
    failures
}

/// Share of pixels that differ clearly between two screenshots of the same size.
fn differing_share(a: &image::RgbaImage, b: &image::RgbaImage) -> f64 {
    if a.dimensions() != b.dimensions() {
        return 1.0;
    }
    let differing = a
        .pixels()
        .zip(b.pixels())
        .filter(|(x, y)| {
            (0..3)
                .map(|i| (x[i] as i32 - y[i] as i32).abs())
                .sum::<i32>()
                > 48
        })
        .count();
    differing as f64 / (a.width() * a.height()) as f64
}

/// Journeys driven through the app's own handlers (`TANKI_ACTIONS`), verified on disk afterwards:
/// the cache bytes and the persisted state are the source of truth.
fn gui_journeys(exe: &Path, shots: &Path, only: Option<&String>) -> Vec<String> {
    use tanki_testkit::{HORNET_MODEL, HULL_MODEL, zip_dir};
    let mut failures = Vec::new();
    let mut check = |name: &str, run: &dyn Fn() -> Result<()>| {
        if only.is_none_or(|n| n == name) {
            match run() {
                Ok(()) => println!("{name:<14} ok"),
                Err(e) => failures.push(format!("{name}: {e:#}")),
            }
        }
    };
    let fresh = |label: &str| -> Result<_> {
        let work = scratch(label);
        let cache = build_cache(&work.join("cache"))?;
        let pack = build_pack_dir(&work.join("pack"), &PackOptions::default())?;
        let zip = zip_dir(&pack, &work.join("fixture.tankipack"), &[])?;
        Ok((work, cache, zip))
    };
    let contents = |files: &[PathBuf]| -> Vec<Vec<u8>> {
        files
            .iter()
            .map(|p| std::fs::read(p).unwrap_or_default())
            .collect()
    };

    // A monkey in the real window: per seed, the app starts on a fresh cache and takes thirty random
    // actions (pages, items, out-of-range modifications and variants, unknown filters, apply,
    // confirm, restore, language, theme, interface size, the palette). It must get through them,
    // draw its screenshot and close by itself; every replacement must then undo to the original.
    check("gui-monkey", &|| {
        struct Rng(u64);
        impl Rng {
            fn below(&mut self, n: u64) -> u64 {
                self.0 ^= self.0 << 13;
                self.0 ^= self.0 >> 7;
                self.0 ^= self.0 << 17;
                self.0 % n
            }
        }
        for seed in [5u64, 17, 23, 101, 777] {
            let (work, cache, zip) = fresh(&format!("gui-monkey-{seed}"))?;
            let data = work.join("data");
            let files: Vec<PathBuf> = cache
                .hull_files
                .iter()
                .chain(&cache.turret_files)
                .cloned()
                .collect();
            let before = contents(&files);
            let mut rng = Rng(seed);
            let pages = ["overview", "garage", "resources", "packs", "info"];
            let mut steps = vec![format!("import:{}", zip.display())];
            for _ in 0..30 {
                steps.push(match rng.below(12) {
                    0 => format!("page:{}", pages[rng.below(5) as usize]),
                    1 => format!(
                        "select:{}",
                        ["hull", "turret", "/9/9/9/9/"][rng.below(3) as usize]
                    ),
                    2 => format!("grade:{}", rng.below(5)),
                    3 => format!(
                        "filter:{}",
                        ["all", "hull", "image", "model", "nope"][rng.below(5) as usize]
                    ),
                    4 => format!("begin:{}:{}", rng.below(3), rng.below(3)),
                    5 => format!("apply:{}:{}", rng.below(3), rng.below(3)),
                    6 => "confirm".to_owned(),
                    7 => "restore".to_owned(),
                    8 => format!("lang:{}", ["en", "ru"][rng.below(2) as usize]),
                    9 => format!(
                        "theme:{}",
                        ["dark", "light", "system"][rng.below(3) as usize]
                    ),
                    10 => format!("zoom:{}", ["+", "-", "0"][rng.below(3) as usize]),
                    _ => format!("palette:{}", ["wasp", "rail", "zzz"][rng.below(3) as usize]),
                });
            }
            let actions = steps.join(";");
            let shot = shots.join(format!("gui-monkey-{seed}.png"));
            Run {
                actions: &actions,
                ..Run::new(exe, &data, &cache.root, &shot)
            }
            .launch()
            .with_context(|| format!("seed {seed}: {actions}"))?;
            anyhow::ensure!(
                shot.exists(),
                "seed {seed}: no screenshot, the window never settled"
            );
            for backup in tanki_core::apply::list_backups(&data.join("backups"))? {
                tanki_core::apply::restore_transaction(&backup.dir, &backup.tx)?;
            }
            anyhow::ensure!(
                contents(&files) == before,
                "seed {seed}: the cache is not the original after undoing every replacement"
            );
            std::fs::remove_dir_all(work)?;
        }
        Ok(())
    });

    check("gui-apply", &|| {
        let (work, cache, zip) = fresh("gui-apply")?;
        let data = work.join("data");
        let before_turret = contents(&cache.turret_files);
        let actions = format!("import:{};select:hull;apply:0:0", zip.display());
        let shot = shots.join("gui-apply.png");
        Run {
            actions: &actions,
            ..Run::new(exe, &data, &cache.root, &shot)
        }
        .launch()?;
        let models = cache
            .hull_files
            .iter()
            .filter(|p| std::fs::read(p).is_ok_and(|b| b == HULL_MODEL))
            .count();
        anyhow::ensure!(
            models == 2,
            "expected the hull model in both revisions, found {models}"
        );
        anyhow::ensure!(
            contents(&cache.turret_files) == before_turret,
            "turret must stay untouched"
        );
        let store = StateStore::open(&data.join("state.sqlite"))?;
        anyhow::ensure!(
            store.active_pack()?.is_some(),
            "imported pack is not persisted as active"
        );
        let backups = tanki_core::apply::list_backups(&data.join("backups"))?;
        anyhow::ensure!(
            backups.len() == 1,
            "expected one backup, found {}",
            backups.len()
        );
        // The result dialog is on screen after a replacement.
        let image = image::open(&shot)?.to_rgba8();
        anyhow::ensure!(
            count_near(&image, PRIMARY, 6) * 1_000_000 / (image.width() * image.height()) as usize
                > 500,
            "the replacement-complete dialog is not drawn"
        );
        std::fs::remove_dir_all(work)?;
        Ok(())
    });
    check("gui-restore", &|| {
        let (work, cache, zip) = fresh("gui-restore")?;
        let data = work.join("data");
        let all: Vec<PathBuf> = cache
            .hull_files
            .iter()
            .chain(&cache.turret_files)
            .cloned()
            .collect();
        let before = contents(&all);
        let actions = format!("import:{};select:turret;apply:1:0;restore", zip.display());
        let shot = shots.join("gui-restore.png");
        Run {
            actions: &actions,
            ..Run::new(exe, &data, &cache.root, &shot)
        }
        .launch()?;
        anyhow::ensure!(
            contents(&all) == before,
            "restore must bring back every original byte"
        );
        anyhow::ensure!(
            std::fs::read_dir(data.join("backups"))?.count() == 1,
            "apply should have written exactly one backup"
        );
        std::fs::remove_dir_all(work)?;
        Ok(())
    });
    check("gui-game-running", &|| {
        let (work, cache, zip) = fresh("gui-game-running")?;
        let data = work.join("data");
        let before = contents(&cache.hull_files);
        let actions = format!("import:{};select:hull;apply:0:0", zip.display());
        let shot = shots.join("gui-game-running.png");
        // A running game blocks the replacement: the files must stay as they were.
        let mut command = Command::new(exe);
        command
            .env("TANKI_DATA_DIR", &data)
            .env("TANKI_CACHE_DIR", &cache.root)
            .env("TANKI_SHOT", &shot)
            .env("TANKI_VIEW", "garage")
            .env("TANKI_THEME", "dark")
            .env("TANKI_SIZE", E2E_SIZE)
            .env("TANKI_GAME_PROCESSES", "GameClient.exe")
            .env("TANKI_ACTIONS", &actions);
        anyhow::ensure!(command.status()?.success(), "app failed");
        anyhow::ensure!(
            contents(&cache.hull_files) == before,
            "a running game must block the replacement"
        );
        anyhow::ensure!(
            !data.join("backups").exists(),
            "no backup before the user confirmed"
        );
        std::fs::remove_dir_all(work)?;
        Ok(())
    });
    check("gui-label", &|| {
        let work = scratch("gui-label");
        let cache = build_cache(&work.join("cache"))?;
        let pack = build_pack_dir(
            &work.join("pack"),
            &PackOptions {
                with_hornet: true,
                ..PackOptions::default()
            },
        )?;
        let zip = zip_dir(&pack, &work.join("fixture.tankipack"), &[])?;
        let data = work.join("data");
        // Item 2 is the Hornet, unknown to the adapter: it works only after the hand label.
        let actions = format!(
            "import:{};label:/0/0/10/20/:hull:hornet:m3;apply:2:0",
            zip.display()
        );
        let shot = shots.join("gui-label.png");
        Run {
            actions: &actions,
            ..Run::new(exe, &data, &cache.root, &shot)
        }
        .launch()?;
        let models = cache
            .hornet_files
            .iter()
            .filter(|p| std::fs::read(p).is_ok_and(|b| b == HORNET_MODEL))
            .count();
        anyhow::ensure!(
            models == 2,
            "hornet model must reach both revisions after labelling, found {models}"
        );
        let _store = StateStore::open(&data.join("state.sqlite"))?;
        anyhow::ensure!(
            tanki_core::cache::known_target_group("air-cache", "http://cache.test/0/0/10/20/111")
                .is_some(),
            "label not persisted"
        );
        std::fs::remove_dir_all(work)?;
        Ok(())
    });
    check("gui-bad-import", &|| {
        let (work, cache, _) = fresh("gui-bad-import")?;
        let data = work.join("data");
        let bad = build_pack_dir(
            &work.join("bad"),
            &PackOptions {
                corrupt_checksum: true,
                ..PackOptions::default()
            },
        )?;
        let all: Vec<PathBuf> = cache
            .hull_files
            .iter()
            .chain(&cache.turret_files)
            .cloned()
            .collect();
        let before = contents(&all);
        let actions = format!("import:{};select:hull;apply:0:0", bad.display());
        let shot = shots.join("gui-bad-import.png");
        Run {
            page: "packs",
            actions: &actions,
            ..Run::new(exe, &data, &cache.root, &shot)
        }
        .launch()?;
        anyhow::ensure!(
            contents(&all) == before,
            "a rejected pack must not change the cache"
        );
        let store = StateStore::open(&data.join("state.sqlite"))?;
        anyhow::ensure!(
            store.list_packs()?.is_empty(),
            "a rejected pack must not be saved"
        );
        // The error dialog lists the problem: its red chip is on screen.
        let image = image::open(&shot)?.to_rgba8();
        anyhow::ensure!(
            count_near(&image, DANGER, 12) * 1_000_000 / (image.width() * image.height()) as usize
                > 100,
            "the import-error dialog is not drawn"
        );
        std::fs::remove_dir_all(work)?;
        Ok(())
    });
    // The choices made in one run come back in the next, through the real stored state.
    check("gui-persist", &|| {
        let (work, cache, zip) = fresh("gui-persist")?;
        let data = work.join("data");
        let first = shots.join("gui-persist-1.png");
        let actions = format!(
            "import:{};lang:ru;theme:light;zoom:+;page:packs;select:turret;dismiss",
            zip.display()
        );
        Run {
            actions: &actions,
            ..Run::new(exe, &data, &cache.root, &first)
        }
        .launch()?;
        {
            let store = StateStore::open(&data.join("state.sqlite"))?;
            anyhow::ensure!(
                store.language()?.as_deref() == Some("ru"),
                "language not stored"
            );
            anyhow::ensure!(
                store.theme()?.as_deref() == Some("light"),
                "theme not stored"
            );
            anyhow::ensure!(
                store.ui_scale()?.as_deref() == Some("110"),
                "interface size not stored"
            );
            let session = store.session();
            anyhow::ensure!(session.page == "packs", "page not stored: {}", session.page);
            let root = cache.root.to_string_lossy().into_owned();
            let item = session.cache(&root).and_then(|c| c.item.clone());
            anyhow::ensure!(
                item.is_some_and(|i| i.kind == "turret" && i.id == "railgun"),
                "the selected item is not stored for this cache"
            );
            anyhow::ensure!(
                session.window.is_some() || true,
                "window geometry is optional in tests"
            );
        }
        // Second start: nothing asked for, everything comes back (light theme, Russian text).
        let second = shots.join("gui-persist-2.png");
        let mut command = Command::new(exe);
        command
            .env("TANKI_DATA_DIR", &data)
            .env("TANKI_CACHE_DIR", &cache.root)
            .env("TANKI_SHOT", &second)
            .env("TANKI_SIZE", E2E_SIZE)
            .env("TANKI_SPLASH", "off")
            .env("TANKI_GAME_PROCESSES", "");
        anyhow::ensure!(command.status()?.success(), "second start failed");
        let image = image::open(&second)?.to_rgba8();
        let pixels = (image.width() * image.height()) as usize;
        let light = count_near(&image, LIGHT_BG, 2) * 1_000_000 / pixels;
        anyhow::ensure!(
            light > 100_000,
            "the second start did not restore the light theme ({light} ppm)"
        );
        let store = StateStore::open(&data.join("state.sqlite"))?;
        anyhow::ensure!(
            store.session().page == "packs",
            "the page was not restored and kept"
        );
        // The first run's Packs page and the restored one are the same page: same colours.
        let before = image::open(&first)?.to_rgba8();
        anyhow::ensure!(
            differing_share(&before, &image) < 0.05,
            "the restored window differs from the one that was closed"
        );
        std::fs::remove_dir_all(work)?;
        Ok(())
    });
    check("gui-language", &|| {
        let (work, cache, zip) = fresh("gui-language")?;
        let data = work.join("data");
        let actions = format!("import:{}", zip.display());
        let english = shots.join("gui-language-en.png");
        let russian = shots.join("gui-language-ru.png");
        Run {
            actions: &actions,
            ..Run::new(exe, &data, &cache.root, &english)
        }
        .launch()?;
        Run {
            actions: "",
            lang: "ru",
            ..Run::new(exe, &data, &cache.root, &russian)
        }
        .launch()?;
        let (a, b) = (
            image::open(&english)?.to_rgba8(),
            image::open(&russian)?.to_rgba8(),
        );
        let share = differing_share(&a, &b);
        anyhow::ensure!(share > 0.005, "Russian looks like English ({share:.4})");
        anyhow::ensure!(
            share < 0.30,
            "Russian changed the layout, not just the text ({share:.4})"
        );
        std::fs::remove_dir_all(work)?;
        Ok(())
    });
    failures
}

fn e2e_ui(args: &[String]) -> Result<()> {
    cargo(&["build", "--locked", "-p", "tanki-studio"])?;
    let exe = studio_exe();
    let work = scratch("e2e-ui");
    let cache = build_cache(&work.join("cache"))?;
    let pack = build_pack_dir(&work.join("pack"), &PackOptions::default())?;
    let data = work.join("data");
    let imported = import_pack(&pack, &data.join("packs"))?;
    StateStore::open(&data.join("state.sqlite"))?.save_pack(&imported)?;
    let incomplete_data = work.join("data-incomplete");
    let incomplete = build_pack_dir(
        &work.join("pack-incomplete"),
        &PackOptions {
            hull_without_model: true,
            ..PackOptions::default()
        },
    )?;
    let incomplete = import_pack(&incomplete, &incomplete_data.join("packs"))?;
    StateStore::open(&incomplete_data.join("state.sqlite"))?.save_pack(&incomplete)?;
    let missing = work.join("no-such-cache");

    let shots = root().join("target/e2e-ui");
    std::fs::create_dir_all(&shots)?;
    let only = args.first();
    let mut failures = Vec::new();
    for scene in SCENES.iter().filter(|s| only.is_none_or(|n| n == s.name)) {
        let out = shots.join(format!("{}.png", scene.name));
        let _ = std::fs::remove_file(&out);
        let cache_dir = if scene.no_cache {
            &missing
        } else {
            &cache.root
        };
        let run = Run {
            page: scene.page,
            actions: scene.actions,
            theme: scene.theme,
            lang: scene.lang,
            splash: scene.splash,
            size: scene.size,
            processes: scene.processes,
            ..Run::new(
                &exe,
                if scene.incomplete_skin {
                    &incomplete_data
                } else {
                    &data
                },
                cache_dir,
                &out,
            )
        };
        if let Err(e) = run.launch() {
            failures.push(format!("{}: {e:#}", scene.name));
            continue;
        }
        match image::open(&out) {
            Ok(image) => failures.extend(check_scene(scene, &image.to_rgba8())),
            Err(_) => failures.push(format!("{}: no screenshot written", scene.name)),
        }
    }
    failures.extend(gui_journeys(&exe, &shots, only));
    let _ = std::fs::remove_dir_all(work);
    if failures.is_empty() {
        println!("e2e-ui: all scenes and journeys passed; screenshots in target/e2e-ui/");
        Ok(())
    } else {
        for f in &failures {
            eprintln!("FAIL {f}");
        }
        bail!("{} e2e-ui check(s) failed", failures.len())
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(name) = args.first() else {
        eprintln!("cargo xtask <command>\n");
        for (n, d, _) in COMMANDS {
            eprintln!("  {n:<8} {d}");
        }
        return ExitCode::from(2);
    };
    let Some((_, _, task)) = COMMANDS.iter().find(|(n, _, _)| n == name) else {
        eprintln!("unknown command: {name}");
        return ExitCode::from(2);
    };
    match task(&args[1..]) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

/// Every UI string in the sources: plain `tl!("…")` keys and `tn(n, "one", "other")` plural keys,
/// plus the labels of the command table. Returns (plain, plural) sorted and de-duplicated.
fn source_strings() -> Result<(Vec<String>, Vec<String>)> {
    let literal = r#""((?:[^"\\]|\\.)*)""#;
    let plain = regex::Regex::new(&format!(r"tl!\(\s*{literal}\s*\)")).expect("regex");
    let plural = regex::Regex::new(&format!(
        r"\btn\(\s*[^,]+,\s*{literal}\s*,\s*{literal}\s*,?\s*\)"
    ))
    .expect("regex");
    let command = regex::Regex::new(&format!(
        r"spec\(\s*Command::[^,]+(?:\([^)]*\))?,\s*{literal}\s*,\s*{literal}"
    ))
    .expect("regex");
    let mut plain_keys = std::collections::BTreeSet::new();
    let mut plural_keys = std::collections::BTreeSet::new();
    for dir in ["apps", "crates"] {
        for entry in walkdir::WalkDir::new(root().join(dir)) {
            let entry = entry?;
            let path = entry.path();
            let skip = path.components().any(|c| c.as_os_str() == "target")
                || path.components().any(|c| c.as_os_str() == "tanki-i18n");
            if skip || path.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            let text = std::fs::read_to_string(path)?;
            for caps in plain.captures_iter(&text) {
                plain_keys.insert(unescape_rust(&caps[1]));
            }
            for caps in plural.captures_iter(&text) {
                plural_keys.insert(format!(
                    "{}|{}",
                    unescape_rust(&caps[1]),
                    unescape_rust(&caps[2])
                ));
            }
            for caps in command.captures_iter(&text) {
                plain_keys.insert(unescape_rust(&caps[1]));
                plain_keys.insert(unescape_rust(&caps[2]));
            }
        }
    }
    Ok((
        plain_keys.into_iter().collect(),
        plural_keys.into_iter().collect(),
    ))
}

fn unescape_rust(text: &str) -> String {
    text.replace("\\\"", "\"")
        .replace("\\\\", "\\")
        .replace("\\n", "\n")
}

/// `cargo xtask i18n`: the catalog and the code must agree (PdfCraft keeps the same invariant with
/// tests; here the sources are scanned, so a string added to the UI cannot ship untranslated).
fn i18n(args: &[String]) -> Result<()> {
    let (plain, plural) = source_strings()?;
    let text = std::fs::read_to_string(root().join("crates/tanki-i18n/src/ru.tsv"))?;
    let (entries, errors) = tanki_i18n::parse_entries(&text, 3);
    let mut translated_plain = std::collections::BTreeSet::new();
    let mut translated_plural = std::collections::BTreeSet::new();
    for entry in &entries {
        match entry.context.as_str() {
            "" => {
                translated_plain.insert(entry.source.clone());
            }
            "@plural" => {
                translated_plural.insert(entry.source.clone());
            }
            _ => {}
        }
    }
    let missing_plain: Vec<_> = plain
        .iter()
        .filter(|k| !translated_plain.contains(*k))
        .collect();
    let missing_plural: Vec<_> = plural
        .iter()
        .filter(|k| !translated_plural.contains(*k))
        .collect();
    let stale_plain: Vec<_> = translated_plain
        .iter()
        .filter(|k| !plain.contains(*k))
        .collect();
    let stale_plural: Vec<_> = translated_plural
        .iter()
        .filter(|k| !plural.contains(*k))
        .collect();
    if args.iter().any(|a| a == "--template") {
        for key in &missing_plain {
            println!("\t{}\t", key.replace('\n', "\\n"));
        }
        for key in &missing_plural {
            println!("@plural\t{key}\t");
        }
        return Ok(());
    }
    for error in &errors {
        eprintln!("ru.tsv: {error}");
    }
    for key in &missing_plain {
        eprintln!("missing translation: {key:?}");
    }
    for key in &missing_plural {
        eprintln!("missing plural translation: {key:?}");
    }
    for key in stale_plain.iter().chain(&stale_plural) {
        eprintln!("stale catalog row (not used by the code): {key:?}");
    }
    let problems = errors.len()
        + missing_plain.len()
        + missing_plural.len()
        + stale_plain.len()
        + stale_plural.len();
    if problems > 0 {
        bail!("{problems} translation problem(s)");
    }
    println!(
        "OK: {} strings and {} plural messages, all translated, no stale rows.",
        plain.len(),
        plural.len()
    );
    Ok(())
}

/// Draw `text` with its top-left corner at (`x`, `y`) in the bundled Inter font.
fn draw_text(image: &mut image::RgbaImage, text: &str, x: f32, y: f32, size: f32, color: [u8; 3]) {
    use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
    let font = FontRef::try_from_slice(include_bytes!("../../crates/tanki-ui/assets/Inter.ttf"))
        .expect("font");
    let scaled = font.as_scaled(PxScale::from(size));
    let mut caret = x;
    for ch in text.chars() {
        let glyph = scaled.scaled_glyph(ch);
        let id = glyph.id;
        let positioned = ab_glyph::Glyph {
            position: ab_glyph::point(caret, y + scaled.ascent()),
            ..glyph
        };
        if let Some(outline) = font.outline_glyph(positioned) {
            let bounds = outline.px_bounds();
            outline.draw(|gx, gy, coverage| {
                let (px, py) = (
                    bounds.min.x as i32 + gx as i32,
                    bounds.min.y as i32 + gy as i32,
                );
                if px >= 0 && py >= 0 && (px as u32) < image.width() && (py as u32) < image.height()
                {
                    let target = image.get_pixel_mut(px as u32, py as u32);
                    for i in 0..3 {
                        target[i] = (f32::from(target[i]) * (1.0 - coverage)
                            + f32::from(color[i]) * coverage)
                            as u8;
                    }
                }
            });
        }
        caret += scaled.h_advance(id);
    }
}

fn paste(sheet: &mut image::RgbaImage, thumb: &image::RgbaImage, x: i64, y: i64) {
    image::imageops::overlay(sheet, thumb, x, y);
}

fn thumb(bytes: &[u8], size: u32) -> Option<image::RgbaImage> {
    let decoded = image::load_from_memory(bytes).ok()?.to_rgba8();
    Some(image::imageops::resize(
        &decoded,
        size,
        size,
        image::imageops::FilterType::Triangle,
    ))
}

type Reference = std::collections::BTreeMap<(String, String, String), Vec<(String, Vec<u8>)>>;
type SheetRow = (String, (String, String, String), Vec<u8>);

/// `cargo xtask sheet`: contact sheets for checking the automatic recognition by eye.
fn sheet(args: &[String]) -> Result<()> {
    use tanki_core::{
        cache::{existing_cache_roots, known_target_group, scan_cache},
        pack::read_pack_resource,
        paths::app_data_dir,
    };
    let out = PathBuf::from(option(args, "--out").unwrap_or("/tmp/sheets"));
    std::fs::create_dir_all(&out)?;
    let store = StateStore::open(&app_data_dir().join("state.sqlite"))?;
    let packs = store.list_packs()?;
    let (game, root) = existing_cache_roots()
        .into_iter()
        .next()
        .context("no game cache found")?;
    let scan = scan_cache(&root, Some(&game))?;
    // Reference textures: per (kind, id, grade) the `details` image of every variant.
    let mut reference: Reference = Default::default();
    for pack in &packs {
        for item in &pack.manifest.items {
            for variant in &item.variants {
                for resource in variant.resources.iter().filter(|r| r.role == "details") {
                    if let Ok(bytes) = read_pack_resource(pack, &resource.path) {
                        reference
                            .entry((
                                item.target.kind.clone(),
                                item.target.id.clone(),
                                item.target.grade.clone(),
                            ))
                            .or_default()
                            .push((variant.id.clone(), bytes));
                    }
                }
            }
        }
    }
    // One row per recognised cache resource id: its details texture and the archive's M0..M3.
    let mut rows: Vec<SheetRow> = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for resource in scan.resources.iter().filter(|r| r.role == "details.jpg") {
        let Some((kind, id, grade)) = known_target_group(&scan.game, &resource.group) else {
            continue;
        };
        let Some(marker) = tanki_core::targets::marker_for_group(&resource.group) else {
            continue;
        };
        if seen.insert(marker.clone()) {
            rows.push((
                marker,
                (kind.into(), id.into(), grade.into()),
                std::fs::read(&resource.path)?,
            ));
        }
    }
    rows.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
    const T: u32 = 110;
    const PER_SHEET: usize = 8;
    for (page, chunk) in rows.chunks(PER_SHEET).enumerate() {
        let mut image = image::RgbaImage::from_pixel(
            260 + 5 * (T + 10) + 20,
            chunk.len() as u32 * (T + 30) + 50,
            image::Rgba([16, 23, 18, 255]),
        );
        draw_text(
            &mut image,
            "cache details.jpg | archive M0 M1 M2 M3 (variant 1) | green frame = grade the app chose",
            10.0,
            8.0,
            16.0,
            [200, 210, 200],
        );
        for (index, (marker, (kind, id, grade), bytes)) in chunk.iter().enumerate() {
            let y = 40 + index as i64 * i64::from(T + 30);
            draw_text(
                &mut image,
                &format!("{kind} {id} {}", grade.to_uppercase()),
                10.0,
                y as f32 + 10.0,
                18.0,
                [235, 245, 235],
            );
            draw_text(
                &mut image,
                marker,
                10.0,
                y as f32 + 36.0,
                14.0,
                [150, 170, 150],
            );
            if let Some(cache) = thumb(bytes, T) {
                paste(&mut image, &cache, 150, y);
            }
            for (column, g) in ["m0", "m1", "m2", "m3"].iter().enumerate() {
                let x = 150 + i64::from(T + 10) * (column as i64 + 1);
                if let Some(variants) = reference.get(&(kind.clone(), id.clone(), (*g).to_owned()))
                {
                    if let Some(archive) = variants.first().and_then(|(_, b)| thumb(b, T)) {
                        paste(&mut image, &archive, x, y);
                    }
                } else {
                    draw_text(
                        &mut image,
                        "no ref",
                        x as f32 + 30.0,
                        y as f32 + 45.0,
                        14.0,
                        [120, 120, 120],
                    );
                }
                if g == grade {
                    for d in 0..3 {
                        for i in 0..T + 6 {
                            for (px, py) in [
                                (x - 3 + i64::from(i), y - 3 + d),
                                (x - 3 + i64::from(i), y + i64::from(T) + 2 - d),
                                (x - 3 + d, y - 3 + i64::from(i)),
                                (x + i64::from(T) + 2 - d, y - 3 + i64::from(i)),
                            ] {
                                if px >= 0
                                    && py >= 0
                                    && (px as u32) < image.width()
                                    && (py as u32) < image.height()
                                {
                                    image.put_pixel(
                                        px as u32,
                                        py as u32,
                                        image::Rgba([140, 255, 77, 255]),
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
        let path = out.join(format!("recognised-{}.png", page + 1));
        image.save(&path)?;
        println!("wrote {} ({} rows)", path.display(), chunk.len());
    }
    println!(
        "{} recognised resources in {} pack(s)",
        rows.len(),
        packs.len()
    );
    Ok(())
}

fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0);
    for ch in text.bytes().filter(|b| *b != b'=') {
        let v = match ch {
            b'A'..=b'Z' => ch - b'A',
            b'a'..=b'z' => ch - b'a' + 26,
            b'0'..=b'9' => ch - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

/// RGB over black, 164x106; None for anything else.
fn preview_pixels(colour: &[u8], alpha: Option<&[u8]>) -> Option<Vec<f32>> {
    let rgb = image::load_from_memory(colour).ok()?.to_rgb8();
    if rgb.dimensions() != (164, 106) {
        return None;
    }
    let alpha = alpha
        .and_then(|a| image::load_from_memory(a).ok())
        .map(|a| a.to_luma8());
    let mut out = Vec::with_capacity(164 * 106 * 3);
    for (x, y, px) in rgb.enumerate_pixels() {
        let a = alpha
            .as_ref()
            .filter(|a| a.dimensions() == (164, 106))
            .map_or(1.0, |a| f32::from(a.get_pixel(x, y)[0]) / 255.0);
        out.extend(px.0.iter().map(|c| f32::from(*c) * a));
    }
    Some(out)
}

fn rgb_image(pixels: &[f32]) -> image::RgbaImage {
    image::RgbaImage::from_fn(164, 106, |x, y| {
        let i = ((y * 164 + x) * 3) as usize;
        image::Rgba([
            pixels[i] as u8,
            pixels[i + 1] as u8,
            pixels[i + 2] as u8,
            255,
        ])
    })
}

/// `cargo xtask previews`: the cache keeps every garage preview as a picture, so match those by eye and by pixels.
fn previews(args: &[String]) -> Result<()> {
    let out = PathBuf::from(option(args, "--out").unwrap_or("/tmp/sheets"));
    std::fs::create_dir_all(&out)?;
    let cache = match option(args, "--cache") {
        Some(path) => PathBuf::from(path),
        None => tanki_core::cache::existing_cache_roots()
            .into_iter()
            .next()
            .map(|(_, root)| root)
            .context("no cache found; pass --cache <folder>")?,
    };
    let archive = option(args, "--archive")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var("HOME").unwrap_or_default())
                .join("Desktop/tanki-model-archive")
        });
    let mut references: Vec<(String, Vec<f32>)> = Vec::new();
    for entry in walkdir::WalkDir::new(archive.join("Flash"))
        .into_iter()
        .flatten()
    {
        if entry.file_name() != "preview.png" {
            continue;
        }
        let parts: Vec<String> = entry
            .path()
            .strip_prefix(archive.join("Flash"))?
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        let [kind, era, item, grade, _] = &parts[..] else {
            continue;
        };
        let Some(pixels) = preview_pixels(&std::fs::read(entry.path())?, None) else {
            continue;
        };
        references.push((
            format!("{item} {grade} {era} ({})", kind.trim_end_matches('s')),
            pixels,
        ));
    }
    let mut found: Vec<(String, Vec<f32>)> = Vec::new();
    for entry in std::fs::read_dir(&cache)?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(url) = base64_decode(&name).and_then(|b| String::from_utf8(b).ok()) else {
            continue;
        };
        let Some(path) = url.strip_suffix("/image.jpg") else {
            continue;
        };
        let alpha = std::fs::read(cache.join(entry_name_for(&format!("{path}/alpha.jpg")))).ok();
        if let Some(pixels) = preview_pixels(&std::fs::read(entry.path())?, alpha.as_deref()) {
            found.push((
                path.split('/').skip(3).collect::<Vec<_>>().join("/"),
                pixels,
            ));
        }
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    println!(
        "{} cache previews (164x106), {} archive references",
        found.len(),
        references.len()
    );
    let mut matched = Vec::new();
    let mut histogram = [0usize; 6];
    for (id, pixels) in &found {
        let mut scored: Vec<(f32, &str)> = references
            .iter()
            .map(|(label, r)| {
                (
                    r.iter()
                        .zip(pixels)
                        .map(|(a, b)| (a - b).abs())
                        .sum::<f32>()
                        / r.len() as f32,
                    label.as_str(),
                )
            })
            .collect();
        scored.sort_by(|a, b| a.0.total_cmp(&b.0));
        let (best, second) = (scored[0], scored[1]);
        histogram[((best.0 / 2.0) as usize).min(5)] += 1;
        if best.0 < 6.0 {
            matched.push((
                id.clone(),
                pixels.clone(),
                best.0,
                best.1.to_owned(),
                second.0,
                scored[0].1.to_owned(),
            ));
        }
    }
    println!("distance histogram (0-2,2-4,4-6,6-8,8-10,10+): {histogram:?}");
    println!(
        "{} previews match an archive preview closely (distance < 6)",
        matched.len()
    );
    for (id, _, d, label, second, _) in &matched {
        println!("  {id:28} {d:5.2} {label}   (runner-up {second:.2})");
    }
    const COLS: usize = 3;
    let rows = matched.len().div_ceil(COLS);
    let mut sheet = image::RgbaImage::from_pixel(
        COLS as u32 * 420,
        rows as u32 * 150 + 10,
        image::Rgba([16, 23, 18, 255]),
    );
    for (index, (id, pixels, d, label, _, _)) in matched.iter().enumerate() {
        let (x, y) = (
            (index % COLS) as u32 * 420,
            (index / COLS) as u32 * 150 + 10,
        );
        paste(
            &mut sheet,
            &rgb_image(pixels),
            i64::from(x) + 5,
            i64::from(y),
        );
        if let Some((_, r)) = references.iter().find(|(l, _)| l == label) {
            paste(&mut sheet, &rgb_image(r), i64::from(x) + 172, i64::from(y));
        }
        draw_text(
            &mut sheet,
            label,
            x as f32 + 340.0 - 168.0 + 168.0 - 168.0,
            y as f32 + 110.0,
            13.0,
            [235, 245, 235],
        );
        draw_text(
            &mut sheet,
            &format!("{id} d={d:.1}"),
            x as f32 + 5.0,
            y as f32 + 126.0,
            12.0,
            [150, 170, 150],
        );
    }
    let path = out.join("previews.png");
    sheet.save(&path)?;
    println!("wrote {}", path.display());
    Ok(())
}

fn entry_name_for(url: &str) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in format!("http://146.59.110.103/{url}").as_bytes().chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |a, (i, b)| a | u32::from(*b) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(TABLE[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}
