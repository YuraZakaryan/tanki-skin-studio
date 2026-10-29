## Downloads

| File | For |
|---|---|
| `tanki-skin-studio-<version>-macos-universal.dmg` | macOS 11 or later, Apple silicon and Intel |
| `tanki-skin-studio-<version>-windows-x64.msi` | Windows 10 or 11 on most PCs: the installer |
| `tanki-skin-studio-<version>-windows-x64-portable.zip` | the same without installing |
| `tanki-skin-studio-<version>-windows-arm64.msi` | Windows on ARM (Snapdragon and other ARM PCs) |
| `tanki-skin-studio-<version>-windows-arm64-portable.zip` | the same without installing |
| `SHA256SUMS.txt` | checksums of the files above |

**macOS:** open the `.dmg` and drag Tanki Skin Studio to Applications. The app is not notarized
yet: the first time, right-click it and choose **Open**.

**Windows:** run the `.msi`. It installs for your account only, without administrator rights,
adds Tanki Skin Studio to the Start menu and can be removed in **Settings → Apps**; a newer
`.msi` updates it in place. Or unzip the `-portable.zip` anywhere and run `tanki-studio.exe`.
The files are not signed yet: if SmartScreen asks, choose **More info**, then **Run anyway**.

## Licence and source

Tanki Skin Studio is free software under GPL-3.0-or-later and comes with no warranty. The source of
this exact version is attached below (**Source code**) and tagged in the repository.

## Verify what you downloaded

Every file is built by GitHub Actions from this repository, with a signed build-provenance
attestation.

```sh
sha256sum -c SHA256SUMS.txt
gh attestation verify <file> -R YuraZakaryan/tanki-skin-studio
```

## Changes

See [CHANGELOG.md](https://github.com/YuraZakaryan/tanki-skin-studio/blob/main/CHANGELOG.md).
