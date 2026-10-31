## Downloads

| File | For |
|---|---|
| `tanki-skin-studio-<version>-macos-universal.dmg` | macOS 11 or later, Apple silicon and Intel |
| `tanki-skin-studio-<version>-windows-x64.msi` | Windows 10 or 11 on most PCs: the installer |
| `tanki-skin-studio-<version>-windows-x64-portable.zip` | the same without installing |
| `tanki-skin-studio-<version>-windows-arm64.msi` | Windows on ARM (Snapdragon and other ARM PCs) |
| `tanki-skin-studio-<version>-windows-arm64-portable.zip` | the same without installing |
| `SHA256SUMS.txt` | checksums of the files above |

**macOS:** open the `.dmg` and drag Tanki Skin Studio to Applications. If macOS says it cannot
check the app for malicious software (a build that is not notarized), open **System Settings →
Privacy & Security**, scroll to Security and choose **Open Anyway** next to Tanki Skin Studio.

**Windows:** run the `.msi`. It installs for your account only, without administrator rights,
adds Tanki Skin Studio to the Start menu and, unless you untick it, to the desktop, and can be
removed in **Settings → Apps**; a newer `.msi` updates it in place. Or unzip the `-portable.zip`
anywhere and run `tanki-studio.exe`. If SmartScreen says it protected your PC (an unsigned build,
or a new certificate it does not know yet), choose **More info**, then **Run anyway**.

**Updates:** from version 0.3.0 on, the app offers each new release when it starts, with what is
new in it, and **Update and restart** installs it (the portable copy opens this page instead).
Earlier versions do not update themselves: install this one over them once.

## Licence and source

Tanki Skin Studio is free software under GPL-3.0-or-later and comes with no warranty. The source of
this exact version is attached below (**Source code**) and tagged in the repository. The Rust crates
it is built with are pinned to exact versions in `Cargo.lock` and published on
[crates.io](https://crates.io); `cargo vendor` in the source fetches all of them. Their licences are
in `THIRD-PARTY-LICENSES.html`, inside every download.

## Verify what you downloaded

Every file is built by GitHub Actions from this repository, with a signed build-provenance
attestation.

```sh
sha256sum -c SHA256SUMS.txt
gh attestation verify <file> -R YuraZakaryan/tanki-skin-studio
```

## Changes

See [CHANGELOG.md](https://github.com/YuraZakaryan/tanki-skin-studio/blob/main/CHANGELOG.md).
