//! Tanki Skin Studio as a library: the application state, its screens and the `eframe::App`. The
//! binary (`main.rs`) only opens the window; tests drive the same app in-process with
//! `egui_kittest`, the way PdfCraft tests its UI.

pub mod actions;
pub mod app;
pub mod commands;
pub mod dev;
pub mod errors;
pub mod icon;
pub mod instance;
pub mod logging;
pub mod model;
pub mod previews;
pub mod runtime;
pub mod views;
