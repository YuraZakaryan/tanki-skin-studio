// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Windows only: embed the app icon and the version details into tanki-studio.exe, so Explorer,
//! the taskbar, the Start menu shortcut and Installed apps show them. A build
//! without the resource compiler only warns; the release sets TANKI_REQUIRE_WINRES so it fails.

const ICON: &str = "../../assets/app-icon/tanki-skin-studio.ico";

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={ICON}");
    println!("cargo:rerun-if-env-changed=TANKI_REQUIRE_WINRES");
    // A build script runs on the host, so the target comes from Cargo, not from cfg!.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon(ICON)
        .set("ProductName", "Tanki Skin Studio")
        .set("FileDescription", "Tanki Skin Studio")
        .set("InternalName", "tanki-studio")
        .set("OriginalFilename", "tanki-studio.exe")
        .set(
            "LegalCopyright",
            "Copyright (C) 2026 Tanki Skin Studio contributors. GPL-3.0-or-later.",
        );
    if let Err(error) = res.compile() {
        if std::env::var_os("TANKI_REQUIRE_WINRES").is_some() {
            panic!("embedding the Windows icon and version: {error}");
        }
        println!("cargo:warning=tanki-studio.exe built without its icon: {error}");
    }
}
