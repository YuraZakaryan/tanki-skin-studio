#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
# SPDX-License-Identifier: GPL-3.0-or-later
# Regenerate every app icon from assets/app-icon/tanki-skin-studio.svg, as WordCraft does. The
# mark itself is drawn in design.pen ("Mark B · 512"); export its shapes to the SVG when it changes.
#
# Needs: resvg (cargo install resvg / brew install resvg). On macOS, iconutil also writes the
# .icns. The outputs are committed, so builds and packaging never need these tools.
#
#   packaging/icons.sh
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIR="$ROOT/assets/app-icon"
NAME="tanki-skin-studio"
SVG="$DIR/$NAME.svg"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

command -v resvg >/dev/null || { echo "error: resvg not found (cargo install resvg)" >&2; exit 1; }

# The mark is a full-bleed 512 tile. Windows uses it as is. macOS icons sit on Apple's grid: an
# 824/1024 body with a transparent margin, so the viewBox widens to 512*1024/824.
MAC="$TMP/mac.svg"
sed 's/viewBox="0 0 512 512"/viewBox="-62.136 -62.136 636.272 636.272"/' "$SVG" >"$MAC"
grep -q 'viewBox="-62.136' "$MAC" || { echo "error: expected viewBox=\"0 0 512 512\" in $SVG" >&2; exit 1; }

render() {
  local src="$1" size="$2" dst="$3"
  resvg -w "$size" -h "$size" "$src" "$dst" </dev/null
}

render "$SVG" 1024 "$DIR/$NAME-1024.png"
# Runtime window and Dock icons (embedded by apps/tanki-studio/src/icon.rs).
render "$MAC" 512 "$DIR/$NAME-macos-512.png"
render "$SVG" 256 "$DIR/$NAME-256.png"

# Windows .ico (embedded into tanki-studio.exe by apps/tanki-studio/build.rs).
ICO_PNGS=()
for s in 16 20 24 32 40 48 64 128 256; do
  render "$SVG" "$s" "$TMP/ico-$s.png"
  ICO_PNGS+=("$TMP/ico-$s.png")
done
(cd "$ROOT" && cargo run -q -p xtask -- ico "$DIR/$NAME.ico" "${ICO_PNGS[@]}")

# macOS .icns (copied into the .app by packaging/macos/package.sh).
if command -v iconutil >/dev/null; then
  SET="$TMP/$NAME.iconset"
  mkdir -p "$SET"
  for s in 16 32 128 256 512; do
    render "$MAC" "$s" "$SET/icon_${s}x${s}.png"
    render "$MAC" $((s * 2)) "$SET/icon_${s}x${s}@2x.png"
  done
  iconutil -c icns -o "$DIR/$NAME.icns" "$SET"
else
  echo "warning: iconutil not found (macOS only); $NAME.icns not regenerated" >&2
fi
echo "icons written to $DIR"
