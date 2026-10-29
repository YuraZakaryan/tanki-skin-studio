//! UI tests that drive the real app in-process with `egui_kittest`, as PdfCraft does: look a
//! widget up by its accessible label, click it, run a few frames, and check the app's state and the
//! files on disk. No window is opened, so these run on every machine in `cargo test`.

mod archive;
mod chaos;
mod common;
mod dialogs;
mod garage;
mod monkey;
mod navigation;
mod preferences;
mod recognise;
