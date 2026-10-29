// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Tanki Skin Studio: replace skin textures in a local game cache, with backups and rollback.

use eframe::egui;
use tanki_core::session::MIN_WINDOW;
use tanki_studio::{
    app::{Startup, Studio},
    icon, logging,
};

/// Wayland/desktop application id (the `.desktop` file name when packaged for Linux).
const APP_ID: &str = "tanki-skin-studio";
/// The size of a first start, in points: the design's frame.
const DEFAULT_WINDOW: (f32, f32) = (1440.0, 900.0);

/// GPU settings, as in PdfCraft: a restored window on a high-DPI display can ask for a surface
/// larger than the 8192 pixels egui-wgpu requests (3440 points at 250% is 8600) and otherwise
/// panics before the app starts, so the adapter's real texture size is enabled; and on Windows,
/// Direct3D 12 presents through a DirectComposition visual, so text does not shimmer while the
/// window is resized.
fn configure_gpu(options: &mut eframe::NativeOptions) {
    let eframe::egui_wgpu::WgpuSetup::CreateNew(setup) = &mut options.wgpu_options.wgpu_setup
    else {
        return;
    };
    let device_descriptor = std::sync::Arc::clone(&setup.device_descriptor);
    setup.device_descriptor = std::sync::Arc::new(move |adapter| {
        let mut descriptor = device_descriptor(adapter);
        let supported = adapter.limits();
        descriptor.required_limits = descriptor.required_limits.or_worse_values_from(&supported);
        descriptor.required_limits.max_texture_dimension_2d = supported.max_texture_dimension_2d;
        descriptor
    });
    if cfg!(target_os = "windows") && std::env::var_os("WGPU_BACKEND").is_none() {
        setup.instance_descriptor.backends =
            eframe::wgpu::Backends::DX12 | eframe::wgpu::Backends::GL;
    }
    if cfg!(target_os = "windows") {
        setup
            .instance_descriptor
            .backend_options
            .dx12
            .presentation_system = eframe::wgpu::Dx12SwapchainKind::from_env()
            .unwrap_or(eframe::wgpu::Dx12SwapchainKind::DxgiFromVisual);
    }
}

fn main() -> eframe::Result<()> {
    // First, so the panic hook and every start-up warning are recorded (`logging`).
    let logger = logging::install();
    logging::install_panic_hook();
    let startup = Startup::from_disk().map_err(|e| {
        log::error!("could not open the data folder: {e:#}");
        eframe::Error::AppCreation(e.into())
    })?;
    // One window per data folder: a second one would write the same database and cache.
    let _instance = match tanki_studio::instance::claim(&startup.data) {
        Ok(Some(lock)) => Some(lock),
        Ok(None) => {
            tanki_i18n::set_current(tanki_i18n::Lang::from_pref(
                startup.store.get("language").ok().flatten().as_deref(),
            ));
            rfd::MessageDialog::new()
                .set_title("Tanki Skin Studio")
                .set_description(tanki_i18n::tl!(
                    "Tanki Skin Studio is already open. Switch to its window."
                ))
                .set_level(rfd::MessageLevel::Info)
                .show();
            return Ok(());
        }
        Err(error) => {
            log::warn!("could not lock the data folder ({error}); running anyway");
            None
        }
    };
    // Only the window that owns the data folder rotates its logs: a second launch would rename
    // the log the first one is still writing.
    if let Some(logger) = logger {
        match logger.attach_dir(&startup.data.join("logs")) {
            Ok(path) => log::info!(
                "Tanki Skin Studio {}, log file {}",
                env!("CARGO_PKG_VERSION"),
                path.display()
            ),
            Err(error) => log::warn!("no log file: {error}"),
        }
    }
    let session = startup.session.clone();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Tanki Skin Studio")
        .with_inner_size([DEFAULT_WINDOW.0, DEFAULT_WINDOW.1])
        .with_min_inner_size([MIN_WINDOW.0, MIN_WINDOW.1])
        .with_drag_and_drop(true)
        .with_app_id(APP_ID)
        .with_icon(icon::app_icon());
    // The screen is unknown before the window exists, so only plausibility is checked here;
    // the first frames repair a window that does not fit the monitor (see `track_window`).
    if std::env::var_os("TANKI_SHOT").is_none()
        && let Some(window) = session
            .window
            .and_then(|w| w.fit_to_screen((7680.0, 4320.0)))
    {
        viewport = viewport
            .with_position([window.x, window.y])
            .with_inner_size([window.width, window.height])
            .with_maximized(window.maximized);
    }
    // `TANKI_SIZE=1100x700` opens the window at that size, to check the layout at other sizes.
    if let Some((w, h)) = std::env::var("TANKI_SIZE").ok().and_then(|v| {
        let (w, h) = v.split_once('x')?;
        Some((w.parse::<f32>().ok()?, h.parse::<f32>().ok()?))
    }) {
        viewport = viewport.with_inner_size([w, h]);
    }
    let mut options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    // The fonts are bundled; reading every installed face into each UI pass would only cost time.
    options.system_font_fallback = false;
    configure_gpu(&mut options);
    eframe::run_native(
        "Tanki Skin Studio",
        options,
        Box::new(move |cc| {
            Studio::new(cc, startup)
                .map(|app| Box::new(app) as Box<dyn eframe::App>)
                .map_err(|e| e.into())
        }),
    )
}
