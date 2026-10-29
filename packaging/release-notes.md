## Downloads

| File | For |
|---|---|
| `tanki-skin-studio-<version>-macos-universal.dmg` | macOS 11 or later, Apple silicon and Intel |
| `tanki-skin-studio-<version>-windows-x64.zip` | Windows 10 or 11, 64-bit |
| `SHA256SUMS.txt` | checksums of the files above |

**macOS:** open the `.dmg` and drag Tanki Skin Studio to Applications. The app is not notarized
yet: the first time, right-click it and choose **Open**.

**Windows:** unzip anywhere and run `tanki-studio.exe`. The files are not signed yet: if
SmartScreen asks, choose **More info**, then **Run anyway**.

## Verify what you downloaded

Every file is built by GitHub Actions from this repository, with a signed build-provenance
attestation.

```sh
sha256sum -c SHA256SUMS.txt
gh attestation verify <file> -R YuraZakaryan/tanki-skin-studio
```

## Changes

See [CHANGELOG.md](https://github.com/YuraZakaryan/tanki-skin-studio/blob/main/CHANGELOG.md).
