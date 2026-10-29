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
# Signing and notarization, as in WordCraft and PdfCraft (CI fills these from secrets through
# import-cert.sh; see docs/releasing.md):
#   MACOS_SIGN_IDENTITY   "Developer ID Application" identity (name or SHA-1). Unset: ad-hoc
#                         signing, which runs elsewhere only after System Settings > Privacy &
#                         Security > Open Anyway; the script warns.
#   MACOS_KEYCHAIN        keychain holding the identity (optional)
#   APPLE_ID, APPLE_PASSWORD (an app-specific password), APPLE_TEAM_ID
#                         notarize the app and the disk image with Apple and staple the tickets
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
cp assets/app-icon/tanki-skin-studio.icns "$APP/Contents/Resources/AppIcon.icns"
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
<key>CFBundleIconFile</key><string>AppIcon</string>
<key>LSMinimumSystemVersion</key><string>11.0</string>
<key>NSHighResolutionCapable</key><true/>
<key>NSHumanReadableCopyright</key><string>Copyright © 2026 Tanki Skin Studio contributors. GPL-3.0-or-later.</string>
</dict></plist>
PLIST

IDENTITY="${MACOS_SIGN_IDENTITY:-}"
NOTARIZE=0
if [ -z "$IDENTITY" ]; then
  warn "macOS: no MACOS_SIGN_IDENTITY, so the app is signed ad-hoc and not notarized"
elif [ -n "${APPLE_ID:-}" ] && [ -n "${APPLE_PASSWORD:-}" ] && [ -n "${APPLE_TEAM_ID:-}" ]; then
  NOTARIZE=1
else
  warn "macOS: APPLE_ID / APPLE_PASSWORD / APPLE_TEAM_ID incomplete, so the app is signed but not notarized"
fi

# Developer ID: hardened runtime, secure timestamp, the (empty) entitlements. Ad-hoc otherwise.
sign() {
  local kc=()
  [ -n "${MACOS_KEYCHAIN:-}" ] && kc=(--keychain "$MACOS_KEYCHAIN")
  if [ -n "$IDENTITY" ]; then
    codesign --force --options runtime --timestamp --entitlements packaging/macos/entitlements.plist \
      --sign "$IDENTITY" ${kc[@]+"${kc[@]}"} "$1"
  else
    codesign --force --sign - "$1"
  fi
}

# `stapler` fetches the ticket from Apple's servers, and a dropped connection fails it although
# notarization worked; try a few times.
stapler_retry() {
  for try in 1 2 3 4; do
    xcrun stapler "$1" "$2" && return 0
    echo "stapler $1 failed (try $try of 4); retrying in $((try * 20)) s" >&2
    sleep $((try * 20))
  done
  echo "error: stapler $1 $(basename "$2") failed 4 times" >&2
  return 1
}

notarize() {
  local file="$1" out id status
  echo "Notarizing $(basename "$file") (this can take a few minutes)"
  out="$(xcrun notarytool submit "$file" --apple-id "$APPLE_ID" --password "$APPLE_PASSWORD" \
    --team-id "$APPLE_TEAM_ID" --wait --timeout 1h --output-format json)" || true
  id="$(printf '%s' "$out" | sed -n 's/.*"id" *: *"\([^"]*\)".*/\1/p' | head -n 1)"
  status="$(printf '%s' "$out" | sed -n 's/.*"status" *: *"\([^"]*\)".*/\1/p' | head -n 1)"
  if [ "$status" != "Accepted" ]; then
    echo "$out" >&2
    [ -n "$id" ] && xcrun notarytool log "$id" --apple-id "$APPLE_ID" --password "$APPLE_PASSWORD" \
      --team-id "$APPLE_TEAM_ID" || true
    echo "error: notarization of $(basename "$file") failed (status: ${status:-unknown})" >&2
    exit 1
  fi
}

# Inner code first, then the bundle.
sign "$APP/Contents/MacOS/tanki-cli"
sign "$APP"
codesign --verify --strict --deep --verbose=2 "$APP"
if [ "$NOTARIZE" = 1 ]; then
  ditto -c -k --keepParent "$APP" "$WORK/notarize.zip"
  notarize "$WORK/notarize.zip"
  stapler_retry staple "$APP"
  stapler_retry validate "$APP"
  spctl --assess --type execute -vvv "$APP"
fi

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
  kc=()
  [ -n "${MACOS_KEYCHAIN:-}" ] && kc=(--keychain "$MACOS_KEYCHAIN")
  codesign --force --timestamp --sign "$IDENTITY" ${kc[@]+"${kc[@]}"} "$DMG"
  codesign --verify --strict --verbose=2 "$DMG"
fi
if [ "$NOTARIZE" = 1 ]; then
  notarize "$DMG"
  stapler_retry staple "$DMG"
  stapler_retry validate "$DMG"
  spctl --assess --type open --context context:primary-signature -vvv "$DMG"
fi
echo "Created $DMG"
