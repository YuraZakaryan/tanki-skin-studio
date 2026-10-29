#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
cargo build --release --locked -p tanki-studio -p tanki-cli
app="dist/Tanki Skin Studio.app"
mkdir -p "$app/Contents/MacOS"
cp target/release/tanki-studio "$app/Contents/MacOS/tanki-studio"
cat > "$app/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>Tanki Skin Studio</string>
<key>CFBundleDisplayName</key><string>Tanki Skin Studio</string>
<key>CFBundleIdentifier</key><string>dev.tanki.skinstudio</string>
<key>CFBundleVersion</key><string>0.1.0</string>
<key>CFBundleShortVersionString</key><string>0.1.0</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleExecutable</key><string>tanki-studio</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
codesign --force --sign - "$app"
printf 'Created %s\n' "$app"
