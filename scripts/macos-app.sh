#!/usr/bin/env bash
# Makes Eppi.app from a built eppi binary: the icon at every size an iconset
# names, an Info.plist with a bundle id (so notifications say "Eppi"), and
# an ad hoc signature (so macOS runs it). install.sh and the release build
# both use it.
#
#   scripts/macos-app.sh <eppi binary> <where Eppi.app goes>
set -euo pipefail

binary="$1"
app="$2"
repo="$(cd "$(dirname "$0")/.." && pwd)"
version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$repo/Cargo.toml" | head -1)"

rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$binary" "$app/Contents/MacOS/eppi"
chmod 755 "$app/Contents/MacOS/eppi"

# The icon, at each size an iconset names, from the 256 px PNG (sizes
# above it would only be blown up, so there's no 256@2x or 512).
work="$(mktemp -d)"
iconset="$work/eppi.iconset"
mkdir -p "$iconset"
icon() {
    sips -z "$1" "$1" "$repo/assets/icon-256.png" --out "$iconset/$2" >/dev/null
}
icon 16 icon_16x16.png
icon 32 icon_16x16@2x.png
icon 32 icon_32x32.png
icon 64 icon_32x32@2x.png
icon 128 icon_128x128.png
icon 256 icon_128x128@2x.png
icon 256 icon_256x256.png
iconutil -c icns "$iconset" -o "$app/Contents/Resources/eppi.icns"
rm -rf "$work"

cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>Eppi</string>
    <key>CFBundleDisplayName</key><string>Eppi</string>
    <key>CFBundleIdentifier</key><string>io.github.rj-jones.eppi</string>
    <key>CFBundleExecutable</key><string>eppi</string>
    <key>CFBundleIconFile</key><string>eppi</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$version</string>
    <key>CFBundleVersion</key><string>$version</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <key>LSApplicationCategoryType</key><string>public.app-category.games</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSHumanReadableCopyright</key><string>GPL-3.0-or-later. Art from Slippi Launcher and Nintendo; see assets/README.md.</string>
</dict>
</plist>
PLIST

# Ad hoc: enough for macOS to run it. (codesign's own message, if it
# fails, stays on stderr.)
if ! codesign --force --deep --sign - "$app" >/dev/null; then
    echo "codesign failed (above); the app may still open" >&2
fi
echo "Made $app (Eppi $version)"
