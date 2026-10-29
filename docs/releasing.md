# Releasing

Downloads are built by GitHub Actions ([release.yml](../.github/workflows/release.yml)), never on a
developer's machine, and every file carries a build-provenance attestation. The pattern follows
PdfCraft: a push to the `release` branch builds everything and drafts a GitHub Release, and a person
publishes it.

## What a release contains

| File | Built by |
|---|---|
| `tanki-skin-studio-<version>-macos-universal.dmg` | [packaging/macos/package.sh](../packaging/macos/package.sh): Apple silicon and Intel joined with `lipo`, the app and the CLI in one `.app` |
| `tanki-skin-studio-<version>-windows-x64.zip` | [packaging/windows/package.ps1](../packaging/windows/package.ps1): `tanki-studio.exe` and `tanki-cli.exe` with the C runtime linked in |
| `SHA256SUMS.txt` | the release job |

Release builds use no build cache, so nothing cached by another branch can reach a published file.

## Cutting a release

1. In a pull request, set the version in the workspace `Cargo.toml` (`[workspace.package] version`)
   and move the `Unreleased` notes in [CHANGELOG.md](../CHANGELOG.md) under that version. Merge it.
2. Point `release` at the merged `main`:

   ```sh
   git push origin main:release
   ```

3. The **Release** workflow builds both platforms and creates the draft release `v<version>` with the
   files, their checksums and the notes from [packaging/release-notes.md](../packaging/release-notes.md).
   Pushing `release` again replaces the files of the same draft.
4. Download the files from the draft, try them on a Mac and on Windows, then press **Publish
   release**. Publishing creates the tag. A published version is never rebuilt: the next release
   raises the version.

A manual run (**Actions → Release → Run workflow**) can name a test version such as `0.2.0-rc.1`;
it drafts a pre-release. A pull request that changes packaging builds both platforms without
drafting anything, and its files can be downloaded from the run.

## Verifying a download

```sh
sha256sum -c SHA256SUMS.txt
gh attestation verify tanki-skin-studio-0.1.0-macos-universal.dmg -R YuraZakaryan/tanki-skin-studio
```

## Signing (not set up yet)

Until certificates exist, the macOS app is signed ad-hoc and not notarized (first start: right-click,
**Open**), and the Windows files are unsigned (SmartScreen: **More info → Run anyway**). Both build
scripts print a warning when they ship unsigned.

- **macOS** needs an Apple Developer ID Application certificate. `package.sh` signs with the hardened
  runtime when `MACOS_SIGN_IDENTITY` names it; notarization (`notarytool submit --wait`,
  `stapler staple`) is the next step to add, with the certificate, the Apple ID, an app-specific
  password and the team id stored as secrets of a `release` environment.
- **Windows** needs a code-signing certificate (or Azure Trusted Signing); `package.ps1` will sign
  both executables with `signtool` before zipping.

Once signing is in place, the release job should fail on the `release` branch when a signing secret
is missing, so an unsigned build can never be published by accident.

## Protecting the `release` branch

Add a ruleset for `release` like the one for `main`: restrict creation, updates and deletion to
repository admins, and block force pushes. Only a maintainer can then start a release.
