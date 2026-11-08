#!/usr/bin/env bash
# Builds Eppi and installs it for this user, on macOS or Linux (Windows has
# scripts/install.ps1). Run it again after changing the code.
#
#   scripts/install.sh               build and install
#   scripts/install.sh --uninstall   remove what it installed (your index and
#                                    settings stay, see below)
#
# macOS: ~/Applications/Eppi.app (Spotlight and Launchpad find it). The app
#   is signed ad hoc so macOS runs it, and gets a bundle id so its
#   notifications say "Eppi".
# Linux: ~/.local/bin/eppi, with an icon and a desktop entry so your
#   desktop's app menu lists it.
#
# Eppi needs Rust 1.95 or newer. To build with a toolchain other than your
# default without changing it: RUSTUP_TOOLCHAIN=1.95.0 scripts/install.sh
#
# The index and settings live in ~/Library/Application Support/eppi (macOS)
# or ~/.local/share/eppi (Linux); delete that folder to remove everything.
set -euo pipefail

repo="$(cd "$(dirname "$0")/.." && pwd)"
version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$repo/Cargo.toml" | head -1)"
uninstall=false
[ "${1:-}" = "--uninstall" ] && uninstall=true

case "$(uname -s)" in
Darwin)
    app="$HOME/Applications/Eppi.app"
    if $uninstall; then
        rm -rf "$app"
        echo "Removed $app"
        exit 0
    fi
    if pgrep -xq eppi; then
        echo "Close Eppi first, then run this again." >&2
        exit 1
    fi
    (cd "$repo" && cargo build --release -p eppi)

    rm -rf "$app"
    mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
    cp "$repo/target/release/eppi" "$app/Contents/MacOS/eppi"

    # The icon, at every size macOS asks for, from the 256 px PNG.
    work="$(mktemp -d)"
    iconset="$work/eppi.iconset"
    mkdir -p "$iconset"
    for size in 16 32 64 128 256; do
        sips -z "$size" "$size" "$repo/assets/icon-256.png" --out "$iconset/icon_${size}x${size}.png" >/dev/null
        half=$((size / 2))
        if [ "$half" -ge 16 ]; then
            cp "$iconset/icon_${size}x${size}.png" "$iconset/icon_${half}x${half}@2x.png"
        fi
    done
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
</dict>
</plist>
PLIST

    # Ad hoc: enough for macOS to run a build made on this Mac.
    codesign --force --deep --sign - "$app" >/dev/null 2>&1 || echo "codesign failed; the app may still open"
    echo "Installed $app (Eppi $version)"
    ;;

Linux)
    bin="$HOME/.local/bin/eppi"
    icon="$HOME/.local/share/icons/hicolor/256x256/apps/eppi.png"
    desktop="$HOME/.local/share/applications/eppi.desktop"
    if $uninstall; then
        rm -f "$bin" "$icon" "$desktop"
        echo "Removed $bin, its icon and its desktop entry"
        exit 0
    fi
    if pgrep -x eppi >/dev/null; then
        echo "Close Eppi first, then run this again." >&2
        exit 1
    fi
    (cd "$repo" && cargo build --release -p eppi)
    mkdir -p "$(dirname "$bin")" "$(dirname "$icon")" "$(dirname "$desktop")"
    install -m 755 "$repo/target/release/eppi" "$bin"
    install -m 644 "$repo/assets/icon-256.png" "$icon"
    cat > "$desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Eppi
Comment=Your Slippi replays: sets, stats, ranks and opponents
Exec=$bin
Icon=eppi
Terminal=false
Categories=Game;Utility;
StartupWMClass=eppi
DESKTOP
    if command -v update-desktop-database >/dev/null; then
        update-desktop-database -q "$(dirname "$desktop")" || true
    fi
    echo "Installed $bin (Eppi $version)"
    case ":$PATH:" in
    *":$HOME/.local/bin:"*) ;;
    *) echo "Note: ~/.local/bin isn't on your PATH; the app menu entry works anyway." ;;
    esac
    ;;

*)
    echo "On Windows, use scripts\\install.ps1." >&2
    exit 1
    ;;
esac
