#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
# SPDX-License-Identifier: GPL-3.0-or-later
# Build the macOS release: Tanki Skin Studio.app (the app and the CLI) in a .dmg.
#
# Usage: packaging/macos/package.sh [--arch universal|aarch64|x86_64] [--version X.Y.Z]
# Output: dist/release/tanki-skin-studio-<version>-macos-<arch>.dmg
#
# `universal` builds for Apple silicon and Intel and joins them with `lipo`; it needs both Rust
# targets (rustup target add aarch64-apple-darwin x86_64-apple-darwin). The licences of the Rust
# crates come from cargo-about (cargo install cargo-about --locked --features cli).
#
# Signing: MACOS_SIGN_IDENTITY (a "Developer ID Application" identity in the keychain) signs with
# the hardened runtime and a secure timestamp. Without it the app is signed ad-hoc, which runs on
# the machine that built it and elsewhere only after right-click > Open; the script says so.
set -euo pipefail
cd "$(dirname "$0")/../.."

ARCH=universal
VERSION=""
while [ $# -gt 0 ]; do
  case "$1" in
    --arch) ARCH="$2"; shift 2 ;;
    --version) VERSION="$2"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
case "$ARCH" in
  universal) TARGETS=(aarch64-apple-darwin x86_64-apple-darwin) ;;
  aarch64) TARGETS=(aarch64-apple-darwin) ;;
  x86_64) TARGETS=(x86_64-apple-darwin) ;;
  *) echo "unknown arch: $ARCH" >&2; exit 2 ;;
esac
[ -n "$VERSION" ] || VERSION="$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)"
if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$ ]]; then
  echo "not a version: $VERSION" >&2
  exit 2
fi

warn() {
  if [ -n "${GITHUB_ACTIONS:-}" ]; then echo "::warning::$*"; else echo "warning: $*" >&2; fi
}

NAME="Tanki Skin Studio"
BINS=(tanki-studio tanki-cli)
WORK="target/macos-package"
DIST="dist/release"
APP="$WORK/stage/$NAME.app"
DMG="$DIST/tanki-skin-studio-$VERSION-macos-$ARCH.dmg"
rm -rf "$WORK"
mkdir -p "$WORK/bin" "$APP/Contents/MacOS" "$APP/Contents/Resources" "$DIST"

for target in "${TARGETS[@]}"; do
  cargo build --release --locked --target "$target" -p tanki-studio -p tanki-cli
done
for bin in "${BINS[@]}"; do
  inputs=()
  for target in "${TARGETS[@]}"; do inputs+=("target/$target/release/$bin"); done
  lipo -create -output "$WORK/bin/$bin" "${inputs[@]}"
  lipo -info "$WORK/bin/$bin"
  cp "$WORK/bin/$bin" "$APP/Contents/MacOS/$bin"
done

cp LICENSE "$APP/Contents/Resources/LICENSE.txt"
mkdir -p "$APP/Contents/Resources/licenses"
cp crates/tanki-ui/assets/OFL-*.txt crates/tanki-ui/assets/LICENSE-lucide.txt "$APP/Contents/Resources/licenses/"
# The notices of every Rust crate built in (their licences ask for them in binaries too).
cargo about generate --workspace --locked --fail -c packaging/about.toml \
  -o "$APP/Contents/Resources/licenses/THIRD-PARTY-LICENSES.html" packaging/about.hbs

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>$NAME</string>
<key>CFBundleDisplayName</key><string>$NAME</string>
<key>CFBundleIdentifier</key><string>dev.tanki.skinstudio</string>
<key>CFBundleVersion</key><string>$VERSION</string>
<key>CFBundleShortVersionString</key><string>$VERSION</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleExecutable</key><string>tanki-studio</string>
<key>LSMinimumSystemVersion</key><string>11.0</string>
<key>NSHighResolutionCapable</key><true/>
<key>NSHumanReadableCopyright</key><string>Copyright © 2026 Tanki Skin Studio contributors. GPL-3.0-or-later.</string>
</dict></plist>
PLIST

IDENTITY="${MACOS_SIGN_IDENTITY:-}"
if [ -n "$IDENTITY" ]; then
  codesign --force --options runtime --timestamp --sign "$IDENTITY" "$APP/Contents/MacOS/tanki-cli"
  codesign --force --options runtime --timestamp --sign "$IDENTITY" "$APP"
else
  warn "macOS: no MACOS_SIGN_IDENTITY, so the app is signed ad-hoc and not notarized"
  codesign --force --sign - "$APP/Contents/MacOS/tanki-cli"
  codesign --force --sign - "$APP"
fi
codesign --verify --strict --deep --verbose=2 "$APP"

# The disk image: the app beside a link to /Applications, so installing is one drag.
ln -s /Applications "$WORK/stage/Applications"
rm -f "$DMG"
# hdiutil fails now and then on CI runners with a busy resource; try a few times.
for attempt in 1 2 3 4 5; do
  if hdiutil create -volname "$NAME" -srcfolder "$WORK/stage" -fs HFS+ -format UDZO \
    -imagekey zlib-level=9 -ov "$DMG"; then
    break
  fi
  [ "$attempt" = 5 ] && { echo "hdiutil create failed 5 times" >&2; exit 1; }
  warn "hdiutil create failed (attempt $attempt of 5); retrying in $((attempt * 10)) s"
  sleep $((attempt * 10))
done
if [ -n "$IDENTITY" ]; then
  codesign --force --timestamp --sign "$IDENTITY" "$DMG"
  codesign --verify --strict --verbose=2 "$DMG"
fi
echo "Created $DMG"
